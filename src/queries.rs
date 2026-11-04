//! Saved queries: SQL statements and Redis commands that the user keeps for later.
//! They are stored in `saved-queries.json` next to `connections.json`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Query kinds: SQL (for SQL dialects) or Redis commands.
pub const KINDS: [&str; 2] = ["sql", "redis"];
const MAX_NAME: usize = 120;
const MAX_TEXT: usize = 1_000_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: String,
    pub name: String,
    /// "sql" or "redis".
    pub kind: String,
    pub text: String,
    /// Dialect of the connection where the query was saved (postgres, mysql, …, redis).
    pub dialect: String,
    /// Set: the query shows only for this connection. Empty: for all connections with the same dialect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Unix time in milliseconds.
    pub updated_at: i64,
}

pub fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("saved-queries.json")
}

pub fn load(path: &Path) -> Vec<SavedQuery> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<SavedQuery>>(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, queries: &[SavedQuery]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("Cannot create {}", dir.display()))?;
    }
    std::fs::write(path, serde_json::to_string_pretty(queries)?).with_context(|| format!("Cannot write {}", path.display()))
}

/// Trims the name and checks the limits. Returns the error message for the user.
pub fn clean_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name".into());
    }
    Ok(name.chars().take(MAX_NAME).collect())
}

pub fn check_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("The query is empty".into());
    }
    if text.len() > MAX_TEXT {
        return Err("The query is too large (more than 1 MB)".into());
    }
    Ok(())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("rowveil-q-{}", now_ms()));
        let path = store_path(&dir);
        assert!(load(&path).is_empty());
        let q = SavedQuery {
            id: "1".into(),
            name: "Active users".into(),
            kind: "sql".into(),
            text: "SELECT 1".into(),
            dialect: "postgres".into(),
            connection_id: None,
            updated_at: 1,
        };
        save(&path, std::slice::from_ref(&q)).unwrap();
        assert_eq!(load(&path), vec![q]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn validates() {
        assert_eq!(clean_name("  a  ").unwrap(), "a");
        assert!(clean_name("  ").is_err());
        assert!(check_text(" \n").is_err());
        assert!(check_text("GET k").is_ok());
    }
}
