//! Schema management ("Schema" view): catalog, per-object DDL, full dump, DDL execution.
//!
//! The server introspects structure and renders SQL text for display. The browser builds
//! change statements (ALTER / CREATE / DROP / GRANT …) and sends them to [`execute`] after the
//! user reviewed them.

use crate::db::{quote_ident, table_ref, Db, Dialect, Sql};
use anyhow::{bail, Result};
use serde::Serialize;

const SEP: char = '\u{1f}';
const PG_SYS: &str = "n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%' AND n.nspname NOT LIKE 'pg_temp%'";
const MY_SYS: &str = "('information_schema','mysql','performance_schema','sys')";

fn s(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

fn split_sep(v: &Option<String>) -> Vec<String> {
    v.as_deref()
        .filter(|x| !x.is_empty())
        .map(|x| x.split(SEP).map(str::to_string).collect())
        .unwrap_or_default()
}

fn truthy(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("true" | "t" | "1" | "YES"))
}

fn q(d: Dialect, text: impl Into<String>, params: &[&str]) -> Sql {
    let mut sql = Sql::new(d);
    let mut text: String = text.into();
    // "{}" placeholders are replaced by dialect parameters in order.
    for p in params {
        let ph = sql.param(Some((*p).to_string()));
        text = text.replacen("{}", &ph, 1);
    }
    sql.text = text;
    sql
}

// ---------------------------------------------------------------- types

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    pub schemas: bool,
    pub rename_schema: bool,
    pub enums: bool,
    pub roles: bool,
    pub policies: bool,
    pub rls: bool,
    pub alter_column: bool,
    pub constraints: bool,
    pub index_methods: bool,
    pub materialized_views: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableItem {
    pub schema: String,
    pub name: String,
    pub rls: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewItem {
    pub schema: String,
    pub name: String,
    pub materialized: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumItem {
    pub schema: String,
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleItem {
    pub name: String,
    pub system: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub schema: String,
    pub table: String,
    pub name: String,
    pub permissive: bool,
    pub command: String,
    pub roles: Vec<String>,
    pub using: Option<String>,
    pub with_check: Option<String>,
    pub definition: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub dialect: Dialect,
    pub features: Features,
    pub default_schema: String,
    pub schemas: Vec<String>,
    pub tables: Vec<TableItem>,
    pub views: Vec<ViewItem>,
    pub enums: Vec<EnumItem>,
    pub roles: Vec<RoleItem>,
    pub policies: Vec<Policy>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub not_null: bool,
    pub default: Option<String>,
    pub primary: bool,
    /// "ALWAYS" / "BY DEFAULT" for identity columns.
    pub identity: Option<String>,
    /// Expression of a generated (computed) column.
    pub generated: Option<String>,
    pub auto_increment: bool,
    /// The column as it appears in CREATE TABLE.
    pub definition: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintDef {
    pub name: String,
    /// primary | unique | foreign | check | exclude
    pub kind: String,
    pub definition: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDef {
    pub name: String,
    pub unique: bool,
    /// The index backs a constraint (primary key / unique) and is dropped with it.
    pub constraint: bool,
    pub definition: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDetail {
    pub schema: String,
    pub name: String,
    pub rls: bool,
    pub columns: Vec<ColumnDef>,
    pub constraints: Vec<ConstraintDef>,
    pub indexes: Vec<IndexDef>,
    pub policies: Vec<Policy>,
    pub triggers: Vec<String>,
    pub ddl: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewDetail {
    pub schema: String,
    pub name: String,
    pub materialized: bool,
    pub definition: String,
    pub ddl: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub schema: String,
    pub table: String,
    pub privileges: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDetail {
    pub name: String,
    pub superuser: bool,
    pub create_db: bool,
    pub create_role: bool,
    pub inherit: bool,
    pub can_login: bool,
    pub bypass_rls: bool,
    pub replication: bool,
    pub connection_limit: i64,
    pub valid_until: Option<String>,
    pub member_of: Vec<String>,
    pub privileges: Vec<Grant>,
    pub definition: String,
}

// ---------------------------------------------------------------- catalog

pub async fn catalog(db: &Db) -> Result<Catalog> {
    match db.dialect() {
        Dialect::Postgres => pg_catalog(db).await,
        Dialect::Mysql => my_catalog(db).await,
        Dialect::Sqlite => lite_catalog(db).await,
    }
}

async fn pg_catalog(db: &Db) -> Result<Catalog> {
    let d = Dialect::Postgres;
    let default_schema = db
        .fetch_strings(&Sql::raw(d, "SELECT current_schema()::text"))
        .await?
        .first()
        .and_then(|r| r[0].clone())
        .unwrap_or_else(|| "public".into());
    let schemas = db
        .fetch_strings(&Sql::raw(d, format!("SELECT n.nspname::text FROM pg_namespace n WHERE {PG_SYS} ORDER BY 1")))
        .await?
        .into_iter()
        .map(|r| s(&r[0]))
        .collect();
    let rels = db
        .fetch_strings(&Sql::raw(
            d,
            format!(
                "SELECT n.nspname::text, c.relname::text, c.relkind::text, c.relrowsecurity::text
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
                 WHERE c.relkind IN ('r','p','v','m') AND NOT c.relispartition AND {PG_SYS}
                 ORDER BY 1, 2"
            ),
        ))
        .await?;
    let mut tables = Vec::new();
    let mut views = Vec::new();
    for r in rels {
        match s(&r[2]).as_str() {
            "v" | "m" => views.push(ViewItem { schema: s(&r[0]), name: s(&r[1]), materialized: s(&r[2]) == "m" }),
            _ => tables.push(TableItem { schema: s(&r[0]), name: s(&r[1]), rls: truthy(&r[3]) }),
        }
    }
    let enums = db
        .fetch_strings(&Sql::raw(
            d,
            format!(
                "SELECT n.nspname::text, t.typname::text, string_agg(e.enumlabel::text, chr(31) ORDER BY e.enumsortorder)
                 FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace JOIN pg_enum e ON e.enumtypid = t.oid
                 WHERE {PG_SYS} GROUP BY 1, 2 ORDER BY 1, 2"
            ),
        ))
        .await?
        .into_iter()
        .map(|r| EnumItem { schema: s(&r[0]), name: s(&r[1]), values: split_sep(&r[2]) })
        .collect();
    let roles = db
        .fetch_strings(&Sql::raw(d, "SELECT rolname::text FROM pg_roles ORDER BY rolname LIKE 'pg\\_%', rolname"))
        .await?
        .into_iter()
        .map(|r| {
            let name = s(&r[0]);
            RoleItem { system: name.starts_with("pg_"), name }
        })
        .collect();
    let policies = pg_policies(db, None).await?;
    Ok(Catalog {
        dialect: d,
        features: Features {
            schemas: true,
            rename_schema: true,
            enums: true,
            roles: true,
            policies: true,
            rls: true,
            alter_column: true,
            constraints: true,
            index_methods: true,
            materialized_views: true,
        },
        default_schema,
        schemas,
        tables,
        views,
        enums,
        roles,
        policies,
    })
}

async fn pg_policies(db: &Db, table: Option<(&str, &str)>) -> Result<Vec<Policy>> {
    let d = Dialect::Postgres;
    let base = "SELECT schemaname::text, tablename::text, policyname::text, permissive::text, cmd::text,
                       array_to_string(roles, chr(31)), qual::text, with_check::text
                FROM pg_policies";
    let sql = match table {
        Some((sc, t)) => q(d, format!("{base} WHERE schemaname = {{}} AND tablename = {{}} ORDER BY policyname"), &[sc, t]),
        None => Sql::raw(d, format!("{base} ORDER BY 1, 2, 3")),
    };
    Ok(db
        .fetch_strings(&sql)
        .await?
        .into_iter()
        .map(|r| {
            let mut p = Policy {
                schema: s(&r[0]),
                table: s(&r[1]),
                name: s(&r[2]),
                permissive: s(&r[3]).eq_ignore_ascii_case("PERMISSIVE"),
                command: s(&r[4]),
                roles: split_sep(&r[5]),
                using: r[6].clone(),
                with_check: r[7].clone(),
                definition: String::new(),
            };
            p.definition = policy_sql(&p);
            p
        })
        .collect())
}

fn policy_sql(p: &Policy) -> String {
    let d = Dialect::Postgres;
    let mut out = format!(
        "CREATE POLICY {} ON {} AS {} FOR {} TO {}",
        quote_ident(d, &p.name),
        table_ref(d, &p.schema, &p.table),
        if p.permissive { "PERMISSIVE" } else { "RESTRICTIVE" },
        p.command,
        if p.roles.is_empty() { "public".to_string() } else { p.roles.iter().map(|r| if r == "public" { r.clone() } else { quote_ident(d, r) }).collect::<Vec<_>>().join(", ") }
    );
    if let Some(u) = &p.using {
        out.push_str(&format!(" USING ({u})"));
    }
    if let Some(w) = &p.with_check {
        out.push_str(&format!(" WITH CHECK ({w})"));
    }
    out.push(';');
    out
}

async fn my_catalog(db: &Db) -> Result<Catalog> {
    let d = Dialect::Mysql;
    let default_schema = db
        .fetch_strings(&Sql::raw(d, "SELECT CAST(DATABASE() AS CHAR)"))
        .await?
        .first()
        .and_then(|r| r[0].clone())
        .unwrap_or_default();
    let schemas = db
        .fetch_strings(&Sql::raw(
            d,
            format!("SELECT CAST(SCHEMA_NAME AS CHAR) FROM information_schema.SCHEMATA WHERE SCHEMA_NAME NOT IN {MY_SYS} ORDER BY 1"),
        ))
        .await?
        .into_iter()
        .map(|r| s(&r[0]))
        .collect();
    let rels = db
        .fetch_strings(&Sql::raw(
            d,
            format!(
                "SELECT CAST(TABLE_SCHEMA AS CHAR), CAST(TABLE_NAME AS CHAR), CAST(TABLE_TYPE AS CHAR)
                 FROM information_schema.TABLES WHERE TABLE_SCHEMA NOT IN {MY_SYS} ORDER BY 1, 2"
            ),
        ))
        .await?;
    let mut tables = Vec::new();
    let mut views = Vec::new();
    for r in rels {
        if s(&r[2]) == "VIEW" {
            views.push(ViewItem { schema: s(&r[0]), name: s(&r[1]), materialized: false });
        } else {
            tables.push(TableItem { schema: s(&r[0]), name: s(&r[1]), rls: false });
        }
    }
    Ok(Catalog {
        dialect: d,
        features: Features {
            schemas: true,
            rename_schema: false,
            enums: false,
            roles: false,
            policies: false,
            rls: false,
            alter_column: true,
            constraints: true,
            index_methods: false,
            materialized_views: false,
        },
        default_schema,
        schemas,
        tables,
        views,
        enums: vec![],
        roles: vec![],
        policies: vec![],
    })
}

async fn lite_catalog(db: &Db) -> Result<Catalog> {
    let d = Dialect::Sqlite;
    let rows = db
        .fetch_strings(&Sql::raw(
            d,
            "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
        ))
        .await?;
    let mut tables = Vec::new();
    let mut views = Vec::new();
    for r in rows {
        if s(&r[1]) == "view" {
            views.push(ViewItem { schema: "main".into(), name: s(&r[0]), materialized: false });
        } else {
            tables.push(TableItem { schema: "main".into(), name: s(&r[0]), rls: false });
        }
    }
    Ok(Catalog {
        dialect: d,
        features: Features {
            schemas: false,
            rename_schema: false,
            enums: false,
            roles: false,
            policies: false,
            rls: false,
            alter_column: false,
            constraints: false,
            index_methods: false,
            materialized_views: false,
        },
        default_schema: "main".into(),
        schemas: vec!["main".into()],
        tables,
        views,
        enums: vec![],
        roles: vec![],
        policies: vec![],
    })
}

// ---------------------------------------------------------------- table

pub async fn table(db: &Db, schema: &str, name: &str) -> Result<TableDetail> {
    match db.dialect() {
        Dialect::Postgres => pg_table(db, schema, name, true).await,
        Dialect::Mysql => my_table(db, schema, name).await,
        Dialect::Sqlite => lite_table(db, name).await,
    }
}

const PG_REL: &str = "(SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND c.relname = {})";

async fn pg_table(db: &Db, schema: &str, name: &str, inline_fks: bool) -> Result<TableDetail> {
    let d = Dialect::Postgres;
    let rel = db
        .fetch_strings(&q(
            d,
            format!("SELECT c.relrowsecurity::text FROM pg_class c WHERE c.oid = {PG_REL}"),
            &[schema, name],
        ))
        .await?;
    let Some(rel) = rel.first() else { bail!("Table {schema}.{name} not found") };
    let rls = truthy(&rel[0]);

    let pk_cols: Vec<String> = db
        .fetch_strings(&q(
            d,
            format!(
                "SELECT a.attname::text FROM pg_constraint k
                 JOIN pg_attribute a ON a.attrelid = k.conrelid AND a.attnum = ANY(k.conkey)
                 WHERE k.contype = 'p' AND k.conrelid = {PG_REL}"
            ),
            &[schema, name],
        ))
        .await?
        .into_iter()
        .map(|r| s(&r[0]))
        .collect();

    let cols = db
        .fetch_strings(&q(
            d,
            format!(
                "SELECT a.attname::text, pg_catalog.format_type(a.atttypid, a.atttypmod), a.attnotnull::text,
                        pg_get_expr(ad.adbin, ad.adrelid), a.attidentity::text, a.attgenerated::text
                 FROM pg_attribute a LEFT JOIN pg_attrdef ad ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum
                 WHERE a.attrelid = {PG_REL} AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum"
            ),
            &[schema, name],
        ))
        .await?;
    let single_pk = pk_cols.len() == 1;
    let columns: Vec<ColumnDef> = cols
        .into_iter()
        .map(|r| {
            let col = s(&r[0]);
            let identity = match s(&r[4]).as_str() {
                "a" => Some("ALWAYS".to_string()),
                "d" => Some("BY DEFAULT".to_string()),
                _ => None,
            };
            let generated = if s(&r[5]) == "s" { r[3].clone() } else { None };
            let default = if generated.is_some() { None } else { r[3].clone() };
            let primary = pk_cols.contains(&col);
            let not_null = truthy(&r[2]);
            let mut def = format!("{} {}", quote_ident(d, &col), s(&r[1]));
            if primary && single_pk {
                def.push_str(" PRIMARY KEY");
            }
            if let Some(i) = &identity {
                def.push_str(&format!(" GENERATED {i} AS IDENTITY"));
            }
            if let Some(g) = &generated {
                def.push_str(&format!(" GENERATED ALWAYS AS ({g}) STORED"));
            }
            if not_null && !(primary && single_pk) {
                def.push_str(" NOT NULL");
            }
            if let Some(x) = &default {
                def.push_str(&format!(" DEFAULT {x}"));
            }
            ColumnDef {
                auto_increment: default.as_deref().is_some_and(|x| x.starts_with("nextval(")),
                name: col,
                type_name: s(&r[1]),
                not_null,
                default,
                primary,
                identity,
                generated,
                definition: def,
            }
        })
        .collect();

    let constraints: Vec<ConstraintDef> = db
        .fetch_strings(&q(
            d,
            format!(
                "SELECT conname::text, contype::text, pg_get_constraintdef(oid, true) FROM pg_constraint
                 WHERE conrelid = {PG_REL} AND contype IN ('p','u','f','c','x')
                 ORDER BY CASE contype WHEN 'p' THEN 0 WHEN 'u' THEN 1 WHEN 'f' THEN 2 WHEN 'c' THEN 3 ELSE 4 END, conname"
            ),
            &[schema, name],
        ))
        .await?
        .into_iter()
        .map(|r| ConstraintDef {
            name: s(&r[0]),
            kind: match s(&r[1]).as_str() {
                "p" => "primary",
                "u" => "unique",
                "f" => "foreign",
                "c" => "check",
                _ => "exclude",
            }
            .into(),
            definition: format!("CONSTRAINT {} {}", quote_ident(d, &s(&r[0])), s(&r[2])),
        })
        .collect();

    let indexes: Vec<IndexDef> = db
        .fetch_strings(&q(
            d,
            format!(
                "SELECT i.relname::text, pg_get_indexdef(ix.indexrelid), ix.indisunique::text,
                        EXISTS (SELECT 1 FROM pg_constraint k WHERE k.conindid = ix.indexrelid AND k.contype IN ('p','u','x'))::text
                 FROM pg_index ix JOIN pg_class i ON i.oid = ix.indexrelid
                 WHERE ix.indrelid = {PG_REL} ORDER BY ix.indisprimary DESC, i.relname"
            ),
            &[schema, name],
        ))
        .await?
        .into_iter()
        .map(|r| IndexDef { name: s(&r[0]), definition: format!("{};", s(&r[1])), unique: truthy(&r[2]), constraint: truthy(&r[3]) })
        .collect();

    let policies = pg_policies(db, Some((schema, name))).await?;
    let triggers = db
        .fetch_strings(&q(
            d,
            format!("SELECT pg_get_triggerdef(oid, true) FROM pg_trigger WHERE tgrelid = {PG_REL} AND NOT tgisinternal ORDER BY tgname"),
            &[schema, name],
        ))
        .await?
        .into_iter()
        .map(|r| format!("{};", s(&r[0])))
        .collect::<Vec<_>>();

    // CREATE TABLE text
    let mut body: Vec<String> = columns.iter().map(|c| c.definition.clone()).collect();
    for c in &constraints {
        let inline_pk = c.kind == "primary" && single_pk;
        if inline_pk || (c.kind == "foreign" && !inline_fks) {
            continue;
        }
        body.push(c.definition.clone());
    }
    let mut ddl = format!("CREATE TABLE {} (\n  {}\n);\n", table_ref(d, schema, name), body.join(",\n  "));
    for i in indexes.iter().filter(|i| !i.constraint) {
        ddl.push_str(&format!("{}\n", i.definition));
    }
    if rls {
        ddl.push_str(&format!("ALTER TABLE {} ENABLE ROW LEVEL SECURITY;\n", table_ref(d, schema, name)));
    }
    for p in &policies {
        ddl.push_str(&format!("{}\n", p.definition));
    }
    for t in &triggers {
        ddl.push_str(&format!("{t}\n"));
    }
    Ok(TableDetail { schema: schema.into(), name: name.into(), rls, columns, constraints, indexes, policies, triggers, ddl })
}

async fn my_show_create(db: &Db, what: &str, schema: &str, name: &str) -> Result<String> {
    let d = Dialect::Mysql;
    let rows = db
        .fetch_strings(&Sql::raw(d, format!("SHOW CREATE {what} {}", table_ref(d, schema, name))))
        .await?;
    let row = rows.first().ok_or_else(|| anyhow::anyhow!("{schema}.{name} not found"))?;
    Ok(s(&row[1]))
}

/// Strip a leading `name ` (backquoted identifier) from a CREATE TABLE line.
fn my_line_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix('`')?;
    let mut name = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '`' {
            if chars.peek() == Some(&'`') {
                chars.next();
                name.push('`');
                continue;
            }
            return Some(name);
        }
        name.push(ch);
    }
    None
}

async fn my_table(db: &Db, schema: &str, name: &str) -> Result<TableDetail> {
    let d = Dialect::Mysql;
    let create = my_show_create(db, "TABLE", schema, name).await?;
    let info = db
        .fetch_strings(&q(
            d,
            "SELECT CAST(COLUMN_NAME AS CHAR), CAST(COLUMN_TYPE AS CHAR), CAST(IS_NULLABLE AS CHAR), CAST(COLUMN_DEFAULT AS CHAR),
                    CAST(EXTRA AS CHAR), CAST(GENERATION_EXPRESSION AS CHAR), CAST(COLUMN_KEY AS CHAR)
             FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = {} AND TABLE_NAME = {} ORDER BY ORDINAL_POSITION",
            &[schema, name],
        ))
        .await?;
    let mut columns = Vec::new();
    let mut constraints = Vec::new();
    let mut indexes = Vec::new();
    let lines: Vec<&str> = create.lines().map(|l| l.trim().trim_end_matches(',')).collect();
    for r in &info {
        let col = s(&r[0]);
        let extra = s(&r[4]).to_lowercase();
        let definition = lines
            .iter()
            .find(|l| my_line_name(l).as_deref() == Some(col.as_str()))
            .map(|l| l.to_string())
            .unwrap_or_else(|| format!("{} {}", quote_ident(d, &col), s(&r[1])));
        let generated = r[5].clone().filter(|g| !g.is_empty() && extra.contains("generated") && !extra.contains("default_generated"));
        columns.push(ColumnDef {
            name: col,
            type_name: s(&r[1]),
            not_null: s(&r[2]) == "NO",
            default: if generated.is_some() { None } else { r[3].clone() },
            primary: s(&r[6]) == "PRI",
            identity: None,
            generated,
            auto_increment: extra.contains("auto_increment"),
            definition,
        });
    }
    for l in &lines {
        let up = l.to_ascii_uppercase();
        if up.starts_with("PRIMARY KEY") {
            constraints.push(ConstraintDef { name: "PRIMARY".into(), kind: "primary".into(), definition: l.to_string() });
            indexes.push(IndexDef { name: "PRIMARY".into(), unique: true, constraint: true, definition: l.to_string() });
        } else if up.starts_with("UNIQUE KEY") || up.starts_with("KEY") || up.starts_with("FULLTEXT KEY") || up.starts_with("SPATIAL KEY") {
            let unique = up.starts_with("UNIQUE");
            let after = l.split_once(' ').map(|x| x.1).unwrap_or("");
            let after = if unique { after.split_once(' ').map(|x| x.1).unwrap_or("") } else { after };
            let iname = my_line_name(after.trim()).unwrap_or_default();
            if unique {
                constraints.push(ConstraintDef { name: iname.clone(), kind: "unique".into(), definition: l.to_string() });
            }
            indexes.push(IndexDef { name: iname, unique, constraint: unique, definition: l.to_string() });
        } else if up.starts_with("CONSTRAINT") {
            let cname = my_line_name(l["CONSTRAINT ".len()..].trim()).unwrap_or_default();
            let kind = if up.contains(" FOREIGN KEY ") { "foreign" } else if up.contains(" CHECK ") { "check" } else { "unique" };
            constraints.push(ConstraintDef { name: cname, kind: kind.into(), definition: l.to_string() });
        }
    }
    let triggers = db
        .fetch_strings(&q(
            d,
            "SELECT CAST(TRIGGER_NAME AS CHAR), CAST(ACTION_TIMING AS CHAR), CAST(EVENT_MANIPULATION AS CHAR), CAST(ACTION_STATEMENT AS CHAR)
             FROM information_schema.TRIGGERS WHERE EVENT_OBJECT_SCHEMA = {} AND EVENT_OBJECT_TABLE = {} ORDER BY 1",
            &[schema, name],
        ))
        .await?
        .into_iter()
        .map(|r| format!("CREATE TRIGGER {} {} {} ON {} FOR EACH ROW {};", quote_ident(d, &s(&r[0])), s(&r[1]), s(&r[2]), quote_ident(d, name), s(&r[3])))
        .collect::<Vec<_>>();
    let mut ddl = format!("{create};\n");
    for t in &triggers {
        ddl.push_str(&format!("{t}\n"));
    }
    Ok(TableDetail { schema: schema.into(), name: name.into(), rls: false, columns, constraints, indexes, policies: vec![], triggers, ddl })
}

/// (id, referenced table, from columns, to columns, on update, on delete)
type FkGroup = (String, String, Vec<String>, Vec<String>, String, String);

async fn lite_table(db: &Db, name: &str) -> Result<TableDetail> {
    let d = Dialect::Sqlite;
    let create = db
        .fetch_strings(&q(d, "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = {}", &[name]))
        .await?
        .first()
        .map(|r| s(&r[0]))
        .ok_or_else(|| anyhow::anyhow!("Table {name} not found"))?;
    let info = db
        .fetch_strings(&q(
            d,
            "SELECT name, type, \"notnull\", dflt_value, pk, hidden FROM pragma_table_xinfo({}) ORDER BY cid",
            &[name],
        ))
        .await?;
    let pk_count = info.iter().filter(|r| s(&r[4]) != "0").count();
    let auto = create.to_ascii_uppercase().contains("AUTOINCREMENT");
    let columns = info
        .iter()
        .map(|r| {
            let col = s(&r[0]);
            let primary = s(&r[4]) != "0";
            let not_null = s(&r[2]) == "1";
            let generated = matches!(s(&r[5]).as_str(), "2" | "3");
            let mut def = format!("{} {}", quote_ident(d, &col), s(&r[1]));
            if primary && pk_count == 1 {
                def.push_str(" PRIMARY KEY");
                if auto {
                    def.push_str(" AUTOINCREMENT");
                }
            }
            if not_null {
                def.push_str(" NOT NULL");
            }
            if let Some(x) = &r[3] {
                def.push_str(&format!(" DEFAULT {x}"));
            }
            if generated {
                def.push_str(" GENERATED ALWAYS AS (…)");
            }
            ColumnDef {
                name: col,
                type_name: s(&r[1]),
                not_null,
                default: r[3].clone(),
                primary,
                identity: None,
                generated: generated.then(|| "…".to_string()),
                auto_increment: primary && pk_count == 1 && s(&r[1]).eq_ignore_ascii_case("integer"),
                definition: def,
            }
        })
        .collect();
    let mut constraints = Vec::new();
    if pk_count > 1 {
        let mut pk: Vec<(i64, String)> = info
            .iter()
            .filter(|r| s(&r[4]) != "0")
            .map(|r| (s(&r[4]).parse().unwrap_or(0), quote_ident(d, &s(&r[0]))))
            .collect();
        pk.sort();
        constraints.push(ConstraintDef {
            name: "PRIMARY KEY".into(),
            kind: "primary".into(),
            definition: format!("PRIMARY KEY ({})", pk.into_iter().map(|x| x.1).collect::<Vec<_>>().join(", ")),
        });
    }
    let fks = db
        .fetch_strings(&q(
            d,
            "SELECT id, \"table\", \"from\", \"to\", on_update, on_delete FROM pragma_foreign_key_list({}) ORDER BY id, seq",
            &[name],
        ))
        .await?;
    let mut grouped: Vec<FkGroup> = Vec::new();
    for r in fks {
        let id = s(&r[0]);
        if grouped.last().map(|g| g.0 != id).unwrap_or(true) {
            grouped.push((id.clone(), s(&r[1]), vec![], vec![], s(&r[4]), s(&r[5])));
        }
        let g = grouped.last_mut().unwrap();
        g.2.push(quote_ident(d, &s(&r[2])));
        if let Some(to) = &r[3] {
            g.3.push(quote_ident(d, to));
        }
    }
    for (_, table, from, to, upd, del) in grouped {
        let mut def = format!("FOREIGN KEY ({}) REFERENCES {}", from.join(", "), quote_ident(d, &table));
        if !to.is_empty() {
            def.push_str(&format!(" ({})", to.join(", ")));
        }
        if upd != "NO ACTION" {
            def.push_str(&format!(" ON UPDATE {upd}"));
        }
        if del != "NO ACTION" {
            def.push_str(&format!(" ON DELETE {del}"));
        }
        constraints.push(ConstraintDef { name: String::new(), kind: "foreign".into(), definition: def });
    }
    let idx = db
        .fetch_strings(&q(
            d,
            "SELECT l.name, l.\"unique\", l.origin, m.sql FROM pragma_index_list({}) l LEFT JOIN sqlite_master m ON m.type = 'index' AND m.name = l.name ORDER BY l.name",
            &[name],
        ))
        .await?;
    let mut indexes = Vec::new();
    for r in idx {
        let iname = s(&r[0]);
        let origin = s(&r[2]);
        let cols = db
            .fetch_strings(&q(d, "SELECT name FROM pragma_index_info({}) ORDER BY seqno", &[&iname]))
            .await?
            .into_iter()
            .map(|c| quote_ident(d, &s(&c[0])))
            .collect::<Vec<_>>()
            .join(", ");
        if origin == "u" {
            constraints.push(ConstraintDef { name: String::new(), kind: "unique".into(), definition: format!("UNIQUE ({cols})") });
        }
        let definition = r[3].clone().map(|x| format!("{x};")).unwrap_or_else(|| {
            format!("-- {} ({cols}), created by {}", iname, if origin == "pk" { "PRIMARY KEY" } else { "UNIQUE" })
        });
        indexes.push(IndexDef { name: iname, unique: s(&r[1]) == "1", constraint: origin != "c", definition });
    }
    let triggers = db
        .fetch_strings(&q(d, "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND tbl_name = {} ORDER BY name", &[name]))
        .await?
        .into_iter()
        .map(|r| format!("{};", s(&r[0])))
        .collect::<Vec<_>>();
    let mut ddl = format!("{create};\n");
    for i in indexes.iter().filter(|i| !i.constraint) {
        ddl.push_str(&format!("{}\n", i.definition));
    }
    for t in &triggers {
        ddl.push_str(&format!("{t}\n"));
    }
    Ok(TableDetail { schema: "main".into(), name: name.into(), rls: false, columns, constraints, indexes, policies: vec![], triggers, ddl })
}

// ---------------------------------------------------------------- view

pub async fn view(db: &Db, schema: &str, name: &str) -> Result<ViewDetail> {
    let d = db.dialect();
    match d {
        Dialect::Postgres => {
            let rows = db
                .fetch_strings(&q(
                    d,
                    format!("SELECT c.relkind::text, pg_get_viewdef(c.oid, true) FROM pg_class c WHERE c.oid = {PG_REL}"),
                    &[schema, name],
                ))
                .await?;
            let r = rows.first().ok_or_else(|| anyhow::anyhow!("View {schema}.{name} not found"))?;
            let materialized = s(&r[0]) == "m";
            let definition = s(&r[1]).trim().trim_end_matches(';').to_string();
            let ddl = format!(
                "CREATE {}VIEW {} AS\n{definition};\n",
                if materialized { "MATERIALIZED " } else { "" },
                table_ref(d, schema, name)
            );
            Ok(ViewDetail { schema: schema.into(), name: name.into(), materialized, definition, ddl })
        }
        Dialect::Mysql => {
            let create = my_show_create(db, "VIEW", schema, name).await?;
            let definition = create
                .find(" AS ")
                .map(|i| create[i + 4..].to_string())
                .unwrap_or_else(|| create.clone());
            Ok(ViewDetail { schema: schema.into(), name: name.into(), materialized: false, definition, ddl: format!("{create};\n") })
        }
        Dialect::Sqlite => {
            let create = db
                .fetch_strings(&q(d, "SELECT sql FROM sqlite_master WHERE type = 'view' AND name = {}", &[name]))
                .await?
                .first()
                .map(|r| s(&r[0]))
                .ok_or_else(|| anyhow::anyhow!("View {name} not found"))?;
            let up = create.to_ascii_uppercase();
            let definition = up.find(" AS").map(|i| create[i + 3..].trim().to_string()).unwrap_or_else(|| create.clone());
            Ok(ViewDetail { schema: "main".into(), name: name.into(), materialized: false, definition, ddl: format!("{create};\n") })
        }
    }
}

// ---------------------------------------------------------------- role

pub async fn role(db: &Db, name: &str) -> Result<RoleDetail> {
    let d = db.dialect();
    if d != Dialect::Postgres {
        bail!("Roles are available for PostgreSQL only");
    }
    let rows = db
        .fetch_strings(&q(
            d,
            "SELECT rolsuper::text, rolcreatedb::text, rolcreaterole::text, rolinherit::text, rolcanlogin::text,
                    rolbypassrls::text, rolreplication::text, rolconnlimit::text, rolvaliduntil::text,
                    (SELECT string_agg(b.rolname::text, chr(31) ORDER BY b.rolname) FROM pg_auth_members m JOIN pg_roles b ON b.oid = m.roleid WHERE m.member = r.oid)
             FROM pg_roles r WHERE rolname = {}",
            &[name],
        ))
        .await?;
    let r = rows.first().ok_or_else(|| anyhow::anyhow!("Role {name} not found"))?;
    let grants = db
        .fetch_strings(&q(
            d,
            "SELECT table_schema::text, table_name::text, string_agg(privilege_type::text, chr(31) ORDER BY privilege_type)
             FROM information_schema.table_privileges
             WHERE grantee = {} AND table_schema NOT IN ('pg_catalog', 'information_schema')
             GROUP BY 1, 2 ORDER BY 1, 2",
            &[name],
        ))
        .await?
        .into_iter()
        .map(|g| Grant { schema: s(&g[0]), table: s(&g[1]), privileges: split_sep(&g[2]) })
        .collect();
    let mut detail = RoleDetail {
        name: name.into(),
        superuser: truthy(&r[0]),
        create_db: truthy(&r[1]),
        create_role: truthy(&r[2]),
        inherit: truthy(&r[3]),
        can_login: truthy(&r[4]),
        bypass_rls: truthy(&r[5]),
        replication: truthy(&r[6]),
        connection_limit: s(&r[7]).parse().unwrap_or(-1),
        valid_until: r[8].clone().filter(|v| v != "infinity"),
        member_of: split_sep(&r[9]),
        privileges: grants,
        definition: String::new(),
    };
    let flag = |on: bool, yes: &str, no: &str| if on { yes.to_string() } else { no.to_string() };
    let mut def = vec![format!("ROLE {}", quote_ident(d, name))];
    if detail.superuser { def.push("SUPERUSER".into()); }
    if detail.create_db { def.push("CREATEDB".into()); }
    if detail.create_role { def.push("CREATEROLE".into()); }
    def.push(flag(detail.inherit, "INHERIT", "NOINHERIT"));
    if detail.can_login { def.push("LOGIN".into()); }
    if detail.replication { def.push("REPLICATION".into()); }
    if detail.bypass_rls { def.push("BYPASSRLS".into()); }
    if detail.connection_limit >= 0 { def.push(format!("CONNECTION LIMIT {}", detail.connection_limit)); }
    if let Some(v) = &detail.valid_until { def.push(format!("VALID UNTIL '{v}'")); }
    detail.definition = def.join(" ");
    Ok(detail)
}

// ---------------------------------------------------------------- dump

/// DDL for the whole database (all user schemas).
pub async fn dump(db: &Db) -> Result<String> {
    let cat = catalog(db).await?;
    let d = cat.dialect;
    let mut out = String::new();
    match d {
        Dialect::Postgres => {
            for sc in cat.schemas.iter().filter(|x| *x != "public") {
                out.push_str(&format!("CREATE SCHEMA IF NOT EXISTS {};\n", quote_ident(d, sc)));
            }
            if !out.is_empty() {
                out.push('\n');
            }
            for e in &cat.enums {
                let vals = e.values.iter().map(|v| format!("'{}'", v.replace('\'', "''"))).collect::<Vec<_>>().join(", ");
                out.push_str(&format!("CREATE TYPE {} AS ENUM ({vals});\n", table_ref(d, &e.schema, &e.name)));
            }
            if !cat.enums.is_empty() {
                out.push('\n');
            }
            // Sequences used by serial / nextval() defaults (identity sequences are implicit).
            let seqs = db
                .fetch_strings(&Sql::raw(
                    d,
                    format!(
                        "SELECT n.nspname::text, c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
                         WHERE c.relkind = 'S' AND {PG_SYS}
                           AND NOT EXISTS (SELECT 1 FROM pg_depend dp WHERE dp.objid = c.oid AND dp.deptype = 'i')
                         ORDER BY 1, 2"
                    ),
                ))
                .await?;
            for r in &seqs {
                out.push_str(&format!("CREATE SEQUENCE IF NOT EXISTS {};\n", table_ref(d, &s(&r[0]), &s(&r[1]))));
            }
            if !seqs.is_empty() {
                out.push('\n');
            }
            let mut fks = Vec::new();
            for t in &cat.tables {
                let detail = pg_table(db, &t.schema, &t.name, false).await?;
                out.push_str(&detail.ddl);
                out.push('\n');
                for c in detail.constraints.iter().filter(|c| c.kind == "foreign") {
                    fks.push(format!("ALTER TABLE {} ADD {};", table_ref(d, &t.schema, &t.name), c.definition));
                }
            }
            if !fks.is_empty() {
                out.push_str(&fks.join("\n"));
                out.push_str("\n\n");
            }
            for v in &cat.views {
                out.push_str(&view(db, &v.schema, &v.name).await?.ddl);
                out.push('\n');
            }
        }
        Dialect::Mysql => {
            // Tables come in name order, so foreign keys may point to tables created later.
            out.push_str("SET FOREIGN_KEY_CHECKS = 0;\n\n");
            for t in &cat.tables {
                out.push_str(&format!("{};\n\n", my_show_create(db, "TABLE", &t.schema, &t.name).await?));
            }
            for v in &cat.views {
                out.push_str(&format!("{};\n\n", my_show_create(db, "VIEW", &v.schema, &v.name).await?));
            }
            out.push_str("SET FOREIGN_KEY_CHECKS = 1;\n");
        }
        Dialect::Sqlite => {
            let rows = db
                .fetch_strings(&Sql::raw(
                    d,
                    "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'
                     ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 WHEN 'view' THEN 2 ELSE 3 END, name",
                ))
                .await?;
            for r in rows {
                out.push_str(&format!("{};\n\n", s(&r[0])));
            }
        }
    }
    Ok(out.trim_end().to_string() + "\n")
}

// ---------------------------------------------------------------- execute

/// Runs reviewed DDL statements. PostgreSQL and SQLite run them in one transaction;
/// MySQL commits each DDL statement implicitly, so it runs them one by one.
pub async fn execute(db: &Db, statements: &[String]) -> Result<()> {
    let stmts: Vec<&str> = statements.iter().map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
    if stmts.is_empty() {
        bail!("Nothing to apply");
    }
    let fail = |i: usize, e: sqlx::Error| anyhow::anyhow!("Statement #{} failed: {}\n{}", i + 1, e, stmts[i]);
    macro_rules! in_tx {
        ($pool:expr) => {{
            let mut tx = $pool.begin().await?;
            for (i, st) in stmts.iter().enumerate() {
                // A prepared, non-cached statement: `raw_sql` on a transaction makes this future !Send.
                sqlx::query(st).persistent(false).execute(&mut *tx).await.map_err(|e| fail(i, e))?;
            }
            tx.commit().await?;
        }};
    }
    match db {
        Db::Pg(p) => in_tx!(p),
        Db::Lite(p, _) => in_tx!(p),
        Db::My(p) => {
            for (i, st) in stmts.iter().enumerate() {
                sqlx::raw_sql(st).execute(p).await.map_err(|e| fail(i, e))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_line_name() {
        assert_eq!(my_line_name("`id` int NOT NULL").as_deref(), Some("id"));
        assert_eq!(my_line_name("`we``ird` text").as_deref(), Some("we`ird"));
        assert_eq!(my_line_name("PRIMARY KEY (`id`)"), None);
    }

    #[test]
    fn placeholders() {
        let s = q(Dialect::Postgres, "a = {} AND b = {}", &["x", "y"]);
        assert_eq!(s.text, "a = $1 AND b = $2");
        let s = q(Dialect::Mysql, "a = {}", &["x"]);
        assert_eq!(s.text, "a = ?");
    }
}

