//! One open connection: the sidebar (schema picker, tables, saved queries) and the tabs.

use crate::backend::{self, Backend};
use crate::conn_form::{self, ConnForm};
use crate::model::{self, Conn, SavedQuery, Schema, Table};
use crate::sql_view::SqlView;
use crate::table_view::{self, TableView};
use crate::{kit, ui, RowveilApp};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Sizable as _, StyledExt as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

actions!(rowveil_next, [RunSql, RunAll, Save, Submit, NewSqlTab, CloseTab, EditCell, CopyCell]);

pub fn bind_keys(cx: &mut App) {
    use crate::sql_view::{Explain, Prettify};
    cx.bind_keys([
        KeyBinding::new("ctrl-enter", RunSql, Some("SqlView")),
        KeyBinding::new("ctrl-shift-enter", RunAll, Some("SqlView")),
        KeyBinding::new("shift-alt-f", Prettify, Some("SqlView")),
        KeyBinding::new("ctrl-shift-e", Explain, Some("SqlView")),
        KeyBinding::new("ctrl-s", Save, Some("SqlView")),
        KeyBinding::new("ctrl-s", Save, Some("TableView")),
        KeyBinding::new("ctrl-enter", Submit, Some("CellEditor")),
        KeyBinding::new("ctrl-t", NewSqlTab, Some("Workspace")),
        KeyBinding::new("ctrl-w", CloseTab, Some("Workspace")),
        KeyBinding::new("enter", EditCell, Some("TableView > Table")),
        KeyBinding::new("f2", EditCell, Some("TableView > Table")),
        KeyBinding::new("ctrl-c", CopyCell, Some("TableView > Table")),
        KeyBinding::new("enter", EditCell, Some("SqlResults > Table")),
        KeyBinding::new("ctrl-c", CopyCell, Some("SqlResults > Table")),
    ]);
}

pub enum Tab {
    Table(Entity<TableView>),
    Sql(Entity<SqlView>),
    Schema(Entity<crate::schema_view::SchemaView>),
}

impl Tab {
    fn title(&self, cx: &App) -> String {
        match self {
            Tab::Table(v) => v.read(cx).table.name.clone(),
            Tab::Sql(v) => v.read(cx).title_text(cx),
            Tab::Schema(_) => "Schema".into(),
        }
    }
    fn dirty(&self, cx: &App) -> bool {
        matches!(self, Tab::Table(v) if v.read(cx).dirty(cx) > 0)
    }
    fn state(&self, cx: &App) -> Value {
        match self {
            Tab::Table(v) => table_view::tab_state(v.read(cx), cx),
            Tab::Sql(v) => {
                let v = v.read(cx);
                json!({ "kind": "sql", "title": v.title_text(cx), "sql": v.text(cx), "savedId": v.saved_id })
            }
            Tab::Schema(v) => crate::schema_view::tab_state(v.read(cx)),
        }
    }
}

pub struct Workspace {
    backend: Arc<Backend>,
    pub conn: Conn,
    conns: Vec<Conn>,
    app: WeakEntity<RowveilApp>,
    pub schema: Option<Schema>,
    error: Option<String>,
    schema_name: String,
    filter: Entity<InputState>,
    tabs: Vec<Tab>,
    active: usize,
    queries: Vec<SavedQuery>,
    saved_collapsed: bool,
    restore: Option<Value>,
    redis: Option<Entity<crate::redis::RedisPane>>,
    quiet: bool,
    save_task: Option<Task<()>>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl Workspace {
    pub fn new(backend: Arc<Backend>, conn_id: String, saved: Option<Value>, app: WeakEntity<RowveilApp>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search tables"));
        let subs = vec![cx.subscribe(&filter, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        })];
        let conn = Conn { id: conn_id, ..Default::default() };
        let schema_name = saved.as_ref().and_then(|s| s["schemaName"].as_str()).unwrap_or("").to_string();
        let mut this = Self {
            backend,
            conn,
            conns: Vec::new(),
            app,
            schema: None,
            error: None,
            schema_name,
            filter,
            tabs: Vec::new(),
            active: 0,
            queries: Vec::new(),
            saved_collapsed: false,
            restore: Some(saved.unwrap_or(Value::Null)),
            redis: None,
            quiet: false,
            save_task: None,
            focus: cx.focus_handle(),
            _subs: subs,
        };
        this.load_connections(cx);
        this.load_queries(cx);
        this.load_schema(false, window, cx);
        this
    }

    fn load_connections(&mut self, cx: &mut Context<Self>) {
        let fut = self.backend.clone().get("/connections");
        cx.spawn(async move |this, cx| {
            if let Ok(conns) = fut.await.and_then(model::parse::<Vec<Conn>>) {
                let _ = this.update(cx, |this, cx| {
                    if let Some(c) = conns.iter().find(|c| c.id == this.conn.id) {
                        this.conn = c.clone();
                    }
                    this.conns = conns;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn load_queries(&mut self, cx: &mut Context<Self>) {
        let fut = self.backend.clone().get("/queries");
        cx.spawn(async move |this, cx| {
            if let Ok(q) = fut.await.and_then(model::parse::<Vec<SavedQuery>>) {
                let _ = this.update(cx, |this, cx| {
                    this.queries = q;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn load_schema(&mut self, refresh: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        let path = format!("/connections/{}/schema{}", backend::enc(&self.conn.id), if refresh { "?refresh=1" } else { "" });
        let fut = self.backend.clone().get(path);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let res = fut.await.and_then(model::parse::<Schema>);
            let _ = this.update_in(cx, |this, window, cx| {
                match res {
                    Ok(s) => {
                        if !s.schemas.contains(&this.schema_name) {
                            this.schema_name = s.default_schema.clone();
                        }
                        // Tabs of tables that no longer exist are closed.
                        let gone: Vec<usize> = this
                            .tabs
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| matches!(t, Tab::Table(v) if s.table(&v.read(cx).table.schema, &v.read(cx).table.name).is_none()))
                            .map(|(i, _)| i)
                            .collect();
                        for i in gone.into_iter().rev() {
                            this.tabs.remove(i);
                        }
                        this.active = this.active.min(this.tabs.len().saturating_sub(1));
                        let shared = Rc::new(s.clone());
                        for t in &this.tabs {
                            if let Tab::Sql(v) = t {
                                let sc = Some(shared.clone());
                                v.update(cx, |v, cx| v.set_schema(sc, cx));
                            }
                        }
                        this.schema = Some(s);
                        if refresh && !this.quiet {
                            ui::toast_ok(window, cx, "Schema reloaded");
                        }
                        this.quiet = false;
                        if let Some(saved) = this.restore.take() {
                            if this.schema.as_ref().is_some_and(|s| s.dialect == "redis") {
                                this.start_redis(&saved, window, cx);
                            } else {
                                this.restore_tabs(&saved, window, cx);
                                this.dev_hooks(window, cx);
                            }
                        } else if let (Some(r), Some(s)) = (&this.redis, &this.schema) {
                            let o = s.redis.clone().unwrap_or_default();
                            r.update(cx, |p, cx| {
                                p.read_only = s.read_only;
                                p.reload(cx);
                                let _ = o;
                            });
                        }
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn dev_hooks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(v) = crate::dev_env("ROWVEIL_NEXT_SCHEMA") {
            // "tables:public.orders", "roles:postgres", "enums:public.order_status"
            let (sec, item) = v.split_once(':').unwrap_or((v.as_str(), ""));
            let sel = match item.split_once('.') {
                Some((s, n)) => json!({ "schema": s, "name": n }),
                None if !item.is_empty() => json!({ "name": item }),
                None => Value::Null,
            };
            let saved = json!({ "section": sec, "sel": sel, "schemaFilter": self.schema_name });
            let view = self.new_schema_view(Some(&saved), window, cx);
            self.tabs.push(Tab::Schema(view));
            self.active = self.tabs.len() - 1;
        }
        if let Some(t) = crate::dev_env("ROWVEIL_NEXT_TABLE") {
            let (s, n) = t.split_once('.').map(|(s, n)| (s.to_string(), n.to_string())).unwrap_or((String::new(), t));
            self.open_table(s, n, true, None, window, cx);
            if crate::dev_env("ROWVEIL_NEXT_STRUCTURE").is_some() {
                if let Some(Tab::Table(v)) = self.tabs.get(self.active) {
                    v.update(cx, |v, cx| {
                        v.structure = true;
                        cx.notify();
                    });
                }
            }
        }
        if let Some(sql) = crate::dev_env("ROWVEIL_NEXT_SQL") {
            self.open_sql(sql.clone(), None, None, window, cx);
            if let Some(Tab::Sql(v)) = self.tabs.last() {
                v.update(cx, |v, cx| v.execute(sql, window, cx));
            }
        }
    }

    fn restore_tabs(&mut self, saved: &Value, window: &mut Window, cx: &mut Context<Self>) {
        for t in saved["tabs"].as_array().into_iter().flatten() {
            match t["kind"].as_str() {
                Some("table") => {
                    let (s, n) = (t["schema"].as_str().unwrap_or("").to_string(), t["table"].as_str().unwrap_or("").to_string());
                    if let Some(table) = self.schema.as_ref().and_then(|sc| sc.table(&s, &n)).cloned() {
                        let view = self.new_table_view(table, Some(t), window, cx);
                        self.tabs.push(Tab::Table(view));
                    }
                }
                Some("schema") => {
                    let view = self.new_schema_view(Some(t), window, cx);
                    self.tabs.push(Tab::Schema(view));
                }
                Some("sql") => {
                    let view = self.new_sql_view(t["sql"].as_str().unwrap_or("").into(), t["title"].as_str().map(str::to_string), t["savedId"].as_str().map(str::to_string), window, cx);
                    self.tabs.push(Tab::Sql(view));
                }
                _ => {}
            }
        }
        self.active = (saved["active"].as_u64().unwrap_or(0) as usize).min(self.tabs.len().saturating_sub(1));
    }

    /// Saves the open tabs (debounced for typing).
    pub fn save_state_later(&mut self, cx: &mut Context<Self>) {
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(400)).await;
            let _ = this.update(cx, |this, cx| this.save_state(cx));
        }));
    }

    fn start_redis(&mut self, saved: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = &self.schema else { return };
        let (backend, conn, ws) = (self.backend.clone(), self.conn.id.clone(), cx.weak_entity());
        let (overview, ro, limit) = (s.redis.clone().unwrap_or_default(), s.read_only, self.conn.scan_limit);
        let saved = saved.clone();
        let pane = cx.new(|cx| crate::redis::RedisPane::new(backend, conn, overview, ro, limit, &saved, ws, window, cx));
        cx.observe(&pane, |_, _, cx| cx.notify()).detach();
        self.redis = Some(pane);
    }

    pub fn save_state(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = &self.redis {
            let mut state = r.read(cx).state(cx);
            state["schemaName"] = json!(self.schema_name);
            let id = self.conn.id.clone();
            let _ = self.app.update(cx, |app, _| app.save_conn_state(&id, state));
            return;
        }
        let state = json!({
            "tabs": self.tabs.iter().map(|t| t.state(cx)).collect::<Vec<_>>(),
            "active": self.active,
            "schemaName": self.schema_name,
        });
        let id = self.conn.id.clone();
        let _ = self.app.update(cx, |app, _| app.save_conn_state(&id, state));
    }

    pub fn history(&self, cx: &App) -> Vec<String> {
        self.app.upgrade().map(|a| a.read(cx).history(&self.conn.id)).unwrap_or_default()
    }

    pub fn push_history(&mut self, sql: &str, cx: &mut Context<Self>) {
        let id = self.conn.id.clone();
        let sql = sql.trim().to_string();
        let _ = self.app.update(cx, |app, _| app.push_history(&id, &sql));
    }

    fn dialect(&self) -> String {
        self.schema.as_ref().map(|s| s.dialect.clone()).unwrap_or_else(|| self.conn.dialect().to_string())
    }

    fn new_table_view(&mut self, table: Table, saved: Option<&Value>, window: &mut Window, cx: &mut Context<Self>) -> Entity<TableView> {
        let schema = self.schema.clone().unwrap_or_default();
        let (backend, conn, ws) = (self.backend.clone(), self.conn.id.clone(), cx.weak_entity());
        let view = cx.new(|cx| TableView::new(backend, conn, &schema, table, saved, ws, window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        view
    }

    fn new_sql_view(&mut self, sql: String, title: Option<String>, saved_id: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Entity<SqlView> {
        let n = self.tabs.iter().filter(|t| matches!(t, Tab::Sql(_))).count() + 1;
        let title = title.unwrap_or_else(|| if n == 1 { "SQL console".into() } else { format!("SQL console {n}") });
        let (backend, conn, dialect, ws) = (self.backend.clone(), self.conn.id.clone(), self.dialect(), cx.weak_entity());
        let view = cx.new(|cx| SqlView::new(backend, conn, dialect, title, sql, saved_id, ws, window, cx));
        let schema = self.schema.clone().map(Rc::new);
        view.update(cx, |v, cx| v.set_schema(schema, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        view
    }

    fn new_schema_view(&mut self, saved: Option<&Value>, window: &mut Window, cx: &mut Context<Self>) -> Entity<crate::schema_view::SchemaView> {
        let (backend, conn, ws) = (self.backend.clone(), self.conn.id.clone(), cx.weak_entity());
        let view = cx.new(|cx| crate::schema_view::SchemaView::new(backend, conn, saved, ws, window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        view
    }

    /// The Schema tab (one per connection).
    pub fn open_schema(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Schema(_))) {
            self.active = i;
        } else {
            let saved = json!({ "schemaFilter": self.schema_name });
            let v = self.new_schema_view(Some(&saved), window, cx);
            self.tabs.push(Tab::Schema(v));
            self.active = self.tabs.len() - 1;
        }
        self.save_state(cx);
        cx.notify();
    }

    /// Reloads the schema after a DDL change, without a toast.
    pub fn reload_schema_quietly(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quiet = true;
        self.load_schema(true, window, cx);
    }

    /// Opens a table. Without `new_tab`, a clean table tab in front is reused (preview tab).
    pub fn open_table(&mut self, schema: String, name: String, new_tab: bool, filter: Option<(String, String, String)>, window: &mut Window, cx: &mut Context<Self>) {
        if filter.is_none() {
            if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Table(v) if v.read(cx).table.schema == schema && v.read(cx).table.name == name)) {
                self.active = i;
                self.save_state(cx);
                cx.notify();
                return;
            }
        }
        let Some(table) = self.schema.as_ref().and_then(|s| s.table(&schema, &name)).cloned() else { return };
        let saved = filter.map(|(c, op, v)| json!({ "filters": [{ "column": c, "op": op, "value": v }] }));
        let reuse = !new_tab && saved.is_none() && matches!(self.tabs.get(self.active), Some(t @ Tab::Table(_)) if !t.dirty(cx));
        let view = self.new_table_view(table, saved.as_ref(), window, cx);
        if reuse {
            self.tabs[self.active] = Tab::Table(view);
        } else {
            self.tabs.push(Tab::Table(view));
            self.active = self.tabs.len() - 1;
        }
        self.save_state(cx);
        cx.notify();
    }

    pub fn open_sql(&mut self, sql: String, title: Option<String>, saved_id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let view = self.new_sql_view(sql, title, saved_id, window, cx);
        self.tabs.push(Tab::Sql(view));
        self.active = self.tabs.len() - 1;
        self.save_state(cx);
        cx.notify();
    }

    fn open_select(&mut self, t: &Table, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.table_ref(t);
        self.open_sql(format!("SELECT  FROM {name}"), None, None, window, cx);
        if let Some(Tab::Sql(v)) = self.tabs.last() {
            v.update(cx, |v, cx| {
                v.editor.update(cx, |e, cx| {
                    e.set_cursor_position(gpui_kit::component::input::Position::new(0, 7), window, cx);
                })
            });
        }
    }

    /// The table name as SQL text: schema-qualified outside the default schema, quoted when needed.
    fn table_ref(&self, t: &Table) -> String {
        let d = self.dialect();
        let q = |s: &str| -> String {
            let plain = s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && !s.starts_with(|c: char| c.is_ascii_digit());
            if plain {
                s.to_string()
            } else if d == "mysql" || d == "bigquery" {
                format!("`{}`", s.replace('`', "``"))
            } else {
                format!("\"{}\"", s.replace('"', "\"\""))
            }
        };
        let default = self.schema.as_ref().map(|s| s.default_schema.clone()).unwrap_or_default();
        if t.schema.is_empty() || t.schema == default || d == "sqlite" {
            q(&t.name)
        } else {
            format!("{}.{}", q(&t.schema), q(&t.name))
        }
    }

    fn close_tab(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(ix) else { return };
        if tab.dirty(cx) {
            let me = cx.weak_entity();
            window.open_alert_dialog(cx, move |a, _, _| {
                let me = me.clone();
                a.title("Discard unsaved changes?").description("The table has changes that are not saved.").show_cancel(true).ok_text("Discard").on_ok(move |_, _, cx| {
                    let _ = me.update(cx, |this, cx| this.remove_tab(ix, cx));
                    true
                })
            });
            return;
        }
        self.remove_tab(ix, cx);
    }

    fn remove_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        self.tabs.remove(ix);
        if self.active > ix || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.save_state(cx);
        cx.notify();
    }

    /// Single-column foreign keys of other tables that point at `t`: (schema, table, column, target column).
    pub fn referenced_by(&self, t: &Table) -> Vec<(String, String, String, String)> {
        let Some(s) = &self.schema else { return Vec::new() };
        let mut out = Vec::new();
        for other in &s.tables {
            for c in &other.columns {
                if let Some(r) = &c.references {
                    if r.schema == t.schema && r.table == t.name {
                        out.push((other.schema.clone(), other.name.clone(), c.name.clone(), r.column.clone()));
                    }
                }
            }
        }
        out
    }

    // ------------------------------------------------------------ saved queries

    fn visible_queries(&self) -> Vec<SavedQuery> {
        let d = self.dialect();
        let mut q: Vec<SavedQuery> = self.queries.iter().filter(|q| q.visible_for(&self.conn.id, &d, "sql")).cloned().collect();
        q.sort_by_key(|q| q.name.to_lowercase());
        q
    }

    fn open_saved(&mut self, q: SavedQuery, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Sql(v) if v.read(cx).saved_id.as_deref() == Some(q.id.as_str()))) {
            self.active = i;
            cx.notify();
            return;
        }
        self.open_sql(q.text, Some(q.name), Some(q.id), window, cx);
    }

    /// Ctrl+S in a SQL tab: updates the linked query, or asks for a name and scope.
    pub fn save_query(&mut self, view: WeakEntity<SqlView>, as_new: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(v) = view.upgrade() else { return };
        let (text, title, saved_id) = {
            let v = v.read(cx);
            (v.text(cx), v.title_text(cx), v.saved_id.clone())
        };
        if text.trim().is_empty() {
            return;
        }
        if let (Some(id), false) = (saved_id, as_new) {
            let fut = self.backend.clone().call(axum::http::Method::PATCH, format!("/queries/{}", backend::enc(&id)), Some(json!({ "text": text })));
            cx.spawn_in(window, async move |this, cx| {
                let res = fut.await;
                let _ = this.update_in(cx, |this, window, cx| match res {
                    Ok(_) => {
                        ui::toast_ok(window, cx, "Query saved");
                        this.load_queries(cx);
                    }
                    Err(e) => ui::toast_err(window, cx, e.to_string()),
                });
            })
            .detach();
            return;
        }
        let name = if title.starts_with("SQL console") { crate::sqltext::suggest_name(&text) } else { title };
        self.query_dialog(None, name, false, Some((view, text)), window, cx);
    }

    pub fn rename_query_of(&mut self, view: WeakEntity<SqlView>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = view.upgrade().and_then(|v| v.read(cx).saved_id.clone()) else { return };
        if let Some(q) = self.queries.iter().find(|q| q.id == id).cloned() {
            self.query_dialog(Some(q.id), q.name, q.connection_id.is_none(), None, window, cx);
        }
    }

    /// Name + scope dialog. `create`: (tab, text) for a new query; `id`: the query to update.
    fn query_dialog(&mut self, id: Option<String>, name: String, shared: bool, create: Option<(WeakEntity<SqlView>, String)>, window: &mut Window, cx: &mut Context<Self>) {
        let form = cx.new(|cx| QueryForm::new(name, shared, window, cx));
        let me = cx.weak_entity();
        let (backend, conn, dialect) = (self.backend.clone(), self.conn.id.clone(), self.dialect());
        let label: SharedString = format!("Show for all {} connections", model::dialect_label(&dialect)).into();
        let is_rename = id.is_some();
        form.update(cx, |f, _| {
            f.label = label;
            f.is_rename = is_rename;
        });
        let submit: Rc<dyn Fn(&mut Window, &mut App)> = {
            let form = form.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                let (name, shared) = {
                    let f = form.read(cx);
                    (f.name.read(cx).value().trim().to_string(), f.shared)
                };
                let conn_id = if shared { "" } else { conn.as_str() };
                let (method, path, body) = match &id {
                    Some(id) => (axum::http::Method::PATCH, format!("/queries/{}", backend::enc(id)), json!({ "name": name, "connectionId": conn_id })),
                    None => {
                        let text = create.as_ref().map(|c| c.1.clone()).unwrap_or_default();
                        (axum::http::Method::POST, "/queries".into(), json!({ "name": name, "kind": "sql", "text": text, "dialect": dialect, "connectionId": conn_id }))
                    }
                };
                let fut = backend.clone().call(method, path, Some(body));
                let (me, tab) = (me.clone(), create.as_ref().map(|c| c.0.clone()));
                window
                    .spawn(cx, async move |cx| {
                        let res = fut.await.and_then(model::parse::<SavedQuery>);
                        let _ = cx.update(|window, cx| match res {
                            Ok(q) => {
                                window.close_dialog(cx);
                                ui::toast_ok(window, cx, "Query saved");
                                if let Some(tab) = &tab {
                                    let _ = tab.update(cx, |v, cx| {
                                        v.saved_id = Some(q.id.clone());
                                        v.title.update(cx, |t, cx| t.set_value(q.name.clone(), window, cx));
                                    });
                                }
                                let _ = me.update(cx, |this, cx| {
                                    this.load_queries(cx);
                                    this.save_state(cx);
                                });
                            }
                            Err(e) => ui::toast_err(window, cx, e.to_string()),
                        });
                    })
                    .detach();
            })
        };
        form.update(cx, |f, _| f.submit = Some(submit));
        let title = if form.read(cx).is_rename { "Rename query" } else { "Save query" };
        window.open_dialog(cx, move |d, _, cx| crate::kit::dialog(d, title, 460., cx).child(form.clone()));
    }

    fn delete_query(&mut self, q: SavedQuery, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.weak_entity();
        let backend = self.backend.clone();
        window.open_alert_dialog(cx, move |a, _, _| {
            let (me, backend, id) = (me.clone(), backend.clone(), q.id.clone());
            a.title("Delete saved query").description(format!("Delete \"{}\"?", q.name)).show_cancel(true).ok_text("Delete").on_ok(move |_, window, cx| {
                let fut = backend.clone().call(axum::http::Method::DELETE, format!("/queries/{}", backend::enc(&id)), None);
                let me = me.clone();
                window
                    .spawn(cx, async move |cx| {
                        let res = fut.await;
                        let _ = cx.update(|window, cx| match res {
                            Ok(_) => {
                                let _ = me.update(cx, |this, cx| this.load_queries(cx));
                            }
                            Err(e) => ui::toast_err(window, cx, e.to_string()),
                        });
                    })
                    .detach();
                true
            })
        });
    }

    // ------------------------------------------------------------ connection menu

    fn edit_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.weak_entity();
        let old = self.conn.clone();
        ConnForm::open(
            self.backend.clone(),
            Some(self.conn.clone()),
            false,
            Rc::new(move |c, window, cx| {
                let _ = me.update(cx, |this, cx| {
                    let reconnect = c.read_only != old.read_only || c.scan_limit != old.scan_limit;
                    this.conn = c;
                    this.load_connections(cx);
                    if reconnect {
                        this.load_schema(true, window, cx);
                    }
                    cx.notify();
                });
            }),
            window,
            cx,
        );
    }

    fn go_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.app.update(cx, |app, cx| app.go_home(window, cx));
    }

    fn switch_to(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.app.update(cx, |app, cx| app.open_connection(id, window, cx));
    }

    // ------------------------------------------------------------ rendering

    pub fn render_conn_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let me = cx.weak_entity();
        let tag_filter: Vec<String> = self.app.upgrade().map(|a| a.read(cx).prefs.tag_filter.clone()).unwrap_or_default();
        let tags = crate::home::all_tags(&self.conns);
        let conns: Vec<Conn> = self.conns.iter().filter(|c| tag_filter.is_empty() || c.tags.iter().any(|t| tag_filter.contains(t))).cloned().collect();
        let current = self.conn.id.clone();
        let app = self.app.clone();
        let name = if self.conn.name.is_empty() { "…".to_string() } else { self.conn.name.clone() };
        let more = self.conn.tags.len().saturating_sub(1);
        let switcher = Button::new("conn-menu")
            .custom(gpui_kit::component::button::ButtonCustomVariant::new(cx).color(p.panel_2).foreground(p.text).hover(p.panel_2).active(p.panel_2))
            .bg(p.panel_2)
            .w_full()
            .h(px(34.))
            .px(px(9.))
            .rounded(px(6.))
            .border_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.))
                    .child(div().size(px(8.)).flex_none().rounded_full().bg(crate::home::dialect_color(self.conn.dialect())))
                    .child(div().flex_1().min_w_0().truncate().font_semibold().text_size(px(13.)).child(name))
                    .children(self.conn.tags.first().map(|t| conn_form::tag_chip(t, cx)))
                    .when(more > 0, |d| d.child(div().text_size(px(10.)).text_color(p.dim).child(format!("+{more}"))))
                    .when(self.conn.tags.is_empty(), |d| d.child(div().text_size(px(11.)).text_color(p.muted).child(model::dialect_label(self.conn.dialect()))))
                    .child(svg().path("rv/chevron.svg").size(px(14.)).flex_none().text_color(p.muted)),
            )
            .dropdown_menu(move |mut menu, _, _| {
                let m = me.clone();
                menu = menu.item(PopupMenuItem::new("All connections").icon(kit::ico("home")).on_click(move |_, window, cx| {
                    let _ = m.update(cx, |this, cx| this.go_home(window, cx));
                }));
                if !tags.is_empty() {
                    menu = menu.separator().item(PopupMenuItem::label("Show tags"));
                    let a = app.clone();
                    menu = menu.item(PopupMenuItem::new("All").checked(tag_filter.is_empty()).on_click(move |_, _, cx| {
                        let _ = a.update(cx, |a, cx| {
                            a.prefs.tag_filter.clear();
                            a.prefs.save();
                            cx.notify();
                        });
                    }));
                    for t in &tags {
                        let (a, t2) = (app.clone(), t.clone());
                        let on = tag_filter.contains(t);
                        menu = menu.item(PopupMenuItem::new(t.clone()).checked(on).on_click(move |_, _, cx| {
                            let _ = a.update(cx, |a, cx| {
                                if let Some(i) = a.prefs.tag_filter.iter().position(|x| *x == t2) {
                                    a.prefs.tag_filter.remove(i);
                                } else {
                                    a.prefs.tag_filter.push(t2.clone());
                                }
                                a.prefs.save();
                                cx.notify();
                            });
                        }));
                    }
                }
                menu = menu.separator();
                for c in &conns {
                    let (m, id) = (me.clone(), c.id.clone());
                    let label = format!("{}  ·  {}{}", c.name, model::dialect_label(c.dialect()), if c.read_only { " · read-only" } else { "" });
                    menu = menu.item(PopupMenuItem::new(label).checked(c.id == current).on_click(move |_, window, cx| {
                        let _ = m.update(cx, |this, cx| this.switch_to(id.clone(), window, cx));
                    }));
                }
                menu = menu.separator();
                let m = me.clone();
                menu = menu.item(PopupMenuItem::new("Edit this connection…").icon(kit::ico("edit")).on_click(move |_, window, cx| {
                    let _ = m.update(cx, |this, cx| this.edit_connection(window, cx));
                }));
                let m = me.clone();
                menu.item(PopupMenuItem::new("Reconnect & reload schema").icon(kit::ico("refresh")).on_click(move |_, window, cx| {
                    let _ = m.update(cx, |this, cx| this.load_schema(true, window, cx));
                }))
            });
        div().p(px(10.)).border_b_1().border_color(p.border).child(switcher)
    }

    /// Which sidebar row is active: ("table", schema.name) / ("sql", "") / ("schema", "").
    fn active_kind(&self, cx: &App) -> (&'static str, String) {
        match self.tabs.get(self.active) {
            Some(Tab::Table(v)) => {
                let t = &v.read(cx).table;
                ("table", format!("{}.{}", t.schema, t.name))
            }
            Some(Tab::Sql(_)) => ("sql", String::new()),
            Some(Tab::Schema(_)) => ("schema", String::new()),
            None => ("", String::new()),
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let q = self.filter.read(cx).value().to_lowercase();
        let (kind, active_table) = self.active_kind(cx);
        let mut list = v_flex().id("tables").flex_1().min_h_0().overflow_y_scroll().px(px(6.)).pb(px(8.));
        let saved = self.visible_queries();
        if !saved.is_empty() {
            let collapsed = self.saved_collapsed;
            list = list.child(
                kit::section(format!("Saved queries  {}", saved.len()), cx)
                    .ml(px(-6.))
                    .id("saved-head")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.saved_collapsed = !this.saved_collapsed;
                        cx.notify();
                    }))
                    .child(svg().path(if collapsed { "rv/right.svg" } else { "rv/chevron.svg" }).size(px(12.)).text_color(p.dim)),
            );
            if !collapsed {
                let current_saved = match self.tabs.get(self.active) {
                    Some(Tab::Sql(v)) => v.read(cx).saved_id.clone(),
                    _ => None,
                };
                for (i, qy) in saved.into_iter().enumerate() {
                    let (q1, q2, q3) = (qy.clone(), qy.clone(), qy.clone());
                    let me = cx.weak_entity();
                    let on = current_saved.as_deref() == Some(qy.id.as_str());
                    let scope = if qy.connection_id.is_none() { Some(kit::count("all", cx)) } else { None };
                    list = list.child(
                        kit::sb_item(SharedString::from(format!("q-{i}")), "bookmark", qy.name.clone(), scope, on, cx)
                            .on_click(cx.listener(move |this, _, window, cx| this.open_saved(q1.clone(), window, cx)))
                            .context_menu(move |menu, _, _| {
                                let (m1, m2) = (me.clone(), me.clone());
                                let (q2, q3) = (q2.clone(), q3.clone());
                                menu.item(PopupMenuItem::new("Rename or change scope…").icon(kit::ico("edit")).on_click(move |_, window, cx| {
                                    let q = q2.clone();
                                    let _ = m1.update(cx, |this, cx| this.query_dialog(Some(q.id.clone()), q.name.clone(), q.connection_id.is_none(), None, window, cx));
                                }))
                                .item(PopupMenuItem::new("Delete").icon(kit::ico("trash")).on_click(move |_, window, cx| {
                                    let q = q3.clone();
                                    let _ = m2.update(cx, |this, cx| this.delete_query(q, window, cx));
                                }))
                            }),
                    );
                }
            }
        }
        list = list.child(
            kit::section("Tables", cx)
                .ml(px(-6.))
                .child(kit::icon_btn("reload", "refresh", cx).size(px(20.)).tooltip("Refresh schema").on_click(cx.listener(|this, _, window, cx| this.load_schema(true, window, cx)))),
        );
        match (&self.schema, &self.error) {
            (_, Some(e)) => {
                list = list.child(
                    v_flex()
                        .p(px(16.))
                        .gap(px(10.))
                        .items_start()
                        .child(div().text_size(px(13.)).text_color(p.danger).child(e.clone()))
                        .child(kit::btn_sm("retry", cx).label("Retry").on_click(cx.listener(|this, _, window, cx| this.load_schema(true, window, cx)))),
                );
            }
            (None, None) => list = list.child(div().p(px(10.)).text_size(px(12.)).text_color(p.muted).child("Loading schema…")),
            (Some(s), None) => {
                let tables: Vec<Table> = s.tables.iter().filter(|t| (s.schemas.len() <= 1 || t.schema == self.schema_name) && (q.is_empty() || t.name.to_lowercase().contains(&q))).cloned().collect();
                if tables.is_empty() {
                    list = list.child(div().p(px(10.)).text_size(px(12.)).text_color(p.muted).child(if q.is_empty() { "No tables in this schema" } else { "No matches" }));
                }
                for (i, t) in tables.into_iter().enumerate() {
                    let est = t.row_estimate.filter(|n| *n >= 0).map(|n| kit::count(compact(n), cx));
                    let icon = if t.is_view() { "view" } else { "table" };
                    let on = kind == "table" && active_table == format!("{}.{}", t.schema, t.name);
                    let (t1, t2, t3) = (t.clone(), t.clone(), t.clone());
                    let me = cx.weak_entity();
                    let full = (s.dialect != "sqlite" && !t.schema.is_empty()).then(|| format!("{}.{}", t.schema, t.name));
                    list = list.child(
                        kit::sb_item(SharedString::from(format!("t-{i}")), icon, t.name.clone(), est, on, cx)
                            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                                let new_tab = e.modifiers().control || e.modifiers().platform;
                                this.open_table(t1.schema.clone(), t1.name.clone(), new_tab, None, window, cx)
                            }))
                            .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| this.open_table(t2.schema.clone(), t2.name.clone(), true, None, window, cx)))
                            .context_menu(move |menu, _, _| {
                                let (m1, m2) = (me.clone(), me.clone());
                                let (ta, tb) = (t3.clone(), t3.clone());
                                let name = t3.name.clone();
                                let mut menu = menu
                                    .item(PopupMenuItem::new("New SQL: SELECT … FROM").icon(kit::ico("terminal")).on_click(move |_, window, cx| {
                                        let _ = m1.update(cx, |this, cx| this.open_select(&ta, window, cx));
                                    }))
                                    .item(PopupMenuItem::new("Open in a new tab").icon(kit::ico("expand")).on_click(move |_, window, cx| {
                                        let _ = m2.update(cx, |this, cx| this.open_table(tb.schema.clone(), tb.name.clone(), true, None, window, cx));
                                    }))
                                    .item(PopupMenuItem::new("Copy name").icon(kit::ico("copy")).on_click(move |_, _, cx| ui::copy(name.clone(), cx)));
                                if let Some(f) = full.clone() {
                                    menu = menu.item(PopupMenuItem::new(format!("Copy {f}")).icon(kit::ico("copy")).on_click(move |_, _, cx| ui::copy(f.clone(), cx)));
                                }
                                menu
                            }),
                    );
                }
            }
        }
        let schema_picker = self.schema.as_ref().filter(|s| s.schemas.len() > 1).map(|s| {
            let word = match s.dialect.as_str() {
                "mysql" => "Database",
                "bigquery" => "Dataset",
                _ => "Schema",
            };
            let me = cx.weak_entity();
            let names = s.schemas.clone();
            let cur = self.schema_name.clone();
            h_flex()
                .gap(px(8.))
                .px(px(10.))
                .pt(px(8.))
                .child(div().text_size(px(12.)).text_color(p.muted).child(word))
                .child(div().flex_1().min_w_0().child(kit::select("schema-pick", self.schema_name.clone(), cx).w_full().dropdown_menu(move |mut menu, _, _| {
                    for n in &names {
                        let (me, n) = (me.clone(), n.clone());
                        menu = menu.item(PopupMenuItem::new(n.clone()).checked(n == cur).on_click(move |_, _, cx| {
                            let _ = me.update(cx, |this, cx| {
                                this.schema_name = n.clone();
                                this.save_state(cx);
                                cx.notify();
                            });
                        }));
                    }
                    menu
                })))
        });
        let fixed = v_flex()
            .px(px(6.))
            .child(
                kit::sb_item("sql", "terminal", "SQL console", Some(kit::kbd("+", cx).into_any_element()), kind == "sql", cx)
                    .on_click(cx.listener(|this, _, window, cx| this.open_sql(String::new(), None, None, window, cx))),
            )
            .child(kit::sb_item("schema", "schema", "Schema", None, kind == "schema", cx).on_click(cx.listener(|this, _, window, cx| this.open_schema(window, cx))));
        v_flex()
            .size_full()
            .bg(p.panel)
            .child(self.render_conn_header(cx))
            .children(schema_picker)
            .child(
                div().mx(px(10.)).mt(px(8.)).mb(px(6.)).child(
                    Input::new(&self.filter).bg(crate::ui::input_bg())
                        .small()
                        .prefix(svg().path("rv/search.svg").size(px(14.)).text_color(p.dim))
                        .bg(p.panel)
                        .cleanable(true),
                ),
            )
            .child(fixed)
            .child(list)
            .child(self.render_footer(cx))
    }

    /// `.sb-foot`: the brand and the version (opens the update dialog), the theme picker.
    pub fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let theme_name = self.app.upgrade().map(|a| a.read(cx).prefs.theme.clone()).unwrap_or_default();
        h_flex()
            .flex_none()
            .px(px(10.))
            .py(px(8.))
            .gap(px(6.))
            .border_t_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .id("version")
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, window, cx| crate::update_ui::UpdateDialog::open(this.app.clone(), window, cx)))
                    .child(img("brand/symbol.svg").size(px(16.)).flex_none())
                    .child(div().text_size(px(12.)).font_semibold().text_color(p.text).child("Rowveil"))
                    .child(div().text_size(px(12.)).text_color(p.dim).truncate().child(format!("v{}", env!("CARGO_PKG_VERSION")))),
            )
            .when_some(self.app.upgrade().and_then(|a| a.read(cx).update_available.clone()), |d, v| {
                d.child(kit::btn_primary("update", cx).h(px(22.)).px(px(6.)).small().label(format!("Update {v}")).on_click(cx.listener(|this, _, window, cx| crate::update_ui::UpdateDialog::open(this.app.clone(), window, cx))))
            })
            .child(crate::theme_button("theme", &theme_name, self.app.clone(), cx))
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        h_flex()
            .id("tabbar")
            .flex_none()
            .h(px(38.))
            .overflow_x_scroll()
            .border_b_1()
            .border_color(p.border)
            .bg(p.panel)
            .children(self.tabs.iter().enumerate().map(|(i, t)| {
                let on = i == self.active;
                let dirty = t.dirty(cx);
                let icon = match t {
                    Tab::Table(v) if v.read(cx).table.is_view() => "view",
                    Tab::Table(_) => "table",
                    Tab::Sql(v) if v.read(cx).saved_id.is_some() => "bookmark",
                    Tab::Sql(_) => "terminal",
                    Tab::Schema(_) => "schema",
                };
                let group = SharedString::from(format!("tab-{i}"));
                h_flex()
                    .id(("tab", i))
                    .group(group.clone())
                    .relative()
                    .h_full()
                    .max_w(px(220.))
                    .pl(px(12.))
                    .pr(px(8.))
                    .gap(px(7.))
                    .flex_shrink_0()
                    .text_size(px(13.))
                    .border_r_1()
                    .border_color(p.border)
                    .map(|d| if on { d.bg(p.bg).text_color(p.text) } else { d.text_color(p.muted).hover(|s| s.bg(p.hover).text_color(p.text)) })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = i;
                        this.save_state(cx);
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| this.close_tab(i, window, cx)))
                    .when(on, |d| d.child(div().absolute().top_0().left_0().right_0().h(px(2.)).bg(p.accent)))
                    .child(svg().path(SharedString::from(format!("rv/{icon}.svg"))).size(px(13.)).flex_none().text_color(if on { p.text } else { p.muted }))
                    .child(div().min_w_0().truncate().child(t.title(cx)))
                    .when(dirty, |d| d.child(div().size(px(7.)).flex_none().rounded_full().bg(rgb(0xfacc15))))
                    .child(
                        div()
                            .id(("close", i))
                            .p(px(2.))
                            .rounded(px(4.))
                            .flex_none()
                            .when(!on, |d| d.invisible().group_hover(group.clone(), |s| s.visible()))
                            .hover(|s| s.bg(p.hover))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_tab(i, window, cx);
                            }))
                            .child(svg().path("rv/x.svg").size(px(12.)).text_color(p.dim)),
                    )
            }))
            .child(
                div().flex().items_center().mx(px(6.)).child(
                    kit::icon_btn("new-tab", "plus", cx).tooltip("New SQL console").on_click(cx.listener(|this, _, window, cx| this.open_sql(String::new(), None, None, window, cx))),
                ),
            )
    }
}

impl Workspace {
    /// Sidebar | main, with a draggable border (180–480 px, saved).
    fn split(&self, side: AnyElement, main: AnyElement, cx: &mut Context<Self>) -> impl IntoElement {
        let width = self.app.upgrade().map(|a| a.read(cx).prefs.sidebar_width).filter(|w| *w >= 180.).unwrap_or(248.);
        let app = self.app.clone();
        gpui_kit::component::h_resizable("sidebar-split")
            .on_resize(move |state, _, cx| {
                if let Some(w) = state.read(cx).sizes().first().copied() {
                    let w = f32::from(w).clamp(180., 480.);
                    let _ = app.update(cx, |a, _| {
                        a.prefs.sidebar_width = w;
                        a.prefs.save();
                    });
                }
            })
            .child(gpui_kit::component::resizable_panel().size(px(width)).size_range(px(180.)..px(480.)).child(side))
            .child(gpui_kit::component::resizable_panel().child(main))
    }

    /// Table tabs with changes that are not saved.
    pub fn has_unsaved(&self, cx: &App) -> bool {
        self.tabs.iter().any(|t| t.dirty(cx))
    }
}

/// The keyboard shortcuts.
pub fn shortcuts_dialog(window: &mut Window, cx: &mut App) {
    const KEYS: &[(&str, &str)] = &[
        ("Ctrl+Enter", "Run the statement at the cursor (or the selection)"),
        ("Ctrl+Shift+Enter", "Run all statements"),
        ("Ctrl+Shift+E", "Explain the statement"),
        ("Shift+Alt+F", "Prettify SQL"),
        ("Ctrl+S", "Save the query / table changes / Redis value"),
        ("Ctrl+Space", "Suggestions"),
        ("Ctrl+T / Ctrl+W", "New SQL tab / close the tab"),
        ("Enter / F2", "Edit (or view) the selected cell"),
        ("Ctrl+C", "Copy the selected cell"),
        ("Arrows, Home, End, PgUp, PgDn", "Move in the grid"),
        ("Ctrl+Enter", "Apply in a value editor"),
        ("Esc", "Close a dialog"),
        ("↑ / ↓", "Redis console history"),
        ("Ctrl+L", "Clear the Redis console"),
        ("Ctrl+F / Ctrl+Z / Ctrl+Y", "Find, undo, redo in editors"),
    ];
    window.open_dialog(cx, |d, _, cx| {
        let muted = cx.theme().muted_foreground;
        d.title("Keyboard shortcuts").w(px(560.)).child(v_flex().gap_1().children(KEYS.iter().map(|(k, v)| h_flex().gap_3().text_sm().child(div().w(px(220.)).font_family("monospace").child(k.to_string())).child(div().text_color(muted).child(v.to_string())))))
    });
}

fn compact(n: i64) -> String {
    match n {
        n if n >= 1_000_000_000 => format!("{:.1}B", n as f64 / 1e9),
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.1}k", n as f64 / 1e3),
        n if n < 0 => String::new(),
        n => n.to_string(),
    }
    .replace(".0", "")
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let prod = self.conn.tags.iter().any(|t| t.contains("PROD"));
        window.set_window_title(&format!("{}{} — Rowveil Next", if prod { "[PROD] " } else { "" }, self.conn.name));
        if self.conn.dialect() == "redis" || self.schema.as_ref().is_some_and(|s| s.dialect == "redis") {
            let (side, main): (AnyElement, AnyElement) = match (&self.redis, &self.error) {
                (Some(r), _) => (r.update(cx, |p, cx| p.render_sidebar(cx).into_any_element()), r.clone().into_any_element()),
                (None, Some(e)) => (div().p_3().text_sm().text_color(theme.danger).child(e.clone()).into_any_element(), div().into_any_element()),
                (None, None) => (div().p_3().child(ui::muted("Connecting…", cx)).into_any_element(), div().into_any_element()),
            };
            return h_flex()
                .size_full()
                .bg(theme.background)
                .text_color(theme.foreground)
                .when(prod, |d| d.border_t_2().border_color(theme.danger))
                .child(self.split(v_flex().size_full().bg(ui::pal(cx).panel).child(self.render_conn_header(cx)).child(side).child(self.render_footer(cx)).into_any_element(), main, cx))
                .into_any_element();
        }
        let body: AnyElement = match self.tabs.get(self.active) {
            None => {
                let p = ui::pal(cx);
                let n = self.schema.as_ref().map(|s| s.tables.len()).unwrap_or(0);
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .p(px(24.))
                    .child(svg().path("rv/db.svg").size(px(40.)).text_color(p.dim))
                    .child(div().mt(px(6.)).text_size(px(18.)).font_bold().text_color(p.text).child(self.conn.name.clone()))
                    .child(div().text_size(px(13.)).text_color(p.text).child(format!("{n} tables and views · {}", model::dialect_label(&self.dialect()))))
                    .child(div().text_size(px(13.)).text_color(p.muted).child("Select a table in the sidebar or open the SQL console."))
                    .child(kit::btn("open-sql", cx).mt(px(10.)).icon(kit::ico("terminal")).label("Open SQL console").on_click(cx.listener(|this, _, window, cx| this.open_sql(String::new(), None, None, window, cx))))
                    .into_any_element()
            }
            Some(Tab::Table(v)) => v.clone().into_any_element(),
            Some(Tab::Sql(v)) => v.clone().into_any_element(),
            Some(Tab::Schema(v)) => v.clone().into_any_element(),
        };
        h_flex()
            .size_full()
            .key_context("Workspace")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &NewSqlTab, window, cx| this.open_sql(String::new(), None, None, window, cx)))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                let i = this.active;
                this.close_tab(i, window, cx)
            }))
            .bg(theme.background)
            .text_color(theme.foreground)
            .when(prod, |d| d.border_t_2().border_color(theme.danger))
            .child(self.split(self.render_sidebar(cx).into_any_element(), v_flex().size_full().child(self.render_tabs(cx)).child(div().flex_1().min_h_0().child(body)).into_any_element(), cx))
            .into_any_element()
    }
}

/// The "Save query" dialog body: name and scope.
pub struct QueryForm {
    name: Entity<InputState>,
    shared: bool,
    is_rename: bool,
    label: SharedString,
    submit: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    _sub: Subscription,
}

impl QueryForm {
    fn new(name: String, shared: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        input.update(cx, |s, cx| s.focus(window, cx));
        let sub = cx.subscribe_in(&input, window, |this: &mut Self, _, ev: &InputEvent, window, cx| {
            if let (InputEvent::PressEnter { .. }, Some(f)) = (ev, this.submit.clone()) {
                f(window, cx);
            }
        });
        Self { name: input, shared, is_rename: false, label: "".into(), submit: None, _sub: sub }
    }
}

impl Render for QueryForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let submit = self.submit.clone();
        v_flex()
            .gap_3()
            .child(Input::new(&self.name).bg(crate::ui::input_bg()))
            .child(Checkbox::new("shared").checked(self.shared).label(self.label.clone()).on_click(cx.listener(|this, c: &bool, _, cx| {
                this.shared = *c;
                cx.notify();
            })))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1())
                    .child(kit::btn("cancel", cx).label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(kit::btn_primary("ok", cx).icon(kit::ico("bookmark")).label("Save").on_click(move |_, window, cx| {
                        if let Some(f) = &submit {
                            f(window, cx);
                        }
                    })),
            )
    }
}
