//! The Updates dialog and the quiet check on start.

use crate::updater::{self, Release};
use crate::{ui, RowveilApp};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, StyledExt as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

enum Phase {
    Checking,
    UpToDate,
    Available(Release),
    Downloading(Release, Arc<Mutex<(u64, u64)>>),
    Ready(PathBuf),
    Error(String),
}

pub struct UpdateDialog {
    app: WeakEntity<RowveilApp>,
    phase: Phase,
    _poll: Option<Task<()>>,
}

/// Checks quietly a few seconds after start; the sidebar then shows an "Update" button.
pub fn check_on_start(cx: &mut Context<RowveilApp>) {
    if cfg!(debug_assertions) {
        return;
    }
    cx.spawn(async move |app, cx| {
        cx.background_executor().timer(std::time::Duration::from_secs(3)).await;
        if let Ok(Some(r)) = crate::backend::run(updater::latest()).await {
            if updater::is_newer(&r) {
                let _ = app.update(cx, |a, cx| {
                    a.update_available = Some(r.version.clone());
                    cx.notify();
                });
            }
        }
    })
    .detach();
}

impl UpdateDialog {
    pub fn open(app: WeakEntity<RowveilApp>, window: &mut Window, cx: &mut App) {
        let dlg = cx.new(|cx| {
            let mut d = UpdateDialog { app, phase: Phase::Checking, _poll: None };
            d.check(cx);
            d
        });
        window.open_dialog(cx, move |d, _, cx| crate::kit::dialog(d, "Updates", 560., cx).child(dlg.clone()));
    }

    fn check(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Checking;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = crate::backend::run(updater::latest()).await;
            let _ = this.update(cx, |this, cx| {
                this.phase = match res {
                    Ok(Some(r)) if updater::is_newer(&r) => {
                        let v = r.version.clone();
                        let _ = this.app.update(cx, |a, _| a.update_available = Some(v));
                        Phase::Available(r)
                    }
                    Ok(_) => Phase::UpToDate,
                    Err(e) => Phase::Error(e.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn download(&mut self, r: Release, cx: &mut Context<Self>) {
        let prog = Arc::new(Mutex::new((0u64, r.asset.as_ref().map(|a| a.1).unwrap_or(0))));
        self.phase = Phase::Downloading(r.clone(), prog.clone());
        let p2 = prog.clone();
        let fut = updater::download(r, move |got, total| *p2.lock().unwrap() = (got, total));
        // Redraw the progress while the download runs.
        self._poll = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_millis(300)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }));
        cx.spawn(async move |this, cx| {
            let res = crate::backend::run(fut).await;
            let _ = this.update(cx, |this, cx| {
                this._poll = None;
                this.phase = match res {
                    Ok(p) => Phase::Ready(p),
                    Err(e) => Phase::Error(e.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for UpdateDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let cur = updater::current();
        let mut body = v_flex().gap_3().child(ui::muted(format!("Rowveil Next {cur} (alpha channel: next-v* pre-releases)"), cx));
        let mut buttons = h_flex().gap_2().pt_2().child(div().flex_1());
        match &self.phase {
            Phase::Checking => body = body.child(ui::muted("Checking…", cx)),
            Phase::UpToDate => {
                body = body.child(div().child("You have the latest alpha."));
                buttons = buttons.child(crate::kit::btn("check", cx).icon(crate::kit::ico("refresh")).label("Check now").on_click(cx.listener(|this, _, _, cx| this.check(cx))));
            }
            Phase::Error(e) => {
                body = body.child(div().text_sm().text_color(theme.danger).child(e.clone()));
                buttons = buttons.child(crate::kit::btn("check", cx).icon(crate::kit::ico("refresh")).label("Try again").on_click(cx.listener(|this, _, _, cx| this.check(cx))));
            }
            Phase::Available(r) => {
                let notes: String = r.notes.chars().take(1500).collect();
                body = body
                    .child(div().font_semibold().child(format!("Rowveil Next {} is available", r.version)))
                    .child(div().id("notes").max_h(px(280.)).overflow_y_scroll().p_2().rounded(px(4.)).bg(theme.secondary).text_xs().children(notes.lines().map(|l| div().child(l.to_string()))));
                let (r2, url) = (r.clone(), r.url.clone());
                buttons = buttons
                    .child(crate::kit::btn_ghost("page", cx).icon(crate::kit::ico("external")).label("Release page").on_click(move |_, _, cx| cx.open_url(&url)))
                    .when(r.asset.is_some(), |b| b.child(crate::kit::btn_primary("dl", cx).icon(crate::kit::ico("download")).label("Download and install").on_click(cx.listener(move |this, _, _, cx| this.download(r2.clone(), cx)))));
            }
            Phase::Downloading(r, p) => {
                let (got, total) = *p.lock().unwrap();
                let pct = if total > 0 { got * 100 / total } else { 0 };
                body = body
                    .child(div().child(format!("Downloading {}… {pct}%", r.version)))
                    .child(div().h(px(6.)).w_full().rounded(px(3.)).bg(theme.secondary).child(div().h_full().rounded(px(3.)).bg(theme.primary).w(relative(pct as f32 / 100.))));
            }
            Phase::Ready(p) => {
                body = body.child(div().child("The update is ready. Rowveil Next restarts to finish."));
                let p = p.clone();
                buttons = buttons.child(crate::kit::btn_primary("restart", cx).icon(crate::kit::ico("refresh")).label("Restart and update").on_click(move |_, window, cx| match updater::install_and_restart(p.clone()) {
                    Ok(()) => cx.quit(),
                    Err(e) => ui::toast_err(window, cx, e.to_string()),
                }));
            }
        }
        body.child(buttons.child(crate::kit::btn_ghost("close", cx).label("Close").on_click(|_, window, cx| window.close_dialog(cx))))
    }
}
