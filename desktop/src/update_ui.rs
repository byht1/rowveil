//! The Updates dialog and the quiet check on start. The router's updater does the work
//! (`/api/update/*`, the same as the earlier desktop app): the latest GitHub release, the MSI /
//! portable exe / app bundle / tarball for this install, SHA-256 check, swap and restart.

use crate::backend::Backend;
use crate::{kit, ui, RowveilApp};
use gpui_kit::component::{h_flex, v_flex, StyledExt as _, WindowExt as _};
use gpui_kit::*;
use serde_json::{json, Value};
use std::sync::Arc;

enum Phase {
    Checking,
    /// The answer of `/update/check`.
    Checked(Value),
    /// `/update/progress`: downloading, ready, restarting or error.
    Progress(Value, Value),
    Error(String),
}

pub struct UpdateDialog {
    app: WeakEntity<RowveilApp>,
    backend: Arc<Backend>,
    phase: Phase,
    _poll: Option<Task<()>>,
}

/// Checks quietly a few seconds after start; the sidebar then shows an "Update" button.
pub fn check_on_start(backend: Arc<Backend>, cx: &mut Context<RowveilApp>) {
    if cfg!(debug_assertions) {
        return;
    }
    // 3 seconds after start, then every 6 hours (the check is cached for 30 minutes).
    cx.spawn(async move |app, cx| {
        let mut wait = std::time::Duration::from_secs(3);
        loop {
            cx.background_executor().timer(wait).await;
            wait = std::time::Duration::from_secs(6 * 3600);
            if let Ok(v) = backend.clone().get("/update/check").await {
                if v["available"].as_bool() == Some(true) {
                    let latest = v["latest"].as_str().unwrap_or("").to_string();
                    if app
                        .update(cx, |a, cx| {
                            a.update_available = Some(latest);
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    })
    .detach();
}

impl UpdateDialog {
    pub fn open(app: WeakEntity<RowveilApp>, window: &mut Window, cx: &mut App) {
        let Some(backend) = app.upgrade().map(|a| a.read(cx).backend.clone()) else { return };
        let dlg = cx.new(|cx| {
            let mut d = UpdateDialog { app, backend, phase: Phase::Checking, _poll: None };
            d.check(cx);
            d
        });
        window.open_dialog(cx, move |d, _, cx| kit::dialog(d, "Updates", 560., cx).child(dlg.clone()));
    }

    fn check(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Checking;
        cx.notify();
        let fut = self.backend.clone().get("/update/check?force=1");
        cx.spawn(async move |this, cx| {
            let res = fut.await;
            let _ = this.update(cx, |this, cx| {
                this.phase = match res {
                    Ok(v) => {
                        if v["available"].as_bool() == Some(true) {
                            let latest = v["latest"].as_str().unwrap_or("").to_string();
                            let _ = this.app.update(cx, |a, _| a.update_available = Some(latest));
                        }
                        Phase::Checked(v)
                    }
                    Err(e) => Phase::Error(e.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn download(&mut self, info: Value, cx: &mut Context<Self>) {
        let start = self.backend.clone().post("/update/download", json!({}));
        let backend = self.backend.clone();
        self.phase = Phase::Progress(info.clone(), json!({ "phase": "downloading" }));
        cx.notify();
        self._poll = Some(cx.spawn(async move |this, cx| {
            if let Err(e) = start.await {
                let _ = this.update(cx, |this, cx| {
                    this.phase = Phase::Error(e.to_string());
                    cx.notify();
                });
                return;
            }
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(300)).await;
                let p = backend.clone().get("/update/progress").await.unwrap_or_else(|e| json!({ "phase": "error", "error": e.to_string() }));
                let done = p["phase"].as_str() != Some("downloading");
                let info = info.clone();
                if this
                    .update(cx, |this, cx| {
                        this.phase = Phase::Progress(info, p);
                        cx.notify();
                    })
                    .is_err()
                    || done
                {
                    break;
                }
            }
        }));
    }

    fn restart(&mut self, info: Value, cx: &mut Context<Self>) {
        self.phase = Phase::Progress(info, json!({ "phase": "restarting" }));
        cx.notify();
        // The router hands over to the updater and exits the process; the new version starts by itself.
        let fut = self.backend.clone().post("/update/restart", json!({}));
        cx.spawn(async move |this, cx| {
            if let Err(e) = fut.await {
                let _ = this.update(cx, |this, cx| {
                    this.phase = Phase::Error(e.to_string());
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

fn mb(n: u64) -> String {
    format!("{:.1} MB", n as f64 / 1_048_576.)
}

impl Render for UpdateDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let mut body = v_flex().gap(px(12.)).child(div().text_size(px(13.)).text_color(p.muted).child(format!("Rowveil {}", rowveil::update::current_version())));
        let mut buttons = h_flex().gap(px(8.)).pt(px(4.)).justify_end();
        let notes = |info: &Value| {
            let text: String = info["release"]["notes"].as_str().unwrap_or("").chars().take(3000).collect();
            div()
                .id("notes")
                .max_h(px(280.))
                .overflow_y_scroll()
                .px(px(12.))
                .py(px(10.))
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
                .bg(p.bg)
                .font_family(kit::cx_mono(cx))
                .text_size(px(12.))
                .text_color(p.muted)
                .children(text.lines().map(|l| div().child(if l.is_empty() { " ".to_string() } else { l.to_string() })).collect::<Vec<_>>())
        };
        match &self.phase {
            Phase::Checking => body = body.child(div().text_size(px(13.)).text_color(p.muted).child("Checking…")),
            Phase::Error(e) => {
                body = body.child(div().px(px(10.)).py(px(8.)).rounded(px(6.)).bg(p.danger_soft).text_color(p.danger).text_size(px(12.5)).child(e.clone()));
                buttons = buttons.child(kit::btn("check", cx).icon(kit::ico("refresh")).label("Try again").on_click(cx.listener(|this, _, _, cx| this.check(cx))));
            }
            Phase::Checked(info) if info["available"].as_bool() != Some(true) => {
                body = body.child(div().text_size(px(13.)).child("You have the latest version."));
                buttons = buttons.child(kit::btn("check", cx).icon(kit::ico("refresh")).label("Check now").on_click(cx.listener(|this, _, _, cx| this.check(cx))));
            }
            Phase::Checked(info) => {
                let url = info["release"]["url"].as_str().unwrap_or("").to_string();
                body = body.child(div().font_semibold().text_size(px(14.)).child(format!("Rowveil {} is available", info["latest"].as_str().unwrap_or("")))).child(notes(info));
                buttons = buttons.child(kit::btn_ghost("page", cx).icon(kit::ico("external")).label("Release page").on_click(move |_, _, cx| cx.open_url(&url)));
                if info["canInstall"].as_bool() == Some(true) {
                    let i2 = info.clone();
                    buttons = buttons.child(kit::btn_primary("dl", cx).icon(kit::ico("download")).label("Download and install").on_click(cx.listener(move |this, _, _, cx| this.download(i2.clone(), cx))));
                } else {
                    body = body.child(div().text_size(px(12.)).text_color(p.muted).child("This install cannot update itself: download the new version from the release page."));
                }
            }
            Phase::Progress(info, pr) => {
                body = body.child(div().font_semibold().text_size(px(14.)).child(format!("Rowveil {}", info["latest"].as_str().unwrap_or(""))));
                match pr["phase"].as_str().unwrap_or("") {
                    "downloading" => {
                        let (got, total) = (pr["received"].as_u64().unwrap_or(0), pr["total"].as_u64().unwrap_or(0));
                        let pct = if total > 0 { (got * 100 / total).min(100) } else { 0 };
                        body = body
                            .child(div().h(px(6.)).w_full().rounded(px(3.)).bg(p.panel_2).child(div().h_full().rounded(px(3.)).bg(p.accent).w(relative(pct as f32 / 100.))))
                            .child(div().text_size(px(12.)).text_color(p.muted).child(format!("Downloading… {}{}", mb(got), if total > 0 { format!(" of {}", mb(total)) } else { String::new() })));
                        buttons = buttons.child(kit::btn_primary("busy", cx).label("Downloading…").loading(true));
                    }
                    "ready" => {
                        body = body.child(div().text_size(px(13.)).child("The update is ready. Rowveil restarts to finish."));
                        let i2 = info.clone();
                        buttons = buttons.child(kit::btn_primary("restart", cx).icon(kit::ico("refresh")).label("Restart and update").on_click(cx.listener(move |this, _, _, cx| this.restart(i2.clone(), cx))));
                    }
                    "restarting" => {
                        body = body.child(div().text_size(px(13.)).text_color(p.muted).child("Restarting…"));
                    }
                    _ => {
                        let e = pr["error"].as_str().unwrap_or("The download failed").to_string();
                        body = body.child(div().px(px(10.)).py(px(8.)).rounded(px(6.)).bg(p.danger_soft).text_color(p.danger).text_size(px(12.5)).child(e));
                        let i2 = info.clone();
                        buttons = buttons.child(kit::btn_primary("dl", cx).icon(kit::ico("download")).label("Try again").on_click(cx.listener(move |this, _, _, cx| this.download(i2.clone(), cx))));
                    }
                }
            }
        }
        body.child(buttons.child(kit::btn_ghost("close", cx).label("Close").on_click(|_, window, cx| window.close_dialog(cx))))
    }
}
