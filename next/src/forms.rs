//! A generic form dialog: labelled fields (single line, multi-line, choice, checkbox),
//! an optional live SQL preview, OK / Cancel.
//!
//! `on_submit` gets the values in field order (checkboxes: "true" / "false") and a handle to
//! the dialog. It returns Ok(true) to close now, Ok(false) to keep the dialog open (an async
//! step closes it or reports an error with [`FormDialog::fail`]), or Err(message).

use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, Disableable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;

pub enum FieldKind {
    Line,
    Multi,
    Sql,
    Choice(Vec<(String, String)>),
    Check,
}

pub struct Field {
    pub label: String,
    pub value: String,
    pub kind: FieldKind,
    pub hint: Option<String>,
    pub masked: bool,
    pub disabled: bool,
}

impl Field {
    fn new(label: &str, value: impl Into<String>, kind: FieldKind) -> Self {
        Self { label: label.into(), value: value.into(), kind, hint: None, masked: false, disabled: false }
    }
    pub fn line(label: &str, value: impl Into<String>) -> Self {
        Self::new(label, value, FieldKind::Line)
    }
    pub fn multi(label: &str, value: impl Into<String>) -> Self {
        Self::new(label, value, FieldKind::Multi)
    }
    pub fn sql(label: &str, value: impl Into<String>) -> Self {
        Self::new(label, value, FieldKind::Sql)
    }
    pub fn choice(label: &str, options: &[&str], value: &str) -> Self {
        Self::new(label, value, FieldKind::Choice(options.iter().map(|s| (s.to_string(), s.to_string())).collect()))
    }
    /// Options as (value, label).
    pub fn choice_labeled(label: &str, options: Vec<(String, String)>, value: &str) -> Self {
        Self::new(label, value, FieldKind::Choice(options))
    }
    pub fn check(label: &str, on: bool) -> Self {
        Self::new(label, if on { "true" } else { "false" }, FieldKind::Check)
    }
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }
    pub fn masked(mut self) -> Self {
        self.masked = true;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

pub type OnSubmit = Rc<dyn Fn(Vec<String>, WeakEntity<FormDialog>, &mut Window, &mut App) -> Result<bool, String>>;
/// SQL statements for the preview: (sql to run, text to show).
pub type Preview = Rc<dyn Fn(&[String]) -> Result<Vec<(String, String)>, String>>;

enum Ctl {
    Line(Entity<InputState>),
    Multi(Entity<EditorState>),
    Choice(Vec<(String, String)>, String),
    Check(bool),
}

pub struct Options {
    pub ok: String,
    pub danger: bool,
    pub note: Option<String>,
    pub preview: Option<Preview>,
    pub width: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self { ok: "Save".into(), danger: false, note: None, preview: None, width: 560. }
    }
}

pub struct FormDialog {
    fields: Vec<(String, Option<String>, bool, Ctl)>,
    opts: Options,
    on_submit: OnSubmit,
    error: Option<String>,
    busy: bool,
    _subs: Vec<Subscription>,
}

impl FormDialog {
    /// A simple form whose `on_submit` returns Ok(()) to close.
    pub fn open(title: impl Into<SharedString>, fields: Vec<Field>, ok: &str, on_submit: Rc<dyn Fn(Vec<String>, &mut Window, &mut App) -> Result<(), String>>, window: &mut Window, cx: &mut App) {
        let opts = Options { ok: ok.into(), ..Default::default() };
        Self::open_with(title, fields, opts, Rc::new(move |v, _, window, cx| on_submit(v, window, cx).map(|_| true)), window, cx);
    }

    pub fn open_with(title: impl Into<SharedString>, fields: Vec<Field>, opts: Options, on_submit: OnSubmit, window: &mut Window, cx: &mut App) {
        let title = title.into();
        let width = opts.width;
        let form = cx.new(|cx| {
            let mut subs = Vec::new();
            let mut first = true;
            let fields = fields
                .into_iter()
                .map(|f| {
                    let ctl = match f.kind {
                        FieldKind::Line => {
                            let e = cx.new(|cx| InputState::new(window, cx).default_value(f.value.clone()).masked(f.masked));
                            subs.push(cx.subscribe_in(&e, window, |this: &mut FormDialog, _, ev: &InputEvent, window, cx| match ev {
                                InputEvent::PressEnter { .. } => this.submit(window, cx),
                                InputEvent::Change => cx.notify(),
                                _ => {}
                            }));
                            if first && !f.disabled {
                                e.update(cx, |s, cx| s.focus(window, cx));
                                first = false;
                            }
                            Ctl::Line(e)
                        }
                        FieldKind::Multi | FieldKind::Sql => {
                            let sql = matches!(f.kind, FieldKind::Sql);
                            let e = cx.new(|cx| {
                                let s = EditorState::new(window, cx).default_value(f.value.clone());
                                if sql { s.language("sql").line_number(true) } else { s }
                            });
                            subs.push(cx.subscribe(&e, |_: &mut FormDialog, _, ev: &InputEvent, cx| {
                                if matches!(ev, InputEvent::Change) {
                                    cx.notify();
                                }
                            }));
                            if first && !f.disabled {
                                e.update(cx, |s, cx| s.focus(window, cx));
                                first = false;
                            }
                            Ctl::Multi(e)
                        }
                        FieldKind::Choice(opts) => Ctl::Choice(opts, f.value.clone()),
                        FieldKind::Check => Ctl::Check(f.value == "true"),
                    };
                    (f.label, f.hint, f.disabled, ctl)
                })
                .collect();
            FormDialog { fields, opts, on_submit, error: None, busy: false, _subs: subs }
        });
        window.open_dialog(cx, move |d, _, cx| crate::kit::dialog(d, title.clone(), width, cx).child(form.clone()));
    }

    fn values(&self, cx: &App) -> Vec<String> {
        self.fields
            .iter()
            .map(|(_, _, _, c)| match c {
                Ctl::Line(e) => e.read(cx).value().to_string(),
                Ctl::Multi(e) => e.read(cx).value().to_string(),
                Ctl::Choice(_, v) => v.clone(),
                Ctl::Check(b) => b.to_string(),
            })
            .collect()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let values = self.values(cx);
        let me = cx.weak_entity();
        match (self.on_submit)(values, me, window, cx) {
            Ok(true) => window.close_dialog(cx),
            Ok(false) => {
                self.busy = true;
                self.error = None;
                cx.notify();
            }
            Err(e) => {
                self.error = Some(e);
                cx.notify();
            }
        }
    }

    /// An async step failed: show the message and allow another try.
    pub fn fail(&mut self, e: String, cx: &mut Context<Self>) {
        self.busy = false;
        self.error = Some(e);
        cx.notify();
    }
}

impl Render for FormDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = crate::ui::pal(cx);
        let mut body = v_flex().gap(px(12.));
        // Consecutive check boxes share one row (`.check-row`).
        let mut checks: Vec<AnyElement> = Vec::new();
        let flush = |body: Div, checks: &mut Vec<AnyElement>| if checks.is_empty() { body } else { body.child(h_flex().flex_wrap().gap_x(px(16.)).gap_y(px(6.)).children(std::mem::take(checks))) };
        for (i, (label, hint, disabled, ctl)) in self.fields.iter().enumerate() {
            let disabled = *disabled;
            if let Ctl::Check(on) = ctl {
                checks.push(
                    Checkbox::new(SharedString::from(format!("k{i}")))
                        .checked(*on)
                        .label(label.clone())
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, c: &bool, _, cx| {
                            if let Some((_, _, _, Ctl::Check(v))) = this.fields.get_mut(i) {
                                *v = *c;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                continue;
            }
            body = flush(body, &mut checks);
            let control: AnyElement = match ctl {
                Ctl::Line(e) => Input::new(e).bg(crate::ui::input_bg()).disabled(disabled).into_any_element(),
                Ctl::Multi(e) => div().h(px(150.)).border_1().border_color(p.border).rounded(px(6.)).overflow_hidden().bg(p.bg).child(Editor::new(e).h_full().bordered(false).disabled(disabled).font_family(crate::kit::cx_mono(cx)).text_size(px(12.5))).into_any_element(),
                Ctl::Choice(opts, cur) => {
                    let label_of = opts.iter().find(|(v, _)| v == cur).map(|(_, l)| l.clone()).unwrap_or_else(|| cur.clone());
                    if opts.len() <= 3 {
                        let vals: Vec<String> = opts.iter().map(|(v, _)| v.clone()).collect();
                        let me = cx.weak_entity();
                        crate::kit::seg(
                            SharedString::from(format!("c{i}")),
                            opts.iter().map(|(v, l)| (SharedString::from(l.clone()), v == cur)).collect(),
                            move |j, _, cx| {
                                if disabled {
                                    return;
                                }
                                let v2 = vals[j].clone();
                                let _ = me.update(cx, |this, cx| {
                                    if let Some((_, _, _, Ctl::Choice(_, v))) = this.fields.get_mut(i) {
                                        *v = v2;
                                    }
                                    cx.notify();
                                });
                            },
                            cx,
                        )
                        .into_any_element()
                    } else {
                        let me = cx.weak_entity();
                        let items: Vec<(String, String)> = opts.clone();
                        let cur = cur.clone();
                        crate::kit::select(SharedString::from(format!("c{i}")), label_of, cx)
                            .w_full()
                            .h(px(32.))
                            .disabled(disabled)
                            .dropdown_menu(move |mut menu, _, _| {
                                for (val, lab) in &items {
                                    let (me, v2) = (me.clone(), val.clone());
                                    menu = menu.item(PopupMenuItem::new(lab.clone()).checked(*val == cur).on_click(move |_, _, cx| {
                                        let _ = me.update(cx, |this, cx| {
                                            if let Some((_, _, _, Ctl::Choice(_, v))) = this.fields.get_mut(i) {
                                                *v = v2.clone();
                                            }
                                            cx.notify();
                                        });
                                    }));
                                }
                                menu
                            })
                            .into_any_element()
                    }
                }
                Ctl::Check(_) => unreachable!(),
            };
            body = body.child(
                v_flex()
                    .gap(px(5.))
                    .child(div().text_size(px(12.)).text_color(p.text).child(label.clone()))
                    .child(control)
                    .when_some(hint.clone(), |d, h| d.child(div().text_size(px(11.5)).text_color(p.muted).child(h))),
            );
        }
        body = flush(body, &mut checks);
        if let Some(pv) = &self.opts.preview {
            let values = self.values(cx);
            let mysql = false;
            let inner: AnyElement = match pv(&values) {
                Ok(st) if !st.is_empty() => v_flex().gap(px(6.)).children(st.into_iter().map(|(_, show)| crate::kit::sql_block(&show, mysql, cx).text_size(px(12.5)))).into_any_element(),
                Ok(_) => div().text_size(px(12.5)).text_color(p.muted).child("Fill in the fields to see the SQL").into_any_element(),
                Err(e) => div().text_size(px(12.5)).text_color(p.muted).child(e).into_any_element(),
            };
            body = body.child(
                v_flex()
                    .gap(px(5.))
                    .child(div().text_size(px(12.)).child("SQL"))
                    .child(div().id("ddl-preview").max_h(px(260.)).overflow_y_scroll().px(px(12.)).py(px(10.)).rounded(px(6.)).border_1().border_color(p.border).bg(p.bg).child(inner)),
            );
        }
        let danger = self.opts.danger;
        body.when_some(self.opts.note.clone(), |d, n| {
            d.child(div().text_size(px(12.5)).px(px(10.)).py(px(8.)).rounded(px(6.)).map(|d| if danger { d.bg(p.danger_soft).text_color(p.danger) } else { d.bg(p.panel_2).border_1().border_color(p.border).text_color(p.text) }).child(n))
        })
        .when_some(self.error.clone(), |d, e| d.child(div().text_size(px(12.5)).px(px(10.)).py(px(8.)).rounded(px(6.)).bg(p.danger_soft).text_color(p.danger).font_family(crate::kit::cx_mono(cx)).child(e)))
        .child(
            h_flex()
                .mt(px(4.))
                .mx(px(-16.))
                .px(px(16.))
                .pt(px(12.))
                .gap(px(8.))
                .justify_end()
                .border_t_1()
                .border_color(p.border)
                .child(crate::kit::btn("cancel", cx).label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                .child({
                    let b = if danger {
                        crate::kit::btn_primary("ok", cx).bg(p.danger).border_color(p.danger).text_color(gpui_kit::white())
                    } else {
                        crate::kit::btn_primary("ok", cx)
                    };
                    b.label(if self.busy { "Applying…".to_string() } else { self.opts.ok.clone() }).loading(self.busy).on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
                }),
        )
    }
}

/// A confirm dialog; `on_ok` runs when the user confirms.
pub fn confirm(title: &str, text: impl Into<String>, ok: &str, on_ok: Rc<dyn Fn(&mut Window, &mut App)>, window: &mut Window, cx: &mut App) {
    let (title, text, ok) = (title.to_string(), text.into(), ok.to_string());
    window.open_alert_dialog(cx, move |a, _, _| {
        let on_ok = on_ok.clone();
        a.title(title.clone()).description(text.clone()).show_cancel(true).ok_text(ok.clone()).on_ok(move |_, window, cx| {
            on_ok(window, cx);
            true
        })
    });
}
