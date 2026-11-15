//! Redis: the key browser (database, glob filter, type filter, tree, chunked SCAN),
//! key tabs (one view per type, edits through /redis/edit) and the redis-cli style console.

use crate::backend::{self, Backend};
use crate::cell_editor::pretty_json;
use crate::forms::{self, Field, FormDialog};
use crate::grid::{self, DataGrid, GridCol, GridHooks};
use crate::{kit, ui};
use crate::workspace::{Save, Workspace};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::table::{TableEvent, TableState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const SCAN_STEP: u64 = 1000;
const PAGE: i64 = 200;
const TYPES: &[&str] = &["string", "hash", "list", "set", "zset", "stream", "ReJSON-RL"];

#[derive(Clone, Debug)]
pub struct KeyItem {
    pub key: String,
    pub kind: String,
    pub ttl: i64,
    pub exact: bool,
}

pub fn type_label(t: &str) -> &str {
    match t {
        "ReJSON-RL" | "json" => "json",
        "string" => "str",
        "stream" => "strm",
        other => other,
    }
}

/// The type names of the stable UI's filter menu.
fn type_name(t: &str) -> &str {
    match t {
        "string" => "String",
        "hash" => "Hash",
        "list" => "List",
        "set" => "Set",
        "zset" => "Sorted set",
        "stream" => "Stream",
        "ReJSON-RL" | "json" => "JSON",
        other => other,
    }
}

/// `.rtype` colors.
fn type_color(t: &str) -> Hsla {
    match t {
        "string" => rgb(0x4f46e5).into(),
        "hash" => rgb(0x0d9488).into(),
        "list" => rgb(0xb45309).into(),
        "set" => rgb(0x7c3aed).into(),
        "zset" => rgb(0xbe185d).into(),
        "stream" => rgb(0x0369a1).into(),
        "ReJSON-RL" | "json" => rgb(0x15803d).into(),
        _ => rgb(0x6b7493).into(),
    }
}

/// `.rtype`: a solid badge with the upper-case type label (STR, HASH, …).
fn type_badge(t: &str) -> Div {
    div()
        .flex_none()
        .min_w(px(36.))
        .px(px(4.))
        .rounded(px(4.))
        .text_center()
        .text_size(px(9.5))
        .line_height(px(16.))
        .font_weight(FontWeight::SEMIBOLD)
        .font_family("Cascadia Code")
        .text_color(gpui_kit::white())
        .bg(type_color(t))
        .child(type_label(t).to_uppercase())
}

/// A filter text becomes a glob: plain text matches anywhere in the key.
pub fn glob(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        "*".into()
    } else if t.contains(['*', '?', '[']) {
        t.into()
    } else {
        format!("*{t}*")
    }
}

/// Formats a typed reply like redis-cli.
pub fn format_reply(r: &Value, indent: &str) -> String {
    let v = &r["v"];
    match r["t"].as_str().unwrap_or("") {
        "nil" => "(nil)".into(),
        "int" => format!("(integer) {v}"),
        "double" => format!("(double) {v}"),
        "bool" => if v.as_bool() == Some(true) { "(true)".into() } else { "(false)".into() },
        "status" => v.as_str().unwrap_or("").into(),
        "error" => format!("(error) {}", v.as_str().unwrap_or("")),
        "string" => serde_json::to_string(v).unwrap_or_default(),
        t @ ("array" | "set") => {
            let a = v.as_array().cloned().unwrap_or_default();
            if a.is_empty() {
                return if t == "set" { "(empty set)".into() } else { "(empty array)".into() };
            }
            let w = a.len().to_string().len();
            a.iter()
                .enumerate()
                .map(|(i, x)| {
                    let prefix = format!("{:>w$}) ", i + 1);
                    let inner = format_reply(x, &format!("{indent}{}", " ".repeat(prefix.len())));
                    format!("{}{prefix}{inner}", if i > 0 { indent } else { "" })
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        "map" => {
            let a = v.as_array().cloned().unwrap_or_default();
            if a.is_empty() {
                return "(empty hash)".into();
            }
            let w = a.len().to_string().len();
            a.iter()
                .enumerate()
                .map(|(i, kv)| {
                    let prefix = format!("{:>w$}# ", i + 1);
                    let k = format_reply(&kv[0], "");
                    let val = format_reply(&kv[1], &format!("{indent}{}", " ".repeat(prefix.len() + 4)));
                    format!("{}{prefix}{k} => {val}", if i > 0 { indent } else { "" })
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => serde_json::to_string(v).unwrap_or_default(),
    }
}

pub enum RTab {
    Key(Entity<KeyView>),
    Console(Entity<ConsoleView>),
}

pub struct RedisPane {
    backend: Arc<Backend>,
    conn: String,
    pub read_only: bool,
    ws: WeakEntity<Workspace>,
    overview: Value,
    db: i64,
    pattern: Entity<InputState>,
    type_filter: Option<String>,
    tree: bool,
    open: HashSet<String>,
    collapsed: HashSet<String>,
    keys: Vec<KeyItem>,
    cursor: String,
    done: bool,
    scanned: u64,
    loading: bool,
    error: Option<String>,
    generation: u64,
    scan_limit: u64,
    pub tabs: Vec<RTab>,
    pub active: usize,
    pub history: Vec<String>,
    pub saved: Vec<crate::model::SavedQuery>,
    debounce: Option<Task<()>>,
    scan_task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl RedisPane {
    pub fn new(backend: Arc<Backend>, conn: String, overview: Value, read_only: bool, scan_limit: u32, saved: &Value, ws: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let r = &saved["redis"];
        let db = r["db"].as_i64().unwrap_or_else(|| overview["defaultDb"].as_i64().unwrap_or(0));
        let pattern = cx.new(|cx| InputState::new(window, cx).placeholder("Filter keys (glob: user:*)").default_value(r["pattern"].as_str().unwrap_or("").to_string()));
        let subs = vec![cx.subscribe(&pattern, |this: &mut Self, _, ev: &InputEvent, cx| match ev {
            InputEvent::Change => {
                this.debounce = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                    let _ = this.update(cx, |this, cx| {
                        this.scan(true, cx);
                        this.changed(cx);
                    });
                }));
            }
            InputEvent::PressEnter { .. } => {
                this.debounce = None;
                this.scan(true, cx);
            }
            _ => {}
        })];
        let mut this = Self {
            backend,
            conn,
            read_only,
            ws,
            overview,
            db,
            pattern,
            type_filter: r["type"].as_str().map(str::to_string),
            tree: r["tree"].as_bool().unwrap_or(true),
            open: r["open"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            collapsed: HashSet::new(),
            keys: Vec::new(),
            cursor: "0".into(),
            done: false,
            scanned: 0,
            loading: false,
            error: None,
            generation: 0,
            scan_limit: scan_limit.max(100) as u64,
            tabs: Vec::new(),
            active: 0,
            history: saved["redisHistory"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            saved: Vec::new(),
            debounce: None,
            scan_task: None,
            _subs: subs,
        };
        for t in saved["tabs"].as_array().into_iter().flatten() {
            match t["kind"].as_str() {
                Some("rkey") => {
                    if let Some(k) = t["key"].as_str() {
                        let db = t["db"].as_i64().unwrap_or(this.db);
                        this.open_key_tab(k.to_string(), db, true, window, cx);
                    }
                }
                Some("rcli") => this.open_console(window, cx),
                _ => {}
            }
        }
        this.active = (saved["active"].as_u64().unwrap_or(0) as usize).min(this.tabs.len().saturating_sub(1));
        if let Some(d) = crate::dev_env("ROWVEIL_NEXT_DB").and_then(|v| v.parse::<i64>().ok()) {
            this.db = d;
            this.open.insert("demo:".into());
            this.scan(true, cx);
        }
        if let Some(k) = crate::dev_env("ROWVEIL_NEXT_KEY") {
            this.open_key_tab(k, this.db, true, window, cx);
        }
        if let Some(c) = crate::dev_env("ROWVEIL_NEXT_REDIS_CMD") {
            this.open_console(window, cx);
            if let Some(RTab::Console(v)) = this.tabs.last() {
                let v = v.clone();
                window.defer(cx, move |window, cx| {
                    v.update(cx, |v, cx| {
                        v.input.update(cx, |s, cx| s.set_value(c, window, cx));
                        v.run(window, cx);
                    });
                });
            }
        }
        this.scan(true, cx);
        this.load_saved(cx);
        this
    }

    /// Saved Redis commands of this connection (and the shared ones).
    pub fn load_saved(&mut self, cx: &mut Context<Self>) {
        let fut = self.backend.clone().get("/queries");
        cx.spawn(async move |this, cx| {
            if let Ok(q) = fut.await.and_then(crate::model::parse::<Vec<crate::model::SavedQuery>>) {
                let _ = this.update(cx, |this, cx| {
                    let conn = this.conn.clone();
                    this.saved = q.into_iter().filter(|q| q.visible_for(&conn, "redis", "redis")).collect();
                    this.saved.sort_by_key(|q| q.name.to_lowercase());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Saves a command (Ctrl+S in the console): asks for a name and the scope.
    pub fn save_command(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            return;
        }
        let (backend, conn, me) = (self.backend.clone(), self.conn.clone(), cx.weak_entity());
        let name: String = text.lines().next().unwrap_or("").chars().take(60).collect();
        FormDialog::open(
            "Save command",
            vec![Field::line("Name", name), forms::Field::check("Show for all Redis connections", false)],
            "Save",
            Rc::new(move |v, window, cx| {
                let n = v[0].trim().to_string();
                if n.is_empty() {
                    return Err("Enter a name".into());
                }
                let cid = if v[1] == "true" { String::new() } else { conn.clone() };
                let fut = backend.clone().post("/queries", json!({ "name": n, "kind": "redis", "text": text, "dialect": "redis", "connectionId": cid }));
                let me = me.clone();
                window
                    .spawn(cx, async move |cx| {
                        let res = fut.await;
                        let _ = cx.update(|window, cx| match res {
                            Ok(_) => {
                                ui::toast_ok(window, cx, "Command saved");
                                let _ = me.update(cx, |p, cx| p.load_saved(cx));
                            }
                            Err(e) => ui::toast_err(window, cx, e.to_string()),
                        });
                    })
                    .detach();
                Ok(())
            }),
            window,
            cx,
        );
    }

    /// State saved with the workspace: browser settings and open tabs.
    pub fn state(&self, cx: &App) -> Value {
        let mut open: Vec<&String> = self.open.iter().collect();
        open.truncate(200);
        json!({
            "redis": { "db": self.db, "pattern": self.pattern.read(cx).value().to_string(), "type": self.type_filter, "tree": self.tree, "open": open },
            "tabs": self.tabs.iter().map(|t| match t {
                RTab::Key(v) => { let v = v.read(cx); json!({ "kind": "rkey", "key": v.key, "db": v.db }) }
                RTab::Console(_) => json!({ "kind": "rcli" }),
            }).collect::<Vec<_>>(),
            "active": self.active,
            "redisHistory": self.history.iter().take(300).collect::<Vec<_>>(),
        })
    }

    /// Saves the state later (deferred: the workspace may be updating this pane right now).
    pub fn changed(&self, cx: &mut Context<Self>) {
        let ws = self.ws.clone();
        cx.defer(move |cx| {
            let _ = ws.update(cx, |ws, cx| ws.save_state_later(cx));
        });
    }

    fn key_count(&self, db: i64) -> i64 {
        self.overview["keyspace"].as_array().into_iter().flatten().find(|k| k["db"].as_i64() == Some(db)).and_then(|k| k["keys"].as_i64()).unwrap_or(0)
    }

    fn filtering(&self, cx: &App) -> bool {
        !self.pattern.read(cx).value().trim().is_empty() || self.type_filter.is_some()
    }

    /// A plain search text (no "*") scans the whole keyspace in one pass.
    fn full_search(&self, cx: &App) -> bool {
        let t = self.pattern.read(cx).value().trim().to_string();
        !t.is_empty() && !t.contains('*')
    }

    pub fn refresh_overview(&mut self, cx: &mut Context<Self>) {
        let fut = self.backend.clone().get(format!("/connections/{}/redis/overview", backend::enc(&self.conn)));
        cx.spawn(async move |this, cx| {
            if let Ok(o) = fut.await {
                let _ = this.update(cx, |this, cx| {
                    this.overview = o;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// SCAN in steps of 1000 keys. One pass walks `scan_limit` keys (a plain search: all keys).
    /// `reset` starts over; "Next" (reset = false) is ignored while a scan runs.
    pub fn scan(&mut self, reset: bool, cx: &mut Context<Self>) {
        if !reset && self.loading {
            return;
        }
        if reset {
            self.generation += 1;
            self.keys.clear();
            self.cursor = "0".into();
            self.done = false;
            self.error = None;
            self.scanned = 0;
            self.collapsed.clear();
        }
        let generation = self.generation;
        self.loading = true;
        cx.notify();
        let text = self.pattern.read(cx).value().trim().to_string();
        let pattern = glob(&text);
        let limit = if self.full_search(cx) { u64::MAX } else { self.scan_limit };
        let (backend, conn, db, ty) = (self.backend.clone(), self.conn.clone(), self.db, self.type_filter.clone());
        let exact_text = (!text.is_empty() && !text.contains(['*', '?', '['])).then(|| text.clone());
        self.scan_task = Some(cx.spawn(async move |this, cx| {
            let mut walked = 0u64;
            loop {
                let Ok(cursor) = this.read_with(cx, |this, _| this.cursor.clone()) else { return };
                let step = SCAN_STEP.min(limit - walked);
                let body = json!({
                    "db": db, "pattern": pattern, "cursor": cursor, "count": step, "type": ty, "single": true,
                    "exact": if cursor == "0" { exact_text.clone() } else { None },
                });
                let res = backend.clone().post(format!("/connections/{}/redis/scan", backend::enc(&conn)), body).await;
                walked += step;
                let more = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return false;
                    }
                    match res {
                        Ok(v) => {
                            let seen: HashSet<String> = this.keys.iter().map(|k| k.key.clone()).collect();
                            for k in v["keys"].as_array().into_iter().flatten() {
                                let key = k["key"].as_str().unwrap_or("").to_string();
                                if !seen.contains(&key) {
                                    this.keys.push(KeyItem { key, kind: k["type"].as_str().unwrap_or("").into(), ttl: k["ttl"].as_i64().unwrap_or(-1), exact: k["exact"].as_bool().unwrap_or(false) });
                                }
                            }
                            this.keys.sort_by(|a, b| b.exact.cmp(&a.exact).then(a.key.cmp(&b.key)));
                            this.cursor = v["cursor"].as_str().unwrap_or("0").to_string();
                            this.done = this.cursor == "0";
                            this.scanned += step;
                            cx.notify();
                            !this.done && walked < limit
                        }
                        Err(e) => {
                            this.error = Some(e.to_string());
                            false
                        }
                    }
                });
                if !more.unwrap_or(false) {
                    break;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.loading = false;
                    cx.notify();
                }
            });
        }));
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.refresh_overview(cx);
        self.scan(true, cx);
    }

    fn set_db(&mut self, db: i64, cx: &mut Context<Self>) {
        self.db = db;
        self.open.clear();
        self.scan(true, cx);
        self.changed(cx);
    }

    pub fn open_key_tab(&mut self, key: String, db: i64, new_tab: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, RTab::Key(v) if v.read(cx).key == key && v.read(cx).db == db)) {
            self.active = i;
            self.changed(cx);
            cx.notify();
            return;
        }
        let (backend, conn, ro, pane) = (self.backend.clone(), self.conn.clone(), self.read_only, cx.weak_entity());
        let view = cx.new(|cx| KeyView::new(backend, conn, db, key, ro, pane, window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        // Preview behaviour: a plain click replaces the key tab in front.
        if !new_tab && matches!(self.tabs.get(self.active), Some(RTab::Key(_))) {
            self.tabs[self.active] = RTab::Key(view);
        } else {
            self.tabs.push(RTab::Key(view));
            self.active = self.tabs.len() - 1;
        }
        self.changed(cx);
        cx.notify();
    }

    pub fn open_console(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (backend, conn, ro, pane, db) = (self.backend.clone(), self.conn.clone(), self.read_only, cx.weak_entity(), self.db);
        let view = cx.new(|cx| ConsoleView::new(backend, conn, db, ro, pane, window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        self.tabs.push(RTab::Console(view));
        self.active = self.tabs.len() - 1;
        self.changed(cx);
        cx.notify();
    }

    pub fn close_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.tabs.len() {
            self.tabs.remove(ix);
            if self.active > ix || self.active >= self.tabs.len() {
                self.active = self.active.saturating_sub(1);
            }
            self.changed(cx);
            cx.notify();
        }
    }

    /// Removes a deleted / renamed key from the list without a new scan.
    pub fn forget_key(&mut self, key: &str, cx: &mut Context<Self>) {
        self.keys.retain(|k| k.key != key);
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, RTab::Key(v) if v.read(cx).key == key)) {
            self.close_tab(i, cx);
        }
        cx.notify();
    }

    fn create_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut types = vec!["string", "hash", "list", "set", "zset", "stream"];
        let has_json = self.overview["modules"].as_array().into_iter().flatten().any(|m| m.as_str().or(m["name"].as_str()).is_some_and(|n| n.to_lowercase().contains("json")));
        if has_json {
            types.push("json");
        }
        let fields = vec![
            Field::line("Key", ""),
            Field::choice("Type", &types, "string"),
            Field::line("TTL, seconds", "").hint("Empty: no expiry"),
            Field::line("Field / score", "").hint("hash: the field; zset: the score; stream: field=value pairs separated by commas"),
            Field::multi("Value", ""),
        ];
        let (backend, conn, db, me) = (self.backend.clone(), self.conn.clone(), self.db, cx.weak_entity());
        FormDialog::open(
            "Add key",
            fields,
            "Create",
            Rc::new(move |v, window, cx| {
                let key = v[0].trim().to_string();
                if key.is_empty() {
                    return Err("Enter the key".into());
                }
                let t = v[1].clone();
                let mut args = Map::new();
                args.insert("type".into(), json!(t));
                if !v[2].trim().is_empty() {
                    args.insert("ttl".into(), json!(v[2].trim()));
                }
                match t.as_str() {
                    "hash" => {
                        args.insert("field".into(), json!(v[3]));
                    }
                    "zset" => {
                        args.insert("score".into(), json!(v[3].trim()));
                    }
                    "stream" => {
                        let pairs: Vec<Value> = v[3].split(',').filter_map(|p| p.split_once('=')).map(|(a, b)| json!([a.trim(), b.trim()])).collect();
                        if pairs.is_empty() {
                            return Err("Stream: enter field=value pairs".into());
                        }
                        args.insert("fields".into(), json!(pairs));
                    }
                    _ => {}
                }
                if t != "stream" {
                    args.insert("value".into(), json!(v[4]));
                }
                let fut = backend.clone().post(format!("/connections/{}/redis/edit", backend::enc(&conn)), json!({ "db": db, "key": key, "op": "create", "args": args }));
                let me = me.clone();
                window
                    .spawn(cx, async move |cx| {
                        let res = fut.await;
                        let _ = cx.update(|window, cx| match res {
                            Ok(_) => {
                                ui::toast_ok(window, cx, "Key created");
                                let _ = me.update(cx, |this, cx| {
                                    this.reload(cx);
                                    this.open_key_tab(key.clone(), db, true, window, cx);
                                });
                            }
                            Err(e) => ui::toast_err(window, cx, e.to_string()),
                        });
                    })
                    .detach();
                Ok(())
            }),
            window,
            cx,
        );
    }

    fn scanned_text(&self) -> String {
        let total = self.key_count(self.db) as u64;
        let s = if total > 0 { self.scanned.min(total) } else { self.scanned };
        if total > 0 {
            format!("~{} of {} keys scanned", ui::fmt_num(s as i64), ui::fmt_num(total as i64))
        } else {
            format!("~{} keys scanned", ui::fmt_num(s as i64))
        }
    }

    fn count_text(&self, cx: &App) -> String {
        if !self.filtering(cx) {
            return ui::fmt_num(self.key_count(self.db));
        }
        let n = ui::fmt_num(self.keys.len() as i64);
        if self.done {
            format!("{n} found")
        } else if self.loading {
            format!("Scanning… {n} found")
        } else {
            format!("{n}+ found")
        }
    }

    fn key_row(&self, k: &KeyItem, label: String, depth: usize, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = ui::pal(cx);
        let (key, key2, db) = (k.key.clone(), k.key.clone(), self.db);
        let active = matches!(self.tabs.get(self.active), Some(RTab::Key(v)) if v.read(cx).key == k.key);
        h_flex()
            .id(("k", ix))
            .w_full()
            .pl(px(8. + depth as f32 * 12.))
            .pr(px(8.))
            .py(px(5.))
            .gap(px(8.))
            .rounded(px(5.))
            .font_family(kit::cx_mono(cx))
            .text_size(px(12.))
            .line_height(px(17.))
            .cursor_pointer()
            .map(|d| if active { d.bg(p.accent_soft).text_color(p.text) } else { d.text_color(p.muted).hover(|s| s.bg(p.hover).text_color(p.text)) })
            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                let new_tab = e.modifiers().control || e.modifiers().platform;
                this.open_key_tab(key.clone(), db, new_tab, window, cx)
            }))
            .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| this.open_key_tab(key2.clone(), db, true, window, cx)))
            .child(type_badge(&k.kind))
            .child(div().flex_1().min_w_0().truncate().when(k.exact, |d| d.font_semibold()).child(label))
            .when(k.ttl > 0, |d| d.child(svg().path("rv/history.svg").size(px(11.)).flex_none().text_color(rgb(0xf59e0b))))
            .into_any_element()
    }

    fn render_keys(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = ui::pal(cx);
        let mut list = v_flex().id("keys").flex_1().min_h_0().overflow_y_scroll().px(px(6.)).pb(px(8.));
        if let Some(e) = &self.error {
            return list.child(div().p(px(10.)).text_size(px(12.)).text_color(p.danger).child(e.clone()));
        }
        if self.keys.is_empty() && self.loading {
            return list.child(div().p(px(10.)).text_size(px(12.)).text_color(p.muted).child(if self.filtering(cx) { "Searching…" } else { "Scanning…" }));
        }
        if self.keys.is_empty() {
            let msg = if self.done {
                if self.filtering(cx) { "No matching keys" } else { "This database is empty" }
            } else {
                "No matching keys in this part. Click \"Next\" to scan further."
            };
            list = list.child(div().p(px(10.)).text_size(px(12.)).text_color(p.muted).child(msg));
        }
        let mut ix = 0;
        let exact: Vec<KeyItem> = self.keys.iter().filter(|k| k.exact).cloned().collect();
        for k in &exact {
            list = list.child(self.key_row(k, k.key.clone(), 0, ix, cx));
            ix += 1;
        }
        let rest: Vec<KeyItem> = self.keys.iter().filter(|k| !k.exact).cloned().collect();
        if self.tree {
            let filtering = !self.pattern.read(cx).value().trim().is_empty();
            list = self.render_tree(list, &rest, "", 0, filtering, &mut ix, cx);
        } else {
            for k in &rest {
                list = list.child(self.key_row(k, k.key.clone(), 0, ix, cx));
                ix += 1;
            }
        }
        if !self.done {
            let full = self.full_search(cx);
            list = list.child(
                v_flex()
                    .items_center()
                    .pt(px(6.))
                    .gap(px(6.))
                    .child(div().text_size(px(12.)).text_color(p.muted).child(self.scanned_text()))
                    .when(!(self.loading && full), |d| {
                        d.child(
                            kit::btn_ghost_sm("next", cx)
                                .loading(self.loading)
                                .disabled(self.loading)
                                .label(if self.loading { "Scanning…".to_string() } else if full { "Continue the search".into() } else { format!("Next {}", ui::fmt_num(self.scan_limit as i64)) })
                                .on_click(cx.listener(|this, _, _, cx| this.scan(false, cx))),
                        )
                    }),
            );
        }
        list
    }

    /// Folders by ":"; a folder with one key shows the key directly.
    #[allow(clippy::too_many_arguments)]
    fn render_tree(&self, mut list: Stateful<Div>, keys: &[KeyItem], prefix: &str, depth: usize, filtering: bool, ix: &mut usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut folders: BTreeMap<String, Vec<KeyItem>> = BTreeMap::new();
        let mut leaves: Vec<(String, KeyItem)> = Vec::new();
        for k in keys {
            let rest = &k.key[prefix.len()..];
            match rest.find(':') {
                Some(i) => folders.entry(rest[..i].to_string()).or_default().push(k.clone()),
                None => leaves.push((rest.to_string(), k.clone())),
            }
        }
        // One alphabetical list of folders and keys.
        let mut items: Vec<(String, Option<Vec<KeyItem>>, Option<KeyItem>)> = Vec::new();
        for (name, ks) in folders {
            if ks.len() == 1 {
                let k = ks.into_iter().next().unwrap();
                items.push((k.key[prefix.len()..].to_string(), None, Some(k)));
            } else {
                items.push((name, Some(ks), None));
            }
        }
        for (label, k) in leaves {
            items.push((label, None, Some(k)));
        }
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let p = ui::pal(cx);
        for (name, folder, key) in items {
            if let Some(k) = key {
                list = list.child(self.key_row(&k, name, depth, *ix, cx));
                *ix += 1;
                continue;
            }
            let ks = folder.unwrap();
            let path = format!("{prefix}{name}:");
            let is_open = if filtering { !self.collapsed.contains(&path) } else { self.open.contains(&path) };
            let p2 = path.clone();
            list = list.child(
                h_flex()
                    .id(("f", *ix))
                    .w_full()
                    .pl(px(8. + depth as f32 * 12.))
                    .pr(px(8.))
                    .py(px(5.))
                    .gap(px(8.))
                    .rounded(px(5.))
                    .font_family(kit::cx_mono(cx))
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(|s| s.bg(p.hover).text_color(p.text))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let set = if filtering { &mut this.collapsed } else { &mut this.open };
                        if !set.remove(&p2) {
                            set.insert(p2.clone());
                        }
                        this.changed(cx);
                        cx.notify();
                    }))
                    .child(svg().path(if is_open { "rv/chevron.svg" } else { "rv/right.svg" }).size(px(12.)).flex_none().text_color(p.dim))
                    .child(svg().path("rv/folder.svg").size(px(13.)).flex_none().text_color(p.muted))
                    .child(div().flex_1().min_w_0().truncate().font_semibold().child(name))
                    .child(div().flex_none().text_size(px(11.)).text_color(p.dim).child(compact_count(ks.len()))),
            );
            *ix += 1;
            if is_open {
                list = self.render_tree(list, &ks, &path, depth + 1, filtering, ix, cx);
            }
        }
        list
    }

    pub fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let me = cx.weak_entity();
        let n = self.overview["databases"].as_i64().unwrap_or(16).max(1);
        let counts: Vec<(i64, i64)> = (0..n).map(|d| (d, self.key_count(d))).collect();
        let cur_db = self.db;
        let db_btn = kit::select("db", format!("db{} · {} keys", self.db, ui::fmt_num(self.key_count(self.db))), cx).w_full().dropdown_menu(move |mut menu, _, _| {
            for (d, c) in &counts {
                let (me, d) = (me.clone(), *d);
                menu = menu.item(PopupMenuItem::new(format!("db{d} · {} keys", ui::fmt_num(*c))).checked(d == cur_db).on_click(move |_, _, cx| {
                    let _ = me.update(cx, |this, cx| this.set_db(d, cx));
                }));
            }
            menu
        });
        let me = cx.weak_entity();
        let cur_type = self.type_filter.clone();
        let type_btn = Button::new("type")
            .custom(gpui_kit::component::button::ButtonCustomVariant::new(cx).color(p.panel).foreground(p.muted).hover(p.hover).active(p.hover))
            .bg(p.panel)
            .border_1()
            .border_color(p.border)
            .rounded(px(4.))
            .h(px(22.))
            .px(px(6.))
            .text_size(px(11.))
            .label(self.type_filter.as_deref().map(type_name).unwrap_or("All").to_string())
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                let m = me.clone();
                menu = menu.item(PopupMenuItem::new("All").checked(cur_type.is_none()).on_click(move |_, _, cx| {
                    let _ = m.update(cx, |this, cx| {
                        this.type_filter = None;
                        this.scan(true, cx);
                        this.changed(cx);
                    });
                }));
                for t in TYPES {
                    let m = me.clone();
                    menu = menu.item(PopupMenuItem::new(type_name(t)).checked(cur_type.as_deref() == Some(*t)).on_click(move |_, _, cx| {
                        let _ = m.update(cx, |this, cx| {
                            this.type_filter = Some(t.to_string());
                            this.scan(true, cx);
                            this.changed(cx);
                        });
                    }));
                }
                menu
            });
        let console_on = matches!(self.tabs.get(self.active), Some(RTab::Console(_)));
        v_flex()
            .flex_1()
            .min_h_0()
            .child(h_flex().gap(px(8.)).px(px(10.)).pt(px(8.)).child(div().text_size(px(12.)).text_color(p.muted).child("Database")).child(div().flex_1().min_w_0().child(db_btn)))
            .child(div().mx(px(10.)).mt(px(8.)).mb(px(6.)).child(Input::new(&self.pattern).bg(crate::ui::input_bg()).small().prefix(svg().path("rv/search.svg").size(px(14.)).text_color(p.dim)).suffix(type_btn)))
            .child(
                v_flex()
                    .px(px(6.))
                    .child(kit::sb_item("console", "terminal", "Redis console", Some(kit::kbd("+", cx).into_any_element()), console_on, cx).on_click(cx.listener(|this, _, window, cx| this.open_console(window, cx))))
                    .when(!self.read_only, |d| d.child(kit::sb_item("addkey", "plus", "Add key", None, false, cx).on_click(cx.listener(|this, _, window, cx| this.create_key(window, cx))))),
            )
            .child(
                kit::section("Keys", cx)
                    .child(div().ml(px(6.)).flex_1().text_color(p.dim).child(self.count_text(cx)))
                    .child(kit::icon_btn("tree", if self.tree { "folder" } else { "list" }, cx).size(px(20.)).tooltip(if self.tree { "Show as a list" } else { "Group by \":\"" }).on_click(cx.listener(|this, _, _, cx| {
                        this.tree = !this.tree;
                        this.changed(cx);
                        cx.notify();
                    })))
                    .child(kit::icon_btn("reload", "refresh", cx).size(px(20.)).tooltip("Reload keys").on_click(cx.listener(|this, _, _, cx| this.reload(cx)))),
            )
            .child(self.render_keys(cx))
    }

    pub fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let s = &self.overview["server"];
        let get = |k: &str| s[k].as_str().map(str::to_string).unwrap_or_else(|| s[k].to_string());
        let version = s["valkey_version"].as_str().map(|v| format!("Valkey {v}")).or(s["redis_version"].as_str().map(|v| format!("Redis {v}"))).unwrap_or("?".into());
        let modules: Vec<String> = self.overview["modules"].as_array().into_iter().flatten().filter_map(|m| m.as_str().or(m["name"].as_str()).map(str::to_string)).collect();
        let up = s["uptime_in_seconds"].as_str().and_then(|v| v.parse::<i64>().ok()).map(|v| fmt_ttl(v * 1000)).unwrap_or_else(|| get("uptime_in_seconds"));
        let keys: Vec<String> = self.overview["keyspace"].as_array().into_iter().flatten().map(|k| format!("db{}: {}", k["db"].as_i64().unwrap_or(0), ui::fmt_num(k["keys"].as_i64().unwrap_or(0)))).collect();
        let mut rows = vec![
            ("Version", version),
            ("Mode", format!("{} · {}", get("redis_mode"), get("role"))),
            ("Memory", get("used_memory_human")),
            ("Clients", get("connected_clients")),
            ("Uptime", up),
        ];
        if !modules.is_empty() {
            rows.push(("Modules", modules.join(", ")));
        }
        if !keys.is_empty() {
            rows.push(("Keys", keys.join(", ")));
        }
        let name = self.ws.upgrade().map(|w| w.read(cx).conn.name.clone()).unwrap_or_else(|| "Redis".into());
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .p(px(24.))
            .child(svg().path("rv/db.svg").size(px(36.)).text_color(p.dim))
            .child(div().mt(px(6.)).text_size(px(18.)).font_bold().text_color(p.text).child(name))
            .child(
                v_flex().my(px(8.)).gap(px(6.)).children(rows.into_iter().map(|(k, v)| {
                    h_flex().gap(px(12.)).text_size(px(12.5)).child(div().w(px(110.)).text_right().text_color(p.dim).child(k)).child(div().w(px(330.)).text_color(p.text).child(v))
                })),
            )
            .child(div().text_size(px(13.)).text_color(p.muted).child("Select a key in the sidebar or open the console."))
            .child(
                h_flex()
                    .mt(px(10.))
                    .gap(px(8.))
                    .child(kit::btn("open-console", cx).icon(kit::ico("terminal")).label("Open console").on_click(cx.listener(|this, _, window, cx| this.open_console(window, cx))))
                    .when(!self.read_only, |d| d.child(kit::btn("add-key", cx).icon(kit::ico("plus")).label("Add key").on_click(cx.listener(|this, _, window, cx| this.create_key(window, cx))))),
            )
    }

    pub fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        h_flex()
            .id("rtabs")
            .flex_none()
            .h(px(38.))
            .overflow_x_scroll()
            .border_b_1()
            .border_color(p.border)
            .bg(p.panel)
            .children(self.tabs.iter().enumerate().map(|(i, t)| {
                let on = i == self.active;
                let (icon, title) = match t {
                    RTab::Key(v) => ("key", v.read(cx).key.clone()),
                    RTab::Console(_) => ("terminal", format!("Console {}", i + 1)),
                };
                let group = SharedString::from(format!("rtab-{i}"));
                h_flex()
                    .id(("rtab", i))
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
                        this.changed(cx);
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, _, cx| this.close_tab(i, cx)))
                    .when(on, |d| d.child(div().absolute().top_0().left_0().right_0().h(px(2.)).bg(p.accent)))
                    .child(svg().path(SharedString::from(format!("rv/{icon}.svg"))).size(px(13.)).flex_none().text_color(if on { p.text } else { p.muted }))
                    .child(div().min_w_0().truncate().child(title))
                    .child(
                        div()
                            .id(("rclose", i))
                            .p(px(2.))
                            .rounded(px(4.))
                            .flex_none()
                            .when(!on, |d| d.invisible().group_hover(group.clone(), |s| s.visible()))
                            .hover(|s| s.bg(p.hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_tab(i, cx);
                            }))
                            .child(svg().path("rv/x.svg").size(px(12.)).text_color(p.dim)),
                    )
            }))
            .child(div().flex().items_center().mx(px(6.)).child(kit::icon_btn("rnew", "plus", cx).tooltip("New Redis console").on_click(cx.listener(|this, _, window, cx| this.open_console(window, cx)))))
    }

    pub fn push_history(&mut self, cmd: &str, cx: &mut Context<Self>) {
        self.history.retain(|c| c != cmd);
        self.history.insert(0, cmd.to_string());
        self.history.truncate(300);
        self.changed(cx);
    }
}

impl Render for RedisPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: AnyElement = match self.tabs.get(self.active) {
            None => self.render_empty(cx).into_any_element(),
            Some(RTab::Key(v)) => v.clone().into_any_element(),
            Some(RTab::Console(v)) => v.clone().into_any_element(),
        };
        v_flex().size_full().child(self.render_tabs(cx)).child(div().flex_1().min_h_0().child(body))
    }
}

// ================================================================ key view

pub struct KeyView {
    backend: Arc<Backend>,
    conn: String,
    pub db: i64,
    pub key: String,
    read_only: bool,
    pane: WeakEntity<RedisPane>,
    data: Option<Value>,
    items: Vec<Value>,
    loading: bool,
    error: Option<String>,
    filter: Entity<InputState>,
    desc: bool,
    editor: Option<Entity<EditorState>>,
    original: String,
    grid: Option<Entity<TableState<DataGrid>>>,
    _subs: Vec<Subscription>,
}

impl KeyView {
    #[allow(clippy::too_many_arguments)]
    fn new(backend: Arc<Backend>, conn: String, db: i64, key: String, read_only: bool, pane: WeakEntity<RedisPane>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter (glob)"));
        let subs = vec![cx.subscribe_in(&filter, window, |this: &mut Self, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.load(false, window, cx);
            }
        })];
        let mut this = Self { backend, conn, db, key, read_only, pane, data: None, items: Vec::new(), loading: false, error: None, filter, desc: false, editor: None, original: String::new(), grid: None, _subs: subs };
        this.load(false, window, cx);
        this
    }

    pub fn kind(&self) -> &str {
        self.data.as_ref().and_then(|d| d["type"].as_str()).unwrap_or("")
    }

    fn path(&self, p: &str) -> String {
        format!("/connections/{}/redis/{p}", backend::enc(&self.conn))
    }

    pub fn load(&mut self, more: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.loading = true;
        cx.notify();
        let mut body = json!({ "db": self.db, "key": self.key, "limit": PAGE, "filter": glob_or_empty(&self.filter.read(cx).value()), "desc": self.desc });
        if more {
            if let Some(d) = &self.data {
                match d["type"].as_str() {
                    Some("list") | Some("zset") => body["offset"] = json!(self.items.len()),
                    _ => body["cursor"] = d["cursor"].clone(),
                }
            }
        }
        let fut = self.backend.clone().post(self.path("key"), body);
        cx.spawn_in(window, async move |this, cx| {
            let res = fut.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match res {
                    Ok(d) => {
                        let new_items = d["items"].as_array().cloned().unwrap_or_default();
                        if more {
                            this.items.extend(new_items);
                        } else {
                            this.items = new_items;
                        }
                        this.error = None;
                        this.build(&d, more, window, cx);
                        this.data = Some(d);
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Builds the editor (string / json) or the grid (collections) for the loaded value.
    fn build(&mut self, d: &Value, more: bool, window: &mut Window, cx: &mut Context<Self>) {
        let t = d["type"].as_str().unwrap_or("");
        match t {
            "string" | "json" => {
                let text = d["value"].as_str().map(str::to_string).unwrap_or_else(|| d["value"].to_string());
                let is_json = t == "json" || grid::looks_like_json(&text);
                let shown = if is_json { pretty_json(&text).unwrap_or(text.clone()) } else { text.clone() };
                // Keep the same editor after a save, so folds and the scroll position stay.
                if let Some(e) = &self.editor {
                    if e.read(cx).value().as_ref() == shown.as_str() {
                        self.original = shown;
                        return;
                    }
                }
                let editor = cx.new(|cx| {
                    let s = EditorState::new(window, cx).line_number(is_json).default_value(shown.clone());
                    if is_json { s.language("json").folding(true) } else { s }
                });
                self._subs.push(cx.subscribe(&editor, |_, _, ev: &InputEvent, cx| {
                    if matches!(ev, InputEvent::Change) {
                        cx.notify();
                    }
                }));
                self.editor = Some(editor);
                self.original = shown;
            }
            "hash" | "list" | "set" | "zset" | "stream" => {
                let (cols, rows) = self.grid_rows(t, d);
                if more {
                    if let Some(g) = &self.grid {
                        g.update(cx, |s, cx| {
                            s.delegate_mut().rows = rows;
                            s.refresh(cx);
                        });
                        return;
                    }
                }
                let cols = cols.into_iter().map(|c| GridCol { name: c.to_string(), kind: "text".into(), ..Default::default() }).collect();
                let mut dg = DataGrid::new(cols, rows);
                let me = cx.weak_entity();
                let ro = self.read_only;
                dg.hooks = GridHooks {
                    menu: Some(Rc::new(move |mut menu, row, _, _, _| {
                        if ro {
                            return menu;
                        }
                        let (m1, m2) = (me.clone(), me.clone());
                        menu = menu.item(PopupMenuItem::new("Edit…").icon(crate::kit::ico("edit")).on_click(move |_, window, cx| {
                            let _ = m1.update(cx, |this, cx| this.edit_item(Some(row), window, cx));
                        }));
                        menu.item(PopupMenuItem::new("Delete").icon(crate::kit::ico("trash")).on_click(move |_, window, cx| {
                            let _ = m2.update(cx, |this, cx| this.delete_item(row, window, cx));
                        }))
                    })),
                    ..Default::default()
                };
                let g = grid::new_grid(dg, false, window, cx);
                self._subs.push(cx.subscribe_in(&g, window, |this: &mut Self, _, ev: &TableEvent, window, cx| {
                    if let TableEvent::DoubleClickedCell(row, _) | TableEvent::DoubleClickedRow(row) = ev {
                        if !this.read_only {
                            this.edit_item(Some(*row), window, cx);
                        }
                    }
                }));
                self.grid = Some(g);
            }
            _ => {}
        }
    }

    fn grid_rows(&self, t: &str, d: &Value) -> (Vec<&'static str>, Vec<Vec<Value>>) {
        let offset = d["offset"].as_i64().unwrap_or(0);
        let _ = offset;
        match t {
            "hash" => (vec!["Field", "Value"], self.items.iter().map(|p| vec![p[0].clone(), p[1].clone()]).collect()),
            "list" => (vec!["Index", "Value"], self.items.iter().enumerate().map(|(i, v)| vec![json!(i), v.clone()]).collect()),
            "set" => (vec!["Member"], self.items.iter().map(|v| vec![v.clone()]).collect()),
            "zset" => (vec!["#", "Member", "Score"], self.items.iter().enumerate().map(|(i, p)| vec![json!(i + 1), p[0].clone(), p[1].clone()]).collect()),
            "stream" => (
                vec!["ID", "Time (UTC)", "Fields"],
                self.items
                    .iter()
                    .map(|e| {
                        let id = e["id"].as_str().unwrap_or("").to_string();
                        let ms: i64 = id.split('-').next().and_then(|s| s.parse().ok()).unwrap_or(0);
                        vec![json!(id), json!(fmt_time(ms)), e["fields"].clone()]
                    })
                    .collect(),
            ),
            _ => (vec![], vec![]),
        }
    }

    pub fn edit(&mut self, op: &str, args: Value, reload: bool, window: &mut Window, cx: &mut Context<Self>) {
        let fut = self.backend.clone().post(self.path("edit"), json!({ "db": self.db, "key": self.key, "op": op, "args": args }));
        let op = op.to_string();
        let key = self.key.clone();
        cx.spawn_in(window, async move |this, cx| {
            let res = fut.await;
            let _ = this.update_in(cx, |this, window, cx| match res {
                Ok(_) => {
                    match op.as_str() {
                        "delete" => {
                            ui::toast_ok(window, cx, "Key deleted");
                            let _ = this.pane.update(cx, |p, cx| {
                                p.forget_key(&key, cx);
                                p.refresh_overview(cx);
                            });
                            return;
                        }
                        "rename" => {
                            ui::toast_ok(window, cx, "Key renamed");
                            let _ = this.pane.update(cx, |p, cx| p.reload(cx));
                        }
                        _ => ui::toast_ok(window, cx, "Saved"),
                    }
                    if reload {
                        this.load(false, window, cx);
                    }
                }
                Err(e) => ui::toast_err(window, cx, e.to_string()),
            });
        })
        .detach();
    }

    fn save_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = &self.editor else { return };
        let text = e.read(cx).value().to_string();
        let op = if self.kind() == "json" { "json_set" } else { "set" };
        // A JSON value that was compact in Redis stays compact.
        let raw = self.data.as_ref().and_then(|d| d["value"].as_str()).unwrap_or("");
        let value = if !raw.contains('\n') && grid::looks_like_json(&text) { serde_json::from_str::<Value>(&text).map(|v| v.to_string()).unwrap_or(text) } else { text };
        self.edit(op, json!({ "value": value }), true, window, cx);
    }

    fn save_action(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_some() && !self.read_only {
            self.save_value(window, cx);
        }
    }

    /// The add / edit form for one collection item (`row` None adds).
    fn edit_item(&mut self, row: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let t = self.kind().to_string();
        let item = row.and_then(|r| self.items.get(r).cloned());
        let me = cx.weak_entity();
        let text = |v: &Value| backend::cell_text(v).replace("NULL", "");
        let (title, fields): (String, Vec<Field>) = match t.as_str() {
            "hash" => ("Field".into(), vec![Field::line("Field", item.as_ref().map(|p| text(&p[0])).unwrap_or_default()), Field::multi("Value", item.as_ref().map(|p| text(&p[1])).unwrap_or_default())]),
            "list" if row.is_none() => ("Push value".into(), vec![Field::choice("Where", &["Tail", "Head"], "Tail"), Field::multi("Value", "")]),
            "list" => ("List item".into(), vec![Field::multi("Value", item.as_ref().map(text).unwrap_or_default())]),
            "set" => ("Member".into(), vec![Field::multi("Member", item.as_ref().map(text).unwrap_or_default())]),
            "zset" => ("Member".into(), vec![Field::multi("Member", item.as_ref().map(|p| text(&p[0])).unwrap_or_default()), Field::line("Score", item.as_ref().map(|p| text(&p[1])).unwrap_or_else(|| "0".into()))]),
            "stream" => ("Add entry".into(), vec![Field::line("ID", "*").hint("* = generated by Redis"), Field::multi("Fields", "").hint("One field=value per line")]),
            _ => return,
        };
        if t == "stream" && row.is_some() {
            return;
        }
        let title = if row.is_some() { format!("Edit {}", title.to_lowercase()) } else { title };
        FormDialog::open(
            title,
            fields,
            "Save",
            Rc::new(move |v, window, cx| {
                let (op, args) = match t.as_str() {
                    "hash" => {
                        let mut a = json!({ "field": v[0], "value": v[1] });
                        if let Some(p) = &item {
                            a["oldField"] = p[0].clone();
                        }
                        ("hset", a)
                    }
                    "list" if row.is_none() => (if v[0] == "Head" { "lpush" } else { "rpush" }, json!({ "value": v[1] })),
                    "list" => ("lset", json!({ "index": row.unwrap().to_string(), "value": v[0] })),
                    "set" => {
                        let mut a = json!({ "value": v[0] });
                        if let Some(old) = &item {
                            a["oldValue"] = old.clone();
                        }
                        ("sadd", a)
                    }
                    "zset" => {
                        if v[1].trim().parse::<f64>().is_err() && !matches!(v[1].trim(), "inf" | "+inf" | "-inf") {
                            return Err("Score: enter a number".into());
                        }
                        let mut a = json!({ "value": v[0], "score": v[1].trim() });
                        if let Some(p) = &item {
                            a["oldValue"] = p[0].clone();
                        }
                        ("zadd", a)
                    }
                    "stream" => {
                        let pairs: Vec<Value> = v[1].lines().filter_map(|l| l.split_once('=')).map(|(a, b)| json!([a.trim(), b.trim()])).collect();
                        if pairs.is_empty() {
                            return Err("Enter at least one field=value line".into());
                        }
                        ("xadd", json!({ "id": if v[0].trim().is_empty() { "*" } else { v[0].trim() }, "fields": pairs }))
                    }
                    _ => return Ok(()),
                };
                let _ = me.update(cx, |this, cx| this.edit(op, args, true, window, cx));
                Ok(())
            }),
            window,
            cx,
        );
    }

    fn delete_item(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(row).cloned() else { return };
        let (op, args) = match self.kind() {
            "hash" => ("hdel", json!({ "field": item[0] })),
            "list" => ("lrem_index", json!({ "index": row.to_string(), "value": item })),
            "set" => ("srem", json!({ "value": item })),
            "zset" => ("zrem", json!({ "value": item[0] })),
            "stream" => ("xdel", json!({ "id": item["id"] })),
            _ => return,
        };
        self.edit(op, args, true, window, cx);
    }

    fn ttl_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ttl = self.data.as_ref().and_then(|d| d["ttl"].as_i64()).filter(|t| *t > 0).map(|t| ((t + 999) / 1000).to_string()).unwrap_or_default();
        let me = cx.weak_entity();
        FormDialog::open(
            "Time to live",
            vec![Field::line("TTL, seconds", ttl).hint("Empty: no expiry (PERSIST)")],
            "Save",
            Rc::new(move |v, window, cx| {
                let t = v[0].trim().to_string();
                if !t.is_empty() && t.parse::<u64>().is_err() {
                    return Err("Enter a number of seconds".into());
                }
                let _ = me.update(cx, |this, cx| this.edit("expire", json!({ "ttl": t }), true, window, cx));
                Ok(())
            }),
            window,
            cx,
        );
    }

    fn rename_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.weak_entity();
        let key = self.key.clone();
        FormDialog::open(
            "Rename key",
            vec![Field::line("New name", key.clone()).hint("RENAMENX: fails when the new name exists")],
            "Rename",
            Rc::new(move |v, window, cx| {
                let n = v[0].trim().to_string();
                if n.is_empty() || n == key {
                    return Err("Enter a new name".into());
                }
                let _ = me.update(cx, |this, cx| {
                    this.edit("rename", json!({ "newKey": n }), false, window, cx);
                    this.key = n.clone();
                    this.load(false, window, cx);
                });
                Ok(())
            }),
            window,
            cx,
        );
    }

    fn delete_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.weak_entity();
        forms::confirm(
            "Delete key",
            format!("Delete \"{}\"?", self.key),
            "Delete",
            Rc::new(move |window, cx| {
                let _ = me.update(cx, |this, cx| this.edit("delete", json!({}), false, window, cx));
            }),
            window,
            cx,
        );
    }

    fn render_header(&self, d: &Value, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let ttl = d["ttl"].as_i64().unwrap_or(-1);
        let len = d["length"].as_i64();
        let mem = d["memory"].as_i64();
        let key = self.key.clone();
        let chip = |s: String| h_flex().flex_none().h(px(22.)).px(px(8.)).gap(px(4.)).rounded(px(11.)).border_1().border_color(p.border).bg(p.panel_2).text_size(px(11.5)).text_color(p.muted).child(s);
        let len_label = |n: i64| {
            let n = ui::fmt_num(n);
            match self.kind() {
                "string" => format!("{n} bytes"),
                "hash" => format!("{n} fields"),
                "list" | "set" | "zset" => format!("{n} items"),
                "stream" => format!("{n} entries"),
                _ => n,
            }
        };
        let warn = ttl > 0;
        let amber: Hsla = rgb(0xf59e0b).into();
        h_flex()
            .flex_none()
            .flex_wrap()
            .px(px(12.))
            .py(px(8.))
            .gap(px(8.))
            .bg(p.panel)
            .border_b_1()
            .border_color(p.border)
            .child(type_badge(self.kind()))
            .child(div().max_w(relative(0.5)).min_w_0().truncate().font_family(kit::cx_mono(cx)).text_size(px(13.)).font_semibold().text_color(p.text).child(self.key.clone()))
            .child(kit::icon_btn("copy", "copy", cx).tooltip("Copy key name").on_click(move |_, window, cx| {
                ui::copy(key.clone(), cx);
                ui::toast_ok(window, cx, "Copied");
            }))
            .when(!self.read_only, |d| d.child(kit::icon_btn("rename", "edit", cx).tooltip("Rename").on_click(cx.listener(|this, _, window, cx| this.rename_form(window, cx)))))
            .child(div().flex_1())
            .child(
                Button::new("ttl")
                    .custom(gpui_kit::component::button::ButtonCustomVariant::new(cx).color(p.panel_2).foreground(if warn { amber } else { p.muted }).hover(p.panel_2).active(p.panel_2))
                    .bg(p.panel_2)
                    .h(px(22.))
                    .px(px(8.))
                    .rounded(px(11.))
                    .border_1()
                    .border_color(if warn { amber.opacity(0.45) } else { p.border })
                    .text_size(px(11.5))
                    .icon(kit::ico("history"))
                    .label(format!("TTL: {}", fmt_ttl(ttl)))
                    .tooltip(if self.read_only { "Time to live" } else { "Change the time to live" })
                    .disabled(self.read_only)
                    .on_click(cx.listener(|this, _, window, cx| this.ttl_form(window, cx))),
            )
            .children(len.map(|l| chip(len_label(l))))
            .children(mem.map(|m| chip(fmt_bytes(m))))
            .children(d["encoding"].as_str().map(|e| chip(e.to_string())))
            .child(kit::icon_btn("reload", "refresh", cx).loading(self.loading).tooltip("Reload").on_click(cx.listener(|this, _, window, cx| this.load(false, window, cx))))
            .when(!self.read_only, |d| d.child(kit::act("delete", "trash", "Delete key", true, cx).size(px(26.)).on_click(cx.listener(|this, _, window, cx| this.delete_key(window, cx)))))
    }

    fn render_body(&self, d: &Value, cx: &mut Context<Self>) -> AnyElement {
        let p = ui::pal(cx);
        let t = self.kind().to_string();
        if let Some(msg) = d["unsupported"].as_str() {
            let key = self.key.clone();
            let pane = self.pane.clone();
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(svg().path("rv/key.svg").size(px(36.)).text_color(p.dim))
                .child(div().text_size(px(13.)).text_color(p.muted).child(msg.to_string()))
                .child(kit::btn("console", cx).mt(px(10.)).icon(kit::ico("terminal")).label("Open console").on_click(move |_, window, cx| {
                    let _ = pane.update(cx, |p, cx| {
                        p.open_console(window, cx);
                        if let Some(RTab::Console(c)) = p.tabs.last() {
                            let cmd = format!("TYPE {key}");
                            c.update(cx, |c, cx| c.input.update(cx, |i, cx| i.set_value(cmd, window, cx)));
                        }
                    });
                }))
                .into_any_element();
        }
        match t.as_str() {
            "string" | "json" => {
                let Some(e) = &self.editor else { return div().into_any_element() };
                let binary = d["binary"].as_bool().unwrap_or(false);
                let truncated = d["truncated"].as_bool().unwrap_or(false);
                let editable = !self.read_only && !binary && !truncated;
                let dirty = e.read(cx).value().as_ref() != self.original.as_str();
                let note = if binary {
                    Some("Binary value: shown with \\xHH escapes, read-only here. Use the console to change it.".to_string())
                } else if truncated {
                    Some(format!("Showing the first {} of {} bytes. Use the console to change it.", ui::fmt_num(self.original.len() as i64), ui::fmt_num(d["length"].as_i64().unwrap_or(0))))
                } else {
                    None
                };
                let e2 = e.clone();
                let original = self.original.clone();
                v_flex()
                    .size_full()
                    .when_some(note, |d, n| d.child(div().mx(px(12.)).mt(px(12.)).px(px(10.)).py(px(8.)).rounded(px(6.)).bg(p.mod_bg).text_size(px(12.)).text_color(p.muted).child(n)))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .mx(px(12.))
                            .mt(px(12.))
                            .when(!editable, |d| d.mb(px(12.)))
                            .border_1()
                            .border_color(p.border)
                            .rounded(px(6.))
                            .overflow_hidden()
                            .bg(p.panel)
                            .child(Editor::new(e).h_full().bordered(false).readonly(!editable).font_family(kit::cx_mono(cx)).text_size(px(12.5))),
                    )
                    .when(editable, |d| {
                        d.child(
                            h_flex()
                                .px(px(12.))
                                .py(px(8.))
                                .gap(px(8.))
                                .child(div().text_size(px(11.5)).text_color(p.dim).child(if t == "json" { "JSON.SET key $ value" } else { "SET key value KEEPTTL" }))
                                .child(div().flex_1())
                                .child(kit::btn_ghost_sm("discard", cx).icon(kit::ico("undo")).label("Discard").disabled(!dirty).on_click(move |_, window, cx| {
                                    e2.update(cx, |s, cx| s.set_value(original.clone(), window, cx));
                                }))
                                .child(
                                    kit::btn_primary("save", cx)
                                        .h(px(24.))
                                        .px(px(8.))
                                        .small()
                                        .icon(kit::ico("check"))
                                        .label("Save")
                                        .child(kit::kbd("Ctrl S", cx).text_color(p.accent_text).border_color(p.accent_text.opacity(0.6)))
                                        .disabled(!dirty)
                                        .on_click(cx.listener(|this, _, window, cx| this.save_value(window, cx))),
                                ),
                        )
                    })
                    .into_any_element()
            }
            "hash" | "list" | "set" | "zset" | "stream" => {
                let length = d["length"].as_i64().unwrap_or(0);
                let more = match t.as_str() {
                    "list" | "zset" => (self.items.len() as i64) < length,
                    _ => d["cursor"].as_str().is_some_and(|c| c != "0"),
                };
                let bar = h_flex()
                    .flex_none()
                    .flex_wrap()
                    .px(px(12.))
                    .py(px(6.))
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(p.border)
                    .when(matches!(t.as_str(), "hash" | "set"), |d| d.child(div().w(px(240.)).child(Input::new(&self.filter).bg(crate::ui::input_bg()).small().prefix(svg().path("rv/search.svg").size(px(14.)).text_color(p.dim)))))
                    .when(t == "zset", |d| {
                        d.child(kit::btn_sm("sort", cx).icon(kit::ico(if self.desc { "arrowDown" } else { "arrowUp" })).label("Score").on_click(cx.listener(|this, _, window, cx| {
                            this.desc = !this.desc;
                            this.load(false, window, cx);
                        })))
                    })
                    .when(t == "stream", |d| {
                        let groups: Vec<String> = self.data.as_ref().and_then(|d| d["groups"].as_array().cloned()).unwrap_or_default().iter().map(|g| format!("{} (pending {}, lag {})", g["name"].as_str().unwrap_or(""), g["pending"].as_i64().unwrap_or(0), g["lag"].as_i64().map(|l| l.to_string()).unwrap_or("?".into()))).collect();
                        d.child(div().text_size(px(12.)).text_color(p.muted).child(if groups.is_empty() { "Newest entries first".to_string() } else { format!("Consumer groups: {}", groups.join(", ")) }))
                    })
                    .child(div().flex_1())
                    .child(div().text_size(px(12.)).text_color(p.muted).child(format!("{} of {} shown", ui::fmt_num(self.items.len() as i64), ui::fmt_num(length))))
                    .when(!self.read_only, |d| d.child(kit::btn_primary("add", cx).h(px(24.)).px(px(8.)).small().icon(kit::ico("plus")).label(if t == "stream" { "Add entry" } else { "Add" }).on_click(cx.listener(|this, _, window, cx| this.edit_item(None, window, cx)))));
                let grid: AnyElement = match &self.grid {
                    Some(g) => grid::table(g).into_any_element(),
                    None => div().into_any_element(),
                };
                v_flex()
                    .size_full()
                    .child(bar)
                    .child(div().flex_1().min_h_0().child(grid))
                    .when(more, |d| {
                        d.child(
                            h_flex().p(px(6.)).justify_center().border_t_1().border_color(p.border).child(
                                kit::btn_ghost_sm("more", cx).loading(self.loading).label(if t == "stream" { format!("Load {PAGE} older") } else { format!("Load {PAGE} more") }).on_click(cx.listener(|this, _, window, cx| this.load(true, window, cx))),
                            ),
                        )
                    })
                    .into_any_element()
            }
            _ => div().into_any_element(),
        }
    }
}

/// "10k" for a folder count (as the stable UI).
fn compact_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6).replace(".0", "")
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1e3).replace(".0", "")
    } else {
        n.to_string()
    }
}

fn glob_or_empty(s: &str) -> String {
    if s.trim().is_empty() {
        String::new()
    } else {
        glob(s)
    }
}

pub fn fmt_ttl(ms: i64) -> String {
    if ms < 0 {
        return "No expiry".into();
    }
    let s = (ms as f64 / 1000.).round() as i64;
    match s {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {}s", s / 60, s % 60),
        s if s < 86400 => format!("{}h {}m", s / 3600, (s % 3600) / 60),
        s => format!("{}d {}h", s / 86400, (s % 86400) / 3600),
    }
}

fn fmt_bytes(n: i64) -> String {
    let units = ["B", "KB", "MB", "GB"];
    let (mut v, mut i) = (n as f64, 0);
    while v >= 1024. && i < units.len() - 1 {
        v /= 1024.;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", units[i])
    }
}

/// "2026-11-12 09:15:03.120" (UTC) from milliseconds since the epoch.
pub fn fmt_time(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil date from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{:03}", rem / 3600, (rem % 3600) / 60, rem % 60, ms.rem_euclid(1000))
}

impl Render for KeyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let root = v_flex().size_full().key_context("KeyView").on_action(cx.listener(Self::save_action));
        if let (Some(e), None) = (&self.error, &self.data) {
            return root.child(v_flex().p_4().gap_2().child(div().font_semibold().child(self.key.clone())).child(div().text_sm().text_color(theme.danger).child(e.clone())));
        }
        let Some(d) = self.data.clone() else {
            return root.child(div().size_full().flex().items_center().justify_center().text_size(px(13.)).text_color(ui::pal(cx).muted).child("Loading…"));
        };
        root.child(self.render_header(&d, cx)).child(div().flex_1().min_h_0().child(self.render_body(&d, cx)))
    }
}

// ================================================================ console

struct LogEntry {
    db: i64,
    cmd: String,
    out: Option<String>,
    error: bool,
    ms: Option<f64>,
}

pub struct ConsoleView {
    backend: Arc<Backend>,
    conn: String,
    db: i64,
    read_only: bool,
    pane: WeakEntity<RedisPane>,
    log: Vec<LogEntry>,
    pub input: Entity<InputState>,
    hist_pos: Option<usize>,
    scroll: ScrollHandle,
    _subs: Vec<Subscription>,
}

const EXAMPLES: &[&str] = &["PING", "INFO server", "DBSIZE", "SCAN 0 COUNT 20", "TYPE key", "TTL key", "MEMORY USAGE key", "CLIENT LIST"];

impl ConsoleView {
    fn new(backend: Arc<Backend>, conn: String, db: i64, read_only: bool, pane: WeakEntity<RedisPane>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Command, e.g. GET user:1   (Enter runs, ↑/↓ history, Ctrl+L clears)"));
        let subs = vec![cx.subscribe_in(&input, window, |this: &mut Self, _, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.run(window, cx);
            }
        })];
        input.update(cx, |s, cx| s.focus(window, cx));
        Self { backend, conn, db, read_only, pane, log: Vec::new(), input, hist_pos: None, scroll: ScrollHandle::new(), _subs: subs }
    }

    fn run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        let lines: Vec<String> = text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        if lines.is_empty() {
            return;
        }
        self.input.update(cx, |s, cx| s.set_value("", window, cx));
        self.hist_pos = None;
        for line in lines {
            if matches!(line.to_lowercase().as_str(), "clear" | "cls") {
                self.log.clear();
                continue;
            }
            let _ = self.pane.update(cx, |p, cx| p.push_history(&line, cx));
            // SELECT n changes the database of the following commands.
            let up = line.to_uppercase();
            if let Some(n) = up.strip_prefix("SELECT ").and_then(|n| n.trim().parse::<i64>().ok()) {
                self.db = n;
                self.log.push(LogEntry { db: n, cmd: line, out: Some("OK".into()), error: false, ms: None });
                continue;
            }
            let ix = self.log.len();
            self.log.push(LogEntry { db: self.db, cmd: line.clone(), out: None, error: false, ms: None });
            let fut = self.backend.clone().post(format!("/connections/{}/redis/command", backend::enc(&self.conn)), json!({ "db": self.db, "command": line }));
            cx.spawn(async move |this, cx| {
                let res = fut.await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(e) = this.log.get_mut(ix) {
                        match res {
                            Ok(v) => {
                                e.error = v["reply"]["t"] == "error";
                                e.out = Some(format_reply(&v["reply"], ""));
                                e.ms = v["durationMs"].as_f64();
                            }
                            Err(err) => {
                                e.error = true;
                                e.out = Some(format!("(error) {err}"));
                            }
                        }
                    }
                    if this.log.len() > 500 {
                        let n = this.log.len() - 500;
                        this.log.drain(..n);
                    }
                    this.scroll.scroll_to_bottom();
                    let _ = this.pane.update(cx, |p, cx| p.refresh_overview(cx));
                    cx.notify();
                });
            })
            .detach();
        }
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn history_step(&mut self, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        let hist = self.pane.upgrade().map(|p| p.read(cx).history.clone()).unwrap_or_default();
        if hist.is_empty() {
            return;
        }
        let pos = match (self.hist_pos, up) {
            (None, true) => Some(0),
            (None, false) => None,
            (Some(p), true) => Some((p + 1).min(hist.len() - 1)),
            (Some(0), false) => None,
            (Some(p), false) => Some(p - 1),
        };
        self.hist_pos = pos;
        let text = pos.map(|p| hist[p].clone()).unwrap_or_default();
        self.input.update(cx, |s, cx| s.set_value(text, window, cx));
    }
}

impl Render for ConsoleView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let mono = kit::cx_mono(cx);
        let mut log = v_flex().id("log").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).px(px(14.)).py(px(10.)).bg(p.bg).font_family(mono.clone()).text_size(px(12.5)).line_height(px(19.));
        if self.log.is_empty() {
            log = log.child(
                v_flex()
                    .gap(px(8.))
                    .font_family(cx.theme().font_family.clone())
                    .child(div().text_size(px(13.)).text_color(p.muted).child(if self.read_only { "Read-only connection: only read commands run." } else { "Run Redis commands. One command per line." }))
                    .child(h_flex().gap(px(4.)).flex_wrap().children(EXAMPLES.iter().enumerate().map(|(i, ex)| {
                        div()
                            .id(("ex", i))
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.panel_2)
                            .text_size(px(11.5))
                            .font_family(mono.clone())
                            .text_color(p.muted)
                            .cursor_pointer()
                            .hover(|s| s.text_color(p.text).border_color(p.border_strong))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.input.update(cx, |s, cx| s.set_value(ex.to_string(), window, cx));
                            }))
                            .child(*ex)
                    }))),
            );
        }
        for e in &self.log {
            log = log.child(
                v_flex()
                    .relative()
                    .pt(px(4.))
                    .pb(px(8.))
                    .mb(px(6.))
                    .border_b_1()
                    .border_dashed()
                    .border_color(p.border)
                    .child(h_flex().gap(px(6.)).child(div().font_semibold().text_color(p.accent).child(format!("db{}>", e.db))).child(div().text_color(p.text).child(e.cmd.clone())))
                    .child(match &e.out {
                        None => div().text_color(p.muted).child("…"),
                        Some(o) => div().mt(px(2.)).text_color(if e.error { p.danger } else { p.t_str }).children(o.lines().map(|l| div().child(if l.is_empty() { " ".to_string() } else { l.to_string() }))),
                    })
                    .children(e.ms.map(|ms| div().absolute().top(px(4.)).right_0().text_size(px(11.)).text_color(p.dim).child(format!("{ms:.1} ms")))),
            );
        }
        let me2 = cx.weak_entity();
        let has_saved = self.pane.upgrade().is_some_and(|pn| !pn.read(cx).saved.is_empty());
        let saved_menu = kit::btn_ghost_sm("saved-cmds", cx).label("Saved").dropdown_caret(true).disabled(!has_saved).dropdown_menu(move |mut menu, _, cx| {
            let list = me2.upgrade().and_then(|m| m.read(cx).pane.upgrade()).map(|p| p.read(cx).saved.clone()).unwrap_or_default();
            if list.is_empty() {
                return menu.item(PopupMenuItem::label("No saved commands (Ctrl+S saves)"));
            }
            for q in list {
                let me = me2.clone();
                menu = menu.item(PopupMenuItem::new(q.name.clone()).icon(kit::ico("bookmark")).on_click(move |_, window, cx| {
                    let t = q.text.clone();
                    let _ = me.update(cx, |this, cx| this.input.update(cx, |s, cx| s.set_value(t, window, cx)));
                }));
            }
            menu
        });
        let me = cx.weak_entity();
        let history = kit::btn_ghost_sm("history", cx).icon(kit::ico("history")).label("History").dropdown_menu(move |mut menu, _, cx| {
            let hist = me.upgrade().and_then(|m| m.read(cx).pane.upgrade()).map(|p| p.read(cx).history.clone()).unwrap_or_default();
            if hist.is_empty() {
                return menu.item(PopupMenuItem::label("No commands yet"));
            }
            for c in hist.into_iter().take(40) {
                let me = me.clone();
                menu = menu.item(PopupMenuItem::new(c.clone()).on_click(move |_, window, cx| {
                    let _ = me.update(cx, |this, cx| this.input.update(cx, |s, cx| s.set_value(c.clone(), window, cx)));
                }));
            }
            menu
        });
        let top = h_flex()
            .flex_none()
            .px(px(12.))
            .py(px(6.))
            .gap(px(8.))
            .border_b_1()
            .border_color(p.border)
            .child(div().flex_1().text_size(px(12.)).text_color(p.muted).child(format!("Database db{} · ↑/↓ history · Ctrl+S save · Ctrl+L clear", self.db)))
            .child(kit::btn_ghost_sm("save", cx).icon(kit::ico("bookmark")).label("Save").on_click(cx.listener(|this, _, window, cx| {
                let t = this.input.read(cx).value().to_string();
                let t = if t.trim().is_empty() { this.log.last().map(|e| e.cmd.clone()).unwrap_or_default() } else { t };
                let _ = this.pane.update(cx, |p, cx| p.save_command(t, window, cx));
            })))
            .child(saved_menu)
            .child(history)
            .child(kit::btn_ghost_sm("clear", cx).label("Clear").on_click(cx.listener(|this, _, _, cx| {
                this.log.clear();
                cx.notify();
            })));
        v_flex()
            .size_full()
            .child(top)
            .child(log)
            .child(
                h_flex()
                    .flex_none()
                    .items_start()
                    .px(px(12.))
                    .py(px(8.))
                    .gap(px(8.))
                    .bg(p.panel)
                    .border_t_1()
                    .border_color(p.border)
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                        match (e.keystroke.key.as_str(), e.keystroke.modifiers.control) {
                            ("up", false) => this.history_step(true, window, cx),
                            ("down", false) => this.history_step(false, window, cx),
                            ("l", true) => {
                                this.log.clear();
                                cx.notify();
                            }
                            ("s", true) => {
                                // The input, or the last command when the input is empty.
                                let t = this.input.read(cx).value().to_string();
                                let t = if t.trim().is_empty() { this.log.last().map(|e| e.cmd.clone()).unwrap_or_default() } else { t };
                                let _ = this.pane.update(cx, |p, cx| p.save_command(t, window, cx));
                            }
                            _ => return,
                        }
                        cx.stop_propagation();
                    }))
                    .child(div().h(px(28.)).flex().items_center().text_size(px(12.5)).font_semibold().font_family(mono.clone()).text_color(p.accent).child(format!("db{}>", self.db)))
                    .child(div().flex_1().child(Input::new(&self.input).bg(crate::ui::input_bg()).font_family(mono.clone()).text_size(px(12.5))))
                    .child(kit::btn_primary("run", cx).icon(kit::ico("play")).label("Run").on_click(cx.listener(|this, _, window, cx| this.run(window, cx)))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{fmt_time, format_reply, glob};
    use serde_json::Value;

    #[test]
    fn formats_replies() {
        let int: Value = serde_json::from_str(r#"{"t":"int","v":3}"#).unwrap();
        assert_eq!(format_reply(&int, ""), "(integer) 3");
        let arr: Value = serde_json::from_str(r#"{"t":"array","v":[{"t":"string","v":"a"},{"t":"nil"}]}"#).unwrap();
        assert_eq!(format_reply(&arr, ""), "1) \"a\"\n2) (nil)");
        assert_eq!(glob("user"), "*user*");
        assert_eq!(glob("user:*"), "user:*");
        assert_eq!(fmt_time(0), "");
        assert_eq!(fmt_time(1_700_000_000_123), "2023-11-14 22:13:20.123");
    }
}
