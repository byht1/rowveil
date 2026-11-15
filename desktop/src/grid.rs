//! The data grid shared by table data and SQL results: visible columns, cell display,
//! staged edits (modified / inserted / deleted rows), sort, widths and a context menu.

use crate::backend;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::{Column, TableDelegate, TableState};
use crate::ui::pal;
use gpui_kit::component::{h_flex, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Clone, Debug, Default)]
pub struct GridCol {
    pub name: String,
    pub type_name: String,
    pub kind: String,
    pub primary: bool,
    /// "schema.table.column" of a foreign key target.
    pub fk: Option<String>,
    #[allow(dead_code)]
    pub tooltip: String,
}

/// Callbacks from the grid to its owner view.
#[derive(Default, Clone)]
pub struct GridHooks {
    /// The header was sorted: (column name, ascending / descending / none, Shift held).
    pub on_sort: Option<Rc<dyn Fn(String, Option<bool>, bool, &mut Window, &mut App)>>,
    /// Extra context menu items for a cell (row, column name) of the original rows.
    pub menu: Option<Rc<dyn Fn(PopupMenu, usize, String, &mut Window, &mut App) -> PopupMenu>>,
    /// Column widths changed (name → px).
    pub on_widths: Option<Rc<dyn Fn(Vec<(String, f32)>, &mut App)>>,
}

pub struct DataGrid {
    pub cols: Vec<GridCol>,
    pub hidden: HashSet<String>,
    pub widths: HashMap<String, f32>,
    /// Rows: inserted rows (new records) come first, then loaded rows.
    pub rows: Vec<Vec<Value>>,
    pub inserted: usize,
    /// Staged values: (row index, column name) → new value.
    pub edited: HashMap<(usize, String), Value>,
    pub deleted: HashSet<usize>,
    /// The current sort shown in the headers: (column, descending), in order.
    pub sort: Vec<(String, bool)>,
    pub hooks: GridHooks,
    /// The number of the first row minus one (the page offset).
    pub row_offset: usize,
}

impl DataGrid {
    pub fn new(cols: Vec<GridCol>, rows: Vec<Vec<Value>>) -> Self {
        Self { cols, hidden: HashSet::new(), widths: HashMap::new(), rows, inserted: 0, edited: HashMap::new(), deleted: HashSet::new(), sort: Vec::new(), hooks: GridHooks::default(), row_offset: 0 }
    }

    /// Indexes of the visible columns in `cols`.
    pub fn visible(&self) -> Vec<usize> {
        (0..self.cols.len()).filter(|i| !self.hidden.contains(&self.cols[*i].name)).collect()
    }

    pub fn col_index(&self, name: &str) -> Option<usize> {
        self.cols.iter().position(|c| c.name == name)
    }

    /// The value shown in a cell: the staged one when there is one.
    pub fn value(&self, row: usize, col: usize) -> Value {
        let name = &self.cols[col].name;
        if let Some(v) = self.edited.get(&(row, name.clone())) {
            return v.clone();
        }
        self.rows.get(row).and_then(|r| r.get(col)).cloned().unwrap_or(Value::Null)
    }

    pub fn original(&self, row: usize, col: usize) -> Value {
        self.rows.get(row).and_then(|r| r.get(col)).cloned().unwrap_or(Value::Null)
    }

    pub fn row_object(&self, row: usize) -> serde_json::Map<String, Value> {
        self.cols.iter().enumerate().map(|(i, c)| (c.name.clone(), self.value(row, i))).collect()
    }
}

/// Text for a cell: NULL, DEFAULT marker, strings cut at 500 characters, one line.
pub fn display(v: &Value) -> (String, bool) {
    match v {
        Value::Null => ("NULL".into(), true),
        Value::Object(o) if o.get("$default").is_some() => ("DEFAULT".into(), true),
        Value::String(s) => {
            let mut t: String = s.chars().take(500).collect();
            if t.len() < s.len() {
                t.push('…');
            }
            (t.replace(['\n', '\r'], " "), false)
        }
        other => {
            let s = backend::cell_text(other);
            (s.chars().take(500).collect::<String>().replace('\n', " "), false)
        }
    }
}

/// The stable UI's default width: by kind, at least room for the name.
fn default_width(kind: &str, name: &str) -> f32 {
    let by_kind = match kind {
        "number" => 110.,
        "bigint" => 140.,
        "boolean" => 100.,
        "uuid" => 300.,
        "date" => 210.,
        "json" => 260.,
        "array" | "binary" => 200.,
        "enum" => 140.,
        _ => 200.,
    };
    f32::max(by_kind, f32::min(320., name.chars().count() as f32 * 8. + 70.))
}

impl DataGrid {
    /// The index in `cols` of a table column (table column 0 is the row number).
    pub fn vis_col(&self, table_col: usize) -> Option<usize> {
        table_col.checked_sub(1).and_then(|i| self.visible().get(i).copied())
    }

    /// The table column of an index in `cols` (when the column is visible).
    pub fn table_col(&self, ci: usize) -> Option<usize> {
        self.visible().iter().position(|x| *x == ci).map(|i| i + 1)
    }

    /// Click on a header: ascending → descending → none; Shift keeps the other sorted columns.
    fn toggle_sort(&mut self, name: String, additive: bool) -> Option<bool> {
        let existing = self.sort.iter().find(|(s, _)| *s == name).map(|(_, d)| *d);
        if !additive {
            self.sort.retain(|(s, _)| *s == name);
        }
        match existing {
            None => {
                self.sort.push((name, false));
                Some(false)
            }
            Some(false) => {
                if let Some(x) = self.sort.iter_mut().find(|(s, _)| *s == name) {
                    x.1 = true;
                }
                Some(true)
            }
            Some(true) => {
                self.sort.retain(|(s, _)| *s != name);
                None
            }
        }
    }

    fn rownum_width(&self) -> f32 {
        let digits = (self.row_offset + self.rows.len()).max(1).to_string().len() as f32;
        (digits * 7. + 22.).max(40.)
    }
}

const NO_PAD: Edges<Pixels> = Edges { top: px(0.), right: px(0.), bottom: px(0.), left: px(0.) };

impl TableDelegate for DataGrid {
    fn columns_count(&self, _: &App) -> usize {
        self.visible().len() + 1
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        if col_ix == 0 {
            return Column::new("#", "#").width(px(self.rownum_width())).fixed_left().resizable(false).movable(false).selectable(false).paddings(NO_PAD);
        }
        let c = &self.cols[self.visible()[col_ix - 1]];
        let w = self.widths.get(&c.name).copied().unwrap_or_else(|| default_width(&c.kind, &c.name));
        Column::new(c.name.clone(), c.name.clone()).width(px(w)).min_width(px(60.)).paddings(NO_PAD)
    }

    fn render_th(&mut self, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let p = pal(cx);
        if col_ix == 0 {
            return div().size_full().bg(p.panel).border_r_1().border_color(p.border).into_any_element();
        }
        let c = self.cols[self.visible()[col_ix - 1]].clone();
        let pos = self.sort.iter().position(|(s, _)| *s == c.name);
        let sort_mark = pos.map(|i| format!("{}{}", if self.sort[i].1 { "↓" } else { "↑" }, if self.sort.len() > 1 { (i + 1).to_string() } else { String::new() }));
        let sortable = self.hooks.on_sort.is_some();
        let name = c.name.clone();
        h_flex()
            .id(("th", col_ix))
            .size_full()
            .px(px(10.))
            .gap(px(6.))
            .overflow_hidden()
            .border_r_1()
            .border_color(p.border)
            .when(sortable, |d| {
                d.cursor_pointer().hover(|s| s.bg(p.hover)).on_click(cx.listener(move |table, _: &ClickEvent, window, cx| {
                    let shift = window.modifiers().shift;
                    let d = table.delegate_mut();
                    let dir = d.toggle_sort(name.clone(), shift);
                    if let Some(f) = d.hooks.on_sort.clone() {
                        f(name.clone(), dir, shift, window, cx);
                    }
                    cx.notify();
                }))
            })
            .when(c.primary, |d| d.child(svg().path("rv/key.svg").size(px(12.)).flex_none().text_color(rgb(0xeab308))))
            .when(!c.primary && c.fk.is_some(), |d| d.child(svg().path("rv/link.svg").size(px(12.)).flex_none().text_color(p.t_id)))
            .child(div().flex_none().max_w_full().truncate().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(p.text).child(c.name.clone()))
            .child(div().min_w_0().flex_1().truncate().text_size(px(11.)).font_family(crate::kit::cx_mono(cx)).text_color(p.dim).child(c.type_name.clone()))
            .children(sort_mark.map(|m| div().flex_none().text_size(px(12.)).font_bold().text_color(p.accent).child(m)))
            .into_any_element()
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let p = pal(cx);
        let inserted = row_ix < self.inserted;
        let deleted = self.deleted.contains(&row_ix);
        if col_ix == 0 {
            let label = if inserted { "+".to_string() } else { (self.row_offset + row_ix - self.inserted + 1).to_string() };
            return div()
                .id(("rn", row_ix))
                .size_full()
                .px(px(6.))
                .flex()
                .items_center()
                .justify_end()
                .bg(p.panel)
                .border_r_1()
                .border_color(p.border)
                .text_size(px(11.))
                .map(|d| if inserted { d.text_color(p.true_).font_bold() } else { d.text_color(p.dim) })
                .cursor_pointer()
                .on_click(cx.listener(move |table, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    table.set_selected_row(row_ix, cx);
                }))
                .child(label)
                .into_any_element();
        }
        let col = self.visible()[col_ix - 1];
        let name = self.cols[col].name.clone();
        let v = self.value(row_ix, col);
        let (text, muted) = display(&v);
        let modified = self.edited.contains_key(&(row_ix, name)) && !inserted;
        let json = !muted && (matches!(v, Value::Object(_) | Value::Array(_)) || self.cols[col].kind == "json");
        div()
            .size_full()
            .px(px(10.))
            .flex()
            .items_center()
            .overflow_hidden()
            .border_r_1()
            .border_color(p.border)
            .font_family(crate::kit::cx_mono(cx))
            .text_size(px(12.))
            .when(inserted, |d| d.bg(p.new_bg))
            .when(modified, |d| d.bg(p.mod_bg).border_1().border_color(p.mod_border))
            .when(deleted, |d| d.bg(p.del_bg).text_color(p.danger).line_through())
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .when(muted, |d| d.text_color(p.dim).italic())
                    .when(v == Value::Bool(true), |d| d.text_color(p.true_))
                    .when(v == Value::Bool(false), |d| d.text_color(p.false_))
                    .when(json, |d| d.text_color(p.t_str))
                    .child(text),
            )
            .into_any_element()
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, window: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let col = cx.entity().read(cx).right_clicked_cell().map(|(_, c)| c).or_else(|| cx.entity().read(cx).selected_cell().map(|(_, c)| c)).unwrap_or(1);
        let Some(ci) = self.vis_col(col).or_else(|| self.visible().first().copied()) else { return menu };
        let name = self.cols[ci].name.clone();
        let text = backend::cell_text(&self.value(row_ix, ci));
        let row_json = serde_json::to_string_pretty(&Value::Object(self.row_object(row_ix))).unwrap_or_default();
        let col_name = name.clone();
        let mut menu = menu
            .item(PopupMenuItem::new("Copy value").icon(crate::kit::ico("copy")).on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))))
            .item(PopupMenuItem::new("Copy row as JSON").icon(crate::kit::ico("copy")).on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(row_json.clone()))))
            .item(PopupMenuItem::new("Copy column name").icon(crate::kit::ico("copy")).on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(col_name.clone()))));
        if let Some(f) = self.hooks.menu.clone() {
            menu = f(menu.separator(), row_ix, name, window, cx);
        }
        menu
    }

    fn render_empty(&mut self, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        h_flex().size_full().justify_center().p(px(40.)).text_size(px(13.)).text_color(pal(cx).muted).child("No rows")
    }
}

/// Creates a grid entity and wires width changes to the hooks.
pub fn new_grid(grid: DataGrid, _editable_cells: bool, window: &mut Window, cx: &mut App) -> Entity<TableState<DataGrid>> {
    let state = cx.new(|cx| TableState::new(grid, window, cx).cell_selectable(true).row_selectable(true).col_resizable(true).sortable(false).row_header(false).col_movable(false));
    cx.subscribe(&state, |state, ev: &gpui_kit::component::table::TableEvent, cx| {
        if let gpui_kit::component::table::TableEvent::ColumnWidthsChanged(ws) = ev {
            let d = state.read(cx).delegate();
            let vis = d.visible();
            // ws[0] is the row number column.
            let pairs: Vec<(String, f32)> = vis.iter().zip(ws.iter().skip(1)).map(|(i, w)| (d.cols[*i].name.clone(), f32::from(*w))).collect();
            let hook = d.hooks.on_widths.clone();
            state.update(cx, |s, _| {
                for (n, w) in &pairs {
                    s.delegate_mut().widths.insert(n.clone(), *w);
                }
            });
            if let Some(f) = hook {
                f(pairs, cx);
            }
        }
    })
    .detach();
    state
}

/// The grid with the stable UI's sizes: 32 px rows, a 34 px header, no stripes.
pub fn table(state: &Entity<TableState<DataGrid>>) -> gpui_kit::component::table::DataTable<DataGrid> {
    use gpui_kit::component::Sizable as _;
    gpui_kit::component::table::DataTable::new(state).with_size(gpui_kit::component::Size::Size(px(32.))).header_row_height(px(34.)).bordered(false)
}

/// Text of a value for the editor and the clipboard (JSON values pretty-printed).
pub fn editor_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Object(_) | Value::Array(_) => serde_json::to_string_pretty(v).unwrap_or_default(),
        other => other.to_string(),
    }
}

pub fn looks_like_json(s: &str) -> bool {
    let t = s.trim();
    ((t.starts_with('{') && t.ends_with('}')) || (t.starts_with('[') && t.ends_with(']'))) && serde_json::from_str::<Value>(t).is_ok()
}

#[allow(dead_code)]
pub fn bold(s: impl Into<SharedString>) -> Div {
    div().font_semibold().child(s.into())
}
