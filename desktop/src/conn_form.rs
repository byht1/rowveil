//! The connection dialog: new (Fields / URL modes, Test, Save & connect) and edit
//! (name, tags, read-only, Redis keys per scan; env connections: tags only).

use crate::backend::Backend;
use crate::connurl::{self, Fields, DB_TYPES};
use crate::model::{self, Conn};
use crate::{kit, ui};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, Sizable as _, StyledExt as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::json;
use std::rc::Rc;
use std::sync::Arc;

pub const TAG_PRESETS: &[&str] = &["LOCAL", "DEV", "TEST", "STAGE", "PROD"];

/// Uppercase, letters / digits / "-_. " only, at most 24 characters.
pub fn clean_tag(s: &str) -> String {
    s.trim().to_uppercase().chars().filter(|c| c.is_alphanumeric() || "-_. ".contains(*c)).take(24).collect::<String>().trim().to_string()
}

/// The color of a tag chip.
/// The tag tones of the stable UI (prod, stage, dev, local, test, other).
pub fn tag_color(tag: &str, _cx: &App) -> Hsla {
    let hex = match tag.to_ascii_uppercase().as_str() {
        "PROD" | "PRODUCTION" | "PRD" | "LIVE" => 0xef4444,
        "STAGE" | "STAGING" | "STG" | "PREPROD" | "PRE-PROD" | "UAT" => 0xf59e0b,
        "DEV" | "DEVELOP" | "DEVELOPMENT" => 0x3b82f6,
        "LOCAL" | "LOCALHOST" => 0x10b981,
        "TEST" | "TESTING" | "QA" | "CI" => 0x8b5cf6,
        _ => 0x64748b,
    };
    rgb(hex).into()
}

/// `.tag-chip`: 17 px, bold 9.5 px, tinted background.
pub fn tag_chip(tag: &str, cx: &App) -> Div {
    let c = tag_color(tag, cx);
    div()
        .flex_none()
        .h(px(17.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .text_size(px(9.5))
        .font_weight(FontWeight::BOLD)
        .text_color(c)
        .bg(c.opacity(0.14))
        .border_1()
        .border_color(c.opacity(0.45))
        .child(tag.to_string())
}

pub type OnSaved = Rc<dyn Fn(Conn, &mut Window, &mut App)>;

pub struct ConnForm {
    backend: Arc<Backend>,
    edit: Option<Conn>,
    on_saved: OnSaved,
    url_mode: bool,
    kind: String,
    ssl: String,
    read_only: bool,
    show_password: bool,
    tags: Vec<String>,
    status: Option<(bool, String)>,
    busy: bool,
    name: Entity<InputState>,
    url: Entity<InputState>,
    host: Entity<InputState>,
    port: Entity<InputState>,
    user: Entity<InputState>,
    password: Entity<InputState>,
    database: Entity<InputState>,
    options: Entity<InputState>,
    file: Entity<InputState>,
    project: Entity<InputState>,
    dataset: Entity<InputState>,
    key_file: Entity<InputState>,
    location: Entity<InputState>,
    max_bytes: Entity<InputState>,
    endpoint: Entity<InputState>,
    tag_input: Entity<InputState>,
    scan_limit: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl ConnForm {
    pub fn new(backend: Arc<Backend>, edit: Option<Conn>, url_mode: bool, on_saved: OnSaved, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subs = Vec::new();
        let mut input = |ph: &str, value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let e = cx.new(|cx| InputState::new(window, cx).placeholder(ph.to_string()).default_value(value.to_string()));
            subs.push(cx.subscribe_in(&e, window, |this: &mut Self, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => {
                    this.status = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.save(window, cx),
                _ => {}
            }));
            e
        };
        let pg = connurl::db_type("postgres");
        let name = input("Optional: taken from the URL", edit.as_ref().map(|c| c.name.as_str()).unwrap_or(""), window, cx);
        let url = input("postgres://user:password@localhost:5432/db", edit.as_ref().map(|c| c.url.as_str()).unwrap_or(""), window, cx);
        let host = input("localhost", "localhost", window, cx);
        let port = input(pg.port, pg.port, window, cx);
        let user = input(pg.user, pg.user, window, cx);
        let database = input("Database", "", window, cx);
        let options = input("connect_timeout=10", "", window, cx);
        let file = input("C:/data/app.db", "", window, cx);
        let project = input("my-project", "", window, cx);
        let dataset = input("Optional", "", window, cx);
        let key_file = input("Optional: path to a service account JSON key", "", window, cx);
        let location = input("Optional: US, EU…", "", window, cx);
        let max_bytes = input("Optional: limit per query", "", window, cx);
        let endpoint = input("Optional: emulator URL", "", window, cx);
        let scan_limit = input("10000", &edit.as_ref().map(|c| c.scan_limit.max(100).to_string()).unwrap_or_else(|| "10000".into()), window, cx);
        let password = cx.new(|cx| InputState::new(window, cx).placeholder("Password").masked(true));
        subs.push(cx.subscribe(&password, |this: &mut Self, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                this.status = None;
                cx.notify();
            }
        }));
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add a tag: Enter"));
        subs.push(cx.subscribe_in(&tag_input, window, |this: &mut Self, input, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                let text = input.read(cx).value().to_string();
                for t in text.split([',', ';', '|']) {
                    this.add_tag(t);
                }
                input.update(cx, |s, cx| s.set_value("", window, cx));
                cx.notify();
            }
        }));
        let tags = edit.as_ref().map(|c| c.tags.clone()).unwrap_or_default();
        let read_only = edit.as_ref().map(|c| c.read_only).unwrap_or(false);
        name.update(cx, |s, cx| s.focus(window, cx));
        Self {
            backend,
            edit,
            on_saved,
            url_mode,
            kind: "postgres".into(),
            ssl: "default".into(),
            read_only,
            show_password: false,
            tags,
            status: None,
            busy: false,
            name,
            url,
            host,
            port,
            user,
            password,
            database,
            options,
            file,
            project,
            dataset,
            key_file,
            location,
            max_bytes,
            endpoint,
            tag_input,
            scan_limit,
            _subs: subs,
        }
    }

    /// Opens the dialog. `edit` = None creates a connection.
    pub fn open(backend: Arc<Backend>, edit: Option<Conn>, url_mode: bool, on_saved: OnSaved, window: &mut Window, cx: &mut App) {
        let title = if edit.is_some() { "Edit connection" } else { "New connection" };
        let form = cx.new(|cx| ConnForm::new(backend, edit, url_mode, on_saved, window, cx));
        window.open_dialog(cx, move |d, _, cx| crate::kit::dialog(d, title, 560., cx).child(form.clone()));
    }

    fn add_tag(&mut self, t: &str) {
        let t = clean_tag(t);
        if !t.is_empty() && !self.tags.contains(&t) && self.tags.len() < 8 {
            self.tags.push(t);
        }
    }

    fn v(e: &Entity<InputState>, cx: &App) -> String {
        e.read(cx).value().to_string()
    }

    fn fields(&self, cx: &App) -> Fields {
        Fields {
            kind: self.kind.clone(),
            host: Self::v(&self.host, cx),
            port: Self::v(&self.port, cx),
            user: Self::v(&self.user, cx),
            password: Self::v(&self.password, cx),
            database: Self::v(&self.database, cx),
            ssl: self.ssl.clone(),
            options: Self::v(&self.options, cx),
            file: Self::v(&self.file, cx),
            project: Self::v(&self.project, cx),
            dataset: Self::v(&self.dataset, cx),
            key_file: Self::v(&self.key_file, cx),
            location: Self::v(&self.location, cx),
            max_bytes: Self::v(&self.max_bytes, cx),
            endpoint: Self::v(&self.endpoint, cx),
        }
    }

    fn set_fields(&mut self, f: &Fields, window: &mut Window, cx: &mut Context<Self>) {
        self.kind = if f.kind.is_empty() { "postgres".into() } else { f.kind.clone() };
        self.ssl = if f.ssl.is_empty() { "default".into() } else { f.ssl.clone() };
        for (e, v) in [
            (&self.host, &f.host),
            (&self.port, &f.port),
            (&self.user, &f.user),
            (&self.password, &f.password),
            (&self.database, &f.database),
            (&self.options, &f.options),
            (&self.file, &f.file),
            (&self.project, &f.project),
            (&self.dataset, &f.dataset),
            (&self.key_file, &f.key_file),
            (&self.location, &f.location),
            (&self.max_bytes, &f.max_bytes),
            (&self.endpoint, &f.endpoint),
        ] {
            let v = v.clone();
            e.update(cx, |s, cx| s.set_value(v, window, cx));
        }
    }

    fn set_mode(&mut self, url_mode: bool, window: &mut Window, cx: &mut Context<Self>) {
        if url_mode == self.url_mode {
            return;
        }
        if url_mode {
            let url = connurl::build(&self.fields(cx), false);
            self.url.update(cx, |s, cx| s.set_value(url, window, cx));
        } else {
            let f = connurl::parse(&Self::v(&self.url, cx));
            if !Self::v(&self.url, cx).trim().is_empty() {
                self.set_fields(&f, window, cx);
            }
        }
        self.url_mode = url_mode;
        self.status = None;
        cx.notify();
    }

    fn set_kind(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        let old = connurl::db_type(&self.kind);
        let new = connurl::db_type(kind);
        // Defaults follow the type unless the user typed something else.
        for (e, o, n) in [(&self.port, old.port, new.port), (&self.user, old.user, new.user)] {
            let cur = Self::v(e, cx);
            if cur.is_empty() || cur == o {
                e.update(cx, |s, cx| s.set_value(n.to_string(), window, cx));
            }
        }
        if new.ssl.is_none() && self.ssl == "disable" {
            self.ssl = "default".into();
        }
        self.kind = kind.to_string();
        self.status = None;
        cx.notify();
    }

    /// The URL to test or save, or an error message.
    fn url(&self, cx: &App) -> Result<String, String> {
        if self.url_mode {
            let u = Self::v(&self.url, cx).trim().to_string();
            return if u.is_empty() { Err("Enter the connection URL".into()) } else { Ok(u) };
        }
        let f = self.fields(cx);
        if let Some(e) = connurl::validate(&f) {
            return Err(e.into());
        }
        Ok(connurl::build(&f, false))
    }

    fn test(&mut self, cx: &mut Context<Self>) {
        let url = match self.url(cx) {
            Ok(u) => u,
            Err(e) => {
                self.status = Some((false, e));
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.status = Some((true, "Connecting…".into()));
        cx.notify();
        let fut = self.backend.clone().post("/connections/test", json!({ "url": url }));
        cx.spawn(async move |this, cx| {
            let res = fut.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.status = Some(match res {
                    Ok(v) => (true, format!("Connected ({})", model::dialect_label(v["dialect"].as_str().unwrap_or("")))),
                    Err(e) => (false, e.to_string()),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        // A tag typed but not confirmed with Enter still counts.
        let pending = Self::v(&self.tag_input, cx);
        for t in pending.split([',', ';', '|']) {
            self.add_tag(t);
        }
        let (path, method_post, body) = match &self.edit {
            Some(c) => {
                let mut body = json!({ "tags": self.tags });
                if !c.is_env() {
                    body["name"] = json!(Self::v(&self.name, cx));
                    body["readOnly"] = json!(self.read_only);
                    if c.dialect() == "redis" {
                        match Self::v(&self.scan_limit, cx).trim().parse::<u32>() {
                            Ok(n) => body["scanLimit"] = json!(n),
                            Err(_) => {
                                self.status = Some((false, "Keys per scan: enter a number from 100 to 1000000".into()));
                                cx.notify();
                                return;
                            }
                        }
                    }
                }
                (format!("/connections/{}", crate::backend::enc(&c.id)), false, body)
            }
            None => match self.url(cx) {
                Ok(url) => ("/connections".to_string(), true, json!({ "name": Self::v(&self.name, cx), "url": url, "readOnly": self.read_only, "tags": self.tags })),
                Err(e) => {
                    self.status = Some((false, e));
                    cx.notify();
                    return;
                }
            },
        };
        self.busy = true;
        self.status = Some((true, if method_post { "Connecting…".into() } else { "Saving…".into() }));
        cx.notify();
        let backend = self.backend.clone();
        let method = if method_post { axum::http::Method::POST } else { axum::http::Method::PATCH };
        let fut = backend.call(method, path, Some(body));
        let on_saved = self.on_saved.clone();
        cx.spawn_in(window, async move |this, cx| {
            let res = fut.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match res.and_then(model::parse::<Conn>) {
                    Ok(conn) => {
                        window.close_dialog(cx);
                        ui::toast_ok(window, cx, "Connection saved");
                        on_saved(conn, window, cx);
                    }
                    Err(e) => {
                        this.status = Some((false, e.to_string()));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn field(label: &str, input: impl IntoElement) -> Div {
        v_flex().gap(px(5.)).flex_1().min_w_0().child(div().text_size(px(12.)).child(label.to_string())).child(input)
    }

    /// A label with a muted "(optional)".
    fn field_opt(label: &str, input: impl IntoElement, cx: &App) -> Div {
        let p = ui::pal(cx);
        v_flex()
            .gap(px(5.))
            .flex_1()
            .min_w_0()
            .child(h_flex().gap(px(6.)).text_size(px(12.)).child(label.to_string()).child(div().text_size(px(11.)).font_family(kit::cx_mono(cx)).text_color(p.dim).child("(optional)")))
            .child(input)
    }

    fn render_fields(&self, cx: &mut Context<Self>) -> Div {
        let p = ui::pal(cx);
        let mono = kit::cx_mono(cx);
        let t = connurl::db_type(&self.kind);
        let kinds: Vec<&'static str> = DB_TYPES.iter().map(|ty| ty.id).collect();
        let me = cx.weak_entity();
        let types = kit::seg("db-type", DB_TYPES.iter().map(|ty| (SharedString::from(ty.label), self.kind == ty.id)).collect(), move |i, window, cx| {
            let id = kinds[i];
            let _ = me.update(cx, |this, cx| this.set_kind(id, window, cx));
        }, cx)
        .w_full()
        .children(Vec::<Div>::new());
        let mut body = v_flex().gap(px(12.)).child(Self::field("Database type", types));
        match t.id {
            "sqlite" => body = body.child(Self::field("Database file", Input::new(&self.file).bg(crate::ui::input_bg()).font_family(mono.clone()))),
            "bigquery" => {
                body = body
                    .child(h_flex().gap(px(12.)).child(Self::field("Project ID", Input::new(&self.project).bg(crate::ui::input_bg()).font_family(mono.clone()))).child(Self::field("Default dataset", Input::new(&self.dataset).bg(crate::ui::input_bg()).font_family(mono.clone()))))
                    .child(Self::field_opt("Service account key file", Input::new(&self.key_file).bg(crate::ui::input_bg()).font_family(mono.clone()), cx))
                    .child(h_flex().gap(px(12.)).child(Self::field_opt("Location", Input::new(&self.location).bg(crate::ui::input_bg()).font_family(mono.clone()), cx)).child(Self::field_opt("Max bytes billed", Input::new(&self.max_bytes).bg(crate::ui::input_bg()).font_family(mono.clone()), cx)))
                    .child(Self::field_opt("API endpoint", Input::new(&self.endpoint).bg(crate::ui::input_bg()).font_family(mono.clone()), cx))
                    .child(div().text_size(px(12.)).text_color(p.muted).child("Without a key file, Rowveil uses GOOGLE_APPLICATION_CREDENTIALS or the gcloud login. Queries are billed by BigQuery."));
            }
            _ => {
                let redis = t.id == "redis";
                let ssl_opts: Vec<(&'static str, &'static str)> = if redis { vec![("default", "No TLS"), ("require", "TLS")] } else { vec![("default", "Default"), ("disable", "Disable"), ("require", "Require")] };
                let cur = ssl_opts.iter().find(|(id, _)| *id == self.ssl).map(|x| x.1).unwrap_or("Default");
                let me = cx.weak_entity();
                let cur_id = self.ssl.clone();
                let ssl = kit::select("ssl", cur, cx).w_full().h(px(32.)).dropdown_menu(move |mut menu, _, _| {
                    for (id, label) in &ssl_opts {
                        let (me, id) = (me.clone(), *id);
                        menu = menu.item(PopupMenuItem::new(*label).checked(cur_id == id).on_click(move |_, _, cx| {
                            let _ = me.update(cx, |this, cx| {
                                this.ssl = id.to_string();
                                cx.notify();
                            });
                        }));
                    }
                    menu
                });
                let show = self.show_password;
                body = body
                    .child(h_flex().gap(px(12.)).child(div().flex_1().child(Self::field("Host", Input::new(&self.host).bg(crate::ui::input_bg()).font_family(mono.clone())))).child(div().w(px(110.)).child(Self::field("Port", Input::new(&self.port).bg(crate::ui::input_bg()).font_family(mono.clone())))))
                    .child(
                        h_flex().gap(px(12.)).child(Self::field(if redis { "User (ACL, optional)" } else { "User" }, Input::new(&self.user).bg(crate::ui::input_bg()).font_family(mono.clone()))).child(Self::field(
                            "Password",
                            Input::new(&self.password).bg(crate::ui::input_bg()).font_family(mono.clone()).suffix(
                                kit::icon_btn("eye", if show { "eyeOff" } else { "eye" }, cx).size(px(22.)).tooltip(if show { "Hide" } else { "Show" }).on_click(cx.listener(|this, _, window, cx| {
                                    this.show_password = !this.show_password;
                                    let m = !this.show_password;
                                    this.password.update(cx, |s, cx| s.set_masked(m, window, cx));
                                    cx.notify();
                                })),
                            ),
                        )),
                    )
                    .child(h_flex().gap(px(12.)).child(Self::field(if redis { "Database number" } else { "Database" }, Input::new(&self.database).bg(crate::ui::input_bg()).font_family(mono.clone()))).child(Self::field(if redis { "TLS" } else { "SSL" }, ssl)))
                    .child(Self::field_opt("Extra parameters", Input::new(&self.options).bg(crate::ui::input_bg()).font_family(mono.clone()), cx));
            }
        }
        let preview = connurl::build(&self.fields(cx), true);
        body.child(Self::field(
            "Connection URL",
            div()
                .min_h(px(32.))
                .px(px(9.))
                .py(px(6.))
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
                .bg(p.panel_2)
                .text_size(px(12.5))
                .font_family(mono)
                .font_semibold()
                .text_color(p.text)
                .child(if preview.is_empty() { " ".to_string() } else { preview }),
        ))
    }

    fn render_tags(&self, cx: &mut Context<Self>) -> Div {
        let p = ui::pal(cx);
        let presets = TAG_PRESETS.iter().map(|tp| {
            let on = self.tags.iter().any(|t| t == tp);
            let c = tag_color(tp, cx);
            div()
                .id(SharedString::from(format!("preset-{tp}")))
                .h(px(22.))
                .px(px(9.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .text_size(px(10.5))
                .font_weight(FontWeight::BOLD)
                .cursor_pointer()
                .border_1()
                .text_color(c)
                .map(|d| if on { d.border_color(c.opacity(0.45)).bg(c.opacity(0.2)) } else { d.border_dashed().border_color(c.opacity(0.45)).opacity(0.55).hover(|s| s.opacity(0.85)) })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(i) = this.tags.iter().position(|t| t == tp) {
                        this.tags.remove(i);
                    } else {
                        this.add_tag(tp);
                    }
                    cx.notify();
                }))
                .child(tp.to_string())
        });
        let custom: Vec<String> = self.tags.iter().filter(|t| !TAG_PRESETS.contains(&t.as_str())).cloned().collect();
        let chips = custom.into_iter().enumerate().map(|(i, t)| {
            let tag = t.clone();
            tag_chip(&t, cx).child(
                div()
                    .id(("rm-tag", i))
                    .pl(px(3.))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .opacity(0.7)
                    .hover(|s| s.opacity(1.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tags.retain(|x| *x != tag);
                        cx.notify();
                    }))
                    .child(svg().path("rv/x.svg").size(px(10.)).text_color(tag_color(&t, cx))),
            )
        });
        let _ = p;
        Self::field_opt("Tags", v_flex().gap(px(6.)).child(h_flex().gap(px(5.)).flex_wrap().children(presets).children(chips)).child(Input::new(&self.tag_input).bg(crate::ui::input_bg()).small()), cx)
    }
}

impl Render for ConnForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let env = self.edit.as_ref().is_some_and(|c| c.is_env());
        let mut body = v_flex().gap(px(12.));
        match &self.edit {
            None => {
                let url_mode = self.url_mode;
                let me = cx.weak_entity();
                let mode = kit::seg("mode", vec![("Parameters".into(), !url_mode), ("URL".into(), url_mode)], move |i, window, cx| {
                    let _ = me.update(cx, |this, cx| this.set_mode(i == 1, window, cx));
                }, cx);
                body = body.child(h_flex().gap(px(12.)).items_end().child(Self::field("Name", Input::new(&self.name).bg(crate::ui::input_bg()))).child(v_flex().gap(px(5.)).child(div().text_size(px(12.)).child("Connect with")).child(mode.h(px(32.)))));
                body = if url_mode { body.child(Self::field("Connection URL", Input::new(&self.url).bg(crate::ui::input_bg()).font_family(kit::cx_mono(cx)))) } else { body.child(self.render_fields(cx)) };
            }
            Some(c) => {
                body = body
                    .child(Self::field("URL", div().px(px(9.)).py(px(6.)).rounded(px(6.)).border_1().border_color(p.border).bg(p.panel_2).text_size(px(12.5)).font_family(kit::cx_mono(cx)).text_color(p.muted).child(rowveil::config::mask_url(&c.url))))
                    .when(!env, |b| b.child(Self::field("Name", Input::new(&self.name).bg(crate::ui::input_bg()))));
            }
        }
        body = body.child(self.render_tags(cx));
        if env {
            body = body.child(div().text_size(px(12.)).text_color(p.muted).child("This connection comes from an environment variable: only its tags can change here."));
        } else {
            let ro = self.read_only;
            body = body.child(Checkbox::new("ro").checked(ro).label("Read-only (block data changes)").on_click(cx.listener(|this, checked: &bool, _, cx| {
                this.read_only = *checked;
                cx.notify();
            })));
            if self.edit.as_ref().is_some_and(|c| c.dialect() == "redis") {
                body = body
                    .child(Self::field("Keys per scan", Input::new(&self.scan_limit).bg(crate::ui::input_bg()).font_family(kit::cx_mono(cx))))
                    .child(div().text_size(px(12.)).text_color(p.muted).child("The key list and a \"*\" search walk this many keys at a time; \"Next\" continues. A search without \"*\" scans all keys. Default: 10000."));
            }
        }
        let status = self.status.clone().map(|(ok, msg)| div().text_size(px(12.5)).px(px(10.)).py(px(8.)).rounded(px(6.)).map(|d| if ok { d.bg(p.new_bg).text_color(p.true_) } else { d.bg(p.danger_soft).text_color(p.danger) }).child(msg));
        v_flex()
            .child(body.children(status))
            .child(
                h_flex()
                    .mt(px(16.))
                    .mx(px(-16.))
                    .px(px(16.))
                    .pt(px(12.))
                    .gap(px(8.))
                    .justify_end()
                    .border_t_1()
                    .border_color(p.border)
                    .when(self.edit.is_none(), |d| d.child(kit::btn("test", cx).label("Test").loading(self.busy).on_click(cx.listener(|this, _, _, cx| this.test(cx)))))
                    .when(self.edit.is_some(), |d| d.child(kit::btn_ghost("cancel", cx).label("Cancel").on_click(|_, window, cx| window.close_dialog(cx))))
                    .child(kit::btn_primary("save", cx).label(if self.edit.is_some() { "Save" } else { "Save & connect" }).loading(self.busy).on_click(cx.listener(|this, _, window, cx| this.save(window, cx)))),
            )
    }
}
