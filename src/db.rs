//! Database drivers: connection, SQL building, value decoding.

use anyhow::{anyhow, bail, Context, Result};
use futures::TryStreamExt;
use serde::Serialize;
use serde_json::Value;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions, MySqlRow};
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Either, Row, TypeInfo, ValueRef};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Postgres,
    Mysql,
    Sqlite,
}

#[derive(Clone)]
pub enum Db {
    Pg(PgPool),
    My(MySqlPool),
    /// SQLite pool and whether the file is opened read-only.
    Lite(SqlitePool, bool),
}

/// Max rows that the SQL console returns per result set.
pub const CONSOLE_ROW_LIMIT: usize = 5000;

/// Normalize user input into a URL that sqlx understands.
pub fn normalize_url(raw: &str) -> Result<(Dialect, String)> {
    let url = raw.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        return Ok((Dialect::Postgres, url.to_string()));
    }
    if lower.starts_with("mysql://") {
        return Ok((Dialect::Mysql, url.to_string()));
    }
    if lower.starts_with("mariadb://") {
        return Ok((Dialect::Mysql, format!("mysql://{}", &url["mariadb://".len()..])));
    }
    if lower.starts_with("sqlite:") {
        return Ok((Dialect::Sqlite, url.to_string()));
    }
    if lower.starts_with("file:") {
        return Ok((Dialect::Sqlite, format!("sqlite://{}", &url["file:".len()..])));
    }
    let is_file_like = [".db", ".sqlite", ".sqlite3", ".db3"].iter().any(|e| lower.ends_with(e));
    if is_file_like || std::path::Path::new(url).is_file() {
        return Ok((Dialect::Sqlite, format!("sqlite://{url}")));
    }
    bail!("Unsupported connection URL. Use postgres://, mysql://, mariadb:// or sqlite:// (or a path to a .db file)")
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolve a SQLite URL into a file path and a read-only flag.
/// Tolerates percent-encoded Windows paths, an empty "user@" part and a
/// trailing segment after the real file (e.g. `sqlite://@C%3A%5Cdir%5Capp.db/app.db`).
pub fn sqlite_target(url: &str) -> (std::path::PathBuf, bool) {
    let lower = url.to_ascii_lowercase();
    let mut rest = url;
    for prefix in ["sqlite://", "sqlite:", "file://", "file:"] {
        if lower.starts_with(prefix) {
            rest = &url[prefix.len()..];
            break;
        }
    }
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let read_only = query.split('&').any(|kv| {
        let kv = kv.to_ascii_lowercase();
        kv == "mode=ro" || kv == "readonly=true" || kv == "read_only=true" || kv == "immutable=1"
    });
    let decoded = percent_decode(path);
    let mut p = decoded.trim_start_matches('@');
    // "/C:/dir/app.db" -> "C:/dir/app.db"
    let pb = p.as_bytes();
    if pb.len() > 2 && pb[0] == b'/' && pb[1].is_ascii_alphabetic() && pb[2] == b':' {
        p = &p[1..];
    }
    let mut path = std::path::PathBuf::from(p);
    if p != ":memory:" && !path.exists() {
        if let Some(file) = path.ancestors().skip(1).find(|a| a.is_file()) {
            path = file.to_path_buf();
        }
    }
    (path, read_only)
}

pub async fn connect(raw_url: &str) -> Result<Db> {
    let (dialect, url) = normalize_url(raw_url)?;
    let timeout = Duration::from_secs(10);
    let db = match dialect {
        Dialect::Postgres => Db::Pg(
            PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(timeout)
                .connect(&url)
                .await
                .context("PostgreSQL connection failed")?,
        ),
        Dialect::Mysql => Db::My(
            MySqlPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(timeout)
                .connect(&url)
                .await
                .context("MySQL connection failed")?,
        ),
        Dialect::Sqlite => {
            let (path, read_only) = sqlite_target(&url);
            let memory = path.as_os_str() == ":memory:";
            if !memory && !path.is_file() {
                bail!("SQLite file not found: {}", path.display());
            }
            let pool = |ro: bool| {
                let opts = SqliteConnectOptions::new()
                    .filename(&path)
                    .read_only(ro)
                    .foreign_keys(true);
                SqlitePoolOptions::new().max_connections(4).acquire_timeout(timeout).connect_with(opts)
            };
            // SQLite silently opens a write-protected file read-only and only fails on
            // the first write, so check write access up front.
            let writable = memory || crate::access::write_blockers(&path).is_empty();
            let read_only = read_only || !writable;
            let pool = match pool(read_only).await {
                Ok(p) => p,
                Err(_) if !read_only => {
                    let p = pool(true).await.with_context(|| {
                        format!("SQLite connection failed: cannot open {}", path.display())
                    })?;
                    return Ok(Db::Lite(p, true));
                }
                Err(e) => return Err(e).with_context(|| format!("SQLite connection failed: {}", path.display())),
            };
            Db::Lite(pool, read_only)
        }
    };
    Ok(db)
}

pub fn quote_ident(d: Dialect, s: &str) -> String {
    match d {
        Dialect::Mysql => format!("`{}`", s.replace('`', "``")),
        _ => format!("\"{}\"", s.replace('"', "\"\"")),
    }
}

pub fn table_ref(d: Dialect, schema: &str, table: &str) -> String {
    match d {
        Dialect::Sqlite if schema.is_empty() || schema == "main" => quote_ident(d, table),
        _ => format!("{}.{}", quote_ident(d, schema), quote_ident(d, table)),
    }
}

/// A SQL statement with positional text parameters.
#[derive(Debug, Clone)]
pub struct Sql {
    pub dialect: Dialect,
    pub text: String,
    pub params: Vec<Option<String>>,
}

impl Sql {
    pub fn new(dialect: Dialect) -> Self {
        Sql { dialect, text: String::new(), params: Vec::new() }
    }
    pub fn raw(dialect: Dialect, text: impl Into<String>) -> Self {
        Sql { dialect, text: text.into(), params: Vec::new() }
    }
    pub fn push(&mut self, s: &str) -> &mut Self {
        self.text.push_str(s);
        self
    }
    /// Register a parameter and return its placeholder.
    pub fn param(&mut self, v: Option<String>) -> String {
        self.params.push(v);
        match self.dialect {
            Dialect::Postgres => format!("${}", self.params.len()),
            _ => "?".to_string(),
        }
    }
}

macro_rules! bind_all {
    ($sql:expr) => {{
        let mut q = sqlx::query(&$sql.text);
        for p in &$sql.params {
            q = q.bind(p.clone());
        }
        q
    }};
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ResultColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub kind: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: u64,
    pub truncated: bool,
}

fn bytes_to_value(b: Vec<u8>) -> Value {
    match String::from_utf8(b) {
        Ok(s) => Value::String(s),
        Err(e) => Value::String(hex_bytes(e.as_bytes())),
    }
}

fn hex_bytes(b: &[u8]) -> String {
    let mut s = String::with_capacity(2 + b.len() * 2);
    s.push_str("\\x");
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

fn pg_cell(row: &PgRow, i: usize) -> Value {
    match row.try_get_unchecked::<Option<String>, _>(i) {
        Ok(Some(s)) => Value::String(s),
        Ok(None) => Value::Null,
        Err(e) => Value::String(format!("<decode error: {e}>")),
    }
}

fn my_cell(row: &MySqlRow, i: usize) -> Value {
    match row.try_get_unchecked::<Option<Vec<u8>>, _>(i) {
        Ok(Some(b)) => bytes_to_value(b),
        Ok(None) => Value::Null,
        Err(e) => Value::String(format!("<decode error: {e}>")),
    }
}

fn lite_cell(row: &SqliteRow, i: usize) -> Value {
    let raw = match row.try_get_raw(i) {
        Ok(r) => r,
        Err(e) => return Value::String(format!("<decode error: {e}>")),
    };
    if raw.is_null() {
        return Value::Null;
    }
    let storage = raw.type_info().name().to_string();
    match storage.as_str() {
        "INTEGER" => match row.try_get_unchecked::<i64, _>(i) {
            Ok(n) if n.unsigned_abs() <= (1u64 << 53) => Value::from(n),
            Ok(n) => Value::String(n.to_string()),
            Err(_) => Value::Null,
        },
        "REAL" => row
            .try_get_unchecked::<f64, _>(i)
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        "BLOB" => row
            .try_get_unchecked::<Vec<u8>, _>(i)
            .map(|b| Value::String(hex_bytes(&b)))
            .unwrap_or(Value::Null),
        _ => row
            .try_get_unchecked::<String, _>(i)
            .map(Value::String)
            .unwrap_or(Value::Null),
    }
}

/// Convert a textual value into a JSON value based on the column kind.
pub fn typed_value(kind: &str, v: Value) -> Value {
    let Value::String(s) = &v else { return v };
    match kind {
        "number" => {
            if let Ok(n) = s.parse::<i64>() {
                if n.unsigned_abs() <= (1u64 << 53) {
                    return Value::from(n);
                }
                return v;
            }
            s.parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or(v)
        }
        "boolean" => match s.as_str() {
            "t" | "true" | "1" | "TRUE" => Value::Bool(true),
            "f" | "false" | "0" | "FALSE" => Value::Bool(false),
            _ => v,
        },
        _ => v,
    }
}

/// Classify a raw type name (from a result set) into a UI kind.
pub fn kind_from_type_name(d: Dialect, name: &str) -> String {
    let n = name.to_ascii_lowercase();
    if n.ends_with("[]") || n.starts_with('_') {
        return "array".into();
    }
    let k = match d {
        Dialect::Postgres => match n.as_str() {
            "bool" | "boolean" => "boolean",
            "int2" | "int4" | "float4" | "float8" | "oid" | "smallint" | "integer" | "real"
            | "double precision" => "number",
            "int8" | "bigint" | "numeric" | "money" => "bigint",
            "json" | "jsonb" => "json",
            "date" | "time" | "timetz" | "timestamp" | "timestamptz" | "interval" => "date",
            "uuid" => "uuid",
            "bytea" => "binary",
            _ => "text",
        },
        Dialect::Mysql => match n.as_str() {
            "boolean" => "boolean",
            "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "year" | "float"
            | "double" | "tinyint unsigned" | "smallint unsigned" | "mediumint unsigned"
            | "int unsigned" => "number",
            "bigint" | "bigint unsigned" | "decimal" => "bigint",
            "json" => "json",
            "date" | "datetime" | "timestamp" | "time" => "date",
            "blob" | "binary" | "varbinary" | "tinyblob" | "mediumblob" | "longblob" | "bit" => {
                "binary"
            }
            _ => "text",
        },
        Dialect::Sqlite => return sqlite_kind(&n),
    };
    k.into()
}

/// SQLite affinity-based kind from a declared column type.
pub fn sqlite_kind(decl: &str) -> String {
    let t = decl.to_ascii_uppercase();
    let k = if t.contains("BOOL") {
        "boolean"
    } else if t.contains("INT")
        || t.contains("REAL") || t.contains("FLOA") || t.contains("DOUB") || t.contains("NUMERIC") || t.contains("DECIMAL")
    {
        "number"
    } else if t.contains("JSON") {
        "json"
    } else if t.contains("DATE") || t.contains("TIME") {
        "date"
    } else if t.contains("BLOB") {
        "binary"
    } else {
        "text"
    };
    k.into()
}

impl Db {
    pub fn dialect(&self) -> Dialect {
        match self {
            Db::Pg(_) => Dialect::Postgres,
            Db::My(_) => Dialect::Mysql,
            Db::Lite(..) => Dialect::Sqlite,
        }
    }

    /// True when the database itself cannot be written (e.g. a write-protected SQLite file).
    pub fn is_read_only(&self) -> bool {
        matches!(self, Db::Lite(_, true))
    }

    pub async fn close(&self) {
        match self {
            Db::Pg(p) => p.close().await,
            Db::My(p) => p.close().await,
            Db::Lite(p, _) => p.close().await,
        }
    }

    /// Run a parameterized query. PostgreSQL and MySQL queries must select
    /// text-cast columns; SQLite values are decoded by storage class.
    pub async fn fetch(&self, sql: &Sql) -> Result<Vec<Vec<Value>>> {
        let rows = match self {
            Db::Pg(p) => bind_all!(sql)
                .fetch_all(p)
                .await?
                .iter()
                .map(|r| (0..r.len()).map(|i| pg_cell(r, i)).collect())
                .collect(),
            Db::My(p) => bind_all!(sql)
                .fetch_all(p)
                .await?
                .iter()
                .map(|r| (0..r.len()).map(|i| my_cell(r, i)).collect())
                .collect(),
            Db::Lite(p, _) => bind_all!(sql)
                .fetch_all(p)
                .await?
                .iter()
                .map(|r| (0..r.len()).map(|i| lite_cell(r, i)).collect())
                .collect(),
        };
        Ok(rows)
    }

    /// Like `fetch`, but returns every cell as an optional string.
    pub async fn fetch_strings(&self, sql: &Sql) -> Result<Vec<Vec<Option<String>>>> {
        Ok(self
            .fetch(sql)
            .await?
            .into_iter()
            .map(|r| {
                r.into_iter()
                    .map(|v| match v {
                        Value::Null => None,
                        Value::String(s) => Some(s),
                        other => Some(other.to_string()),
                    })
                    .collect()
            })
            .collect())
    }

    /// Execute statements in one transaction. Returns total rows affected.
    pub async fn execute_tx(&self, stmts: &[Sql]) -> Result<u64> {
        let mut total = 0u64;
        macro_rules! run_tx {
            ($pool:expr) => {{
                let mut tx = $pool.begin().await?;
                for (i, s) in stmts.iter().enumerate() {
                    let res = bind_all!(s)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| anyhow!("Change #{} failed: {}", i + 1, e))?;
                    total += res.rows_affected();
                }
                tx.commit().await?;
            }};
        }
        match self {
            Db::Pg(p) => run_tx!(p),
            Db::My(p) => run_tx!(p),
            Db::Lite(p, _) => run_tx!(p),
        }
        Ok(total)
    }

    /// Run raw SQL (may contain several statements) in text mode.
    #[allow(deprecated)]
    pub async fn run_console(&self, sql: &str) -> Result<Vec<ResultSet>> {
        let d = self.dialect();
        macro_rules! collect {
            ($pool:expr, $cell:ident) => {{
                let mut sets: Vec<ResultSet> = Vec::new();
                let mut cols: Option<Vec<ResultColumn>> = None;
                let mut rows: Vec<Vec<Value>> = Vec::new();
                let mut truncated = false;
                let mut stream = sqlx::raw_sql(sql).fetch_many($pool);
                while let Some(item) = stream.try_next().await? {
                    match item {
                        Either::Left(res) => {
                            sets.push(ResultSet {
                                columns: cols.take().unwrap_or_default(),
                                rows: std::mem::take(&mut rows),
                                rows_affected: res.rows_affected(),
                                truncated,
                            });
                            truncated = false;
                        }
                        Either::Right(row) => {
                            if cols.is_none() {
                                cols = Some(
                                    row.columns()
                                        .iter()
                                        .map(|c| {
                                            let t = c.type_info().name().to_string();
                                            ResultColumn {
                                                name: c.name().to_string(),
                                                kind: kind_from_type_name(d, &t),
                                                type_name: t,
                                            }
                                        })
                                        .collect(),
                                );
                            }
                            if rows.len() >= CONSOLE_ROW_LIMIT {
                                truncated = true;
                                continue;
                            }
                            let kinds: Vec<String> = cols
                                .as_ref()
                                .map(|c| c.iter().map(|c| c.kind.clone()).collect())
                                .unwrap_or_default();
                            let vals = (0..row.len())
                                .map(|i| {
                                    let v = $cell(&row, i);
                                    match kinds.get(i) {
                                        Some(k) => typed_value(k, v),
                                        None => v,
                                    }
                                })
                                .collect();
                            rows.push(vals);
                        }
                    }
                }
                if cols.is_some() || !rows.is_empty() {
                    sets.push(ResultSet {
                        columns: cols.take().unwrap_or_default(),
                        rows,
                        rows_affected: 0,
                        truncated,
                    });
                }
                sets
            }};
        }
        let sets = match self {
            Db::Pg(p) => collect!(p, pg_cell),
            Db::My(p) => collect!(p, my_cell),
            Db::Lite(p, _) => collect!(p, lite_cell),
        };
        Ok(sets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_target_handles_encoded_windows_paths() {
        let dir = std::env::temp_dir().join("rs-sqlite-target");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app.db");
        std::fs::write(&file, b"").unwrap();
        let enc: String = file
            .to_string_lossy()
            .chars()
            .map(|c| match c {
                ':' => "%3A".to_string(),
                '\\' => "%5C".to_string(),
                '/' => "%2F".to_string(),
                c => c.to_string(),
            })
            .collect();
        let (p, ro) = sqlite_target(&format!("sqlite://@{enc}/app.db"));
        assert_eq!(p, file);
        assert!(!ro);
        let (_, ro) = sqlite_target("sqlite://./x.db?mode=ro");
        assert!(ro);
        assert_eq!(sqlite_target("sqlite::memory:").0.as_os_str(), ":memory:");
    }
}
