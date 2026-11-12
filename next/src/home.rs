//! The start page: connection cards, search, tag filter, add / edit / delete.

use crate::backend::{self, Backend};
use crate::conn_form::{self, ConnForm};
use crate::model::{self, Conn};
use crate::{ui, RowveilApp};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, StyledExt as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

pub struct Home {
    backend: Arc<Backend>,
    conns: Option<Vec<Conn>>,
    error: Option<String>,
    search: Entity<InputState>,
    tag_filter: BTreeSet<String>,
    app: WeakEntity<RowveilApp>,
    _sub: Subscription,
}

pub fn dialect_color(d: &str) -> Hsla {
    match d {
        "postgres" => rgb(0x336791).into(),
        "mysql" => rgb(0xe48e00).into(),
        "sqlite" => rgb(0x0f80cc).into(),
        "redis" => rgb(0xdc382d).into(),
        "bigquery" => rgb(0x4285f4).into(),
        _ => rgb(0x888888).into(),
    }
}

/// Tags of all connections: presets first, then A–Z.
pub fn all_tags(conns: &[Conn]) -> Vec<String> {
    let set: BTreeSet<String> = conns.iter().flat_map(|c| c.tags.iter().cloned()).collect();
    let mut out: Vec<String> = conn_form::TAG_PRESETS.iter().filter(|p| set.contains(**p)).map(|s| s.to_string()).collect();
    out.extend(set.into_iter().filter(|t| !conn_form::TAG_PRESETS.contains(&t.as_str())));
    out
}

impl Home {
    pub fn new(backend: Arc<Backend>, app: WeakEntity<RowveilApp>, tag_filter: Vec<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search connections"));
        let sub = cx.subscribe_in(&search, window, |this: &mut Self, _, e: &InputEvent, window, cx| match e {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } => {
                if let Some(c) = this.visible(cx).first() {
                    let id = c.id.clone();
                    this.open(id, window, cx);
                }
            }
            _ => {}
        });
        search.update(cx, |s, cx| s.focus(window, cx));
        let tag_filter = tag_filter.into_iter().collect();
        let mut this = Self { backend, conns: None, error: None, search, tag_filter, app, _sub: sub };
        this.reload(cx);
        this
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let fut = self.backend.clone().get("/connections");
        cx.spawn(async move |this, cx| {
            let res = fut.await.and_then(model::parse::<Vec<Conn>>);
            let _ = this.update(cx, |this, cx| {
                if this.conns.is_none() && crate::dev_env("ROWVEIL_NEXT_DIALOG").as_deref() == Some("new") {
                    let me = cx.weak_entity();
                    cx.defer(move |cx| {
                        if let Some(w) = cx.windows().first().copied() {
                            let _ = w.update(cx, |_, window, cx| {
                                let _ = me.update(cx, |this, cx| this.add(window, cx));
                            });
                        }
                    });
                }
                match res {
                    Ok(c) => this.conns = Some(c),
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn visible(&self, cx: &App) -> Vec<Conn> {
        let q = self.search.read(cx).value().to_lowercase();
        self.conns
            .iter()
            .flatten()
            .filter(|c| self.tag_filter.is_empty() || c.tags.iter().any(|t| self.tag_filter.contains(t)))
            .filter(|c| {
                q.is_empty()
                    || c.name.to_lowercase().contains(&q)
                    || c.url.to_lowercase().contains(&q)
                    || model::dialect_label(c.dialect()).to_lowercase().contains(&q)
                    || c.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .cloned()
            .collect()
    }

    fn open(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.app.update(cx, |app, cx| app.open_connection(id, window, cx));
    }

    fn toggle_tag(&mut self, tag: Option<String>, multi: bool, cx: &mut Context<Self>) {
        match tag {
            None => self.tag_filter.clear(),
            Some(t) if multi => {
                if !self.tag_filter.remove(&t) {
                    self.tag_filter.insert(t);
                }
            }
            Some(t) => {
                if self.tag_filter.len() == 1 && self.tag_filter.contains(&t) {
                    self.tag_filter.clear();
                } else {
                    self.tag_filter = [t].into();
                }
            }
        }
        let filter: Vec<String> = self.tag_filter.iter().cloned().collect();
        let _ = self.app.update(cx, |app, _| {
            app.prefs.tag_filter = filter;
            app.prefs.save();
        });
        cx.notify();
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        ConnForm::open(
            self.backend.clone(),
            None,
            false,
            Rc::new(move |conn, window, cx| {
                let _ = this.update(cx, |this, cx| this.open(conn.id, window, cx));
            }),
            window,
            cx,
        );
    }

    fn edit(&mut self, conn: Conn, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        ConnForm::open(
            self.backend.clone(),
            Some(conn),
            false,
            Rc::new(move |_, _, cx| {
                let _ = this.update(cx, |this, cx| this.reload(cx));
            }),
            window,
            cx,
        );
    }

    fn delete(&mut self, conn: Conn, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let backend = self.backend.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (this, backend, id) = (this.clone(), backend.clone(), conn.id.clone());
            alert
                .title("Delete connection")
                .description(format!("Delete \"{}\"? The database is not changed.", conn.name))
                .show_cancel(true)
                .ok_text("Delete")
                .on_ok(move |_, window, cx| {
                    let fut = backend.clone().call(axum::http::Method::DELETE, format!("/connections/{}", backend::enc(&id)), None);
                    let this = this.clone();
                    window
                        .spawn(cx, async move |cx| {
                            let res = fut.await;
                            let _ = cx.update(|window, cx| match res {
                                Ok(_) => {
                                    let _ = this.update(cx, |this, cx| this.reload(cx));
                                }
                                Err(e) => ui::toast_err(window, cx, e.to_string()),
                            });
                        })
                        .detach();
                    true
                })
        });
    }

    fn card(&self, i: usize, c: Conn, last: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let id = c.id.clone();
        let (edit_c, del_c) = (c.clone(), c.clone());
        let prod = c.tags.iter().any(|t| t.contains("PROD"));
        v_flex()
            .id(("conn", i))
            .w(px(268.))
            .p_3()
            .gap_1()
            .rounded(theme.radius)
            .border_1()
            .border_color(if prod { theme.danger.opacity(0.6) } else { theme.border })
            .bg(theme.background)
            .cursor_pointer()
            .hover(|s| s.border_color(theme.primary))
            .on_click(cx.listener(move |this, _, window, cx| this.open(id.clone(), window, cx)))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().size(px(8.)).rounded_full().bg(dialect_color(c.dialect())))
                    .child(div().flex_1().truncate().font_semibold().child(c.name.clone()))
                    .child(
                        crate::kit::icon_btn(("edit", i), "edit", cx).size(px(24.)).tooltip("Edit connection").on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.edit(edit_c.clone(), window, cx);
                        })),
                    )
                    .when(!c.is_env(), |d| {
                        d.child(crate::kit::act(("del", i), "trash", "Delete connection", true, cx).on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.delete(del_c.clone(), window, cx);
                        })))
                    }),
            )
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(model::dialect_label(c.dialect()))
                    .when(c.read_only, |d| d.child("· read-only"))
                    .when(c.is_env(), |d| d.child("· env"))
                    .when(last, |d| d.child("· last used")),
            )
            .child(div().text_xs().font_family("monospace").text_color(theme.muted_foreground).truncate().child(rowveil::config::mask_url(&c.url)))
            .when(!c.tags.is_empty(), |d| d.child(h_flex().gap_1().children(c.tags.iter().take(4).map(|t| conn_form::tag_chip(t, cx)))))
    }
}

impl Render for Home {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (open_last, last, theme_name) = self
            .app
            .upgrade()
            .map(|a| {
                let p = &a.read(cx).prefs;
                (p.open_last, p.last_conn.clone(), p.theme.clone())
            })
            .unwrap_or_default();
        let conns = self.conns.clone().unwrap_or_default();
        let tags = all_tags(&conns);
        if crate::dev_env("ROWVEIL_NEXT_DEBUG").is_some() {
            eprintln!("home: conns={:?} query={:?} filter={:?} visible={}", self.conns.as_ref().map(|c| c.len()), self.search.read(cx).value(), self.tag_filter, self.visible(cx).len());
        }
        let cards: Vec<AnyElement> = self
            .visible(cx)
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let is_last = last.as_deref() == Some(c.id.as_str());
                self.card(i, c, is_last, cx).into_any_element()
            })
            .collect();
        let add_card = v_flex()
            .id("add")
            .w(px(268.))
            .h(px(92.))
            .items_center()
            .justify_center()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .text_color(theme.muted_foreground)
            .cursor_pointer()
            .hover(|s| s.border_color(theme.primary).text_color(theme.foreground))
            .on_click(cx.listener(|this, _, window, cx| this.add(window, cx)))
            .child(h_flex().gap(px(6.)).child(svg().path("rv/plus.svg").size(px(15.)).text_color(theme.muted_foreground)).child("Add connection"));
        let mut tag_row = h_flex().gap_1().flex_wrap().justify_center();
        if !tags.is_empty() {
            let mut items: Vec<(SharedString, String, bool, Option<String>)> = vec![("tag-all".into(), "All".into(), self.tag_filter.is_empty(), None)];
            for t in &tags {
                items.push((SharedString::from(format!("tag-{t}")), t.clone(), self.tag_filter.contains(t), Some(t.clone())));
            }
            for (id, label, on, tag) in items {
                tag_row = tag_row.child(
                    div()
                        .id(id)
                        .px_2()
                        .py(px(2.))
                        .rounded(px(10.))
                        .text_xs()
                        .cursor_pointer()
                        .border_1()
                        .border_color(if on { theme.primary } else { theme.border })
                        .when(on, |d| d.bg(theme.primary.opacity(0.12)))
                        .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                            let m = e.modifiers();
                            this.toggle_tag(tag.clone(), m.control || m.shift || m.platform, cx)
                        }))
                        .child(label),
                );
            }
        }
        v_flex()
            .id("home")
            .size_full()
            .overflow_y_scroll()
            .bg(theme.secondary)
            .text_color(theme.foreground)
            .items_center()
            .pt(px(56.))
            .pb(px(40.))
            .gap_4()
            .child(div().text_2xl().font_semibold().child("Rowveil"))
            .child(ui::muted(if conns.is_empty() && self.conns.is_some() { "Add your first connection" } else { "Choose a connection" }, cx))
            .child(div().w(px(560.)).child(Input::new(&self.search).bg(crate::ui::input_bg())))
            .child(tag_row)
            .when_some(self.error.clone(), |d, e| d.child(div().text_color(theme.danger).child(e)))
            .child(h_flex().w(px(860.)).flex_wrap().items_start().justify_center().gap_3().children(cards).child(add_card))
            .child(
                h_flex()
                    .gap_4()
                    .child(Checkbox::new("open-last").checked(open_last).label("Open the last connection on start").on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let on = *checked;
                        let _ = this.app.update(cx, |app, _| app.set_open_last(on));
                        cx.notify();
                    })))
                    .child(crate::theme_button("theme", &theme_name, self.app.clone(), cx)),
            )
    }
}
