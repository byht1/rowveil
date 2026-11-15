//! The value editor / viewer dialog for one cell.

use crate::grid::{editor_text, looks_like_json};
use crate::model::Column;
use crate::ui;
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::Value;
use std::rc::Rc;

pub type OnApply = Rc<dyn Fn(Value, &mut Window, &mut App)>;

enum Control {
    Choice(Vec<String>, Option<String>),
    Line(Entity<InputState>),
    Text(Entity<EditorState>, bool),
}

pub struct CellEditor {
    column: Column,
    control: Control,
    read_only: bool,
    allow_default: bool,
    on_apply: Option<OnApply>,
    raw: bool,
    error: Option<String>,
    _subs: Vec<Subscription>,
}

/// Re-indents JSON text without changing numbers or key order (serde_json keeps the order
/// with preserve_order and keeps big integers as they are via arbitrary precision text).
pub fn pretty_json(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text.trim()).ok()?;
    serde_json::to_string_pretty(&v).ok()
}

impl CellEditor {
    pub fn new(column: Column, value: Value, read_only: bool, allow_default: bool, on_apply: Option<OnApply>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subs = Vec::new();
        let text = editor_text(&value);
        let is_json = column.kind == "json" || matches!(value, Value::Object(_) | Value::Array(_)) || (matches!(value, Value::String(_)) && looks_like_json(&text));
        let control = if column.kind == "boolean" && !read_only {
            let cur = match &value {
                Value::Bool(b) => Some(b.to_string()),
                Value::String(s) => Some(s.to_lowercase()),
                _ => None,
            };
            Control::Choice(vec!["true".into(), "false".into()], cur)
        } else if let (Some(vals), false) = (&column.enum_values, read_only) {
            Control::Choice(vals.clone(), value.as_str().map(str::to_string))
        } else if is_json || column.kind == "text" || text.contains('\n') || text.len() > 80 || read_only {
            let text = if is_json { pretty_json(&text).unwrap_or(text) } else { text };
            let e = cx.new(|cx| {
                let s = EditorState::new(window, cx).line_number(is_json).default_value(text);
                if is_json { s.language("json") } else { s }
            });
            e.update(cx, |s, cx| s.focus(window, cx));
            Control::Text(e, is_json)
        } else {
            let e = cx.new(|cx| InputState::new(window, cx).default_value(text));
            subs.push(cx.subscribe_in(&e, window, |this: &mut Self, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.apply(window, cx);
                }
            }));
            e.update(cx, |s, cx| s.focus(window, cx));
            Control::Line(e)
        };
        Self { column, control, read_only, allow_default, on_apply, raw: false, error: None, _subs: subs }
    }

    /// Opens the dialog: an editor when `on_apply` is set, otherwise a viewer.
    pub fn open(column: Column, value: Value, allow_default: bool, on_apply: Option<OnApply>, window: &mut Window, cx: &mut App) {
        let read_only = on_apply.is_none();
        let (name, ty) = (column.name.clone(), column.data_type.clone());
        let ed = cx.new(|cx| CellEditor::new(column, value, read_only, allow_default, on_apply, window, cx));
        let width = match &ed.read(cx).control {
            Control::Choice(..) => 320.,
            Control::Line(_) => 460.,
            Control::Text(..) => 680.,
        };
        window.open_dialog(cx, move |d, _, cx| {
            let p = ui::pal(cx);
            let title = h_flex().gap(px(6.)).child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).child(name.clone())).child(div().text_size(px(11.)).font_family(crate::kit::cx_mono(cx)).text_color(p.dim).child(ty.clone()));
            crate::kit::dialog_el(d, title, width, cx).child(ed.clone())
        });
    }

    fn current(&self, cx: &App) -> Result<Value, String> {
        Ok(match &self.control {
            Control::Choice(_, cur) => match cur {
                Some(v) if self.column.kind == "boolean" => Value::Bool(v == "true"),
                Some(v) => Value::String(v.clone()),
                None => Value::Null,
            },
            Control::Line(e) => Value::String(e.read(cx).value().to_string()),
            Control::Text(e, is_json) => {
                let t = e.read(cx).value().to_string();
                if *is_json && self.column.kind == "json" {
                    serde_json::from_str::<Value>(&t).map_err(|e| format!("Invalid JSON: {e}"))?;
                }
                Value::String(t)
            }
        })
    }

    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        match self.current(cx) {
            Ok(v) => self.finish(v, window, cx),
            Err(e) => {
                self.error = Some(e);
                cx.notify();
            }
        }
    }

    fn finish(&mut self, v: Value, window: &mut Window, cx: &mut Context<Self>) {
        window.close_dialog(cx);
        if let Some(f) = self.on_apply.clone() {
            f(v, window, cx);
        }
    }
}

impl Render for CellEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let mono = crate::kit::cx_mono(cx);
        let body: AnyElement = match &self.control {
            Control::Choice(opts, cur) => v_flex()
                .gap(px(2.))
                .children(opts.iter().enumerate().map(|(i, o)| {
                    let o2 = o.clone();
                    let on = cur.as_deref() == Some(o.as_str());
                    let ro = self.read_only;
                    div()
                        .id(("opt", i))
                        .px(px(8.))
                        .py(px(7.))
                        .rounded(px(5.))
                        .font_family(mono.clone())
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.text)
                        .when(on, |d| d.bg(p.accent_soft))
                        .when(!ro, |d| d.cursor_pointer().hover(|s| s.bg(p.hover)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.read_only {
                                return;
                            }
                            if let Control::Choice(_, c) = &mut this.control {
                                *c = Some(o2.clone());
                            }
                            this.apply(window, cx);
                        }))
                        .child(o.clone())
                }))
                .into_any_element(),
            Control::Line(e) => Input::new(e).bg(crate::ui::input_bg()).font_family(mono.clone()).into_any_element(),
            Control::Text(e, is_json) => {
                let is_json = *is_json;
                let valid = is_json.then(|| serde_json::from_str::<Value>(&e.read(cx).value()).is_ok());
                let e2 = e.clone();
                let me = cx.weak_entity();
                v_flex()
                    .gap(px(8.))
                    .when(is_json, |d| {
                        d.child(
                            h_flex()
                                .gap(px(8.))
                                .child(crate::kit::seg("fmt", vec![("Formatted".into(), !self.raw), ("Raw".into(), self.raw)], move |i, window, cx| {
                                    let want_raw = i == 1;
                                    let _ = me.update(cx, |this, cx| {
                                        if this.raw == want_raw {
                                            return;
                                        }
                                        let t = e2.read(cx).value().to_string();
                                        let next = if this.raw { pretty_json(&t) } else { serde_json::from_str::<Value>(&t).ok().map(|v| v.to_string()) };
                                        if let Some(n) = next {
                                            e2.update(cx, |s, cx| s.set_value(n, window, cx));
                                            this.raw = want_raw;
                                        }
                                        cx.notify();
                                    });
                                }, cx))
                                .child(div().flex_1())
                                .child(div().text_size(px(11.5)).text_color(if valid == Some(true) { p.true_ } else { p.danger }).child(if valid == Some(true) { "Valid JSON" } else { "Invalid JSON" })),
                        )
                    })
                    .child(div().h(px(320.)).border_1().border_color(p.border).rounded(px(6.)).overflow_hidden().bg(p.bg).child(Editor::new(e).h_full().bordered(false).readonly(self.read_only).font_family(mono.clone()).text_size(px(12.5))))
                    .into_any_element()
            }
        };
        let choice = matches!(self.control, Control::Choice(..));
        let mut footer = h_flex().gap(px(8.)).pt(px(4.));
        if self.read_only {
            let text = match &self.control {
                Control::Text(e, _) => e.read(cx).value().to_string(),
                Control::Line(e) => e.read(cx).value().to_string(),
                Control::Choice(_, c) => c.clone().unwrap_or_default(),
            };
            footer = footer
                .child(div().flex_1())
                .child(crate::kit::btn_ghost_sm("copy", cx).icon(crate::kit::ico("copy")).label("Copy").on_click(move |_, window, cx| {
                    ui::copy(text.clone(), cx);
                    ui::toast_ok(window, cx, "Copied");
                }))
                .child(crate::kit::btn_ghost_sm("close", cx).label("Close").on_click(|_, window, cx| window.close_dialog(cx)));
        } else {
            footer = footer
                .when(self.column.nullable, |f| f.child(crate::kit::btn_ghost_sm("null", cx).label("Set NULL").on_click(cx.listener(|this, _, window, cx| this.finish(Value::Null, window, cx)))))
                .when(self.allow_default, |f| {
                    f.child(crate::kit::btn_ghost_sm("default", cx).label("Use default").on_click(cx.listener(|this, _, window, cx| this.finish(serde_json::json!({ "$default": true }), window, cx))))
                })
                .child(div().flex_1())
                .child(crate::kit::btn_ghost_sm("cancel", cx).label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                .when(!choice, |f| f.child(crate::kit::btn_primary("apply", cx).h(px(26.)).px(px(10.)).label("Apply").on_click(cx.listener(|this, _, window, cx| this.apply(window, cx)))));
        }
        let hint = if self.read_only { "Esc to close" } else if choice { "Click a value to apply · Esc to cancel" } else { "Ctrl+Enter to apply · Esc to cancel" };
        v_flex()
            .gap(px(8.))
            .key_context("CellEditor")
            .on_action(cx.listener(|this, _: &crate::workspace::Submit, window, cx| this.apply(window, cx)))
            .child(body)
            .when_some(self.error.clone(), |d, e| d.child(div().text_size(px(12.)).text_color(p.danger).child(e)))
            .child(footer)
            .child(div().text_size(px(11.)).text_color(p.dim).child(hint))
    }
}
