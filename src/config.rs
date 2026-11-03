//! Saved connections: environment-defined plus user-defined (persisted as JSON).

use crate::db::{normalize_url, Dialect};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConnConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub read_only: bool,
    /// "env" (from environment / CLI, not persisted) or "user".
    #[serde(default = "user_source")]
    pub source: String,
}

fn user_source() -> String {
    "user".into()
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConnInfo {
    pub id: String,
    pub name: String,
    pub dialect: Option<Dialect>,
    pub url: String,
    pub read_only: bool,
    pub source: String,
}

impl ConnConfig {
    pub fn info(&self, global_read_only: bool) -> ConnInfo {
        ConnInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            dialect: normalize_url(&self.url).ok().map(|(d, _)| d),
            url: mask_url(&self.url),
            read_only: self.read_only || global_read_only,
            source: self.source.clone(),
        }
    }
}

/// Hide the password part of a URL: scheme://user:secret@host -> scheme://user:****@host
pub fn mask_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else { return url.to_string() };
    let rest = &url[scheme_end + 3..];
    let Some(at) = rest.rfind('@') else { return url.to_string() };
    let creds = &rest[..at];
    match creds.find(':') {
        Some(colon) => format!("{}://{}:****@{}", &url[..scheme_end], &creds[..colon], &rest[at + 1..]),
        None => url.to_string(),
    }
}

/// Derive a readable name from a connection URL.
pub fn name_from_url(url: &str) -> String {
    let base = url.split('?').next().unwrap_or(url);
    let tail = base.rsplit(['/', '\\']).next().unwrap_or("database");
    if tail.is_empty() || tail.contains('@') || tail.contains(':') {
        "database".into()
    } else {
        tail.to_string()
    }
}

pub fn env_connections(cli_url: Option<&str>) -> Vec<ConnConfig> {
    let mut out = Vec::new();
    if let Some(url) = cli_url.filter(|u| !u.trim().is_empty()) {
        out.push(ConnConfig {
            id: "env-default".into(),
            name: name_from_url(url),
            url: url.to_string(),
            read_only: false,
            source: "env".into(),
        });
    }
    let mut extra: Vec<(String, String)> = std::env::vars()
        .filter_map(|(k, v)| k.strip_prefix("DATABASE_URL_").map(|n| (n.to_ascii_lowercase(), v)))
        .filter(|(_, v)| !v.trim().is_empty())
        .collect();
    extra.sort();
    for (name, url) in extra {
        out.push(ConnConfig {
            id: format!("env-{name}"),
            name,
            url,
            read_only: false,
            source: "env".into(),
        });
    }
    out
}

pub fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("connections.json")
}

pub fn load_user_connections(path: &Path) -> Vec<ConnConfig> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<ConnConfig>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|mut c| {
            c.source = "user".into();
            c
        })
        .collect()
}

pub fn save_user_connections(path: &Path, conns: &[ConnConfig]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("Cannot create {}", dir.display()))?;
    }
    let user: Vec<&ConnConfig> = conns.iter().filter(|c| c.source == "user").collect();
    std::fs::write(path, serde_json::to_string_pretty(&user)?)
        .with_context(|| format!("Cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_password() {
        assert_eq!(mask_url("postgres://u:secret@h:5432/db"), "postgres://u:****@h:5432/db");
        assert_eq!(mask_url("sqlite://./a.db"), "sqlite://./a.db");
    }

    #[test]
    fn names_from_url() {
        assert_eq!(name_from_url("postgres://u:p@h/shop?ssl=true"), "shop");
        assert_eq!(name_from_url("sqlite://C:\\data\\app.db"), "app.db");
    }
}
