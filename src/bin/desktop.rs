//! Roostbase desktop app: the studio in its own native window (WebView2 on Windows),
//! without a console or a browser. The HTTP server runs inside the process on a random
//! loopback port and accepts only requests that carry a per-launch secret.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use clap::Parser;
use roostbase::{default_data_dir, router, Options};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tao::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Icon, Window, WindowBuilder, WindowId};
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder};

const APP_NAME: &str = "Roostbase";
const ICON_RGBA: &[u8] = include_bytes!("../../assets/window-icon-64.rgba");

#[derive(Parser, Debug)]
#[command(name = "roostbase", version, about = "Roostbase: database explorer and query tool")]
struct Cli {
    /// Database URL or path to a SQLite file to open on start.
    #[arg(env = "DATABASE_URL")]
    url: Option<String>,
    /// Directory for saved connections and window state.
    #[arg(long, env = "STUDIO_DATA_DIR")]
    data_dir: Option<PathBuf>,
    /// Block all data changes.
    #[arg(long, env = "STUDIO_READ_ONLY")]
    read_only: bool,
}

#[derive(Debug)]
enum UserEvent {
    /// Open a URL of this app in a new window (window.open from the UI).
    NewWindow(String),
    /// The page title changed: show it in the window title.
    Title(WindowId, String),
}

/// Size, position and maximized state of the main window, restored on the next start.
#[derive(Serialize, Deserialize, Default, Clone, Copy)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

fn load_window_state(path: &Path) -> Option<WindowState> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok().filter(|s: &WindowState| s.width >= 400 && s.height >= 300)
}

fn save_window_state(path: &Path, window: &Window) {
    let maximized = window.is_maximized();
    let (Ok(pos), size) = (window.outer_position(), window.inner_size()) else { return };
    let mut state = load_window_state(path).unwrap_or_default();
    state.maximized = maximized;
    if !maximized && !window.is_minimized() {
        state = WindowState { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized };
    }
    if let Ok(json) = serde_json::to_string(&state) {
        let _ = std::fs::write(path, json);
    }
}

fn init_logging(data_dir: &Path) {
    let _ = std::fs::create_dir_all(data_dir);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "roostbase=info,warn".into());
    match std::fs::File::create(data_dir.join("roostbase.log")) {
        Ok(file) => tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false).with_writer(std::sync::Mutex::new(file)).init(),
        Err(_) => tracing_subscriber::fmt().with_env_filter(filter).init(),
    }
}

#[cfg(target_os = "macos")]
fn mac_menu() -> muda::Menu {
    use muda::{AboutMetadata, Menu, PredefinedMenuItem, Submenu};
    let menu = Menu::new();
    let about = AboutMetadata { name: Some(APP_NAME.into()), version: Some(env!("CARGO_PKG_VERSION").into()), ..Default::default() };
    let app = Submenu::with_items(APP_NAME, true, &[
        &PredefinedMenuItem::about(None, Some(about)),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::services(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ]);
    let edit = Submenu::with_items("Edit", true, &[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ]);
    let window = Submenu::with_items("Window", true, &[
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::maximize(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::close_window(None),
        &PredefinedMenuItem::fullscreen(None),
    ]);
    for sub in [app, edit, window].into_iter().flatten() {
        let _ = menu.append(&sub);
    }
    menu.init_for_nsapp();
    menu
}

/// A file path given on the command line ("Open with Roostbase") becomes a SQLite URL.
fn to_url(arg: String) -> String {
    let p = Path::new(&arg);
    if !arg.contains("://") && p.is_file() {
        let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let s = abs.display().to_string();
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s).replace('\\', "/");
        return format!("sqlite://{s}");
    }
    arg
}

/// Starts the server on a random loopback port. Returns the base URL.
fn start_server(opts: Options) -> anyhow::Result<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    std::thread::Builder::new().name("roostbase-server".into()).spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
        rt.block_on(async move {
            let (app, store) = router(&opts);
            tracing::info!("saved connections: {}", store.display());
            let listener = tokio::net::TcpListener::from_std(listener).expect("listener");
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("server stopped: {e}");
            }
        });
    })?;
    Ok(format!("http://127.0.0.1:{port}"))
}

struct App {
    base: String,
    token: String,
    icon: Option<Icon>,
    web_context: WebContext,
    windows: HashMap<WindowId, (Window, WebView)>,
    main_id: Option<WindowId>,
    state_file: PathBuf,
}

impl App {
    /// The first URL a window loads carries the token; the server turns it into a cookie.
    fn entry_url(&self, url: &str) -> String {
        let rest = url.strip_prefix(&self.base).unwrap_or("/");
        let (path, hash) = rest.split_once('#').map(|(p, h)| (p, format!("#{h}"))).unwrap_or((rest, String::new()));
        let path = if path.is_empty() { "/" } else { path };
        let sep = if path.contains('?') { '&' } else { '?' };
        format!("{}{path}{sep}token={}{hash}", self.base, self.token)
    }

    fn open_window(&mut self, target: &EventLoopWindowTarget<UserEvent>, proxy: &EventLoopProxy<UserEvent>, url: &str, main: bool) -> anyhow::Result<()> {
        let mut builder = WindowBuilder::new()
            .with_title(APP_NAME)
            .with_window_icon(self.icon.clone())
            .with_min_inner_size(LogicalSize::new(720.0, 480.0))
            .with_inner_size(LogicalSize::new(1360.0, 860.0));
        let saved = if main { load_window_state(&self.state_file) } else { None };
        if let Some(s) = saved {
            builder = builder
                .with_inner_size(PhysicalSize::new(s.width, s.height))
                .with_position(PhysicalPosition::new(s.x, s.y))
                .with_maximized(s.maximized);
        }
        let window = builder.build(target)?;
        if saved.is_none() && !main {
            // Cascade new windows a little so they do not cover the opener exactly.
            if let Ok(p) = window.outer_position() {
                window.set_outer_position(PhysicalPosition::new(p.x + 32, p.y + 32));
            }
        }
        let id = window.id();
        let base = self.base.clone();
        let nav_base = self.base.clone();
        let new_win = proxy.clone();
        let title = proxy.clone();
        let entry = self.entry_url(url);
        let webview = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_url(entry)
            .with_devtools(cfg!(debug_assertions))
            .with_clipboard(true)
            .with_background_color((11, 11, 26, 255))
            // Links to other sites open in the system browser.
            .with_navigation_handler(move |u| {
                if u.starts_with(&nav_base) || u.starts_with("about:") || u.starts_with("data:") || u.starts_with("blob:") {
                    true
                } else {
                    let _ = open::that_detached(&u);
                    false
                }
            })
            // window.open: our pages get a new app window, other sites the browser.
            .with_new_window_req_handler(move |u, _features| {
                if u.starts_with(&base) {
                    let _ = new_win.send_event(UserEvent::NewWindow(u));
                } else {
                    let _ = open::that_detached(&u);
                }
                NewWindowResponse::Deny
            })
            .with_document_title_changed_handler(move |t| {
                let _ = title.send_event(UserEvent::Title(id, t));
            });
        // Linux: the webview lives inside the window's GTK box (works on X11 and Wayland).
        #[cfg(target_os = "linux")]
        let webview = {
            use tao::platform::unix::WindowExtUnix;
            use wry::WebViewBuilderExtUnix;
            let vbox = window.default_vbox().ok_or_else(|| anyhow::anyhow!("no GTK container"))?;
            webview.build_gtk(vbox)?
        };
        #[cfg(not(target_os = "linux"))]
        let webview = webview.build(&window)?;
        if main {
            self.main_id = Some(id);
        }
        self.windows.insert(id, (window, webview));
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let data_dir = cli.data_dir.clone().unwrap_or_else(default_data_dir);
    init_logging(&data_dir);

    let token = uuid::Uuid::new_v4().simple().to_string();
    let base = start_server(Options {
        url: cli.url.clone().map(to_url),
        data_dir: data_dir.clone(),
        read_only: cli.read_only,
        basic_auth: None,
        app_token: Some(token.clone()),
    })?;
    tracing::info!("server at {base}");

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    // macOS: the app menu provides Quit and the Edit shortcuts (Cmd+C / Cmd+V / Cmd+Z …),
    // which a WebView only receives through menu items.
    #[cfg(target_os = "macos")]
    let _menu = mac_menu();
    let mut app = App {
        base: base.clone(),
        token,
        icon: Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok(),
        web_context: WebContext::new(Some(data_dir.join("webview"))),
        windows: HashMap::new(),
        main_id: None,
        state_file: data_dir.join("window.json"),
    };

    let mut started = false;
    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(tao::event::StartCause::Init) if !started => {
                started = true;
                let url = format!("{base}/");
                if let Err(e) = app.open_window(target, &proxy, &url, true) {
                    // No WebView2 runtime (rare on Windows 10/11): fall back to the browser.
                    tracing::error!("cannot create the app window: {e:#}");
                    let _ = open::that_detached(app.entry_url(&url));
                }
            }
            Event::UserEvent(UserEvent::NewWindow(url)) => {
                if let Err(e) = app.open_window(target, &proxy, &url, false) {
                    tracing::error!("cannot open a new window: {e:#}");
                }
            }
            Event::UserEvent(UserEvent::Title(id, title)) => {
                if let Some((w, _)) = app.windows.get(&id) {
                    w.set_title(if title.is_empty() { APP_NAME } else { &title });
                }
            }
            Event::WindowEvent { window_id, event, .. } => match event {
                WindowEvent::CloseRequested => {
                    if Some(window_id) == app.main_id {
                        if let Some((w, _)) = app.windows.get(&window_id) {
                            save_window_state(&app.state_file, w);
                        }
                    }
                    app.windows.remove(&window_id);
                    if app.windows.is_empty() {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                WindowEvent::Moved(_) | WindowEvent::Resized(_) if Some(window_id) == app.main_id => {
                    if let Some((w, _)) = app.windows.get(&window_id) {
                        save_window_state(&app.state_file, w);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    });
}
