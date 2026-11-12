//! The Schema tab: schemas, tables, views, enums, roles and policies, with DDL forms.
//! Every change shows its SQL first; the statements run through POST /ddl.

use crate::backend::{self, Backend};
use crate::ddlsql::{self as q, ColumnDef, NewCol, D};
use crate::forms::{Field, FormDialog, Options};
use crate::{kit, ui};
use crate::workspace::Workspace;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Sizable as _, StyledExt as _, WindowExt as _};
#[allow(unused_imports)]
use gpui_kit::component::Disableable as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::Arc;

const SECTIONS: &[(&str, &str, Option<&str>)] = &[
    ("schemas", "Schemas", Some("schemas")),
    ("tables", "Tables", None),
    ("views", "Views", None),
    ("enums", "Enums", Some("enums")),
    ("roles", "Roles", Some("roles")),
    ("policies", "Policies", Some("policies")),
];

type Stmts = Vec<(String, String)>;

pub struct SchemaView {
    backend: Arc<Backend>,
    conn: String,
    ws: WeakEntity<Workspace>,
    me: WeakEntity<SchemaView>,
    pub section: String,
    pub sel: Option<Value>,
    pub schema_filter: String,
    filter: Entity<InputState>,
    catalog: Option<Value>,
    detail: Option<Value>,
    error: Option<String>,
    detail_error: Option<String>,
    ddl_open: bool,
    _subs: Vec<Subscription>,
}

/// The Schema tab's saved state.
pub fn tab_state(v: &SchemaView) -> Value {
    json!({ "kind": "schema", "section": v.section, "sel": v.sel, "schemaFilter": v.schema_filter })
}

impl SchemaView {
    pub fn new(backend: Arc<Backend>, conn: String, saved: Option<&Value>, ws: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search…"));
        let subs = vec![cx.subscribe(&filter, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        })];
        let mut this = Self {
            backend,
            conn,
            ws,
            me: cx.weak_entity(),
            section: saved.and_then(|s| s["section"].as_str()).unwrap_or("tables").to_string(),
            sel: saved.map(|s| s["sel"].clone()).filter(|v| !v.is_null()),
            schema_filter: saved.and_then(|s| s["schemaFilter"].as_str()).unwrap_or("").to_string(),
            filter,
            catalog: None,
            detail: None,
            error: None,
            detail_error: None,
            ddl_open: false,
            _subs: subs,
        };
        this.load_catalog(false, cx);
        this
    }

    fn d(&self) -> D {
        D::from(self.catalog.as_ref().and_then(|c| c["dialect"].as_str()).unwrap_or("postgres"))
    }

    fn feature(&self, f: &str) -> bool {
        self.catalog.as_ref().is_some_and(|c| c["features"][f].as_bool().unwrap_or(false))
    }

    fn read_only(&self) -> bool {
        self.catalog.as_ref().is_some_and(|c| !c["readOnlyReason"].is_null())
    }

    fn changed(&self, cx: &mut Context<Self>) {
        let ws = self.ws.clone();
        cx.defer(move |cx| {
            let _ = ws.update(cx, |ws, cx| ws.save_state(cx));
        });
    }

    pub fn load_catalog(&mut self, keep_detail: bool, cx: &mut Context<Self>) {
        let fut = self.backend.clone().get(format!("/connections/{}/ddl/catalog", backend::enc(&self.conn)));
        cx.spawn(async move |this, cx| {
            let res = fut.await;
            let _ = this.update(cx, |this, cx| {
                match res {
                    Ok(c) => {
                        let schemas: Vec<String> = c["schemas"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect();
                        if !schemas.contains(&this.schema_filter) {
                            this.schema_filter = c["defaultSchema"].as_str().unwrap_or("").to_string();
                        }
                        this.catalog = Some(c);
                        if let Some(f) = SECTIONS.iter().find(|s| s.0 == this.section).and_then(|s| s.2) {
                            if !this.feature(f) {
                                this.section = "tables".into();
                            }
                        }
                        this.error = None;
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                if !keep_detail {
                    this.detail = None;
                }
                this.load_detail(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn load_detail(&mut self, cx: &mut Context<Self>) {
        let Some(sel) = self.sel.clone() else {
            self.detail = None;
            return;
        };
        let (s, n) = (sel["schema"].as_str().unwrap_or(""), sel["name"].as_str().unwrap_or(""));
        let url = match self.section.as_str() {
            "tables" => format!("/ddl/table?schema={}&name={}", backend::enc(s), backend::enc(n)),
            "views" => format!("/ddl/view?schema={}&name={}", backend::enc(s), backend::enc(n)),
            "roles" => format!("/ddl/role?name={}", backend::enc(n)),
            _ => {
                self.detail = Some(json!({ "ok": true }));
                return;
            }
        };
        let fut = self.backend.clone().get(format!("/connections/{}{url}", backend::enc(&self.conn)));
        cx.spawn(async move |this, cx| {
            let res = fut.await;
            let _ = this.update(cx, |this, cx| {
                if this.sel.as_ref() != Some(&sel) {
                    return;
                }
                match res {
                    Ok(d) => {
                        if crate::dev_env("ROWVEIL_NEXT_FORM").as_deref() == Some("column") && this.detail.is_none() {
                            let (me, t) = (cx.weak_entity(), d.clone());
                            cx.defer(move |cx| {
                                if let Some(w) = cx.windows().first().copied() {
                                    let _ = w.update(cx, |_, window, cx| {
                                        let _ = me.update(cx, |this, cx| this.column_form(&t, None, window, cx));
                                    });
                                }
                            });
                        }
                        this.detail = Some(d);
                        this.detail_error = None;
                    }
                    Err(e) => {
                        this.detail = None;
                        this.detail_error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select(&mut self, sel: Option<Value>, cx: &mut Context<Self>) {
        self.sel = sel;
        self.detail = None;
        self.detail_error = None;
        self.changed(cx);
        self.load_detail(cx);
        cx.notify();
    }

    /// Runs statements after the user saw them; then refreshes the schema and the catalog.
    fn run_ddl(&self, stmts: Stmts, after: Option<Value>, form: WeakEntity<FormDialog>, window: &mut Window, cx: &mut App) {
        let fut = self.backend.clone().post(format!("/connections/{}/ddl", backend::enc(&self.conn)), json!({ "statements": stmts.iter().map(|s| s.0.clone()).collect::<Vec<_>>() }));
        let (ws, me) = (self.ws.clone(), self.me.clone());
        window
            .spawn(cx, async move |cx| {
                let res = fut.await;
                let _ = cx.update(|window, cx| match res {
                    Ok(_) => {
                        window.close_dialog(cx);
                        ui::toast_ok(window, cx, "Schema updated");
                        let _ = ws.update(cx, |ws, cx| ws.reload_schema_quietly(window, cx));
                        let _ = me.update(cx, |this, cx| {
                            if let Some(sel) = after {
                                this.sel = if sel.is_null() { None } else { Some(sel) };
                                this.changed(cx);
                            }
                            this.load_catalog(true, cx);
                        });
                    }
                    Err(e) => {
                        let _ = form.update(cx, |f, cx| f.fail(e.to_string(), cx));
                    }
                });
            })
            .detach();
    }

    /// The SQL preview dialog for fixed statements.
    fn apply(&self, title: &str, stmts: Stmts, danger: bool, note: Option<String>, after: Option<Value>, window: &mut Window, cx: &mut Context<Self>) {
        if stmts.is_empty() {
            ui::toast_ok(window, cx, "Nothing changed");
            return;
        }
        let mysql_note = (self.d() == D::Mysql && stmts.len() > 1).then(|| "MySQL commits each DDL statement separately: a failure stops at that statement.".to_string());
        let note = match (note, mysql_note) {
            (Some(a), Some(b)) => Some(format!("{a} {b}")),
            (a, b) => a.or(b),
        };
        let me = cx.weak_entity();
        let st2 = stmts.clone();
        let opts = Options { ok: if danger { "Apply (destructive)".into() } else { "Apply".into() }, danger, note, preview: Some(Rc::new(move |_| Ok(st2.clone()))), width: 640. };
        FormDialog::open_with(
            title.to_string(),
            vec![],
            opts,
            Rc::new(move |_, form, window, cx| {
                let stmts = stmts.clone();
                let after = after.clone();
                let _ = me.update(cx, |this, cx| this.run_ddl(stmts, after, form, window, cx));
                Ok(false)
            }),
            window,
            cx,
        );
    }

    /// A form with live SQL; `build` turns the field values into statements.
    fn ddl_form(&self, title: &str, fields: Vec<Field>, ok: &str, build: Rc<dyn Fn(&[String]) -> Result<Stmts, String>>, after: Option<Rc<dyn Fn(&[String]) -> Value>>, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.weak_entity();
        let b2 = build.clone();
        let opts = Options { ok: ok.into(), danger: false, note: None, preview: Some(Rc::new(move |v| b2(v))), width: 640. };
        FormDialog::open_with(
            title.to_string(),
            fields,
            opts,
            Rc::new(move |v, form, window, cx| {
                let stmts = build(&v)?;
                if stmts.is_empty() {
                    return Err("Nothing to apply".into());
                }
                let sel = after.as_ref().map(|f| f(&v));
                let _ = me.update(cx, |this, cx| this.run_ddl(stmts, sel, form, window, cx));
                Ok(false)
            }),
            window,
            cx,
        );
    }

    fn copy_schema(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        let fut = self.backend.clone().get(format!("/connections/{}/ddl/dump", backend::enc(&self.conn)));
        let ws = self.ws.clone();
        cx.spawn_in(window, async move |_, cx| {
            let res = fut.await;
            let _ = cx.update(|window, cx| match res {
                Ok(v) => {
                    let sql = v["sql"].as_str().unwrap_or("").to_string();
                    if open {
                        let _ = ws.update(cx, |ws, cx| ws.open_sql(sql, Some("Schema DDL".into()), None, window, cx));
                    } else {
                        ui::copy(sql, cx);
                        ui::toast_ok(window, cx, "Schema DDL copied");
                    }
                }
                Err(e) => ui::toast_err(window, cx, e.to_string()),
            });
        })
        .detach();
    }

    fn items(&self) -> Vec<(String, String, Option<String>, Option<String>, bool, Value)> {
        // (key, label, sub, badge, system, sel)
        let Some(c) = &self.catalog else { return Vec::new() };
        let in_schema = |x: &Value| !self.feature("schemas") || x["schema"].as_str() == Some(self.schema_filter.as_str());
        let arr = |k: &str| c[k].as_array().cloned().unwrap_or_default();
        let s = |v: &Value, k: &str| v[k].as_str().unwrap_or("").to_string();
        match self.section.as_str() {
            "schemas" => arr("schemas").iter().map(|x| {
                let n = x.as_str().unwrap_or("").to_string();
                (n.clone(), n.clone(), None, None, false, json!({ "name": n }))
            }).collect(),
            "tables" => arr("tables").iter().filter(|x| in_schema(x)).map(|t| (format!("{}.{}", s(t, "schema"), s(t, "name")), s(t, "name"), None, t["rls"].as_bool().unwrap_or(false).then(|| "RLS".to_string()), false, json!({ "schema": t["schema"], "name": t["name"] }))).collect(),
            "views" => arr("views").iter().filter(|x| in_schema(x)).map(|v| (format!("{}.{}", s(v, "schema"), s(v, "name")), s(v, "name"), None, v["materialized"].as_bool().unwrap_or(false).then(|| "MAT".to_string()), false, json!({ "schema": v["schema"], "name": v["name"] }))).collect(),
            "enums" => arr("enums").iter().filter(|x| in_schema(x)).map(|e| (format!("{}.{}", s(e, "schema"), s(e, "name")), s(e, "name"), None, Some(e["values"].as_array().map(|a| a.len()).unwrap_or(0).to_string()), false, json!({ "schema": e["schema"], "name": e["name"] }))).collect(),
            "roles" => {
                let mut r: Vec<_> = arr("roles").iter().map(|r| (s(r, "name"), s(r, "name"), None, None, r["system"].as_bool().unwrap_or(false), json!({ "name": r["name"] }))).collect();
                r.sort_by_key(|x| x.4);
                r
            }
            "policies" => arr("policies").iter().filter(|x| in_schema(x)).map(|p| (format!("{}.{}.{}", s(p, "schema"), s(p, "table"), s(p, "name")), s(p, "name"), Some(s(p, "table")), None, false, json!({ "schema": p["schema"], "table": p["table"], "name": p["name"] }))).collect(),
            _ => Vec::new(),
        }
    }

    fn sel_key(&self) -> Option<String> {
        let s = self.sel.as_ref()?;
        let g = |k: &str| s[k].as_str().unwrap_or("").to_string();
        Some(match self.section.as_str() {
            "policies" => format!("{}.{}.{}", g("schema"), g("table"), g("name")),
            "tables" | "views" | "enums" => format!("{}.{}", g("schema"), g("name")),
            _ => g("name"),
        })
    }

    // ------------------------------------------------------------ create

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.d();
        let schemas = self.schemas();
        let sf = self.schema_filter.clone();
        let has_schemas = self.feature("schemas");
        match self.section.as_str() {
            "schemas" => {
                let word = if d == D::Mysql { "DATABASE" } else { "SCHEMA" };
                self.ddl_form(
                    if d == D::Mysql { "Create database" } else { "Create schema" },
                    vec![Field::line("Name", "")],
                    "Create",
                    Rc::new(move |v| Ok(vec![sql(format!("CREATE {word} {}", q::qi(d, &q::req(&v[0], "the name")?)))])),
                    Some(Rc::new(|v| json!({ "name": v[0].trim() }))),
                    window,
                    cx,
                );
            }
            "tables" => {
                let types = q::column_types(d);
                let (id_t, name_t) = match d {
                    D::Mysql => ("int", "varchar(255)"),
                    D::Sqlite => ("INTEGER", "TEXT"),
                    _ => ("integer", "text"),
                };
                let mut fields = vec![Field::line("Table name", "")];
                if has_schemas {
                    fields.push(Field::choice("Schema", &schemas.iter().map(String::as_str).collect::<Vec<_>>(), &sf));
                }
                fields.push(
                    Field::multi("Columns", format!("id {id_t} pk\nname {name_t} not null"))
                        .hint(format!("One column per line: name type [pk] [not null] [default <expr>]. Types: {}", types.join(", "))),
                );
                let schema_ix = has_schemas;
                self.ddl_form(
                    "Create table",
                    fields,
                    "Create",
                    Rc::new(move |v| {
                        let schema = if schema_ix { v[1].clone() } else { String::new() };
                        let cols = parse_col_lines(&v[if schema_ix { 2 } else { 1 }]);
                        Ok(vec![sql(q::create_table(d, &schema, &v[0], &cols)?)])
                    }),
                    Some(Rc::new(move |v| json!({ "schema": if schema_ix { v[1].clone() } else { "main".into() }, "name": v[0].trim() }))),
                    window,
                    cx,
                );
            }
            "views" => {
                let mut fields = vec![Field::line("View name", "")];
                if has_schemas {
                    fields.push(Field::choice("Schema", &schemas.iter().map(String::as_str).collect::<Vec<_>>(), &sf));
                }
                if self.feature("materializedViews") {
                    fields.push(Field::check("Materialized", false));
                }
                fields.push(Field::sql("Definition", "SELECT "));
                let (hs, hm) = (has_schemas, self.feature("materializedViews"));
                self.ddl_form(
                    "Create view",
                    fields,
                    "Create",
                    Rc::new(move |v| {
                        let mut i = 1;
                        let schema = if hs { i += 1; v[1].clone() } else { String::new() };
                        let mat = if hm { i += 1; v[i - 1] == "true" } else { false };
                        let def = q::req(v[i].trim().trim_end_matches(';'), "the definition")?;
                        Ok(vec![sql(format!("CREATE {}VIEW {} AS\n{def}", if mat { "MATERIALIZED " } else { "" }, q::qref(d, &schema, &q::req(&v[0], "the view name")?)))])
                    }),
                    Some(Rc::new(move |v| json!({ "schema": if hs { v[1].clone() } else { "main".into() }, "name": v[0].trim() }))),
                    window,
                    cx,
                );
            }
            "enums" => {
                self.ddl_form(
                    "Create enum",
                    vec![Field::line("Enum name", ""), Field::choice("Schema", &schemas.iter().map(String::as_str).collect::<Vec<_>>(), &sf), Field::multi("Values (one per line)", "active\ninactive")],
                    "Create",
                    Rc::new(move |v| {
                        let vals: Vec<String> = v[2].lines().map(str::trim).filter(|x| !x.is_empty()).map(q::lit).collect();
                        if vals.is_empty() {
                            return Err("Enter at least one value".into());
                        }
                        Ok(vec![sql(format!("CREATE TYPE {} AS ENUM ({})", q::qref(d, &v[1], &q::req(&v[0], "the enum name")?), vals.join(", ")))])
                    }),
                    Some(Rc::new(|v| json!({ "schema": v[1], "name": v[0].trim() }))),
                    window,
                    cx,
                );
            }
            "roles" => {
                self.ddl_form(
                    "Create role",
                    vec![Field::line("Role name", ""), Field::check("Can login", true), Field::line("Password (optional)", "").masked()],
                    "Create",
                    Rc::new(move |v| {
                        let base = format!("CREATE ROLE {}{}", q::qi(d, &q::req(&v[0], "the role name")?), if v[1] == "true" { " LOGIN" } else { "" });
                        Ok(vec![if v[2].is_empty() { sql(base) } else { (format!("{base} PASSWORD {}", q::lit(&v[2])), format!("{base} PASSWORD '••••••••'")) }])
                    }),
                    Some(Rc::new(|v| json!({ "name": v[0].trim() }))),
                    window,
                    cx,
                );
            }
            "policies" => self.policy_form(None, None, window, cx),
            _ => {}
        }
    }

    fn schemas(&self) -> Vec<String> {
        self.catalog.as_ref().and_then(|c| c["schemas"].as_array().cloned()).unwrap_or_default().iter().filter_map(|s| s.as_str().map(str::to_string)).collect()
    }

    fn rename_form(&self, title: &str, current: &str, build: Rc<dyn Fn(&str) -> Stmts>, after: Rc<dyn Fn(&str) -> Value>, window: &mut Window, cx: &mut Context<Self>) {
        let cur = current.to_string();
        self.ddl_form(
            title,
            vec![Field::line("New name", current)],
            "Rename",
            Rc::new(move |v| {
                let n = q::req(&v[0], "the new name")?;
                if n == cur {
                    return Ok(vec![]);
                }
                Ok(build(&n))
            }),
            Some(Rc::new(move |v| after(v[0].trim()))),
            window,
            cx,
        );
    }

    fn column_form(&self, t: &Value, col: Option<&Value>, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.d();
        let r = q::qref(d, t["schema"].as_str().unwrap_or(""), t["name"].as_str().unwrap_or(""));
        let old = col.map(|c| ColumnDef {
            name: c["name"].as_str().unwrap_or("").into(),
            type_name: c["type"].as_str().unwrap_or("").into(),
            not_null: c["notNull"].as_bool().unwrap_or(false),
            default: c["default"].as_str().unwrap_or("").into(),
            auto: c["autoIncrement"].as_bool().unwrap_or(false),
        });
        let lite_edit = old.is_some() && d == D::Sqlite;
        let editing = old.is_some();
        let o = old.as_ref();
        let auto_label = if d == D::Postgres { "Identity (GENERATED BY DEFAULT AS IDENTITY)" } else { "AUTO_INCREMENT" };
        let mut fields = vec![
            Field::line("Name", o.map(|c| c.name.clone()).unwrap_or_default()),
            Field::line("Type", o.map(|c| c.type_name.clone()).unwrap_or_default()).hint(q::column_types(d).join(", ")).disabled(lite_edit),
            Field::line("Default (SQL expression)", o.map(|c| c.default.clone()).unwrap_or_default()).hint("e.g. 0, now(), 'text'").disabled(lite_edit),
            Field::check("NOT NULL", o.is_some_and(|c| c.not_null)).disabled(lite_edit),
        ];
        if !editing {
            fields.push(Field::check("PRIMARY KEY", false));
            fields.push(Field::check("UNIQUE", false));
        }
        let show_auto = !(d == D::Sqlite || (editing && d == D::Postgres));
        if show_auto {
            fields.push(Field::check(auto_label, o.is_some_and(|c| c.auto)));
        }
        let title = match o {
            Some(c) => format!("Edit column {}", c.name),
            None => format!("Add column to {}", t["name"].as_str().unwrap_or("")),
        };
        self.ddl_form(
            &title,
            fields,
            "Apply",
            Rc::new(move |v| {
                let n = q::req(&v[0], "the column name")?;
                if lite_edit {
                    let o = old.as_ref().unwrap();
                    return Ok(if n != o.name { vec![sql(format!("ALTER TABLE {r} RENAME COLUMN {} TO {}", q::qi(d, &o.name), q::qi(d, &n)))] } else { vec![] });
                }
                let ty = q::req(&v[1], "the type")?;
                let auto = if show_auto { v[v.len() - 1] == "true" } else { false };
                let new = ColumnDef { name: n, type_name: ty, not_null: v[3] == "true", default: v[2].trim().to_string(), auto };
                Ok(match &old {
                    None => vec![sql(q::add_column(d, &r, &new, v[4] == "true", v[5] == "true"))],
                    Some(o) => q::alter_column(d, &r, o, &new).into_iter().map(sql).collect(),
                })
            }),
            None,
            window,
            cx,
        );
    }

    fn constraint_form(&self, t: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.d();
        let r = q::qref(d, t["schema"].as_str().unwrap_or(""), t["name"].as_str().unwrap_or(""));
        let tables: Vec<(String, String)> = self.catalog.as_ref().and_then(|c| c["tables"].as_array().cloned()).unwrap_or_default().iter().map(|x| {
            let k = format!("{}.{}", x["schema"].as_str().unwrap_or(""), x["name"].as_str().unwrap_or(""));
            (k.clone(), if d == D::Sqlite { x["name"].as_str().unwrap_or("").to_string() } else { k })
        }).collect();
        let cols: Vec<String> = t["columns"].as_array().into_iter().flatten().filter_map(|c| c["name"].as_str().map(str::to_string)).collect();
        let me_t = format!("{}.{}", t["schema"].as_str().unwrap_or(""), t["name"].as_str().unwrap_or(""));
        let fields = vec![
            Field::choice_labeled("Type", vec![("foreign".into(), "FOREIGN KEY".into()), ("unique".into(), "UNIQUE".into()), ("primary".into(), "PRIMARY KEY".into()), ("check".into(), "CHECK".into())], "foreign"),
            Field::line("Name (optional)", ""),
            Field::line("Columns (comma-separated)", "").hint(format!("Columns: {}", cols.join(", "))),
            Field::choice_labeled("References table (FOREIGN KEY)", tables, &me_t),
            Field::line("Referenced columns (FOREIGN KEY)", "id"),
            Field::choice("ON DELETE", q::FK_ACTIONS, "NO ACTION"),
            Field::choice("ON UPDATE", q::FK_ACTIONS, "NO ACTION"),
            Field::line("Check expression (CHECK)", "").hint("price >= 0"),
        ];
        self.ddl_form(
            &format!("Add constraint to {}", t["name"].as_str().unwrap_or("")),
            fields,
            "Apply",
            Rc::new(move |v| Ok(vec![sql(q::add_constraint(d, &r, &v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7])?)])),
            None,
            window,
            cx,
        );
    }

    fn index_form(&self, t: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.d();
        let (s, n) = (t["schema"].as_str().unwrap_or("").to_string(), t["name"].as_str().unwrap_or("").to_string());
        let cols: Vec<String> = t["columns"].as_array().into_iter().flatten().filter_map(|c| c["name"].as_str().map(str::to_string)).collect();
        let mut fields = vec![
            Field::line("Name", format!("{n}_idx")),
            Field::line("Columns / expressions (comma-separated)", "").hint(format!("Plain names are quoted; lower(email) or col DESC are used as written. Columns: {}", cols.join(", "))),
            Field::check("UNIQUE", false),
        ];
        if d == D::Postgres {
            fields.push(Field::choice("Method", &["btree", "hash", "gin", "gist", "brin", "spgist"], "btree"));
        }
        if d != D::Mysql {
            fields.push(Field::line("Partial index WHERE (optional)", "").hint("deleted_at IS NULL"));
        }
        self.ddl_form(
            &format!("Add index to {n}"),
            fields,
            "Apply",
            Rc::new(move |v| {
                let method = if d == D::Postgres { v[3].clone() } else { String::new() };
                let where_ = match d {
                    D::Postgres => v[4].clone(),
                    D::Mysql => String::new(),
                    _ => v[3].clone(),
                };
                Ok(vec![sql(q::create_index(d, &s, &n, &v[0], &v[1], v[2] == "true", &method, &where_)?)])
            }),
            None,
            window,
            cx,
        );
    }

    fn policy_form(&self, p: Option<&Value>, table: Option<&Value>, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.d();
        let tables: Vec<(String, String)> = self.catalog.as_ref().and_then(|c| c["tables"].as_array().cloned()).unwrap_or_default().iter().map(|x| {
            let k = format!("{}.{}", x["schema"].as_str().unwrap_or(""), x["name"].as_str().unwrap_or(""));
            (k.clone(), k)
        }).collect();
        let cur_t = p.map(|p| format!("{}.{}", p["schema"].as_str().unwrap_or(""), p["table"].as_str().unwrap_or(""))).or_else(|| table.map(|t| format!("{}.{}", t["schema"].as_str().unwrap_or(""), t["name"].as_str().unwrap_or("")))).or_else(|| tables.first().map(|t| t.0.clone())).unwrap_or_default();
        let fixed = p.is_some() || table.is_some();
        let roles: Vec<String> = p.and_then(|p| p["roles"].as_array().cloned()).unwrap_or_default().iter().filter_map(|r| r.as_str()).filter(|r| *r != "public").map(str::to_string).collect();
        let fields = vec![
            Field::line("Name", p.and_then(|p| p["name"].as_str()).unwrap_or("")),
            Field::choice_labeled("Table", tables, &cur_t).disabled(fixed),
            Field::choice("Command", &["ALL", "SELECT", "INSERT", "UPDATE", "DELETE"], p.and_then(|p| p["command"].as_str()).unwrap_or("ALL")),
            Field::choice("Type", &["PERMISSIVE", "RESTRICTIVE"], if p.is_some_and(|p| p["permissive"] == false) { "RESTRICTIVE" } else { "PERMISSIVE" }),
            Field::line("Roles (comma-separated, empty = public)", roles.join(", ")),
            Field::multi("USING (rows the role can see / change)", p.and_then(|p| p["using"].as_str()).unwrap_or("")),
            Field::multi("WITH CHECK (rows the role can write)", p.and_then(|p| p["withCheck"].as_str()).unwrap_or("")),
        ];
        let old = p.map(|p| (p["name"].as_str().unwrap_or("").to_string(), p["schema"].as_str().unwrap_or("").to_string(), p["table"].as_str().unwrap_or("").to_string()));
        let section_policies = self.section == "policies";
        self.ddl_form(
            if p.is_some() { "Edit policy" } else { "Create policy" },
            fields,
            if p.is_some() { "Save" } else { "Create" },
            Rc::new(move |v| {
                let (s, t) = v[1].split_once('.').unwrap_or(("", &v[1]));
                let r = q::qref(d, s, t);
                let rl = q::split_cols(&v[4]);
                let to = if rl.is_empty() { "public".to_string() } else { rl.iter().map(|x| if x == "public" { x.clone() } else { q::qi(d, x) }).collect::<Vec<_>>().join(", ") };
                let mut s2 = format!("CREATE POLICY {} ON {r} AS {} FOR {} TO {to}", q::qi(d, &q::req(&v[0], "the policy name")?), v[3], v[2]);
                if !v[5].trim().is_empty() {
                    s2.push_str(&format!(" USING ({})", v[5].trim()));
                }
                if !v[6].trim().is_empty() {
                    s2.push_str(&format!(" WITH CHECK ({})", v[6].trim()));
                }
                let mut out = Vec::new();
                if let Some((n, os, ot)) = &old {
                    out.push(sql(format!("DROP POLICY {} ON {}", q::qi(d, n), q::qref(d, os, ot))));
                }
                out.push(sql(s2));
                Ok(out)
            }),
            section_policies.then(|| {
                Rc::new(|v: &[String]| {
                    let (s, t) = v[1].split_once('.').unwrap_or(("", &v[1]));
                    json!({ "schema": s, "table": t, "name": v[0].trim() })
                }) as Rc<dyn Fn(&[String]) -> Value>
            }),
            window,
            cx,
        );
    }

    // ------------------------------------------------------------ rendering

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let q_ = self.filter.read(cx).value().to_lowercase();
        let items: Vec<_> = self.items().into_iter().filter(|i| q_.is_empty() || i.1.to_lowercase().contains(&q_) || i.2.as_deref().unwrap_or("").to_lowercase().contains(&q_)).collect();
        let key = self.sel_key();
        let icon = match self.section.as_str() {
            "schemas" => "folder",
            "tables" => "table",
            "views" => "view",
            "enums" => "list",
            "roles" => "user",
            _ => "shield",
        };
        let mut list = v_flex().id("schema-items").flex_1().min_h_0().gap(px(1.)).overflow_y_scroll();
        let mut system_shown = false;
        for (i, (k, label, sub, badge, system, sel)) in items.iter().cloned().enumerate() {
            if system && !system_shown {
                list = list.child(div().mx(px(4.)).my(px(4.)).h(px(1.)).bg(p.border));
                system_shown = true;
            }
            let active = key.as_deref() == Some(k.as_str());
            let right = match (sub, badge) {
                (None, None) => None,
                (s, b) => Some(
                    h_flex()
                        .gap(px(6.))
                        .max_w(px(110.))
                        .children(s.map(|s| div().min_w_0().truncate().text_size(px(11.)).text_color(p.dim).child(s)))
                        .children(b.map(|b| kit::count(b, cx)))
                        .into_any_element(),
                ),
            };
            list = list.child(kit::sb_item(("si", i), icon, label, right, active, cx).on_click(cx.listener(move |this, _, _, cx| this.select(Some(sel.clone()), cx))));
        }
        if items.is_empty() {
            let what = if self.feature("schemas") && !matches!(self.section.as_str(), "schemas" | "roles") { format!("schema {}", self.schema_filter) } else { "database".into() };
            list = list.child(div().p(px(16.)).text_size(px(13.)).text_color(p.muted).child(if q_.is_empty() { format!("No {} in {what}", self.section) } else { "Nothing found".into() }));
        }
        let pick = (self.feature("schemas") && !matches!(self.section.as_str(), "schemas" | "roles")).then(|| {
            let me = cx.weak_entity();
            let names = self.schemas();
            let cur = self.schema_filter.clone();
            h_flex()
                .gap(px(8.))
                .child(div().text_size(px(12.)).text_color(p.muted).child(if self.d() == D::Mysql { "Database" } else { "Schema" }))
                .child(kit::select("sf", self.schema_filter.clone(), cx).h(px(26.)).dropdown_menu(move |mut m, _, _| {
                    for n in &names {
                        let (me, n) = (me.clone(), n.clone());
                        m = m.item(PopupMenuItem::new(n.clone()).checked(n == cur).on_click(move |_, _, cx| {
                            let _ = me.update(cx, |this, cx| {
                                this.schema_filter = n.clone();
                                this.select(None, cx);
                            });
                        }));
                    }
                    m
                }))
        });
        v_flex()
            .w(px(260.))
            .flex_none()
            .h_full()
            .p(px(10.))
            .gap(px(8.))
            .bg(p.panel)
            .border_1()
            .border_color(p.border)
            .rounded(px(10.))
            .children(pick)
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(div().flex_1().child(Input::new(&self.filter).bg(crate::ui::input_bg()).small().prefix(svg().path("rv/search.svg").size(px(14.)).text_color(p.dim))))
                    .child(kit::icon_btn("refresh", "refresh", cx).tooltip("Refresh").on_click(cx.listener(|this, _, _, cx| this.load_catalog(true, cx))))
                    .when(!self.read_only(), |d| d.child(kit::icon_btn("create", "plus", cx).tooltip("Create").on_click(cx.listener(|this, _, window, cx| this.create(window, cx))))),
            )
            .child(list)
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let Some(sel) = self.sel.clone() else {
            return div().w_full().p(px(40.)).flex().justify_center().text_size(px(13.)).text_color(ui::pal(cx).muted).child("Select an item on the left").into_any_element();
        };
        if let Some(e) = &self.detail_error {
            return div().p_4().text_sm().text_color(theme.danger).child(e.clone()).into_any_element();
        }
        let Some(cat) = self.catalog.clone() else { return div().into_any_element() };
        let body = match self.section.as_str() {
            "schemas" => self.schema_card(&sel, &cat, cx),
            "tables" => match &self.detail {
                Some(t) if t.get("columns").is_some() => self.table_card(t, cx),
                _ => ui::muted("Loading…", cx).into_any_element(),
            },
            "views" => match &self.detail {
                Some(v) if v.get("definition").is_some() => self.view_card(&sel, v, cx),
                _ => ui::muted("Loading…", cx).into_any_element(),
            },
            "enums" => self.enum_card(&sel, &cat, cx),
            "roles" => match &self.detail {
                Some(r) if r.get("definition").is_some() => self.role_card(&sel, r, cx),
                _ => ui::muted("Loading…", cx).into_any_element(),
            },
            "policies" => self.policy_card(&sel, &cat, cx),
            _ => div().into_any_element(),
        };
        div().id("detail").size_full().overflow_y_scroll().child(body).into_any_element()
    }

    fn schema_card(&self, sel: &Value, cat: &Value, cx: &mut Context<Self>) -> AnyElement {
        let d = self.d();
        let ro = self.read_only();
        let name = sel["name"].as_str().unwrap_or("").to_string();
        let count = |k: &str| cat[k].as_array().map(|a| a.iter().filter(|x| x["schema"].as_str() == Some(name.as_str())).count()).unwrap_or(0);
        let mut counts = vec![format!("{} tables", count("tables")), format!("{} views", count("views"))];
        if self.feature("enums") {
            counts.push(format!("{} enums", count("enums")));
        }
        let (n1, n2) = (name.clone(), name.clone());
        kit::card(cx).child(card_title(cx, if d == D::Mysql { "Database" } else { "Schema" }, &name))
            .child(ui::muted(counts.join(" · "), cx))
            .when(!ro, |c| {
                c.child(
                    h_flex()
                        .gap_2()
                        .when(self.feature("renameSchema"), |h| {
                            h.child(kit::btn("rename", cx).icon(kit::ico("edit")).label("Rename").on_click(cx.listener(move |this, _, window, cx| {
                                let old = n1.clone();
                                this.rename_form("Rename schema", &old.clone(), Rc::new(move |n| vec![sql(format!("ALTER SCHEMA {} RENAME TO {}", q::qi(d, &old), q::qi(d, n)))]), Rc::new(|n| json!({ "name": n })), window, cx);
                            })))
                        })
                        .child(div().flex_1())
                        .child(kit::btn_danger("drop", cx).icon(kit::ico("trash")).label(if d == D::Mysql { "Drop database" } else { "Drop schema" }).on_click(cx.listener(move |_this, _, window, cx| {
                            let n = n2.clone();
                            let fields = if d == D::Mysql { vec![] } else { vec![Field::check("CASCADE (also drop all objects in it)", false)] };
                            let what = if d == D::Mysql { "the database" } else { "the schema" };
                            let me = cx.weak_entity();
                            let n3 = n.clone();
                            let opts = Options {
                                ok: "Drop".into(),
                                danger: true,
                                note: Some(format!("This permanently deletes {what} \"{n}\".")),
                                preview: Some(Rc::new(move |v| Ok(vec![sql(if d == D::Mysql { format!("DROP DATABASE {}", q::qi(d, &n3)) } else { format!("DROP SCHEMA {}{}", q::qi(d, &n3), if v.first().map(String::as_str) == Some("true") { " CASCADE" } else { "" }) })]))),
                                width: 600.,
                            };
                            let n4 = n.clone();
                            FormDialog::open_with(
                                if d == D::Mysql { "Drop database" } else { "Drop schema" },
                                fields,
                                opts,
                                Rc::new(move |v, form, window, cx| {
                                    let st = vec![sql(if d == D::Mysql { format!("DROP DATABASE {}", q::qi(d, &n4)) } else { format!("DROP SCHEMA {}{}", q::qi(d, &n4), if v.first().map(String::as_str) == Some("true") { " CASCADE" } else { "" }) })];
                                    let _ = me.update(cx, |this, cx| this.run_ddl(st, Some(Value::Null), form, window, cx));
                                    Ok(false)
                                }),
                                window,
                                cx,
                            );
                        }))),
                )
            })
            .into_any_element()
    }

    fn table_card(&self, t: &Value, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let d = self.d();
        let ro = self.read_only();
        let (schema, name) = (t["schema"].as_str().unwrap_or("").to_string(), t["name"].as_str().unwrap_or("").to_string());
        let r = q::qref(d, &schema, &name);
        let arr = |k: &str| t[k].as_array().cloned().unwrap_or_default();
        let mut card = kit::card(cx);
        let mut footer: Option<Div> = None;
        {
            let (t1, t2) = (t.clone(), t.clone());
            let (s1, n1, r1) = (schema.clone(), name.clone(), r.clone());
            let (s2, n2, r2) = (schema.clone(), name.clone(), r.clone());
            let rls = t["rls"].as_bool().unwrap_or(false);
            let r3 = r.clone();
            let p = ui::pal(cx);
            let editable = |d: Stateful<Div>| if ro { d } else { d.cursor_pointer().hover(move |s| s.border_color(p.border_strong)) };
            let name_field = editable(kit::card_field("f-name", "Table name", div().font_family(kit::cx_mono(cx)).child(name.clone()), cx))
                .min_w(px(200.))
                .when(!ro, |f| {
                    f.on_click(cx.listener(move |this, _, window, cx| {
                        let (s, n, r) = (s1.clone(), n1.clone(), r1.clone());
                        let s_after = s.clone();
                        this.rename_form("Rename table", &n, Rc::new(move |v| vec![sql(if d == D::Mysql { format!("RENAME TABLE {r} TO {}", q::qref(d, &s, v)) } else { format!("ALTER TABLE {r} RENAME TO {}", q::qi(d, v)) })]), Rc::new(move |v| json!({ "schema": s_after, "name": v })), window, cx);
                    }))
                });
            let schema_field = self.feature("schemas").then(|| {
                let schemas = self.schemas();
                editable(kit::card_field("f-schema", if d == D::Mysql { "Database" } else { "Schema" }, h_flex().w_full().gap(px(6.)).child(div().flex_1().child(schema.clone())).child(svg().path("rv/chevron.svg").size(px(14.)).text_color(p.muted)), cx))
                    .when(!ro, |f| {
                        f.on_click(cx.listener(move |this, _, window, cx| {
                            let (s, n, r) = (s2.clone(), n2.clone(), r2.clone());
                            let n_after = n.clone();
                            this.ddl_form(
                                "Move table",
                                vec![Field::choice(if d == D::Mysql { "Database" } else { "Schema" }, &schemas.iter().map(String::as_str).collect::<Vec<_>>(), &s)],
                                "Move",
                                Rc::new(move |v| Ok(if v[0] == s { vec![] } else { vec![sql(if d == D::Mysql { format!("RENAME TABLE {r} TO {}", q::qref(d, &v[0], &n)) } else { format!("ALTER TABLE {r} SET SCHEMA {}", q::qi(d, &v[0])) })] })),
                                Some(Rc::new(move |v| json!({ "schema": v[0], "name": n_after }))),
                                window,
                                cx,
                            );
                        }))
                    })
            });
            let rls_field = self.feature("rls").then(|| {
                editable(
                    h_flex()
                        .id("f-rls")
                        .gap(px(8.))
                        .px(px(10.))
                        .py(px(6.))
                        .border_1()
                        .border_color(p.border)
                        .rounded(px(6.))
                        .bg(p.panel_2)
                        .text_size(px(13.))
                        .child(svg().path("rv/lock.svg").size(px(13.)).text_color(p.muted))
                        .child("Row Level Security")
                        .child(kit::switch(rls, cx)),
                )
                .when(!ro, |f| {
                    f.on_click(cx.listener(move |this, _, window, cx| {
                        this.apply(if rls { "Disable RLS" } else { "Enable RLS" }, vec![sql(format!("ALTER TABLE {r3} {} ROW LEVEL SECURITY", if rls { "DISABLE" } else { "ENABLE" }))], false, None, None, window, cx);
                    }))
                })
            });
            card = card.child(h_flex().flex_wrap().gap(px(10.)).items_stretch().child(name_field).children(schema_field).children(rls_field));
            if !ro {
                footer = Some(
                    h_flex()
                        .gap(px(8.))
                        .pt(px(14.))
                        .border_t_1()
                        .border_color(p.border)
                        .child(kit::btn_ghost("open-data", cx).icon(kit::ico("table")).label("Open data").on_click(cx.listener(move |this, _, window, cx| {
                            let (s, n) = (t1["schema"].as_str().unwrap_or("").to_string(), t1["name"].as_str().unwrap_or("").to_string());
                            let _ = this.ws.update(cx, |ws, cx| ws.open_table(s, n, true, None, window, cx));
                        })))
                        .child(div().flex_1())
                        .child(kit::btn_danger("drop-table", cx).icon(kit::ico("trash")).label("Drop table").on_click(cx.listener(move |this, _, window, cx| {
                            let rr = q::qref(d, t2["schema"].as_str().unwrap_or(""), t2["name"].as_str().unwrap_or(""));
                            this.apply("Drop table", vec![sql(format!("DROP TABLE {rr}"))], true, Some(format!("Table \"{}\" and all its rows will be deleted.", t2["name"].as_str().unwrap_or(""))), Some(Value::Null), window, cx);
                        }))),
                );
            }
        }
        // Columns
        let tc = t.clone();
        let mut cols = Vec::new();
        for (i, c) in arr("columns").into_iter().enumerate() {
            let (c1, c2, t1) = (c.clone(), c.clone(), tc.clone());
            let r2 = r.clone();
            let actions = (!ro).then(|| {
                h_flex()
                    .gap_1()
                    .child(kit::act(("ec", i), "edit", "Edit", false, cx).on_click(cx.listener(move |this, _, window, cx| this.column_form(&t1, Some(&c1), window, cx))))
                    .child(kit::act(("dc", i), "trash", "Drop", true, cx).on_click(cx.listener(move |this, _, window, cx| {
                        let n = c2["name"].as_str().unwrap_or("").to_string();
                        this.apply("Drop column", vec![sql(format!("ALTER TABLE {r2} DROP COLUMN {}", q::qi(d, &n)))], true, Some(format!("Column \"{n}\" and its data will be deleted.")), None, window, cx);
                    })))
            });
            cols.push(def_row(c["definition"].as_str().unwrap_or(""), actions, cx));
        }
        let t_add = t.clone();
        card = card.child(section("Columns", (!ro).then(|| kit::link_btn("add-col", "Add column", cx).on_click(cx.listener(move |this, _, window, cx| this.column_form(&t_add, None, window, cx)))), cols, cx));
        // Constraints
        let mut cons = Vec::new();
        for (i, c) in arr("constraints").into_iter().enumerate() {
            let r2 = r.clone();
            let c1 = c.clone();
            let actions = (!ro && self.feature("constraints")).then(|| {
                h_flex().child(kit::act(("dk", i), "trash", "Drop", true, cx).on_click(cx.listener(move |this, _, window, cx| {
                    this.apply("Drop constraint", vec![sql(q::drop_constraint(d, &r2, c1["name"].as_str().unwrap_or(""), c1["kind"].as_str().unwrap_or("")))], true, None, None, window, cx);
                })))
            });
            cons.push(def_row(c["definition"].as_str().unwrap_or(""), actions, cx));
        }
        let t_add = t.clone();
        card = card.child(section("Constraints", (!ro && self.feature("constraints")).then(|| kit::link_btn("add-con", "Add constraint", cx).on_click(cx.listener(move |this, _, window, cx| this.constraint_form(&t_add, window, cx)))), cons, cx));
        // Indexes
        let mut idx = Vec::new();
        for (i, ix) in arr("indexes").into_iter().filter(|i| i["constraint"].is_null() || i["constraint"] == false).enumerate() {
            let (s, n) = (schema.clone(), name.clone());
            let iname = ix["name"].as_str().unwrap_or("").to_string();
            let actions = (!ro).then(|| {
                h_flex().child(kit::act(("di", i), "trash", "Drop", true, cx).on_click(cx.listener(move |this, _, window, cx| {
                    this.apply("Drop index", vec![sql(q::drop_index(d, &s, &n, &iname))], true, None, None, window, cx);
                })))
            });
            idx.push(def_row(ix["definition"].as_str().unwrap_or(""), actions, cx));
        }
        let t_add = t.clone();
        card = card.child(section("Indexes", (!ro).then(|| kit::link_btn("add-idx", "Add index", cx).on_click(cx.listener(move |this, _, window, cx| this.index_form(&t_add, window, cx)))), idx, cx));
        // Policies
        if self.feature("policies") {
            let mut pols = Vec::new();
            for (i, p) in arr("policies").into_iter().enumerate() {
                let (p1, r2) = (p.clone(), r.clone());
                let pn = p["name"].as_str().unwrap_or("").to_string();
                let actions = (!ro).then(|| {
                    h_flex()
                        .gap_1()
                        .child(kit::act(("ep", i), "edit", "Edit", false, cx).on_click(cx.listener(move |this, _, window, cx| this.policy_form(Some(&p1), None, window, cx))))
                        .child(kit::act(("dp", i), "trash", "Drop", true, cx).on_click(cx.listener(move |this, _, window, cx| {
                            this.apply("Drop policy", vec![sql(format!("DROP POLICY {} ON {r2}", q::qi(d, &pn)))], true, None, None, window, cx);
                        })))
                });
                pols.push(def_row(p["definition"].as_str().unwrap_or(""), actions, cx));
            }
            let t_add = t.clone();
            card = card.child(section("Policies", (!ro).then(|| kit::link_btn("add-pol", "Add policy", cx).on_click(cx.listener(move |this, _, window, cx| this.policy_form(None, Some(&t_add), window, cx)))), pols, cx));
        }
        let triggers = arr("triggers");
        if !triggers.is_empty() {
            let rows = triggers.iter().map(|x| def_row(x.as_str().unwrap_or(&x.to_string()), None::<Div>, cx)).collect();
            card = card.child(section("Triggers", None::<Button>, rows, cx));
        }
        let ddl = t["ddl"].as_str().unwrap_or("").trim().to_string();
        card = card.child(ddl_block(ddl, self.ddl_open, self.me.clone(), self.d() == D::Mysql, cx));
        let _ = theme;
        card.children(footer).into_any_element()
    }

    fn view_card(&self, sel: &Value, v: &Value, cx: &mut Context<Self>) -> AnyElement {
        let d = self.d();
        let ro = self.read_only();
        let (schema, name) = (sel["schema"].as_str().unwrap_or("").to_string(), sel["name"].as_str().unwrap_or("").to_string());
        let r = q::qref(d, &schema, &name);
        let mat = v["materialized"].as_bool().unwrap_or(false);
        let kw = if mat { "MATERIALIZED VIEW" } else { "VIEW" };
        let def = v["definition"].as_str().unwrap_or("").trim().to_string();
        let mut card = kit::card(cx).child(card_title(cx, if mat { "Materialized view" } else { "View" }, &format!("{}{}", if schema.is_empty() { String::new() } else { format!("{schema}.") }, name)));
        if !ro {
            let (r1, r2, r3, r4) = (r.clone(), r.clone(), r.clone(), r.clone());
            let (s1, n1, def1, def2) = (schema.clone(), name.clone(), def.clone(), def.clone());
            let (s2, n2) = (schema.clone(), name.clone());
            card = card.child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(kit::btn("rename", cx).icon(kit::ico("edit")).label("Rename").on_click(cx.listener(move |this, _, window, cx| {
                        let (r, s, def) = (r1.clone(), s1.clone(), def1.clone());
                        let s_after = s.clone();
                        this.rename_form(
                            "Rename view",
                            &n1,
                            Rc::new(move |n| match d {
                                D::Mysql => vec![sql(format!("RENAME TABLE {r} TO {}", q::qref(d, &s, n)))],
                                D::Sqlite => vec![sql(format!("DROP VIEW {r}")), sql(format!("CREATE VIEW {} AS\n{def}", q::qi(d, n)))],
                                _ => vec![sql(format!("ALTER {kw} {r} RENAME TO {}", q::qi(d, n)))],
                            }),
                            Rc::new(move |n| json!({ "schema": s_after, "name": n })),
                            window,
                            cx,
                        );
                    })))
                    .child(kit::btn("edit-def", cx).icon(kit::ico("edit")).label("Edit definition").on_click(cx.listener(move |_this, _, window, cx| {
                        let (r, def) = (r2.clone(), def2.clone());
                        let note = (mat && d != D::Mysql).then(|| "A materialized view is dropped and created again; its data is recomputed.".to_string());
                        let me = cx.weak_entity();
                        let build = move |v: &[String]| -> Result<Stmts, String> {
                            let nd = v[0].trim().trim_end_matches(';').trim().to_string();
                            if nd == def {
                                return Ok(vec![]);
                            }
                            Ok(if mat || d == D::Sqlite { vec![sql(format!("DROP {kw} {r}")), sql(format!("CREATE {kw} {r} AS\n{nd}"))] } else { vec![sql(format!("CREATE OR REPLACE VIEW {r} AS\n{nd}"))] })
                        };
                        let b2 = build.clone();
                        let opts = Options { ok: "Save".into(), danger: false, note, preview: Some(Rc::new(move |v| b2(v))), width: 720. };
                        FormDialog::open_with(
                            "Edit view definition",
                            vec![Field::sql("Definition", def2.clone())],
                            opts,
                            Rc::new(move |v, form, window, cx| {
                                let st = build(&v)?;
                                if st.is_empty() {
                                    return Err("Nothing changed".into());
                                }
                                let _ = me.update(cx, |this, cx| this.run_ddl(st, None, form, window, cx));
                                Ok(false)
                            }),
                            window,
                            cx,
                        );
                    })))
                    .when(mat, |h| h.child(kit::btn("refresh-mat", cx).icon(kit::ico("refresh")).label("Refresh data").on_click(cx.listener(move |this, _, window, cx| this.apply("Refresh materialized view", vec![sql(format!("REFRESH MATERIALIZED VIEW {r3}"))], false, None, None, window, cx)))))
                    .child(div().flex_1())
                    .child(kit::btn_ghost("open-data", cx).icon(kit::ico("table")).label("Open data").on_click(cx.listener(move |this, _, window, cx| {
                        let (s, n) = (s2.clone(), n2.clone());
                        let _ = this.ws.update(cx, |ws, cx| ws.open_table(s, n, true, None, window, cx));
                    })))
                    .child(kit::btn_danger("drop-view", cx).icon(kit::ico("trash")).label("Drop view").on_click(cx.listener(move |this, _, window, cx| this.apply("Drop view", vec![sql(format!("DROP {kw} {r4}"))], true, None, Some(Value::Null), window, cx)))),
            );
        }
        card = card.child(section("Definition", None::<Button>, vec![def_row(&def, None::<Div>, cx)], cx));
        card.child(ddl_block(v["ddl"].as_str().unwrap_or("").trim().to_string(), self.ddl_open, self.me.clone(), self.d() == D::Mysql, cx)).into_any_element()
    }

    fn enum_card(&self, sel: &Value, cat: &Value, cx: &mut Context<Self>) -> AnyElement {
        let d = self.d();
        let ro = self.read_only();
        let Some(e) = cat["enums"].as_array().into_iter().flatten().find(|x| x["schema"] == sel["schema"] && x["name"] == sel["name"]).cloned() else {
            return ui::muted("Enum not found", cx).into_any_element();
        };
        let (schema, name) = (e["schema"].as_str().unwrap_or("").to_string(), e["name"].as_str().unwrap_or("").to_string());
        let r = q::qref(d, &schema, &name);
        let vals: Vec<String> = e["values"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
        let mut rows = Vec::new();
        for (i, v) in vals.iter().enumerate() {
            let (r1, v1) = (r.clone(), v.clone());
            let actions = (!ro).then(|| {
                h_flex().child(kit::act(("rv", i), "edit", "Rename", false, cx).on_click(cx.listener(move |this, _, window, cx| {
                    let (r, old) = (r1.clone(), v1.clone());
                    this.ddl_form(&format!("Rename value {old}"), vec![Field::line("New value", old.clone())], "Apply", Rc::new(move |x| Ok(vec![sql(format!("ALTER TYPE {r} RENAME VALUE {} TO {}", q::lit(&old), q::lit(&q::req(&x[0], "the value")?)))])), None, window, cx);
                })))
            });
            rows.push(def_row(&q::lit(v), actions, cx));
        }
        let (r1, r2, r3) = (r.clone(), r.clone(), r.clone());
        let (s1, n1) = (schema.clone(), name.clone());
        let vals2 = vals.clone();
        kit::card(cx).child(card_title(cx, "Enum", &format!("{schema}.{name}")))
            .child(def_row(&format!("CREATE TYPE {r} AS ENUM ({});", vals.iter().map(|v| q::lit(v)).collect::<Vec<_>>().join(", ")), None::<Div>, cx))
            .when(!ro, |c| {
                c.child(h_flex().gap_2().child(kit::btn("rename", cx).icon(kit::ico("edit")).label("Rename").on_click(cx.listener(move |this, _, window, cx| {
                    let (r, s) = (r1.clone(), s1.clone());
                    this.rename_form("Rename enum", &n1, Rc::new(move |n| vec![sql(format!("ALTER TYPE {r} RENAME TO {}", q::qi(d, n)))]), Rc::new(move |n| json!({ "schema": s, "name": n })), window, cx);
                }))))
            })
            .child(section(
                "Values",
                (!ro).then(|| {
                    kit::link_btn("add-val", "Add value", cx).on_click(cx.listener(move |this, _, window, cx| {
                        let r = r2.clone();
                        let mut pos = vec![("end".to_string(), "At the end".to_string())];
                        for v in &vals2 {
                            pos.push((format!("before:{v}"), format!("Before {v}")));
                            pos.push((format!("after:{v}"), format!("After {v}")));
                        }
                        this.ddl_form(
                            "Add value",
                            vec![Field::line("Value", ""), Field::choice_labeled("Position", pos, "end")],
                            "Apply",
                            Rc::new(move |x| {
                                let mut s = format!("ALTER TYPE {r} ADD VALUE {}", q::lit(&q::req(&x[0], "the value")?));
                                if let Some((k, v)) = x[1].split_once(':') {
                                    s.push_str(&format!(" {} {}", k.to_uppercase(), q::lit(v)));
                                }
                                Ok(vec![sql(s)])
                            }),
                            None,
                            window,
                            cx,
                        );
                    }))
                }),
                rows,
                cx,
            ))
            .child(ui::muted("PostgreSQL cannot remove a value from an enum.", cx))
            .when(!ro, |c| {
                c.child(h_flex().child(div().flex_1()).child(kit::btn_danger("drop-enum", cx).icon(kit::ico("trash")).label("Drop enum").on_click(cx.listener(move |this, _, window, cx| {
                    this.apply("Drop enum", vec![sql(format!("DROP TYPE {r3}"))], true, Some("Fails while a column still uses this type.".into()), Some(Value::Null), window, cx);
                }))))
            })
            .into_any_element()
    }

    fn role_card(&self, sel: &Value, r: &Value, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let d = self.d();
        let ro = self.read_only();
        let rname = sel["name"].as_str().unwrap_or("").to_string();
        let n = q::qi(d, &rname);
        let mut flags = v_flex().gap_1();
        for (i, (key, label, kw, desc)) in q::ROLE_FLAGS.iter().enumerate() {
            let on = r[*key].as_bool().unwrap_or(false);
            let n1 = n.clone();
            flags = flags.child(
                h_flex()
                    .gap_3()
                    .child(div().w(px(160.)).text_sm().child(label.to_string()))
                    .child(div().id(("flag", i)).when(!ro, |d| d.cursor_pointer()).child(kit::switch(on, cx)).on_click(cx.listener(move |this, _, window, cx| {
                        if ro {
                            return;
                        }
                        this.apply(&format!("{label}: {}", if on { "off" } else { "on" }), vec![sql(format!("ALTER ROLE {n1} {}{kw}", if on { "NO" } else { "" }))], false, None, None, window, cx);
                    })))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(desc.to_string())),
            );
        }
        let limit = r["connectionLimit"].as_i64().unwrap_or(-1);
        let until = r["validUntil"].as_str().unwrap_or("").to_string();
        let mut privs = Vec::new();
        for (i, g) in r["privileges"].as_array().cloned().unwrap_or_default().into_iter().enumerate() {
            let (gs, gt) = (g["schema"].as_str().unwrap_or("").to_string(), g["table"].as_str().unwrap_or("").to_string());
            let ps: Vec<String> = g["privileges"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(str::to_string)).collect();
            let n1 = n.clone();
            let target = q::qref(d, &gs, &gt);
            let t2 = target.clone();
            let actions = (!ro).then(|| {
                h_flex().child(kit::act(("rv", i), "trash", "Revoke all", true, cx).on_click(cx.listener(move |this, _, window, cx| this.apply("Revoke privileges", vec![sql(format!("REVOKE ALL ON {t2} FROM {n1}"))], true, None, None, window, cx))))
            });
            privs.push(def_row(&format!("GRANT {} ON {target} TO {n};", ps.join(", ")), actions, cx));
        }
        let (n1, n2, n3, n4, n5) = (n.clone(), n.clone(), n.clone(), n.clone(), n.clone());
        let rn = rname.clone();
        let tables: Vec<(String, String)> = std::iter::once(("*".to_string(), "All tables in schema".to_string()))
            .chain(self.catalog.as_ref().and_then(|c| c["tables"].as_array().cloned()).unwrap_or_default().iter().map(|t| {
                let k = format!("{}.{}", t["schema"].as_str().unwrap_or(""), t["name"].as_str().unwrap_or(""));
                (k.clone(), k)
            }))
            .collect();
        let schemas = self.schemas();
        let sf = self.schema_filter.clone();
        kit::card(cx).child(card_title(cx, "Role", &rname))
            .child(def_row(r["definition"].as_str().unwrap_or(""), None::<Div>, cx))
            .when(!ro, |c| {
                c.child(
                    h_flex()
                        .gap_2()
                        .child(kit::btn("rename", cx).icon(kit::ico("edit")).label("Rename").on_click(cx.listener(move |this, _, window, cx| {
                            let n = n1.clone();
                            this.rename_form("Rename role", &rn, Rc::new(move |x| vec![sql(format!("ALTER ROLE {n} RENAME TO {}", q::qi(d, x)))]), Rc::new(|x| json!({ "name": x })), window, cx);
                        })))
                        .child(kit::btn("password", cx).icon(kit::ico("key")).label("Reset password").on_click(cx.listener(move |this, _, window, cx| {
                            let n = n2.clone();
                            this.ddl_form(
                                "Reset password",
                                vec![Field::line("New password", "").masked(), Field::line("Repeat password", "").masked()],
                                "Apply",
                                Rc::new(move |v| {
                                    let p = q::req(&v[0], "the password")?;
                                    if p != v[1] {
                                        return Err("The passwords do not match".into());
                                    }
                                    Ok(vec![(format!("ALTER ROLE {n} PASSWORD {}", q::lit(&p)), format!("ALTER ROLE {n} PASSWORD '••••••••'"))])
                                }),
                                None,
                                window,
                                cx,
                            );
                        })))
                        .child(kit::btn("limits", cx).icon(kit::ico("user")).label(format!("Connection limit: {}", if limit < 0 { "none".to_string() } else { limit.to_string() })).on_click(cx.listener(move |this, _, window, cx| {
                            let n = n3.clone();
                            this.ddl_form(
                                "Connection limit",
                                vec![Field::line("Connections (empty: no limit)", if limit < 0 { String::new() } else { limit.to_string() })],
                                "Apply",
                                Rc::new(move |v| {
                                    let x = if v[0].trim().is_empty() { -1 } else { v[0].trim().parse::<i64>().map_err(|_| "Enter a whole number".to_string())? };
                                    Ok(vec![sql(format!("ALTER ROLE {n} CONNECTION LIMIT {x}"))])
                                }),
                                None,
                                window,
                                cx,
                            );
                        })))
                        .child(kit::btn("until", cx).icon(kit::ico("history")).label(format!("Valid until: {}", if until.is_empty() { "always".to_string() } else { until.clone() })).on_click(cx.listener(move |this, _, window, cx| {
                            let n = n4.clone();
                            this.ddl_form(
                                "Valid until",
                                vec![Field::line("Date and time (empty: always)", "").hint("2027-01-31 23:59")],
                                "Apply",
                                Rc::new(move |v| Ok(vec![sql(format!("ALTER ROLE {n} VALID UNTIL {}", if v[0].trim().is_empty() { "'infinity'".to_string() } else { q::lit(v[0].trim()) }))])),
                                None,
                                window,
                                cx,
                            );
                        }))),
                )
            })
            .child(flags)
            .when_some(r["memberOf"].as_array().filter(|a| !a.is_empty()).map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")), |c, m| c.child(ui::muted(format!("Member of: {m}"), cx)))
            .child(section(
                "Privileges",
                (!ro).then(|| {
                    kit::link_btn("grant", "Add privilege", cx).on_click(cx.listener(move |this, _, window, cx| {
                        let n = n5.clone();
                        let mut fields = vec![Field::choice_labeled("Table", tables.clone(), "*"), Field::choice("Schema (for \"All tables\")", &schemas.iter().map(String::as_str).collect::<Vec<_>>(), &sf)];
                        for p in q::PG_PRIVILEGES {
                            fields.push(Field::check(p, *p == "SELECT"));
                        }
                        this.ddl_form(
                            "Grant privileges",
                            fields,
                            "Apply",
                            Rc::new(move |v| {
                                let ps: Vec<&str> = q::PG_PRIVILEGES.iter().enumerate().filter(|(i, _)| v[2 + i] == "true").map(|(_, p)| *p).collect();
                                if ps.is_empty() {
                                    return Err("Select at least one privilege".into());
                                }
                                let target = if v[0] == "*" {
                                    format!("ALL TABLES IN SCHEMA {}", q::qi(d, &v[1]))
                                } else {
                                    let (s, t) = v[0].split_once('.').unwrap_or(("", &v[0]));
                                    format!("TABLE {}", q::qref(d, s, t))
                                };
                                Ok(vec![sql(format!("GRANT {} ON {target} TO {n}", ps.join(", ")))])
                            }),
                            None,
                            window,
                            cx,
                        );
                    }))
                }),
                privs,
                cx,
            ))
            .when(!ro, |c| {
                let n = n.clone();
                c.child(h_flex().child(div().flex_1()).child(kit::btn_danger("drop-role", cx).icon(kit::ico("trash")).label("Drop role").on_click(cx.listener(move |this, _, window, cx| {
                    this.apply("Drop role", vec![sql(format!("DROP ROLE {n}"))], true, Some("Fails while the role owns objects or has privileges.".into()), Some(Value::Null), window, cx);
                }))))
            })
            .into_any_element()
    }

    fn policy_card(&self, sel: &Value, cat: &Value, cx: &mut Context<Self>) -> AnyElement {
        let d = self.d();
        let ro = self.read_only();
        let Some(p) = cat["policies"].as_array().into_iter().flatten().find(|x| x["schema"] == sel["schema"] && x["table"] == sel["table"] && x["name"] == sel["name"]).cloned() else {
            return ui::muted("Policy not found", cx).into_any_element();
        };
        let (s, t, n) = (p["schema"].as_str().unwrap_or("").to_string(), p["table"].as_str().unwrap_or("").to_string(), p["name"].as_str().unwrap_or("").to_string());
        let roles: Vec<&str> = p["roles"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        let facts = format!("Command {} · {} · Roles {}", p["command"].as_str().unwrap_or(""), if p["permissive"] == false { "RESTRICTIVE" } else { "PERMISSIVE" }, if roles.is_empty() { "public".to_string() } else { roles.join(", ") });
        let (p1, s1, t1) = (p.clone(), s.clone(), t.clone());
        let drop_sql = format!("DROP POLICY {} ON {}", q::qi(d, &n), q::qref(d, &s, &t));
        kit::card(cx).child(card_title(cx, "Policy", &format!("{n} on {s}.{t}")))
            .child(def_row(p["definition"].as_str().unwrap_or(""), None::<Div>, cx))
            .child(ui::muted(facts, cx))
            .when(!ro, |c| {
                c.child(
                    h_flex()
                        .gap_2()
                        .child(kit::btn("edit", cx).icon(kit::ico("edit")).label("Edit").on_click(cx.listener(move |this, _, window, cx| this.policy_form(Some(&p1), None, window, cx))))
                        .child(kit::btn_ghost("open-table", cx).icon(kit::ico("table")).label("Open table").on_click(cx.listener(move |this, _, _, cx| {
                            this.section = "tables".into();
                            this.select(Some(json!({ "schema": s1, "name": t1 })), cx);
                        })))
                        .child(div().flex_1())
                        .child(kit::btn_danger("drop", cx).icon(kit::ico("trash")).label("Drop policy").on_click(cx.listener(move |this, _, window, cx| this.apply("Drop policy", vec![sql(drop_sql.clone())], true, None, Some(Value::Null), window, cx)))),
                )
            })
            .into_any_element()
    }
}

fn sql(s: String) -> (String, String) {
    (s.clone(), s)
}

/// "name type [pk] [not null] [default <expr>]" per line.
fn parse_col_lines(text: &str) -> Vec<NewCol> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            let lower = l.to_lowercase();
            let (head, default) = match lower.find(" default ") {
                Some(i) => (&l[..i], l[i + 9..].trim().to_string()),
                None => (l, String::new()),
            };
            let mut words: Vec<&str> = head.split_whitespace().collect();
            let name = words.first()?.to_string();
            words.remove(0);
            let mut pk = false;
            let mut not_null = false;
            let mut ty = Vec::new();
            let mut i = 0;
            while i < words.len() {
                let w = words[i].to_lowercase();
                if w == "pk" || (w == "primary" && words.get(i + 1).is_some_and(|x| x.eq_ignore_ascii_case("key"))) {
                    pk = true;
                    i += if w == "pk" { 1 } else { 2 };
                } else if w == "not" && words.get(i + 1).is_some_and(|x| x.eq_ignore_ascii_case("null")) {
                    not_null = true;
                    i += 2;
                } else if w == "nn" {
                    not_null = true;
                    i += 1;
                } else {
                    ty.push(words[i]);
                    i += 1;
                }
            }
            Some(NewCol { name, type_name: ty.join(" "), not_null, pk, default })
        })
        .collect()
}

/// The name of the object as a `.card-field` (label + monospace value).
fn card_title(cx: &App, kind: &str, name: &str) -> Div {
    h_flex().gap(px(10.)).flex_wrap().child(kit::card_field(SharedString::from(format!("title-{kind}")), format!("{kind} name"), div().font_family(kit::cx_mono(cx)).child(name.to_string()), cx))
}

fn def_row(text: &str, actions: Option<impl IntoElement>, cx: &App) -> AnyElement {
    let id = SharedString::from(format!("d{}", ROW_ID.with(|c| { let v = c.get(); c.set(v + 1); v })));
    kit::def_row(id, text, false, actions.map(|a| a.into_any_element()), false, cx)
}

thread_local! {
    static ROW_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn section(title: &str, add: Option<Button>, rows: Vec<AnyElement>, cx: &App) -> Div {
    kit::card_section(title, add, rows, cx)
}

/// The DDL of the object, collapsed by default (`<details class="card-ddl">`).
fn ddl_block(ddl: String, open: bool, me: WeakEntity<SchemaView>, mysql: bool, cx: &App) -> Div {
    let p = ui::pal(cx);
    let copy = ddl.clone();
    v_flex()
        .gap(px(6.))
        .child(
            h_flex()
                .id("ddl-head")
                .gap(px(6.))
                .cursor_pointer()
                .text_size(px(11.5))
                .font_semibold()
                .text_color(p.muted)
                .on_click(move |_, _, cx| {
                    let _ = me.update(cx, |this, cx| {
                        this.ddl_open = !this.ddl_open;
                        cx.notify();
                    });
                })
                .child(svg().path(if open { "rv/chevron.svg" } else { "rv/right.svg" }).size(px(12.)).text_color(p.muted))
                .child("DDL"),
        )
        .when(open, |d| {
            d.child(
                h_flex()
                    .id("ddl-body")
                    .group("ddl")
                    .items_start()
                    .gap(px(8.))
                    .px(px(12.))
                    .py(px(9.))
                    .bg(p.panel_2)
                    .border_1()
                    .border_color(p.border)
                    .rounded(px(6.))
                    .child(kit::sql_block(&ddl, mysql, cx))
                    .child(kit::act("copy-ddl", "copy", "Copy", false, cx).on_click(move |_, window, cx| {
                        ui::copy(copy.clone(), cx);
                        ui::toast_ok(window, cx, "Copied");
                    })),
            )
        })
}

impl Render for SchemaView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ui::pal(cx);
        let sections: Vec<(&str, &str)> = SECTIONS.iter().filter(|s| s.2.is_none_or(|f| self.feature(f))).map(|s| (s.0, s.1)).collect();
        let icon_of = |id: &str| match id {
            "schemas" => "folder",
            "tables" => "table",
            "views" => "view",
            "enums" => "list",
            "roles" => "user",
            _ => "shield",
        };
        let seg = h_flex()
            .flex_none()
            .p(px(2.))
            .gap(px(2.))
            .bg(p.panel_2)
            .border_1()
            .border_color(p.border)
            .rounded(px(6.))
            .children(sections.into_iter().map(|(id, label)| {
                let on = self.section == id;
                h_flex()
                    .id(SharedString::from(format!("sec-{id}")))
                    .h(px(26.))
                    .px(px(10.))
                    .gap(px(6.))
                    .rounded(px(4.))
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .map(|d| if on { d.bg(p.elev).text_color(p.text).border_1().border_color(p.border_strong) } else { d.text_color(p.muted).hover(|s| s.text_color(p.text)) })
                    .child(svg().path(SharedString::from(format!("rv/{}.svg", icon_of(id)))).size(px(13.)).text_color(if on { p.text } else { p.muted }))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = id.to_string();
                        this.select(None, cx);
                    }))
            }));
        let bar = kit::toolbar(cx)
            .child(kit::group().child(seg))
            .child(
                kit::group()
                    .when_some(self.catalog.as_ref().and_then(|c| c["readOnlyReason"].as_str().map(str::to_string)), |d, r| {
                        d.child(div().id("ro").tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(r.clone()).build(window, cx)).child(kit::badge("read-only", true, cx)))
                    })
                    .child(
                        h_flex()
                            .child(kit::btn("copy-schema", cx).rounded_r(px(0.)).icon(kit::ico("copy")).label("Copy schema").tooltip("Copy the DDL of the whole database").on_click(cx.listener(|this, _, window, cx| this.copy_schema(false, window, cx))))
                            .child(kit::btn("open-schema", cx).rounded_l(px(0.)).ml(px(-1.)).px(px(8.)).icon(kit::ico("external")).tooltip("Open the DDL in a SQL console").on_click(cx.listener(|this, _, window, cx| this.copy_schema(true, window, cx)))),
                    ),
            );
        let body: AnyElement = match (&self.error, &self.catalog) {
            (Some(e), _) => div().m(px(12.)).p(px(12.)).rounded(px(6.)).bg(p.danger_soft).text_color(p.danger).font_family(kit::cx_mono(cx)).text_size(px(12.)).child(e.clone()).into_any_element(),
            (None, None) => div().p(px(40.)).flex().justify_center().text_size(px(13.)).text_color(p.muted).child("Loading…").into_any_element(),
            _ => h_flex().size_full().items_start().gap(px(16.)).p(px(16.)).child(self.render_list(cx)).child(div().flex_1().min_w_0().h_full().child(self.render_detail(cx))).into_any_element(),
        };
        v_flex().size_full().child(bar).child(div().relative().flex_1().min_h_0().bg(p.bg).child(kit::grid_bg(cx)).child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_col_lines;

    #[test]
    fn parses_column_lines() {
        let c = parse_col_lines("id integer pk\nname varchar(255) not null default 'x'\n\nprice numeric(12,2)");
        assert_eq!(c.len(), 3);
        assert!(c[0].pk && c[0].type_name == "integer");
        assert!(c[1].not_null && c[1].default == "'x'" && c[1].type_name == "varchar(255)");
        assert_eq!(c[2].type_name, "numeric(12,2)");
    }
}
