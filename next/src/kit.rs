//! Widgets with the look of the stable UI (static/style.css): .btn, .icon-btn, .seg, .kbd, .badge,
//! .sb-item, .toolbar. Colors come from the theme palette (ui::pal).

use crate::ui::pal;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{h_flex, Icon, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// A line icon of the stable UI for buttons and menus.
pub fn ico(name: &'static str) -> Icon {
    Icon::empty().path(SharedString::from(format!("rv/{name}.svg")))
}

/// `.btn`: 28 px, panel-2 background, a border.
pub fn btn(id: impl Into<ElementId>, cx: &App) -> Button {
    let p = pal(cx);
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).color(p.panel_2).foreground(p.text).hover(p.hover).active(p.hover))
        .bg(p.panel_2)
        .h(px(28.))
        .px(px(10.))
        .border_1()
        .border_color(p.border)
        .rounded(px(6.))
}

/// `.btn.sm`: 24 px.
pub fn btn_sm(id: impl Into<ElementId>, cx: &App) -> Button {
    btn(id, cx).h(px(24.)).px(px(8.)).small()
}

/// `.btn.primary`
pub fn btn_primary(id: impl Into<ElementId>, cx: &App) -> Button {
    let p = pal(cx);
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).color(p.accent).foreground(p.accent_text).hover(p.accent.opacity(0.9)).active(p.accent.opacity(0.8)))
        .bg(p.accent)
        .h(px(28.))
        .px(px(10.))
        .border_1()
        .border_color(p.accent)
        .rounded(px(6.))
        .font_semibold()
}

/// `.btn.ghost`: no background, muted text.
pub fn btn_ghost(id: impl Into<ElementId>, cx: &App) -> Button {
    let p = pal(cx);
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).color(p.bg.opacity(0.)).foreground(p.muted).hover(p.hover).active(p.hover))
        .h(px(28.))
        .px(px(10.))
        .rounded(px(6.))
}

/// `.btn.ghost.sm`
pub fn btn_ghost_sm(id: impl Into<ElementId>, cx: &App) -> Button {
    btn_ghost(id, cx).h(px(24.)).px(px(8.)).small()
}

/// `.btn.danger-ghost`
pub fn btn_danger(id: impl Into<ElementId>, cx: &App) -> Button {
    let p = pal(cx);
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).color(p.danger_soft).foreground(p.danger).hover(p.danger_soft.opacity(1.)).active(p.danger_soft))
        .bg(p.danger_soft)
        .h(px(28.))
        .px(px(10.))
        .rounded(px(6.))
}

/// `.icon-btn`: 26 x 26, an icon only.
pub fn icon_btn(id: impl Into<ElementId>, icon: &'static str, cx: &App) -> Button {
    let p = pal(cx);
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).color(p.bg.opacity(0.)).foreground(p.muted).hover(p.hover).active(p.hover))
        .icon(ico(icon))
        .size(px(26.))
        .rounded(px(5.))
}

/// `.kbd`: a key hint.
pub fn kbd(text: impl Into<SharedString>, cx: &App) -> Div {
    let p = pal(cx);
    div()
        .flex_none()
        .px(px(4.))
        .rounded(px(4.))
        .border_1()
        .border_color(p.border)
        .text_size(px(10.))
        .line_height(px(14.))
        .text_color(p.dim)
        .font_family(cx_mono(cx))
        .child(text.into())
}

/// `.badge`: small upper-case label; `muted` for the grey variant.
pub fn badge(text: impl Into<SharedString>, muted: bool, cx: &App) -> Div {
    let p = pal(cx);
    h_flex()
        .flex_none()
        .gap(px(3.))
        .px(px(6.))
        .py(px(1.))
        .rounded(px(4.))
        .text_size(px(10.))
        .font_semibold()
        .map(|d| if muted { d.bg(p.panel_2).text_color(p.muted).border_1().border_color(p.border) } else { d.bg(p.accent_soft).text_color(p.accent) })
        .child(text.into().to_uppercase())
}

/// `.seg`: a segmented control. `items` are (id, label, on); `on_pick` gets the index.
pub fn seg(id: impl Into<ElementId>, items: Vec<(SharedString, bool)>, on_pick: impl Fn(usize, &mut Window, &mut App) + 'static, cx: &App) -> Stateful<Div> {
    let p = pal(cx);
    let on_pick = std::rc::Rc::new(on_pick);
    h_flex()
        .id(id)
        .flex_none()
        .p(px(2.))
        .gap(px(2.))
        .bg(p.panel_2)
        .border_1()
        .border_color(p.border)
        .rounded(px(6.))
        .children(items.into_iter().enumerate().map(|(i, (label, on))| {
            let f = on_pick.clone();
            div()
                .id(("seg", i))
                .h(px(22.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .text_size(px(12.))
                .cursor_pointer()
                .map(|d| if on { d.bg(p.elev).text_color(p.text).border_1().border_color(p.border_strong) } else { d.text_color(p.muted).hover(|s| s.text_color(p.text)) })
                .on_click(move |_, window, cx| f(i, window, cx))
                .child(label)
        }))
}

/// `.sb-item`: a sidebar row with an icon, a name and an optional count on the right.
pub fn sb_item(id: impl Into<ElementId>, icon: &'static str, label: impl Into<SharedString>, count: Option<AnyElement>, active: bool, cx: &App) -> Stateful<Div> {
    let p = pal(cx);
    h_flex()
        .id(id)
        .w_full()
        .gap(px(8.))
        .px(px(8.))
        .py(px(5.))
        .rounded(px(5.))
        .cursor_pointer()
        .text_size(px(13.))
        .line_height(px(18.))
        .map(|d| if active { d.bg(p.accent_soft).text_color(p.text) } else { d.text_color(p.muted).hover(|s| s.bg(p.hover).text_color(p.text)) })
        .child(svg().path(SharedString::from(format!("rv/{icon}.svg"))).size(px(15.)).flex_none().text_color(if active { p.accent } else { p.muted }))
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .children(count)
}

/// `.sb-count`
pub fn count(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div().flex_none().text_size(px(11.)).text_color(pal(cx).dim).child(text.into()).into_any_element()
}

/// `.sb-section`: an upper-case section title with optional buttons on the right.
pub fn section(title: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .pt(px(10.))
        .pb(px(4.))
        .pl(px(14.))
        .pr(px(10.))
        .text_size(px(11.))
        .text_color(pal(cx).dim)
        .child(div().flex_1().child(title.into().to_uppercase()))
}

/// `.toolbar`: 44 px high, a bottom border; left and right groups.
pub fn toolbar(cx: &App) -> Div {
    h_flex()
        .flex_none()
        .min_h(px(44.))
        .px(px(12.))
        .py(px(7.))
        .gap(px(12.))
        .justify_between()
        .border_b_1()
        .border_color(pal(cx).border)
}

/// `.tb-group`
pub fn group() -> Div {
    h_flex().gap(px(6.)).items_center()
}

/// `.tb-title`: an icon and a bold title.
pub fn tb_title(icon: &'static str, title: impl Into<SharedString>, cx: &App) -> Div {
    let p = pal(cx);
    h_flex()
        .gap(px(8.))
        .mr(px(6.))
        .text_size(px(14.))
        .font_semibold()
        .text_color(p.text)
        .child(svg().path(SharedString::from(format!("rv/{icon}.svg"))).size(px(15.)).flex_none().text_color(p.muted))
        .child(title.into())
}

/// A select-like button: the value and a chevron (`.schema-select`).
pub fn select(id: impl Into<ElementId>, value: impl Into<SharedString>, cx: &App) -> Button {
    let p = pal(cx);
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).color(p.panel_2).foreground(p.text).hover(p.panel_2).active(p.panel_2))
        .bg(p.panel_2)
        .border_1()
        .border_color(p.border)
        .rounded(px(6.))
        .h(px(28.))
        .px(px(8.))
        .label(value)
        .dropdown_caret(true)
}

/// The monospace family of the theme.
pub fn cx_mono(cx: &App) -> SharedString {
    use gpui_kit::component::ActiveTheme as _;
    cx.theme().mono_font_family.clone()
}

/// `.link-btn`: a text button in the identifier color (section "Add column").
pub fn link_btn(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    let p = pal(cx);
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).color(p.bg.opacity(0.)).foreground(p.t_id).hover(p.bg.opacity(0.)).active(p.bg.opacity(0.)))
        .h(px(18.))
        .px(px(0.))
        .text_size(px(12.))
        .label(label)
}

/// `.card`: max 820 px, panel background, a border, 10 px radius.
pub fn card(cx: &App) -> Div {
    let p = pal(cx);
    gpui_kit::component::v_flex()
        .w_full()
        .max_w(px(820.))
        .bg(p.panel)
        .border_1()
        .border_color(p.border)
        .rounded(px(10.))
        .px(px(24.))
        .py(px(20.))
        .gap(px(18.))
}

/// `.card-field`: a small label over a value, panel-2 box.
pub fn card_field(id: impl Into<ElementId>, label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> Stateful<Div> {
    let p = pal(cx);
    gpui_kit::component::v_flex()
        .id(id)
        .min_w(px(160.))
        .gap(px(2.))
        .px(px(10.))
        .py(px(6.))
        .border_1()
        .border_color(p.border)
        .rounded(px(6.))
        .bg(p.panel_2)
        .child(div().text_size(px(11.)).text_color(p.dim).child(label.into()))
        .child(div().min_h(px(22.)).flex().items_center().text_size(px(13.)).text_color(p.text).child(value))
}

/// `input.switch`: 30 x 17 toggle.
pub fn switch(on: bool, cx: &App) -> Div {
    let p = pal(cx);
    div()
        .flex_none()
        .w(px(30.))
        .h(px(17.))
        .rounded(px(9.))
        .relative()
        .bg(if on { p.accent } else { p.border_strong })
        .child(div().absolute().top(px(2.)).left(px(if on { 15. } else { 2. })).size(px(13.)).rounded_full().bg(if on { p.accent_text } else { gpui_kit::white() }))
}

/// `.card-section`: an upper-case head with an optional link button, then a `.def-list`.
pub fn card_section(title: &str, add: Option<Button>, rows: Vec<AnyElement>, cx: &App) -> Div {
    let p = pal(cx);
    let empty = rows.is_empty();
    gpui_kit::component::v_flex()
        .gap(px(6.))
        .child(
            h_flex()
                .gap(px(10.))
                .text_size(px(11.5))
                .font_semibold()
                .text_color(p.muted)
                .child(title.to_uppercase())
                .children(add),
        )
        .when(empty, |d| d.child(div().text_size(px(12.)).text_color(p.dim).child("None")))
        .when(!empty, |d| d.child(gpui_kit::component::v_flex().border_1().border_color(p.border).rounded(px(6.)).overflow_hidden().children(rows)))
}

/// `.def-row`: highlighted SQL with action buttons shown on hover.
pub fn def_row(id: impl Into<ElementId>, sql: &str, mysql: bool, actions: Option<AnyElement>, last: bool, cx: &App) -> AnyElement {
    let p = pal(cx);
    let id: ElementId = id.into();
    let group = SharedString::from(format!("def-{id}"));
    h_flex()
        .id(id)
        .group(group.clone())
        .items_start()
        .gap(px(8.))
        .px(px(12.))
        .py(px(9.))
        .bg(p.panel_2)
        .hover(|s| s.bg(p.hover))
        .when(!last, |d| d.border_b_1().border_color(p.border))
        .child(sql_block(sql, mysql, cx))
        .children(actions.map(|a| h_flex().gap(px(2.)).invisible().group_hover(group, |s| s.visible()).child(a)))
        .into_any_element()
}

/// `.sql-line`: monospace 13 px, highlighted, wrapped.
pub fn sql_block(sql: &str, mysql: bool, cx: &App) -> Div {
    let p = pal(cx);
    div()
        .flex_1()
        .min_w_0()
        .font_family(cx_mono(cx))
        .text_size(px(13.))
        .line_height(px(20.))
        .text_color(p.text)
        .whitespace_normal()
        .child(crate::sqlhl::styled(sql, mysql, &p))
}

/// A small icon action of a definition row (edit / drop).
pub fn act(id: impl Into<ElementId>, icon: &'static str, tooltip: &'static str, danger: bool, cx: &App) -> Button {
    let p = pal(cx);
    icon_btn(id, icon, cx).size(px(24.)).tooltip(tooltip).when(danger, |b| b.custom(ButtonCustomVariant::new(cx).color(p.bg.opacity(0.)).foreground(p.muted).hover(p.danger_soft).active(p.danger_soft)))
}

/// The dotted 24 px grid behind the Schema view (`.schema-body`).
pub fn grid_bg(cx: &App) -> impl IntoElement {
    let c = pal(cx).border.opacity(0.55);
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let step = px(24.);
            let mut x = bounds.origin.x + step - px(1.);
            while x < bounds.origin.x + bounds.size.width {
                window.paint_quad(fill(Bounds::new(point(x, bounds.origin.y), size(px(1.), bounds.size.height)), c));
                x += step;
            }
            let mut y = bounds.origin.y + step - px(1.);
            while y < bounds.origin.y + bounds.size.height {
                window.paint_quad(fill(Bounds::new(point(bounds.origin.x, y), size(bounds.size.width, px(1.))), c));
                y += step;
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// `.modal`: the stable UI's dialog surface and title.
pub fn dialog(d: gpui_kit::component::dialog::Dialog, title: impl Into<SharedString>, width: f32, cx: &App) -> gpui_kit::component::dialog::Dialog {
    let p = pal(cx);
    d.title(div().text_size(px(15.)).font_weight(FontWeight::BOLD).text_color(p.text).child(title.into()))
        .w(px(width))
        .bg(p.elev)
        .border_color(p.border_strong)
        .rounded(px(10.))
}

/// A dialog with a custom title element.
pub fn dialog_el(d: gpui_kit::component::dialog::Dialog, title: impl IntoElement, width: f32, cx: &App) -> gpui_kit::component::dialog::Dialog {
    let p = pal(cx);
    d.title(title).w(px(width)).bg(p.elev).border_color(p.border_strong).rounded(px(10.))
}
