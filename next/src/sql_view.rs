//! A SQL tab: the editor, Run / Run all / Explain, results (table or JSON), history,
//! prettify and saved queries.

use crate::backend::{self, Backend};
use crate::cell_editor::CellEditor;
use crate::grid::{self, DataGrid, GridCol};
use crate::model::{self, Column, ResultSet};
use crate::sqltext;
use crate::{kit, ui};
use crate::workspace::{RunAll, RunSql, Save, Workspace};
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::table::{TableEvent, TableState};
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{json, Value};
use std::sync::Arc;

actions!(rowveil_sql, [Explain, Prettify]);

pub struct SqlView {
    backend: Arc<Backend>,
    conn: String,
    dialect: String,
    ws: WeakEntity<Workspace>,
    pub editor: Entity<EditorState>,
    pub title: Entity<InputState>,
    pub saved_id: Option<String>,
    running: bool,
    status: String,
    error: Option<String>,
    results: Vec<ResultSet>,
    shown: usize,
    grid: Option<Entity<TableState<DataGrid>>>,
    json_view: bool,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl SqlView {
    pub fn new(backend: Arc<Backend>, conn: String, dialect: String, title: String, sql: String, saved_id: Option<String>, ws: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| EditorState::new(window, cx).language("sql").line_number(true).default_value(sql).placeholder("SELECT …   Ctrl+Enter runs the statement at the cursor"));
        let title = cx.new(|cx| InputState::new(window, cx).default_value(title));
        let ws2 = ws.clone();
        let subs = vec![
            cx.subscribe(&editor, move |_, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    let _ = ws2.update(cx, |ws, cx| ws.save_state_later(cx));
                }
            }),
            cx.subscribe(&title, |this: &mut Self, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change | InputEvent::Blur) {
                    let _ = this.ws.update(cx, |ws, cx| ws.save_state_later(cx));
                }
            }),
        ];
        editor.update(cx, |e, cx| e.focus(window, cx));
        Self { backend, conn, dialect, ws, editor, title, saved_id, running: false, status: String::new(), error: None, results: Vec::new(), shown: 0, grid: None, json_view: false, focus: cx.focus_handle(), _subs: subs }
    }

    /// Autocomplete uses this schema (set again after a schema reload).
    pub fn set_schema(&mut self, schema: Option<std::rc::Rc<crate::model::Schema>>, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, _| {
            e.lsp_mut().completion_provider = schema.map(|s| std::rc::Rc::new(crate::sql_complete::SqlCompletion { schema: s }) as std::rc::Rc<dyn gpui_kit::component::input::CompletionProvider>);
        });
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    pub fn title_text(&self, cx: &App) -> String {
        self.title.read(cx).value().to_string()
    }

    /// The selection, or the statement at the cursor.
    fn current_statement(&self, cx: &App) -> String {
        let e = self.editor.read(cx);
        let sel = e.selected_range();
        let text = e.value().to_string();
        if sel.start < sel.end && sel.end <= text.len() {
            return text[sel].to_string();
        }
        sqltext::statement_at(&text, e.cursor()).map(|r| text[r].to_string()).unwrap_or_default()
    }

    fn run_current(&mut self, _: &RunSql, window: &mut Window, cx: &mut Context<Self>) {
        let sql = self.current_statement(cx);
        self.execute(sql, window, cx);
    }

    fn run_all(&mut self, _: &RunAll, window: &mut Window, cx: &mut Context<Self>) {
        let sql = self.text(cx);
        self.execute(sql, window, cx);
    }

    fn explain(&mut self, _: &Explain, window: &mut Window, cx: &mut Context<Self>) {
        let sql = self.current_statement(cx);
        if sql.trim().is_empty() {
            return;
        }
        let prefix = if self.dialect == "sqlite" { "EXPLAIN QUERY PLAN " } else { "EXPLAIN " };
        self.execute(format!("{prefix}{sql}"), window, cx);
    }

    fn prettify(&mut self, _: &Prettify, window: &mut Window, cx: &mut Context<Self>) {
        let e = self.editor.read(cx);
        let sel = e.selected_range();
        let text = e.value().to_string();
        let opts = sqlformat::FormatOptions { indent: sqlformat::Indent::Spaces(2), uppercase: Some(true), lines_between_queries: 1, ..Default::default() };
        let (range, part) = if sel.start < sel.end { (sel.clone(), text[sel].to_string()) } else { (0..text.len(), text.clone()) };
        let pretty = sqlformat::format(&part, &sqlformat::QueryParams::None, &opts);
        let new_text = format!("{}{}{}", &text[..range.start], pretty, &text[range.end..]);
        self.editor.update(cx, |s, cx| s.set_value(new_text, window, cx));
    }

    pub fn execute(&mut self, sql: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.running || sql.trim().is_empty() {
            return;
        }
        self.running = true;
        self.error = None;
        self.status = "Running…".into();
        cx.notify();
        let (ws, hist) = (self.ws.clone(), sql.clone());
        cx.defer(move |cx| {
            let _ = ws.update(cx, |ws, cx| ws.push_history(&hist, cx));
        });
        let ddl = sqltext::is_ddl(&sql);
        let fut = self.backend.clone().post(format!("/connections/{}/sql", backend::enc(&self.conn)), json!({ "sql": sql }));
        cx.spawn_in(window, async move |this, cx| {
            let res = fut.await.and_then(model::parse::<model::SqlAnswer>);
            let _ = this.update_in(cx, |this, window, cx| {
                this.running = false;
                match res {
                    Ok(a) => {
                        let ms = a.duration_ms;
                        this.results = a.results;
                        this.shown = this.results.iter().rposition(|r| !r.columns.is_empty()).unwrap_or(this.results.len().saturating_sub(1));
                        this.status = format!("{} · {:.1} ms", this.result_summary(), ms);
                        this.build_grid(window, cx);
                        if ddl {
                            let _ = this.ws.update(cx, |ws, cx| ws.load_schema(true, window, cx));
                        }
                    }
                    Err(e) => {
                        this.error = Some(e.to_string());
                        this.status.clear();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn result_summary(&self) -> String {
        match self.results.get(self.shown) {
            Some(r) if !r.columns.is_empty() => format!("{} rows{}{}", ui::fmt_num(r.rows.len() as i64), if r.truncated { " (truncated)" } else { "" }, r.info.as_ref().map(|i| format!(" · {i}")).unwrap_or_default()),
            Some(r) => format!("{} rows affected", ui::fmt_num(r.rows_affected as i64)),
            None => "Done".into(),
        }
    }

    fn build_grid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.grid = self.results.get(self.shown).filter(|r| !r.columns.is_empty()).map(|r| {
            let cols = r.columns.iter().map(|c| GridCol { name: c.name.clone(), type_name: c.type_name.to_lowercase(), kind: c.kind.clone(), ..Default::default() }).collect();
            let g = grid::new_grid(DataGrid::new(cols, r.rows.clone()), false, window, cx);
            self._subs.push(cx.subscribe_in(&g, window, |this: &mut Self, _, ev: &TableEvent, window, cx| {
                if let TableEvent::DoubleClickedCell(row, col) = ev {
                    this.view_cell(*row, *col, window, cx);
                }
            }));
            g
        });
    }

    fn view_cell(&mut self, row: usize, col: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(col) = self.grid.as_ref().and_then(|g| g.read(cx).delegate().vis_col(col)) else { return };
        let Some(r) = self.results.get(self.shown) else { return };
        let Some(c) = r.columns.get(col) else { return };
        let v = r.rows.get(row).and_then(|x| x.get(col)).cloned().unwrap_or(Value::Null);
        let column = Column { name: c.name.clone(), data_type: c.type_name.clone(), kind: c.kind.clone(), nullable: true, ..Default::default() };
        CellEditor::open(column, v, false, None, window, cx);
    }

    fn result_json(&self) -> String {
        let Some(r) = self.results.get(self.shown) else { return "[]".into() };
        let names: Vec<String> = r.columns.iter().map(|c| c.name.clone()).collect();
        crate::table_view::export_text(&names, &r.rows, false)
    }

    pub fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        let ws = self.ws.clone();
        let me = cx.weak_entity();
        let _ = ws.update(cx, |ws, cx| ws.save_query(me, false, window, cx));
    }

    fn render_results(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = ui::pal(cx);
        if let Some(e) = &self.error {
            return v_flex()
                .flex_1()
                .child(
                    h_flex()
                        .m(px(10.))
                        .mx(px(12.))
                        .px(px(12.))
                        .py(px(10.))
                        .gap(px(8.))
                        .items_start()
                        .rounded(px(6.))
                        .bg(p.danger_soft)
                        .text_color(p.danger)
                        .child(svg().path("rv/x.svg").size(px(15.)).flex_none().text_color(p.danger))
                        .child(div().flex_1().min_w_0().font_family(kit::cx_mono(cx)).text_size(px(12.)).child(e.clone())),
                )
                .into_any_element();
        }
        if self.results.is_empty() {
            return div()
                .flex_1()
                .p(px(40.))
                .flex()
                .justify_center()
                .text_size(px(13.))
                .text_color(p.muted)
                .child(if self.running { "Running…" } else { "Run a query to see results" })
                .into_any_element();
        }
        let n = self.results.len();
        let shown = self.shown;
        let picker = (n > 1).then(|| {
            let me = cx.weak_entity();
            kit::seg(
                "result-pick",
                (0..n).map(|i| (SharedString::from(format!("Result {}", i + 1)), i == shown)).collect(),
                move |i, window, cx| {
                    let _ = me.update(cx, |this, cx| {
                        this.shown = i;
                        this.build_grid(window, cx);
                        cx.notify();
                    });
                },
                cx,
            )
        });
        let has_cols = self.results.get(self.shown).is_some_and(|r| !r.columns.is_empty());
        let me = cx.weak_entity();
        let mode = kit::seg(
            "res-mode",
            vec![("▦".into(), !self.json_view), ("{ }".into(), self.json_view)],
            move |i, _, cx| {
                let _ = me.update(cx, |this, cx| {
                    this.json_view = i == 1;
                    cx.notify();
                });
            },
            cx,
        );
        let count = |text: String| div().px(px(6.)).rounded(px(9.)).bg(p.accent).text_color(p.accent_text).text_size(px(11.)).font_bold().child(text);
        let bar = h_flex()
            .flex_none()
            .min_h(px(38.))
            .px(px(12.))
            .py(px(6.))
            .gap(px(10.))
            .bg(p.panel)
            .border_b_1()
            .border_color(p.border)
            .when(has_cols, |b| b.child(mode))
            .children(picker)
            .child(div().text_size(px(12.)).text_color(p.muted).child(self.status.clone()))
            .child(div().flex_1())
            .when(has_cols, |b| {
                b.when_some(self.grid.clone().filter(|g| g.read(cx).delegate().cols.len() > 1), |b, g| {
                    let d = g.read(cx).delegate();
                    let (vis, total) = (d.visible().len(), d.cols.len());
                    let items: Vec<(String, String, bool)> = d.cols.iter().map(|c| (c.name.clone(), c.type_name.clone(), !d.hidden.contains(&c.name))).collect();
                    b.child(kit::btn_ghost_sm("res-cols", cx).icon(kit::ico("columns")).label("Columns").when(vis < total, |b| b.child(count(format!("{vis}/{total}")))).dropdown_menu(move |mut menu, _, _| {
                        let g1 = g.clone();
                        menu = menu.item(PopupMenuItem::new("Show all").icon(crate::kit::ico("eye")).on_click(move |_, _, cx| {
                            g1.update(cx, |s, cx| {
                                s.delegate_mut().hidden.clear();
                                s.refresh(cx);
                            });
                        }));
                        menu = menu.separator();
                        for (name, ty, shown) in &items {
                            let (g, n) = (g.clone(), name.clone());
                            menu = menu.item(PopupMenuItem::new(format!("{name}  {ty}")).checked(*shown).on_click(move |_, _, cx| {
                                g.update(cx, |s, cx| {
                                    let h = &mut s.delegate_mut().hidden;
                                    if !h.remove(&n) {
                                        h.insert(n.clone());
                                    }
                                    s.refresh(cx);
                                });
                            }));
                        }
                        menu
                    }))
                })
                .child(kit::btn_ghost_sm("copy-json", cx).icon(kit::ico("copy")).label("Copy").tooltip("Copy all rows as JSON").on_click(cx.listener(|this, _, window, cx| {
                    ui::copy(this.result_json(), cx);
                    ui::toast_ok(window, cx, "Copied");
                })))
                .child(kit::btn_ghost_sm("csv", cx).icon(kit::ico("download")).label("CSV").on_click(cx.listener(|this, _, window, cx| {
                    if let Some(r) = this.results.get(this.shown) {
                        let names: Vec<String> = r.columns.iter().map(|c| c.name.clone()).collect();
                        ui::save_file("result.csv".into(), crate::table_view::export_text(&names, &r.rows, true), window, cx);
                    }
                })))
                .child(kit::btn_ghost_sm("json", cx).icon(kit::ico("download")).label("JSON").on_click(cx.listener(|this, _, window, cx| {
                    ui::save_file("result.json".into(), this.result_json(), window, cx);
                })))
            });
        let body: AnyElement = match (&self.grid, self.json_view) {
            (Some(_), true) => div()
                .id("json")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(14.))
                .py(px(10.))
                .bg(p.bg)
                .text_size(px(12.))
                .line_height(px(18.6))
                .font_family(kit::cx_mono(cx))
                .text_color(p.text)
                .child(self.result_json())
                .into_any_element(),
            (Some(g), false) => div()
                .flex_1()
                .min_h_0()
                .key_context("SqlResults")
                .on_action(cx.listener(|this, _: &crate::workspace::EditCell, window, cx| {
                    if let Some((r, c)) = this.grid.as_ref().and_then(|g| g.read(cx).selected_cell()) {
                        this.view_cell(r, c, window, cx);
                    }
                }))
                .on_action(cx.listener(|this, _: &crate::workspace::CopyCell, window, cx| {
                    let Some((r, c)) = this.grid.as_ref().and_then(|g| g.read(cx).selected_cell()) else { return };
                    let Some(c) = this.grid.as_ref().and_then(|g| g.read(cx).delegate().vis_col(c)) else { return };
                    if let Some(v) = this.results.get(this.shown).and_then(|rs| rs.rows.get(r)).and_then(|row| row.get(c)) {
                        ui::copy(crate::backend::cell_text(v), cx);
                        ui::toast_ok(window, cx, "Copied to clipboard");
                    }
                }))
                .child(grid::table(g))
                .into_any_element(),
            (None, _) => div().p(px(40.)).flex().justify_center().text_size(px(13.)).text_color(p.muted).child("Query executed").into_any_element(),
        };
        v_flex().flex_1().min_h_0().child(bar).child(body).into_any_element()
    }
}

impl Render for SqlView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let ws = self.ws.clone();
        let me = cx.weak_entity();
        let has_history = self.ws.upgrade().is_some_and(|w| !w.read(cx).history(cx).is_empty());
        let history = kit::btn_ghost("history", cx).icon(kit::ico("history")).label("History").disabled(!has_history).dropdown_menu(move |mut menu, _, cx| {
            let items = ws.upgrade().map(|w| w.read(cx).history(cx)).unwrap_or_default();
            if items.is_empty() {
                return menu.item(PopupMenuItem::label("No queries yet"));
            }
            for sql in items.into_iter().take(50) {
                let me = me.clone();
                let label: String = sql.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect();
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let _ = me.update(cx, |this, cx| this.editor.update(cx, |e, cx| e.set_value(sql.clone(), window, cx)));
                }));
            }
            menu
        });
        let me = cx.weak_entity();
        let ws2 = self.ws.clone();
        let save_menu = kit::btn_ghost("save-more", cx).px(px(6.)).icon(kit::ico("chevron")).tooltip("More").dropdown_menu(move |menu, _, _| {
            let (me, ws2) = (me.clone(), ws2.clone());
            let (me2, ws3) = (me.clone(), ws2.clone());
            menu.item(PopupMenuItem::new("Save as new query…").icon(kit::ico("plus")).on_click(move |_, window, cx| {
                let _ = ws2.update(cx, |ws, cx| ws.save_query(me.clone(), true, window, cx));
            }))
            .item(PopupMenuItem::new("Rename or change scope…").icon(kit::ico("edit")).on_click(move |_, window, cx| {
                let _ = ws3.update(cx, |ws, cx| ws.rename_query_of(me2.clone(), window, cx));
            }))
        });
        let me = cx.weak_entity();
        let run_more = kit::btn_primary("run-more", cx).px(px(6.)).rounded_l(px(0.)).border_l_1().border_color(p.accent_text.opacity(0.25)).icon(kit::ico("chevron")).tooltip("More run options").disabled(self.running).dropdown_menu(move |menu, _, _| {
            let (m1, m2) = (me.clone(), me.clone());
            menu.item(PopupMenuItem::new("Run all").icon(kit::ico("play")).on_click(move |_, window, cx| {
                let _ = m1.update(cx, |this, cx| this.run_all(&RunAll, window, cx));
            }))
            .item(PopupMenuItem::new("Explain").icon(kit::ico("structure")).on_click(move |_, window, cx| {
                let _ = m2.update(cx, |this, cx| this.explain(&Explain, window, cx));
            }))
        });
        let kbd_on_accent = |t: &'static str, cx: &App| kit::kbd(t, cx).text_color(p.accent_text).border_color(p.accent_text.opacity(0.6));
        let toolbar = kit::toolbar(cx)
            .child(
                kit::group()
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .mr(px(6.))
                            .child(svg().path("rv/terminal.svg").size(px(15.)).flex_none().text_color(p.muted))
                            .child(div().w(px(160.)).text_size(px(14.)).font_semibold().child(Input::new(&self.title).bg(crate::ui::input_bg()).small().appearance(false))),
                    )
                    .child(kit::btn("pretty", cx).icon(kit::ico("wand")).label("Prettify").child(kit::kbd("Shift+Alt+F", cx)).tooltip("Format SQL (selection or all)").on_click(cx.listener(|this, _, window, cx| this.prettify(&Prettify, window, cx))))
                    .child(
                        h_flex()
                            .child(
                                kit::btn_primary("run", cx)
                                    .rounded_r(px(0.))
                                    .icon(kit::ico("play"))
                                    .label("Run")
                                    .child(kbd_on_accent("Ctrl+Enter", cx))
                                    .loading(self.running)
                                    .tooltip("Run the statement under the cursor (or the selection)")
                                    .on_click(cx.listener(|this, _, window, cx| this.run_current(&RunSql, window, cx))),
                            )
                            .child(run_more),
                    ),
            )
            .child(
                kit::group()
                    .child(
                        h_flex()
                            .child(
                                kit::btn_ghost("save", cx)
                                    .icon(kit::ico("bookmark"))
                                    .label("Save")
                                    .child(kit::kbd("Ctrl+S", cx))
                                    .tooltip(if self.saved_id.is_some() { "Update the saved query" } else { "Save the query for later" })
                                    .on_click(cx.listener(|this, _, window, cx| this.save(&Save, window, cx))),
                            )
                            .when(self.saved_id.is_some(), |d| d.child(save_menu)),
                    )
                    .child(history)
                    .child(kit::icon_btn("keys", "keyboard", cx).tooltip("Keyboard shortcuts").on_click(|_, window, cx| crate::workspace::shortcuts_dialog(window, cx))),
            );
        v_flex()
            .size_full()
            .key_context("SqlView")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::run_current))
            .on_action(cx.listener(Self::run_all))
            .on_action(cx.listener(Self::explain))
            .on_action(cx.listener(Self::prettify))
            .on_action(cx.listener(Self::save))
            .child(toolbar)
            .child(
                div().flex_1().min_h_0().child(
                    gpui_kit::component::v_resizable("sql-split")
                        .child(gpui_kit::component::resizable_panel().size(px(240.)).size_range(px(90.)..px(2000.)).child(div().size_full().bg(p.bg).py(px(10.)).child(Editor::new(&self.editor).h_full().bordered(false).font_family(kit::cx_mono(cx)).text_size(px(13.)))))
                        .child(gpui_kit::component::resizable_panel().child(v_flex().size_full().child(self.render_results(cx)))),
                ),
            )
    }
}
