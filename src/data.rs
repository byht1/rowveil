//! Table data: browsing with filters, staged changes, export helpers.

use crate::db::{quote_ident, table_ref, typed_value, Db, Dialect, Sql};
use crate::schema::{Column, Table};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Deserialize, Debug, Clone)]
pub struct Filter {
    pub column: String,
    pub op: String,
    #[serde(default)]
    pub value: Value,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Sort {
    pub column: String,
    #[serde(default)]
    pub desc: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RowsRequest {
    pub schema: String,
    pub table: String,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub sort: Vec<Sort>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "yes")]
    pub with_count: bool,
}

fn default_limit() -> i64 {
    50
}
fn yes() -> bool {
    true
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RowsResponse {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub total: Option<i64>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Change {
    Insert { values: Map<String, Value> },
    Update { pk: Map<String, Value>, values: Map<String, Value> },
    Delete { pk: Map<String, Value> },
}

#[derive(Deserialize, Debug)]
pub struct ApplyRequest {
    pub schema: String,
    pub table: String,
    pub changes: Vec<Change>,
}

fn find_col<'a>(t: &'a Table, name: &str) -> Result<&'a Column> {
    t.columns
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| anyhow!("Unknown column \"{name}\" in table \"{}\"", t.name))
}

/// Convert a JSON value from the UI into a text parameter.
pub fn to_param(d: Dialect, v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::Bool(b) => Some(match (d, b) {
            (Dialect::Postgres, true) => "true".into(),
            (Dialect::Postgres, false) => "false".into(),
            (_, true) => "1".into(),
            (_, false) => "0".into(),
        }),
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// Placeholder expression that converts a text parameter into the column type.
fn typed_param(q: &mut Sql, col: &Column, v: Option<String>) -> String {
    let p = q.param(v);
    match q.dialect {
        Dialect::Postgres => format!("CAST({p}::text AS {})", col.cast_type),
        Dialect::Bigquery if col.kind != "json" => format!("CAST({p} AS {})", col.cast_type),
        _ => p,
    }
}

/// Column expression as text (used for LIKE filters and text comparison).
fn text_expr(d: Dialect, col: &Column) -> String {
    let q = quote_ident(d, &col.name);
    match d {
        Dialect::Postgres => format!("{q}::text"),
        Dialect::Mysql => format!("CAST({q} AS CHAR)"),
        Dialect::Sqlite | Dialect::Redis => format!("CAST({q} AS TEXT)"),
        Dialect::Bigquery if col.kind == "json" => format!("TO_JSON_STRING({q})"),
        Dialect::Bigquery => format!("CAST({q} AS STRING)"),
    }
}

fn filter_values(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().filter_map(|x| to_param(Dialect::Sqlite, x)).collect(),
        Value::String(s) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        Value::Null => vec![],
        other => vec![other.to_string()],
    }
}

fn build_where(q: &mut Sql, t: &Table, filters: &[Filter]) -> Result<()> {
    let d = q.dialect;
    let mut parts = Vec::new();
    for f in filters {
        let col = find_col(t, &f.column)?;
        let ident = quote_ident(d, &col.name);
        // PostgreSQL and BigQuery compare json/array/binary columns as text.
        let typed_compare = !matches!(d, Dialect::Postgres | Dialect::Bigquery)
            || matches!(col.kind.as_str(), "number" | "bigint" | "boolean" | "date" | "uuid" | "enum" | "text");
        let op = f.op.to_ascii_lowercase();
        let part = match op.as_str() {
            "=" | "<>" | "!=" | ">" | ">=" | "<" | "<=" => {
                let op = if op == "!=" { "<>" } else { op.as_str() };
                let v = to_param(d, &f.value);
                if typed_compare {
                    let p = typed_param(q, col, v);
                    format!("{ident} {op} {p}")
                } else {
                    let p = q.param(v);
                    format!("{} {op} {p}", text_expr(d, col))
                }
            }
            "like" | "not like" | "ilike" | "not ilike" => {
                let v = to_param(d, &f.value).unwrap_or_default();
                let neg = if op.starts_with("not") { "NOT " } else { "" };
                let ci = op.ends_with("ilike");
                let p = q.param(Some(v));
                match (d, ci) {
                    (Dialect::Postgres, true) => format!("{} {neg}ILIKE {p}", text_expr(d, col)),
                    (_, true) => format!("LOWER({}) {neg}LIKE LOWER({p})", text_expr(d, col)),
                    (_, false) => format!("{} {neg}LIKE {p}", text_expr(d, col)),
                }
            }
            "is null" => format!("{ident} IS NULL"),
            "is not null" => format!("{ident} IS NOT NULL"),
            "in" | "not in" => {
                let vals = filter_values(&f.value);
                if vals.is_empty() {
                    if op == "in" { "1 = 0".to_string() } else { "1 = 1".to_string() }
                } else {
                    let ps: Vec<String> = vals
                        .into_iter()
                        .map(|v| if typed_compare { typed_param(q, col, Some(v)) } else { q.param(Some(v)) })
                        .collect();
                    let lhs = if typed_compare { ident.clone() } else { text_expr(d, col) };
                    let neg = if op == "not in" { "NOT " } else { "" };
                    format!("{lhs} {neg}IN ({})", ps.join(", "))
                }
            }
            other => bail!("Unsupported filter operator: {other}"),
        };
        parts.push(part);
    }
    if !parts.is_empty() {
        q.push(" WHERE ");
        q.push(&parts.join(" AND "));
    }
    Ok(())
}

/// Alias of the browsed table. Sorting uses `alias.column`, because a bare
/// name in ORDER BY would resolve to the text-cast output column.
const T: &str = "_t";

fn select_expr(d: Dialect, c: &Column) -> String {
    let q = quote_ident(d, &c.name);
    let full = format!("{}.{q}", quote_ident(d, T));
    match d {
        Dialect::Postgres => format!("{full}::text AS {q}"),
        Dialect::Mysql => format!("CAST({full} AS CHAR) AS {q}"),
        Dialect::Sqlite | Dialect::Bigquery | Dialect::Redis => format!("{full} AS {q}"),
    }
}

pub async fn fetch_rows(db: &Db, t: &Table, req: &RowsRequest) -> Result<RowsResponse> {
    let d = db.dialect();
    let from = table_ref(d, &t.schema, &t.name);
    let limit = req.limit.clamp(1, 10_000);
    let offset = req.offset.max(0);

    // BigQuery: plain browsing reads table data directly (no query job, no cost).
    if let Db::Bq(bq) = db {
        if req.filters.is_empty() && req.sort.is_empty() && t.kind == "table" {
            let (raw, total) = bq.browse(&t.schema, &t.name, offset, limit).await?;
            let rows = raw
                .into_iter()
                .map(|r| r.into_iter().zip(&t.columns).map(|(v, c)| typed_value(&c.kind, v)).collect())
                .collect();
            return Ok(RowsResponse { columns: t.columns.iter().map(|c| c.name.clone()).collect(), rows, total });
        }
    }

    let mut q = Sql::new(d);
    let cols: Vec<String> = t.columns.iter().map(|c| select_expr(d, c)).collect();
    q.push(&format!("SELECT {} FROM {from} AS {}", cols.join(", "), quote_ident(d, T)));
    build_where(&mut q, t, &req.filters)?;

    let mut order: Vec<String> = Vec::new();
    for s in &req.sort {
        let c = find_col(t, &s.column)?;
        order.push(format!("{}.{} {}", quote_ident(d, T), quote_ident(d, &c.name), if s.desc { "DESC" } else { "ASC" }));
    }
    if order.is_empty() {
        order = t.primary_key.iter().map(|c| format!("{}.{} ASC", quote_ident(d, T), quote_ident(d, c))).collect();
    }
    if !order.is_empty() {
        q.push(&format!(" ORDER BY {}", order.join(", ")));
    }
    q.push(&format!(" LIMIT {limit} OFFSET {offset}"));

    let raw = db.fetch(&q).await?;
    let rows = raw
        .into_iter()
        .map(|r| {
            r.into_iter()
                .zip(&t.columns)
                .map(|(v, c)| typed_value(&c.kind, v))
                .collect()
        })
        .collect();

    let total = if req.with_count {
        let mut cq = Sql::new(d);
        let count_expr = match d {
            Dialect::Postgres => "COUNT(*)::text",
            Dialect::Mysql => "CAST(COUNT(*) AS CHAR)",
            Dialect::Sqlite | Dialect::Bigquery | Dialect::Redis => "COUNT(*)",
        };
        cq.push(&format!("SELECT {count_expr} FROM {from}"));
        build_where(&mut cq, t, &req.filters)?;
        db.fetch_strings(&cq)
            .await?
            .first()
            .and_then(|r| r[0].as_deref().and_then(|x| x.parse().ok()))
    } else {
        None
    };

    Ok(RowsResponse { columns: t.columns.iter().map(|c| c.name.clone()).collect(), rows, total })
}

fn pk_where(q: &mut Sql, t: &Table, pk: &Map<String, Value>) -> Result<()> {
    if t.primary_key.is_empty() {
        bail!("Table \"{}\" has no primary key; rows cannot be edited", t.name);
    }
    let d = q.dialect;
    let mut parts = Vec::new();
    for name in &t.primary_key {
        let col = find_col(t, name)?;
        let v = pk.get(name).ok_or_else(|| anyhow!("Missing primary key value for \"{name}\""))?;
        let p = typed_param(q, col, to_param(d, v));
        parts.push(format!("{} = {p}", quote_ident(d, name)));
    }
    q.push(" WHERE ");
    q.push(&parts.join(" AND "));
    Ok(())
}

pub fn build_changes(d: Dialect, t: &Table, changes: &[Change]) -> Result<Vec<Sql>> {
    if t.kind != "table" {
        bail!("\"{}\" is a {}; it cannot be edited", t.name, t.kind.replace('_', " "));
    }
    let from = table_ref(d, &t.schema, &t.name);
    let mut out = Vec::new();
    for ch in changes {
        let mut q = Sql::new(d);
        match ch {
            Change::Insert { values } => {
                if values.is_empty() {
                    match d {
                        Dialect::Mysql => q.push(&format!("INSERT INTO {from} () VALUES ()")),
                        _ => q.push(&format!("INSERT INTO {from} DEFAULT VALUES")),
                    };
                } else {
                    let mut names = Vec::new();
                    let mut ps = Vec::new();
                    for (k, v) in values {
                        let col = find_col(t, k)?;
                        names.push(quote_ident(d, k));
                        ps.push(typed_param(&mut q, col, to_param(d, v)));
                    }
                    let sql = format!("INSERT INTO {from} ({}) VALUES ({})", names.join(", "), ps.join(", "));
                    // Placeholders were registered before the text; prepend the statement.
                    q.text = sql;
                }
            }
            Change::Update { pk, values } => {
                if values.is_empty() {
                    continue;
                }
                let mut sets = Vec::new();
                for (k, v) in values {
                    let col = find_col(t, k)?;
                    let p = typed_param(&mut q, col, to_param(d, v));
                    sets.push(format!("{} = {p}", quote_ident(d, k)));
                }
                q.push(&format!("UPDATE {from} SET {}", sets.join(", ")));
                pk_where(&mut q, t, pk)?;
            }
            Change::Delete { pk } => {
                q.push(&format!("DELETE FROM {from}"));
                pk_where(&mut q, t, pk)?;
            }
        }
        out.push(q);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Column;

    fn col(name: &str, ty: &str, kind: &str) -> Column {
        Column {
            name: name.into(),
            data_type: ty.into(),
            cast_type: ty.into(),
            kind: kind.into(),
            nullable: true,
            default: None,
            primary: false,
            auto: false,
            generated: false,
            enum_values: None,
            references: None,
        }
    }

    fn table() -> Table {
        Table {
            schema: "public".into(),
            name: "users".into(),
            kind: "table".into(),
            columns: vec![col("id", "integer", "number"), col("name", "text", "text"), col("meta", "jsonb", "json")],
            primary_key: vec!["id".into()],
            foreign_keys: vec![],
            indexes: vec![],
            row_estimate: None,
        }
    }

    #[test]
    fn pg_update_uses_casts_and_numbered_params() {
        let mut values = Map::new();
        values.insert("name".into(), Value::String("Bob".into()));
        let mut pk = Map::new();
        pk.insert("id".into(), Value::from(7));
        let sql = build_changes(Dialect::Postgres, &table(), &[Change::Update { pk, values }]).unwrap();
        assert_eq!(
            sql[0].text,
            "UPDATE \"public\".\"users\" SET \"name\" = CAST($1::text AS text) WHERE \"id\" = CAST($2::text AS integer)"
        );
        assert_eq!(sql[0].params, vec![Some("Bob".into()), Some("7".into())]);
    }

    #[test]
    fn mysql_insert_uses_question_marks() {
        let mut values = Map::new();
        values.insert("name".into(), Value::String("A".into()));
        values.insert("meta".into(), Value::Null);
        let sql = build_changes(Dialect::Mysql, &table(), &[Change::Insert { values }]).unwrap();
        assert_eq!(sql[0].text, "INSERT INTO `public`.`users` (`name`, `meta`) VALUES (?, ?)");
        assert_eq!(sql[0].params, vec![Some("A".into()), None]);
    }

    #[test]
    fn filters_build_where_clause() {
        let t = table();
        let mut q = Sql::new(Dialect::Postgres);
        let filters = vec![
            Filter { column: "id".into(), op: ">".into(), value: Value::from(3) },
            Filter { column: "name".into(), op: "ilike".into(), value: Value::String("%a%".into()) },
            Filter { column: "meta".into(), op: "is null".into(), value: Value::Null },
            Filter { column: "id".into(), op: "in".into(), value: Value::String("1, 2".into()) },
        ];
        build_where(&mut q, &t, &filters).unwrap();
        assert_eq!(
            q.text,
            " WHERE \"id\" > CAST($1::text AS integer) AND \"name\"::text ILIKE $2 AND \"meta\" IS NULL AND \"id\" IN (CAST($3::text AS integer), CAST($4::text AS integer))"
        );
    }

    #[test]
    fn rejects_unknown_columns() {
        let t = table();
        let mut q = Sql::new(Dialect::Sqlite);
        let f = vec![Filter { column: "nope".into(), op: "=".into(), value: Value::Null }];
        assert!(build_where(&mut q, &t, &f).is_err());
    }
}
