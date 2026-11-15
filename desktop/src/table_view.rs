//! A table tab: data (paging, sort, filters, columns, staged edits, export) and structure.

use crate::backend::{self, Backend};
use crate::cell_editor::CellEditor;
use crate::grid::{self, DataGrid, GridCol, GridHooks};
use crate::model::{self, Column, Schema, Table};
use crate::{kit, ui};
use crate::workspace::Workspace;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::table::{TableEvent, TableState};
use gpui_kit::component::{Disableable as _, h_flex, v_flex, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

pub const OPS: &[&str] = &["=", "<>", ">", ">=", "<", "<=", "like", "not like", "ilike", "not ilike", "in", "not in", "is null", "is not null"];
pub const PAGE_SIZES: &[i64] = &[25, 50, 100, 250, 500, 1000];

pub struct FilterRow {
    pub column: String,
    pub op: String,
    pub value: Entity<InputState>,
}

pub struct TableView {
    backend: Arc<Backend>,
    conn: String,
    pub table: Table,
    dialect: String,
    /// Why the data cannot change here (None: editable).
    block_reason: Option<String>,
    can_grant: bool,
    ws: WeakEntity<Workspace>,
    pub grid: Entity<TableState<DataGrid>>,
    pub sort: Vec<(String, bool)>,
    pub filters: Vec<FilterRow>,
    show_filters: bool,
    pub page_size: i64,
    page: i64,
    total: Option<i64>,
    loaded: usize,
    loading: bool,
    error: Option<String>,
    pub structure: bool,
    /// The row shown in the side panel.
    drawer: Option<usize>,
    generation: u64,
    loaded_once: bool,
    _subs: Vec<Subscription>,
}

/// Saved state of a table tab.
pub fn tab_state(v: &TableView, cx: &App) -> Value {
    let d = v.grid.read(cx).delegate();
    json!({
        "kind": "table", "schema": v.table.schema, "table": v.table.name,
        "filters": v.filters.iter().map(|f| json!({ "column": f.column, "op": f.op, "value": f.value.read(cx).value().to_string() })).collect::<Vec<_>>(),
        "sort": v.sort.iter().map(|(c, d)| json!({ "column": c, "desc": d })).collect::<Vec<_>>(),
        "pageSize": v.page_size,
        "hidden": d.hidden.iter().collect::<Vec<_>>(),
        "widths": d.widths,
        "view": if v.structure { "structure" } else { "data" },
    })
}

impl TableView {
    pub fn new(backend: Arc<Backend>, conn: String, schema: &Schema, table: Table, saved: Option<&Value>, ws: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let block_reason = if schema.read_only {
            Some(schema.read_only_reason.clone().unwrap_or_else(|| "The connection is read-only".into()))
        } else if table.is_view() {
            Some(format!("This is a {}", if table.kind == "view" { "view".into() } else { table.kind.clone() }))
        } else if table.primary_key.is_empty() {
            Some("Table has no primary key".into())
        } else {
            None
        };
        let cols: Vec<GridCol> = table
            .columns
            .iter()
            .map(|c| GridCol {
                name: c.name.clone(),
                type_name: c.data_type.clone(),
                kind: c.kind.clone(),
                primary: c.primary,
                fk: c.references.as_ref().map(|r| format!("{}.{}.{}", r.schema, r.table, r.column)),
                tooltip: column_tooltip(c),
            })
            .collect();
        let mut dg = DataGrid::new(cols, Vec::new());
        let me = cx.weak_entity();
        let me2 = me.clone();
        let me3 = me.clone();
        dg.hooks = GridHooks {
            on_sort: Some(Rc::new(move |_name, _dir, _shift, _, cx| {
                let _ = me.update(cx, |this, cx| {
                    this.sort = this.grid.read(cx).delegate().sort.clone();
                    this.page = 0;
                    this.load(cx);
                    this.changed(cx);
                });
            })),
            menu: Some(Rc::new(move |menu, row, col, _, cx| {
                let Some(this) = me2.upgrade() else { return menu };
                this.read(cx).row_menu(menu, row, col, me2.clone(), cx)
            })),
            on_widths: Some(Rc::new(move |_, cx| {
                let _ = me3.update(cx, |this, cx| this.changed(cx));
            })),
        };
        let mut page_size = 50;
        let mut sort = Vec::new();
        let mut filters = Vec::new();
        let mut structure = false;
        if let Some(s) = saved {
            page_size = s["pageSize"].as_i64().filter(|n| PAGE_SIZES.contains(n)).unwrap_or(50);
            sort = s["sort"].as_array().into_iter().flatten().filter_map(|x| Some((x["column"].as_str()?.to_string(), x["desc"].as_bool().unwrap_or(false)))).collect();
            dg.hidden = s["hidden"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect();
            dg.widths = s["widths"].as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.clone(), v.as_f64()? as f32))).collect();
            structure = s["view"].as_str() == Some("structure");
            for f in s["filters"].as_array().into_iter().flatten() {
                filters.push((f["column"].as_str().unwrap_or("").to_string(), f["op"].as_str().unwrap_or("=").to_string(), backend::cell_text(&f["value"]).replace("NULL", "")));
            }
        }
        dg.sort = sort.clone();
        let grid = grid::new_grid(dg, true, window, cx);
        let subs = vec![cx.subscribe_in(&grid, window, |this: &mut Self, _, ev: &TableEvent, window, cx| {
            if let TableEvent::DoubleClickedCell(row, col) = ev {
                this.edit_cell(*row, *col, window, cx);
            }
        })];
        let mut this = Self {
            backend,
            conn,
            table,
            dialect: schema.dialect.clone(),
            block_reason,
            can_grant: schema.can_grant_write,
            ws,
            grid,
            sort,
            filters: Vec::new(),
            show_filters: false,
            page_size,
            page: 0,
            total: None,
            loaded: 0,
            loading: false,
            error: None,
            structure,
            drawer: None,
            generation: 0,
            loaded_once: false,
            _subs: subs,
        };
        for (c, op, v) in filters {
            this.add_filter(Some((c, op, v)), window, cx);
        }
        this.show_filters = !this.filters.is_empty();
        this.load(cx);
        this
    }

    fn changed(&self, cx: &mut Context<Self>) {
        let ws = self.ws.clone();
        cx.defer(move |cx| {
            let _ = ws.update(cx, |ws, cx| ws.save_state(cx));
        });
    }

    pub fn dirty(&self, cx: &App) -> usize {
        let d = self.grid.read(cx).delegate();
        let updated: HashSet<usize> = d.edited.keys().map(|(r, _)| *r).filter(|r| *r >= d.inserted && !d.deleted.contains(r)).collect();
        updated.len() + d.inserted + d.deleted.len()
    }

    fn active_filters(&self, cx: &App) -> Vec<Value> {
        self.filters
            .iter()
            .filter(|f| !f.column.is_empty())
            .map(|f| {
                let v = f.value.read(cx).value().to_string();
                json!({ "column": f.column, "op": f.op, "value": v })
            })
            .collect()
    }

    fn rows_body(&self, limit: i64, offset: i64, with_count: bool, cx: &App) -> Value {
        json!({
            "schema": self.table.schema, "table": self.table.name,
            "filters": self.active_filters(cx),
            "sort": self.sort.iter().map(|(c, d)| json!({ "column": c, "desc": d })).collect::<Vec<_>>(),
            "limit": limit, "offset": offset, "withCount": with_count,
        })
    }

    pub fn load(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        let body = self.rows_body(self.page_size, self.page * self.page_size, true, cx);
        let fut = self.backend.clone().post(format!("/connections/{}/rows", backend::enc(&self.conn)), body);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = fut.await.and_then(model::parse::<model::Rows>);
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match res {
                    Ok(r) => {
                        this.total = r.total;
                        this.loaded = r.rows.len();
                        // The answer's columns may differ in order: map them to the table's columns.
                        let order: Vec<Option<usize>> = this.table.columns.iter().map(|c| r.columns.iter().position(|n| *n == c.name)).collect();
                        let rows: Vec<Vec<Value>> = r.rows.into_iter().map(|row| order.iter().map(|i| i.and_then(|i| row.get(i).cloned()).unwrap_or(Value::Null)).collect()).collect();
                        let first_load = this.loaded_once;
                        this.loaded_once = true;
                        if !first_load {
                            if let Some(r) = crate::dev_env("ROWVEIL_NEXT_ROW").and_then(|v| v.parse::<usize>().ok()) {
                                this.drawer = Some(r);
                            }
                            if let Some((r, c)) = crate::dev_env("ROWVEIL_NEXT_EDIT").and_then(|v| v.split_once(',').map(|(a, b)| (a.parse::<usize>().ok(), b.parse::<usize>().ok()))) {
                                if let (Some(r), Some(c)) = (r, c) {
                                    let me = cx.weak_entity();
                                    cx.defer(move |cx| {
                                        if let Some(w) = cx.windows().first().copied() {
                                            let _ = w.update(cx, |_, window, cx| {
                                                let _ = me.update(cx, |this, cx| this.edit_cell(r, c, window, cx));
                                            });
                                        }
                                    });
                                }
                            }
                        }
                        this.grid.update(cx, |s, cx| {
                            let d = s.delegate_mut();
                            d.rows = rows;
                            d.row_offset = (this.page * this.page_size).max(0) as usize;
                            this.drawer = this.drawer.filter(|r| *r < d.rows.len());
                            d.inserted = 0;
                            d.edited.clear();
                            d.deleted.clear();
                            s.refresh(cx);
                        });
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn add_filter(&mut self, init: Option<(String, String, String)>, window: &mut Window, cx: &mut Context<Self>) {
        let (column, op, value) = init.unwrap_or_else(|| (self.table.columns.first().map(|c| c.name.clone()).unwrap_or_default(), "=".into(), String::new()));
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("value").default_value(value));
        self._subs.push(cx.subscribe(&input, |this: &mut Self, _, ev: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.apply_filters(cx);
            }
        }));
        self.filters.push(FilterRow { column, op, value: input });
        cx.notify();
    }

    fn apply_filters(&mut self, cx: &mut Context<Self>) {
        self.page = 0;
        self.load(cx);
        self.changed(cx);
    }

    /// Opens the editor (or the viewer) for a cell. `col` is a grid column (0 is the row number).
    pub fn edit_cell(&mut self, row: usize, col: usize, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.grid.read(cx).delegate();
        let Some(ci) = d.vis_col(col) else { return };
        let Some(column) = self.table.column(&d.cols[ci].name).cloned() else { return };
        let value = d.value(row, ci);
        let new_row = row < d.inserted;
        let deleted = d.deleted.contains(&row);
        let editable = !column.generated && !deleted && (if new_row { !self.read_only_conn() } else { self.block_reason.is_none() });
        let allow_default = new_row && (column.default.is_some() || column.auto || column.nullable);
        let me = cx.weak_entity();
        let name = column.name.clone();
        let on_apply: Option<crate::cell_editor::OnApply> = editable.then(|| {
            Rc::new(move |v: Value, _: &mut Window, cx: &mut App| {
                let _ = me.update(cx, |this, cx| this.set_cell(row, &name, v, cx));
            }) as crate::cell_editor::OnApply
        });
        CellEditor::open(column, value, allow_default, on_apply, window, cx);
    }

    fn read_only_conn(&self) -> bool {
        self.block_reason.as_deref().is_some_and(|r| !r.starts_with("Table has no primary key") && !r.starts_with("This is a"))
    }

    fn set_cell(&mut self, row: usize, col: &str, v: Value, cx: &mut Context<Self>) {
        self.grid.update(cx, |s, cx| {
            let d = s.delegate_mut();
            let Some(ci) = d.col_index(col) else { return };
            if row >= d.inserted && d.original(row, ci) == v {
                d.edited.remove(&(row, col.to_string()));
            } else {
                d.edited.insert((row, col.to_string()), v);
            }
            cx.notify();
        });
        cx.notify();
    }

    fn add_record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let first = self.table.columns.iter().position(|c| !c.auto && !c.generated);
        self.grid.update(cx, |s, cx| {
            let d = s.delegate_mut();
            let n = d.cols.len();
            d.rows.insert(0, vec![Value::Null; n]);
            // Shift staged state of existing rows down by one.
            d.edited = d.edited.drain().map(|((r, c), v)| ((r + 1, c), v)).collect();
            d.deleted = d.deleted.drain().map(|r| r + 1).collect();
            d.inserted += 1;
            // New rows start with DEFAULT where the database has one.
            for (i, c) in self.table.columns.iter().enumerate() {
                if c.default.is_some() || c.auto || c.generated {
                    d.edited.insert((0, d.cols[i].name.clone()), json!({ "$default": true }));
                }
            }
            s.refresh(cx);
        });
        if let Some(ci) = first {
            if let Some(vc) = self.grid.read(cx).delegate().table_col(ci) {
                self.edit_cell(0, vc, window, cx);
            }
        }
        cx.notify();
    }

    fn toggle_delete_selected(&mut self, cx: &mut Context<Self>) {
        let sel = self.grid.read(cx).selected_row().or_else(|| self.grid.read(cx).selected_cell().map(|(r, _)| r));
        let Some(row) = sel else { return };
        self.grid.update(cx, |s, cx| {
            let d = s.delegate_mut();
            if row < d.inserted {
                d.rows.remove(row);
                d.inserted -= 1;
                d.edited = d.edited.drain().filter(|((r, _), _)| *r != row).map(|((r, c), v)| ((if r > row { r - 1 } else { r }, c), v)).collect();
                d.deleted = d.deleted.drain().map(|r| if r > row { r - 1 } else { r }).collect();
            } else if !d.deleted.remove(&row) {
                d.deleted.insert(row);
            }
            s.refresh(cx);
        });
        cx.notify();
    }

    fn discard(&mut self, cx: &mut Context<Self>) {
        self.grid.update(cx, |s, cx| {
            let d = s.delegate_mut();
            let n = d.inserted;
            d.rows.drain(..n);
            d.inserted = 0;
            d.edited.clear();
            d.deleted.clear();
            s.refresh(cx);
        });
        cx.notify();
    }

    fn pk_of(&self, d: &DataGrid, row: usize) -> Map<String, Value> {
        self.table.primary_key.iter().filter_map(|k| d.col_index(k).map(|i| (k.clone(), d.original(row, i)))).collect()
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.grid.read(cx).delegate();
        let mut changes = Vec::new();
        for row in (0..d.inserted).rev() {
            if d.deleted.contains(&row) {
                continue;
            }
            let mut values = Map::new();
            for (i, c) in d.cols.iter().enumerate() {
                let v = d.value(row, i);
                if !matches!(&v, Value::Object(o) if o.contains_key("$default")) {
                    values.insert(c.name.clone(), v);
                }
                let _ = i;
            }
            changes.push(json!({ "type": "insert", "values": values }));
        }
        let mut rows: Vec<usize> = d.edited.keys().map(|(r, _)| *r).filter(|r| *r >= d.inserted && !d.deleted.contains(r)).collect::<HashSet<_>>().into_iter().collect();
        rows.sort();
        for row in rows {
            let values: Map<String, Value> = d.edited.iter().filter(|((r, _), _)| *r == row).map(|((_, c), v)| (c.clone(), v.clone())).collect();
            changes.push(json!({ "type": "update", "pk": self.pk_of(d, row), "values": values }));
        }
        let mut dels: Vec<usize> = d.deleted.iter().copied().filter(|r| *r >= d.inserted).collect();
        dels.sort();
        for row in dels {
            changes.push(json!({ "type": "delete", "pk": self.pk_of(d, row) }));
        }
        if changes.is_empty() {
            return;
        }
        let n = changes.len();
        let body = json!({ "schema": self.table.schema, "table": self.table.name, "changes": changes });
        let fut = self.backend.clone().post(format!("/connections/{}/apply", backend::enc(&self.conn)), body);
        cx.spawn_in(window, async move |this, cx| {
            let res = fut.await;
            let _ = this.update_in(cx, |this, window, cx| match res {
                Ok(v) => {
                    let affected = v["rowsAffected"].as_u64().unwrap_or(0);
                    ui::toast_ok(window, cx, format!("Saved {n} change{} ({affected} row{} affected)", if n == 1 { "" } else { "s" }, if affected == 1 { "" } else { "s" }));
                    this.load(cx);
                }
                Err(e) => ui::toast_err(window, cx, e.to_string()),
            });
        })
        .detach();
    }

    fn row_menu(&self, mut menu: gpui_kit::component::menu::PopupMenu, row: usize, col: String, me: WeakEntity<Self>, cx: &App) -> gpui_kit::component::menu::PopupMenu {
        let d = self.grid.read(cx).delegate();
        // Follow a foreign key: open the referenced table filtered by this value.
        if let Some(r) = self.table.column(&col).and_then(|c| c.references.clone()) {
            let v = d.col_index(&col).map(|i| d.original(row, i)).unwrap_or(Value::Null);
            if !v.is_null() {
                let ws = self.ws.clone();
                let label = format!("Open {}.{} where {} = {}", r.schema, r.table, r.column, backend::cell_text(&v));
                menu = menu.item(PopupMenuItem::new(label).icon(crate::kit::ico("link")).on_click(move |_, window, cx| {
                    let filter = (r.column.clone(), "=".to_string(), backend::cell_text(&v));
                    let _ = ws.update(cx, |ws, cx| ws.open_table(r.schema.clone(), r.table.clone(), true, Some(filter.clone()), window, cx));
                }));
            }
        }
        let me2 = me.clone();
        menu = menu.item(PopupMenuItem::new("Row details").icon(crate::kit::ico("expand")).on_click(move |_, window, cx| {
            let _ = me2.update(cx, |this, cx| this.row_details(row, window, cx));
        }));
        if self.block_reason.is_none() {
            menu = menu.item(PopupMenuItem::new(if d.deleted.contains(&row) { "Undo delete" } else { "Delete row" }).icon(crate::kit::ico(if d.deleted.contains(&row) { "undo" } else { "trash" })).on_click(move |_, _, cx| {
                let _ = me.update(cx, |this, cx| {
                    this.grid.update(cx, |s, cx| {
                        let d = s.delegate_mut();
                        if !d.deleted.remove(&row) {
                            d.deleted.insert(row);
                        }
                        s.refresh(cx);
                    });
                    cx.notify();
                });
            }));
        }
        menu
    }

    /// All fields of one row in a dialog, with links for foreign keys and reverse references.
    fn row_details(&mut self, row: usize, _window: &mut Window, cx: &mut Context<Self>) {
        self.drawer = Some(row);
        cx.notify();
    }

    /// `.drawer`: the fields of one row on the right of the grid.
    fn render_drawer(&self, row: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = ui::pal(cx);
        let mono = kit::cx_mono(cx);
        let d = self.grid.read(cx).delegate();
        if row >= d.rows.len() {
            return div().into_any_element();
        }
        let n_rows = d.rows.len();
        let new_row = row < d.inserted;
        let deleted = d.deleted.contains(&row);
        let editable = |c: &Column| !c.generated && !deleted && (if new_row { !self.read_only_conn() } else { self.block_reason.is_none() });
        let fields: Vec<(Column, Value, bool)> = self.table.columns.iter().filter_map(|c| d.col_index(&c.name).map(|i| (c.clone(), d.value(row, i), d.edited.contains_key(&(row, c.name.clone())) && !new_row))).collect();
        let json = serde_json::to_string_pretty(&Value::Object(d.row_object(row))).unwrap_or_default();
        let title = if new_row { "New record".to_string() } else { format!("Row {}", d.row_offset + row - d.inserted + 1) };
        let mut body = v_flex().id("drawer-body").flex_1().min_h_0().overflow_y_scroll().px(px(14.)).pt(px(8.)).pb(px(20.));
        for (i, (c, v, modified)) in fields.iter().enumerate() {
            let (text, muted) = grid::display(v);
            let is_json = matches!(v, Value::Object(_) | Value::Array(_)) && !muted;
            let text = if is_json { serde_json::to_string_pretty(v).unwrap_or(text) } else { text };
            let can = editable(c);
            let name = c.name.clone();
            let fk = c.references.clone().filter(|_| !muted);
            let ws = self.ws.clone();
            let fk_val = backend::cell_text(v);
            body = body.child(
                v_flex()
                    .py(px(8.))
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .mb(px(5.))
                            .text_size(px(12.))
                            .font_semibold()
                            .when(c.primary, |d| d.child(svg().path("rv/key.svg").size(px(11.)).text_color(rgb(0xeab308))))
                            .child(c.name.clone())
                            .child(div().min_w_0().truncate().text_size(px(11.)).font_family(mono.clone()).font_weight(FontWeight::NORMAL).text_color(p.dim).child(c.data_type.clone()))
                            .when_some(fk, |d, r| {
                                d.child(div().flex_1()).child(
                                    h_flex()
                                        .id(("fk", i))
                                        .gap(px(3.))
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(p.t_id)
                                        .cursor_pointer()
                                        .hover(|s| s.underline())
                                        .on_click(move |_, window, cx| {
                                            let f = (r.column.clone(), "=".to_string(), fk_val.clone());
                                            let _ = ws.update(cx, |ws, cx| ws.open_table(r.schema.clone(), r.table.clone(), false, Some(f), window, cx));
                                        })
                                        .child(format!("→ {}", c.references.as_ref().map(|r| r.table.clone()).unwrap_or_default()))
                                        .child(svg().path("rv/link.svg").size(px(11.)).text_color(p.t_id)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(("fv", i))
                            .min_h(px(30.))
                            .max_h(px(260.))
                            .overflow_y_scroll()
                            .px(px(8.))
                            .py(px(6.))
                            .rounded(px(5.))
                            .border_1()
                            .border_color(if *modified { p.mod_border } else { p.border })
                            .bg(if *modified { p.mod_bg } else { p.bg })
                            .font_family(mono.clone())
                            .text_size(px(12.))
                            .map(|d| {
                                if muted {
                                    d.text_color(p.dim).italic()
                                } else if *v == Value::Bool(true) {
                                    d.text_color(p.true_)
                                } else if *v == Value::Bool(false) {
                                    d.text_color(p.false_)
                                } else if is_json {
                                    d.text_color(p.t_str)
                                } else {
                                    d.text_color(p.text)
                                }
                            })
                            .when(can, |d| d.cursor_text().hover(|s| s.border_color(p.border_strong)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let d = this.grid.read(cx).delegate();
                                if let Some(vc) = d.col_index(&name).and_then(|ci| d.table_col(ci)) {
                                    this.edit_cell(row, vc, window, cx);
                                }
                            }))
                            .children(text.lines().map(|l| div().child(if l.is_empty() { " ".to_string() } else { l.to_string() }))),
                    ),
            );
        }
        // Reverse relations: tables whose foreign keys point at this table.
        let refs = self.ws.upgrade().map(|w| w.read(cx).referenced_by(&self.table)).unwrap_or_default();
        if !new_row && !refs.is_empty() {
            let mut list = h_flex().flex_wrap().gap(px(6.));
            for (i, (schema, table, col, target_col)) in refs.iter().enumerate() {
                let v = fields.iter().find(|(c, _, _)| c.name == *target_col).map(|(_, v, _)| v.clone()).unwrap_or(Value::Null);
                if v.is_null() {
                    continue;
                }
                let v = backend::cell_text(&v);
                let (ws, schema, table2, col2) = (self.ws.clone(), schema.clone(), table.clone(), col.clone());
                list = list.child(
                    h_flex()
                        .id(("rel", i))
                        .gap(px(4.))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(5.))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.panel_2)
                        .font_family(mono.clone())
                        .text_size(px(12.))
                        .cursor_pointer()
                        .hover(|s| s.border_color(p.border_strong))
                        .on_click(move |_, window, cx| {
                            let f = (col2.clone(), "=".to_string(), v.clone());
                            let _ = ws.update(cx, |ws, cx| ws.open_table(schema.clone(), table2.clone(), true, Some(f), window, cx));
                        })
                        .child(svg().path("rv/table.svg").size(px(12.)).text_color(p.muted))
                        .child(table.clone())
                        .child(div().text_color(p.muted).child(format!(".{col}"))),
                );
            }
            body = body.child(v_flex().py(px(8.)).child(div().mb(px(5.)).text_size(px(12.)).font_semibold().child("Referenced by")).child(list));
        }
        v_flex()
            .w(px(380.))
            .flex_none()
            .h_full()
            .bg(p.panel)
            .border_l_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(2.))
                    .pl(px(14.))
                    .pr(px(8.))
                    .py(px(8.))
                    .border_b_1()
                    .border_color(p.border)
                    .child(div().flex_1().text_size(px(13.)).font_bold().child(title))
                    .child(kit::icon_btn("d-prev", "left", cx).tooltip("Previous row").disabled(row == 0).on_click(cx.listener(move |this, _, _, cx| {
                        this.drawer = Some(row.saturating_sub(1));
                        cx.notify();
                    })))
                    .child(kit::icon_btn("d-next", "right", cx).tooltip("Next row").disabled(row + 1 >= n_rows).on_click(cx.listener(move |this, _, _, cx| {
                        this.drawer = Some(row + 1);
                        cx.notify();
                    })))
                    .child(kit::icon_btn("d-copy", "copy", cx).tooltip("Copy as JSON").on_click(move |_, window, cx| {
                        ui::copy(json.clone(), cx);
                        ui::toast_ok(window, cx, "Copied");
                    }))
                    .child(kit::icon_btn("d-close", "x", cx).tooltip("Close").on_click(cx.listener(|this, _, _, cx| {
                        this.drawer = None;
                        cx.notify();
                    }))),
            )
            .child(body)
            .into_any_element()
    }

    fn export(&mut self, all: bool, csv: bool, window: &mut Window, cx: &mut Context<Self>) {
        let name = format!("{}.{}", self.table.name, if csv { "csv" } else { "json" });
        if !all {
            let d = self.grid.read(cx).delegate();
            let cols: Vec<String> = d.cols.iter().map(|c| c.name.clone()).collect();
            let rows: Vec<Vec<Value>> = (d.inserted..d.rows.len()).map(|r| (0..cols.len()).map(|c| d.original(r, c)).collect()).collect();
            ui::save_file(name, export_text(&cols, &rows, csv), window, cx);
            return;
        }
        let body = self.rows_body(10_000, 0, false, cx);
        let fut = self.backend.clone().post(format!("/connections/{}/rows", backend::enc(&self.conn)), body);
        cx.spawn_in(window, async move |_, cx| {
            let res = fut.await.and_then(model::parse::<model::Rows>);
            let _ = cx.update(|window, cx| match res {
                Ok(r) => ui::save_file(name, export_text(&r.columns, &r.rows, csv), window, cx),
                Err(e) => ui::toast_err(window, cx, e.to_string()),
            });
        })
        .detach();
    }

    fn grant_write(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fut = self.backend.clone().post(format!("/connections/{}/grant-write", backend::enc(&self.conn)), json!({}));
        let ws = self.ws.clone();
        cx.spawn_in(window, async move |_, cx| {
            let res = fut.await;
            let _ = cx.update(|window, cx| match res {
                Ok(_) => {
                    ui::toast_ok(window, cx, "Write access granted");
                    let _ = ws.update(cx, |ws, cx| ws.load_schema(true, window, cx));
                }
                Err(e) => ui::toast_err(window, cx, e.to_string()),
            });
        })
        .detach();
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let nf = self.filters.iter().filter(|f| !f.column.is_empty()).count();
        let d = self.grid.read(cx).delegate();
        let (vis, total) = (d.visible().len(), d.cols.len());
        let me = cx.weak_entity();
        let count = |text: String| div().px(px(6.)).rounded(px(9.)).bg(p.accent).text_color(p.accent_text).text_size(px(11.)).font_bold().child(text);
        let cols_menu = {
            let me = me.clone();
            let items: Vec<(String, String, bool)> = d.cols.iter().map(|c| (c.name.clone(), c.type_name.clone(), !d.hidden.contains(&c.name))).collect();
            kit::btn_ghost("cols", cx)
                .icon(kit::ico("columns"))
                .label("Columns")
                .when(vis < total, |b| b.child(count(format!("{vis}/{total}"))))
                .dropdown_menu(move |mut menu, _, _| {
                    let me1 = me.clone();
                    menu = menu.item(PopupMenuItem::new("Show all").icon(crate::kit::ico("eye")).on_click(move |_, _, cx| {
                        let _ = me1.update(cx, |this, cx| this.set_hidden(HashSet::new(), cx));
                    }));
                    let me2 = me.clone();
                    let names: Vec<String> = items.iter().map(|x| x.0.clone()).collect();
                    menu = menu.item(PopupMenuItem::new("Hide all").icon(crate::kit::ico("eyeOff")).on_click(move |_, _, cx| {
                        let all: HashSet<String> = names.iter().skip(1).cloned().collect();
                        let _ = me2.update(cx, |this, cx| this.set_hidden(all, cx));
                    }));
                    menu = menu.separator();
                    for (name, ty, shown) in &items {
                        let me = me.clone();
                        let n = name.clone();
                        menu = menu.item(PopupMenuItem::new(format!("{name}  {ty}")).checked(*shown).on_click(move |_, _, cx| {
                            let _ = me.update(cx, |this, cx| this.toggle_column(&n, cx));
                        }));
                    }
                    menu
                })
        };
        let export = {
            let me = me.clone();
            kit::btn_ghost("export", cx).icon(kit::ico("download")).label("Export").dropdown_menu(move |menu, _, _| {
                let mut menu = menu;
                for (label, all, csv) in [("Page as CSV", false, true), ("Page as JSON", false, false), ("All filtered rows as CSV (max 10,000)", true, true), ("All filtered rows as JSON (max 10,000)", true, false)] {
                    let me = me.clone();
                    menu = menu.item(PopupMenuItem::new(label).icon(kit::ico("download")).on_click(move |_, window, cx| {
                        let _ = me.update(cx, |this, cx| this.export(all, csv, window, cx));
                    }));
                }
                menu
            })
        };
        let selected = self.grid.read(cx).selected_row().is_some();
        let icon = if self.table.is_view() { "view" } else { "table" };
        let left = kit::group()
            .child(
                kit::tb_title(icon, self.table.name.clone(), cx).when_some(self.block_reason.clone(), |d, r| {
                    d.child(
                        div()
                            .id("ro-badge")
                            .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(r.clone()).build(window, cx))
                            .child(kit::badge("read-only", true, cx).child(svg().path("rv/lock.svg").size(px(11.)).text_color(p.muted))),
                    )
                }),
            )
            .when(self.can_grant, |d| d.child(kit::btn_sm("grant", cx).icon(kit::ico("key")).label("Grant write access").on_click(cx.listener(|this, _, window, cx| this.grant_write(window, cx)))))
            .child(kit::seg(
                "view-seg",
                vec![("Data".into(), !self.structure), ("Structure".into(), self.structure)],
                {
                    let me = me.clone();
                    move |i, _, cx| {
                        let _ = me.update(cx, |this, cx| {
                            this.structure = i == 1;
                            this.changed(cx);
                            cx.notify();
                        });
                    }
                },
                cx,
            ));
        let right = (!self.structure).then(|| {
            kit::group()
                .child(
                    kit::btn_ghost("filters", cx)
                        .icon(kit::ico("filter"))
                        .label("Filters")
                        .when(nf > 0, |b| b.text_color(p.text).bg(p.accent_soft).child(count(nf.to_string())))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.show_filters = !this.show_filters;
                            if this.show_filters && this.filters.is_empty() {
                                this.add_filter(None, window, cx);
                            }
                            cx.notify();
                        })),
                )
                .child(cols_menu)
                .child(kit::btn_ghost("refresh", cx).icon(kit::ico("refresh")).tooltip("Refresh (reload rows)").on_click(cx.listener(|this, _, _, cx| this.load(cx))))
                .child(export)
                .when(selected && self.block_reason.is_none(), |b| b.child(kit::btn_danger("del", cx).icon(kit::ico("trash")).label("Delete").on_click(cx.listener(|this, _, _, cx| this.toggle_delete_selected(cx)))))
                .when(!self.read_only_conn() && !self.table.is_view(), |b| b.child(kit::btn_primary("add", cx).icon(kit::ico("plus")).label("Add record").on_click(cx.listener(|this, _, window, cx| this.add_record(window, cx)))))
        });
        kit::toolbar(cx).child(left).children(right)
    }

    fn set_hidden(&mut self, hidden: HashSet<String>, cx: &mut Context<Self>) {
        self.grid.update(cx, |s, cx| {
            s.delegate_mut().hidden = hidden;
            s.refresh(cx);
        });
        self.changed(cx);
        cx.notify();
    }

    fn toggle_column(&mut self, name: &str, cx: &mut Context<Self>) {
        let mut h = self.grid.read(cx).delegate().hidden.clone();
        if !h.remove(name) {
            h.insert(name.to_string());
        }
        self.set_hidden(h, cx);
    }

    fn render_filters(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let cols: Vec<String> = self.table.columns.iter().map(|c| c.name.clone()).collect();
        let rows = self.filters.iter().enumerate().map(|(i, f)| {
            let me = cx.weak_entity();
            let cols = cols.clone();
            let cur = f.column.clone();
            let col_btn = kit::select(("fcol", i), if f.column.is_empty() { "column".to_string() } else { f.column.clone() }, cx).h(px(26.)).w(px(220.)).dropdown_menu(move |mut menu, _, _| {
                for c in &cols {
                    let (me, c) = (me.clone(), c.clone());
                    menu = menu.item(PopupMenuItem::new(c.clone()).checked(c == cur).on_click(move |_, _, cx| {
                        let _ = me.update(cx, |this, cx| {
                            this.filters[i].column = c.clone();
                            cx.notify();
                        });
                    }));
                }
                menu
            });
            let me = cx.weak_entity();
            let cur_op = f.op.clone();
            let op_btn = kit::select(("fop", i), f.op.clone(), cx).h(px(26.)).w(px(120.)).dropdown_menu(move |mut menu, _, _| {
                for op in OPS {
                    let me = me.clone();
                    menu = menu.item(PopupMenuItem::new(*op).checked(*op == cur_op).on_click(move |_, _, cx| {
                        let _ = me.update(cx, |this, cx| {
                            this.filters[i].op = op.to_string();
                            cx.notify();
                        });
                    }));
                }
                menu
            });
            let no_value = f.op.starts_with("is ");
            h_flex()
                .gap(px(6.))
                .max_w(px(820.))
                .child(div().w(px(44.)).flex_none().text_right().text_size(px(12.)).font_family(kit::cx_mono(cx)).text_color(p.dim).child(if i == 0 { "where" } else { "and" }))
                .child(col_btn)
                .child(op_btn)
                .child(div().flex_1().min_w(px(140.)).when(!no_value, |d| d.child(Input::new(&f.value).bg(crate::ui::input_bg()).small().font_family(kit::cx_mono(cx)))))
                .child(kit::icon_btn(("frm", i), "x", cx).tooltip("Remove filter").on_click(cx.listener(move |this, _, _, cx| {
                    this.filters.remove(i);
                    cx.notify();
                })))
        });
        v_flex()
            .flex_none()
            .px(px(12.))
            .py(px(10.))
            .gap(px(6.))
            .bg(p.panel)
            .border_b_1()
            .border_color(p.border)
            .children(rows)
            .when(self.filters.is_empty(), |d| d.child(div().text_size(px(12.)).text_color(p.muted).child("No filters")))
            .child(
                h_flex()
                    .gap(px(6.))
                    .max_w(px(820.))
                    .pl(px(50.))
                    .child(kit::btn_ghost_sm("fadd", cx).icon(kit::ico("plus")).label("Add filter").on_click(cx.listener(|this, _, window, cx| this.add_filter(None, window, cx))))
                    .child(div().flex_1())
                    .child(kit::btn_ghost_sm("fclear", cx).label("Clear").on_click(cx.listener(|this, _, _, cx| {
                        this.filters.clear();
                        this.apply_filters(cx);
                    })))
                    .child(kit::btn_primary("fapply", cx).h(px(24.)).px(px(8.)).small().label("Apply").on_click(cx.listener(|this, _, _, cx| this.apply_filters(cx)))),
            )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let dirty = self.dirty(cx);
        let start = self.page * self.page_size;
        let range = match self.total {
            _ if self.loaded == 0 => format!("0 of {}", ui::fmt_num(self.total.unwrap_or(0))),
            Some(t) => format!("{}–{} of {}", ui::fmt_num(start + 1), ui::fmt_num(start + self.loaded as i64), ui::fmt_num(t)),
            None => format!("{}–{}", ui::fmt_num(start + 1), ui::fmt_num(start + self.loaded as i64)),
        };
        let last_page = match self.total {
            Some(t) => (self.page + 1) * self.page_size >= t,
            None => (self.loaded as i64) < self.page_size,
        };
        let me = cx.weak_entity();
        let cur = self.page_size;
        let size_btn = kit::select("psize", format!("{} / page", self.page_size), cx).h(px(26.)).small().tooltip("Rows per page").dropdown_menu(move |mut menu, _, _| {
            for n in PAGE_SIZES {
                let me = me.clone();
                menu = menu.item(PopupMenuItem::new(format!("{n} / page")).checked(*n == cur).on_click(move |_, _, cx| {
                    let _ = me.update(cx, |this, cx| {
                        this.page_size = *n;
                        this.page = 0;
                        this.load(cx);
                        this.changed(cx);
                    });
                }));
            }
            menu
        });
        let filtered = self.filters.iter().any(|f| !f.column.is_empty()) || !self.sort.is_empty() || self.table.is_view();
        let bq_note = (self.dialect == "bigquery").then(|| if filtered { " · filtered view: runs a query job (billed)" } else { " · table preview: no query, free" });
        let left = if dirty > 0 {
            h_flex()
                .gap(px(8.))
                .child(
                    h_flex()
                        .gap(px(7.))
                        .mr(px(4.))
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.text)
                        .child(div().size(px(7.)).rounded_full().bg(rgb(0xfacc15)))
                        .child(format!("{dirty} unsaved change{}", if dirty == 1 { "" } else { "s" })),
                )
                .child(kit::btn_ghost_sm("discard", cx).icon(kit::ico("undo")).label("Discard").on_click(cx.listener(|this, _, _, cx| this.discard(cx))))
                .child(
                    kit::btn_primary("save", cx)
                        .h(px(24.))
                        .px(px(8.))
                        .small()
                        .icon(kit::ico("check"))
                        .label("Save changes")
                        .child(kit::kbd("Ctrl S", cx).text_color(p.accent_text).border_color(p.accent_text.opacity(0.6)))
                        .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                )
        } else {
            h_flex()
                .text_size(px(12.))
                .text_color(p.muted)
                .when_some(self.error.clone(), |d, e| d.text_color(p.danger).child(e))
                .when(self.error.is_none(), |d| d.child(format!("{} columns", self.table.columns.len())))
                .when_some(bq_note, |d, n| d.child(n))
        };
        h_flex()
            .flex_none()
            .min_h(px(42.))
            .px(px(12.))
            .py(px(6.))
            .gap(px(12.))
            .justify_between()
            .bg(p.panel)
            .border_t_1()
            .border_color(p.border)
            .child(left)
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(size_btn)
                    .child(div().mx(px(4.)).text_size(px(12.)).text_color(p.muted).child(if self.loading { "Loading…".to_string() } else { range }))
                    .child(kit::icon_btn("prev", "left", cx).tooltip("Previous page").disabled(self.page == 0).on_click(cx.listener(|this, _, _, cx| {
                        this.page = (this.page - 1).max(0);
                        this.load(cx);
                    })))
                    .child(kit::icon_btn("next", "right", cx).tooltip("Next page").disabled(last_page).on_click(cx.listener(|this, _, _, cx| {
                        this.page += 1;
                        this.load(cx);
                    }))),
            )
    }

    fn render_structure(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let mono = kit::cx_mono(cx);
        let t = &self.table;
        // `.struct`: a header row with a strong border, rows with a light one.
        let table = |widths: &[f32], head: &[&str], rows: Vec<Vec<AnyElement>>| -> AnyElement {
            let row = |cells: Vec<AnyElement>, is_head: bool| {
                h_flex()
                    .w_full()
                    .items_start()
                    .border_b_1()
                    .border_color(if is_head { p.border_strong } else { p.border })
                    .children(cells.into_iter().enumerate().map(|(i, c)| {
                        let w = widths.get(i).copied().unwrap_or(0.);
                        div().px(px(10.)).py(px(if is_head { 6. } else { 7. })).min_w_0().map(|d| if w > 0. { d.w(px(w)).flex_none() } else { d.flex_1() }).child(c)
                    }))
            };
            v_flex()
                .w_full()
                .max_w(px(1100.))
                .child(row(head.iter().map(|h| div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(p.muted).child(h.to_string()).into_any_element()).collect(), true))
                .children(rows.into_iter().map(|r| row(r, false)))
                .into_any_element()
        };
        let section = |title: &str, n: usize, body: AnyElement| {
            v_flex()
                .mb(px(26.))
                .child(h_flex().mb(px(8.)).gap(px(4.)).text_size(px(13.)).font_bold().text_color(p.text).child(title.to_string()).child(div().font_weight(FontWeight::NORMAL).text_color(p.muted).child(n.to_string())))
                .child(if n == 0 { div().text_size(px(12.)).text_color(p.muted).child("None").into_any_element() } else { body })
        };
        let txt = |s: String, mono: Option<SharedString>, color: Hsla| {
            div().text_size(px(if mono.is_some() { 12. } else { 12.5 })).text_color(color).when_some(mono, |d, f| d.font_family(f)).child(s).into_any_element()
        };
        let link = |id: SharedString, label: String, schema: String, table: String, cx: &mut Context<Self>| {
            div()
                .id(id)
                .text_size(px(12.))
                .font_family(kit::cx_mono(cx))
                .text_color(p.t_id)
                .cursor_pointer()
                .hover(|s| s.underline())
                .on_click(cx.listener(move |this, _, window, cx| {
                    let (s, t) = (schema.clone(), table.clone());
                    let _ = this.ws.update(cx, |ws, cx| ws.open_table(s, t, false, None, window, cx));
                }))
                .child(label)
                .into_any_element()
        };
        let mut col_rows = Vec::new();
        for (i, c) in t.columns.iter().enumerate() {
            let flags = h_flex()
                .gap(px(4.))
                .flex_wrap()
                .when(c.primary, |d| d.child(svg().path("rv/key.svg").size(px(12.)).text_color(rgb(0xeab308))))
                .when(c.auto || c.generated, |d| d.child(kit::badge(if c.generated { "generated" } else { "auto" }, true, cx)))
                .into_any_element();
            let ty = v_flex()
                .child(txt(c.data_type.clone(), Some(mono.clone()), p.t_id))
                .when_some(c.enum_values.clone(), |d, e| d.child(div().text_size(px(12.)).font_family(mono.clone()).text_color(p.muted).child(e.join(" | "))))
                .into_any_element();
            let nullable = if c.nullable { txt("yes".into(), None, p.text) } else { div().text_size(px(12.5)).font_bold().text_color(p.text).child("not null").into_any_element() };
            let refs = match &c.references {
                Some(r) => link(SharedString::from(format!("ref-{i}")), format!("{}.{}", r.table, r.column), r.schema.clone(), r.table.clone(), cx),
                None => div().into_any_element(),
            };
            col_rows.push(vec![flags, txt(c.name.clone(), Some(mono.clone()), p.text), ty, nullable, txt(c.default.clone().unwrap_or_default(), Some(mono.clone()), p.muted), refs]);
        }
        let mut body = v_flex().id("structure").size_full().overflow_y_scroll().px(px(20.)).pt(px(16.)).pb(px(30.));
        body = body.child(section("Columns", t.columns.len(), table(&[90., 160., 250., 80., 260., 0.], &["", "Name", "Type", "Nullable", "Default", "References"], col_rows)));
        let idx_rows: Vec<Vec<AnyElement>> = t
            .indexes
            .iter()
            .map(|i| vec![txt(i.name.clone(), Some(mono.clone()), p.text), txt(i.columns.join(", "), Some(mono.clone()), p.text), txt(if i.unique { "yes".into() } else { String::new() }, None, p.text), txt(i.definition.clone().unwrap_or_default(), Some(mono.clone()), p.muted)])
            .collect();
        body = body.child(section("Indexes", t.indexes.len(), table(&[220., 200., 70., 0.], &["Name", "Columns", "Unique", "Definition"], idx_rows)));
        let mut fk_rows = Vec::new();
        for (i, f) in t.foreign_keys.iter().enumerate() {
            fk_rows.push(vec![txt(f.name.clone(), Some(mono.clone()), p.text), txt(f.columns.join(", "), Some(mono.clone()), p.text), link(SharedString::from(format!("fk-{i}")), format!("{}({})", f.ref_table, f.ref_columns.join(", ")), f.ref_schema.clone(), f.ref_table.clone(), cx)]);
        }
        body = body.child(section("Foreign keys", t.foreign_keys.len(), table(&[260., 200., 0.], &["Name", "Columns", "References"], fk_rows)));
        let refs = self.ws.upgrade().map(|w| w.read(cx).referenced_by(t)).unwrap_or_default();
        let mut ref_rows = Vec::new();
        for (i, (s, tb, c, target)) in refs.iter().cloned().enumerate() {
            ref_rows.push(vec![link(SharedString::from(format!("rb-{i}")), tb.clone(), s, tb, cx), txt(c, Some(mono.clone()), p.text), txt(target, Some(mono.clone()), p.text)]);
        }
        body = body.child(section("Referenced by", refs.len(), table(&[260., 200., 0.], &["Table", "Columns", "Target"], ref_rows)));
        body
    }
}

fn column_tooltip(c: &Column) -> String {
    let mut s = format!("{} {}", c.name, c.data_type);
    if !c.nullable {
        s.push_str(" not null");
    }
    if let Some(d) = &c.default {
        s.push_str(&format!(" default {d}"));
    }
    s
}

/// CSV (RFC 4180, NULL as empty) or a JSON array of objects.
pub fn export_text(cols: &[String], rows: &[Vec<Value>], csv: bool) -> String {
    if csv {
        let esc = |s: &str| if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
        let mut out = cols.iter().map(|c| esc(c)).collect::<Vec<_>>().join(",");
        out.push('\n');
        for r in rows {
            out.push_str(&r.iter().map(|v| if v.is_null() { String::new() } else { esc(&backend::cell_text(v)) }).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
        out
    } else {
        let arr: Vec<Value> = rows.iter().map(|r| Value::Object(cols.iter().cloned().zip(r.iter().cloned()).collect())).collect();
        serde_json::to_string_pretty(&arr).unwrap_or_default()
    }
}

impl Render for TableView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .key_context("TableView")
            .on_action(cx.listener(|this, _: &crate::workspace::Save, window, cx| this.save(window, cx)))
            .on_action(cx.listener(|this, _: &crate::workspace::EditCell, window, cx| {
                if let Some((r, c)) = this.grid.read(cx).selected_cell() {
                    this.edit_cell(r, c, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::workspace::CopyCell, window, cx| {
                if let Some((r, c)) = this.grid.read(cx).selected_cell() {
                    let d = this.grid.read(cx).delegate();
                    if let Some(ci) = d.vis_col(c) {
                        ui::copy(backend::cell_text(&d.value(r, ci)), cx);
                        ui::toast_ok(window, cx, "Copied to clipboard");
                    }
                }
            }))
            .child(self.render_toolbar(cx))
            .when(self.structure, |d| d.child(div().flex_1().min_h_0().child(self.render_structure(cx))))
            .when(!self.structure, |d| {
                d.when(self.show_filters, |d| d.child(self.render_filters(cx)))
                    .child(h_flex().flex_1().min_h_0().items_start().child(div().flex_1().min_w_0().h_full().child(grid::table(&self.grid))).when_some(self.drawer, |d, r| d.child(self.render_drawer(r, cx))))
                    .child(self.render_footer(cx))
            })
    }
}
