//! Typed views of the API's JSON answers.

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Conn {
    pub id: String,
    pub name: String,
    pub dialect: Option<String>,
    pub url: String,
    pub read_only: bool,
    pub source: String,
    pub tags: Vec<String>,
    pub scan_limit: u32,
}

impl Conn {
    pub fn dialect(&self) -> &str {
        self.dialect.as_deref().unwrap_or("")
    }
    pub fn is_env(&self) -> bool {
        self.source == "env"
    }
}

pub fn dialect_label(d: &str) -> &'static str {
    match d {
        "postgres" => "PostgreSQL",
        "mysql" => "MySQL",
        "sqlite" => "SQLite",
        "bigquery" => "BigQuery",
        "redis" => "Redis",
        _ => "Unknown",
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ColumnRef {
    pub schema: String,
    pub table: String,
    pub column: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub kind: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub primary: bool,
    pub auto: bool,
    pub generated: bool,
    pub enum_values: Option<Vec<String>>,
    pub references: Option<ColumnRef>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub definition: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub columns: Vec<Column>,
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<Index>,
    pub row_estimate: Option<i64>,
}

impl Table {
    pub fn is_view(&self) -> bool {
        self.kind != "table"
    }
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Schema {
    pub dialect: String,
    pub schemas: Vec<String>,
    pub default_schema: String,
    pub tables: Vec<Table>,
    pub read_only: bool,
    pub read_only_reason: Option<String>,
    pub can_grant_write: bool,
    /// Redis: the server overview (databases, key counts, server info).
    pub redis: Option<Value>,
}

impl Schema {
    pub fn table(&self, schema: &str, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.schema == schema && t.name == name)
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub total: Option<i64>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ResultColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub kind: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ResultSet {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: u64,
    pub truncated: bool,
    pub info: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SqlAnswer {
    pub results: Vec<ResultSet>,
    pub duration_ms: f64,
}

pub fn parse<T: for<'de> Deserialize<'de>>(v: Value) -> anyhow::Result<T> {
    Ok(serde_json::from_value(v)?)
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedQuery {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub text: String,
    pub dialect: String,
    pub connection_id: Option<String>,
    pub updated_at: i64,
}

impl SavedQuery {
    /// Shown for this connection: its own queries and the shared ones of its dialect.
    pub fn visible_for(&self, conn: &str, dialect: &str, kind: &str) -> bool {
        self.kind == kind && match &self.connection_id {
            Some(c) => c == conn,
            None => self.dialect == dialect,
        }
    }
}
