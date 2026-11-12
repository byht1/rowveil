//! Small shared UI helpers: notifications, muted text, a section header.

use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _};
use gpui_kit::*;

pub fn toast_ok(window: &mut Window, cx: &mut App, msg: impl Into<SharedString>) {
    window.push_notification(Notification::success(msg), cx);
}

pub fn toast_err(window: &mut Window, cx: &mut App, msg: impl Into<SharedString>) {
    window.push_notification(Notification::error(msg), cx);
}

pub fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_sm().text_color(cx.theme().muted_foreground).child(text.into())
}

/// Copies text to the clipboard.
pub fn copy(text: impl Into<String>, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.into()));
}

/// "1,234,567"
pub fn fmt_num(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Asks where to save (native "Save as", Downloads first) and writes the file.
pub fn save_file(name: String, content: String, window: &mut Window, cx: &mut App) {
    let dir = dirs_download();
    let rx = cx.prompt_for_new_path(&dir, Some(&name));
    window
        .spawn(cx, async move |cx| {
            let picked = rx.await.ok().and_then(|r| r.ok()).flatten();
            let Some(path) = picked else { return };
            let res = std::fs::write(&path, content);
            let _ = cx.update(|window, cx| match res {
                Ok(()) => {
                    toast_ok(window, cx, format!("Saved: {}", path.display()));
                    cx.reveal_path(&path);
                }
                Err(e) => toast_err(window, cx, format!("Cannot write {}: {e}", path.display())),
            });
        })
        .detach();
}

fn dirs_download() -> std::path::PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| std::path::PathBuf::from(h).join("Downloads")).filter(|p| p.is_dir()).unwrap_or_else(std::env::temp_dir)
}

macro_rules! palette {
    ($($f:ident),* $(,)?) => {
        /// The stable UI's CSS variables for the current theme (static/style.css), e.g. `dim`, `accent_soft`.
        #[derive(Clone, Copy)]
        #[allow(dead_code)]
        pub struct Pal { $(pub $f: Hsla,)* }

        impl Pal {
            fn from_map(m: &std::collections::HashMap<String, String>) -> Pal {
                let c = |k: &str| m.get(k).and_then(|v| Rgba::try_from(v.as_str()).ok()).map(Hsla::from).unwrap_or_default();
                Pal { $($f: c(stringify!($f).trim_end_matches('_')),)* }
            }
        }
    };
}
palette!(
    bg, panel, panel_2, elev, hover, border, border_strong, text, muted, dim, accent, accent_text, accent_soft, danger, danger_soft,
    mod_bg, mod_border, new_bg, del_bg, sel, row_sel, true_, false_, t_kw, t_str, t_num, t_com, t_id, t_fn, t_type, t_const,
);

impl Global for Pal {}

/// The current palette.
pub fn pal(cx: &App) -> Pal {
    *cx.global::<Pal>()
}

thread_local! {
    static INPUT_BG: std::cell::Cell<Hsla> = std::cell::Cell::new(Hsla::default());
}

/// The background of text inputs (`.input`: var(--bg)).
pub fn input_bg() -> Hsla {
    INPUT_BG.with(|c| c.get())
}

/// Sets the palette of a theme by name (see palette.json).
pub fn set_palette(name: &str, dark: bool, cx: &mut App) {
    let all: std::collections::HashMap<String, std::collections::HashMap<String, String>> = serde_json::from_str(include_str!("palette.json")).unwrap_or_default();
    let m = all.get(name).or_else(|| all.get(if dark { "Rowveil Dark" } else { "Rowveil Light" }));
    if let Some(m) = m {
        let pal = Pal::from_map(m);
        INPUT_BG.with(|c| c.set(pal.bg));
        cx.set_global(pal);
    }
}
