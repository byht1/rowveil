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
    /// Labels such as DEV, PROD, LOCAL.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// "env" (from environment / CLI, not persisted) or "user".
    #[serde(default = "user_source")]
    pub source: String,
    /// Redis: how many keys one pass of the key browser scans (None: SCAN_LIMIT_DEFAULT).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_limit: Option<u32>,
}

/// Redis key browser: keys per pass by default, and the allowed range.
pub const SCAN_LIMIT_DEFAULT: u32 = 10_000;
pub const SCAN_LIMIT_RANGE: std::ops::RangeInclusive<u32> = 100..=1_000_000;

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
    pub tags: Vec<String>,
    pub scan_limit: u32,
}

/// Cleans user input: trimmed, upper case, no duplicates, at most 8 tags of 24 characters.
pub fn normalize_tags<I: IntoIterator<Item = S>, S: AsRef<str>>(tags: I) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t: String = t
            .as_ref()
            .trim()
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
            .take(24)
            .collect::<String>()
            .trim()
            .to_uppercase();
        if !t.is_empty() && !out.contains(&t) && out.len() < 8 {
            out.push(t);
        }
    }
    out
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
            tags: self.tags.clone(),
            scan_limit: self.scan_limit.unwrap_or(SCAN_LIMIT_DEFAULT),
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
    let lower = url.to_ascii_lowercase();
    if crate::kv::is_redis_url(&lower) || crate::bigquery::is_bigquery_url(&lower) {
        // redis://user:pass@host:6379/0 -> "host"; bigquery://project/dataset -> "project"
        let rest = url.split_once("://").map(|x| x.1).unwrap_or(url);
        let rest = rest.rsplit_once('@').map(|x| x.1).unwrap_or(rest);
        let host = rest.split(['/', '?']).next().unwrap_or("");
        let host = match host.strip_prefix('[') {
            Some(v6) => v6.split(']').next().unwrap_or(""),
            None => host.split(':').next().unwrap_or(""),
        };
        return if host.is_empty() { "database".into() } else { host.to_string() };
    }
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
            tags: env_tags("DATABASE_TAGS"),
            source: "env".into(),
            scan_limit: None,
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
            tags: env_tags(&format!("DATABASE_TAGS_{}", name.to_ascii_uppercase())),
            name,
            url,
            read_only: false,
            source: "env".into(),
            scan_limit: None,
        });
    }
    out
}

/// `DATABASE_TAGS_<NAME>=DEV,LOCAL` (also `|` or `;` as separators).
fn env_tags(var: &str) -> Vec<String> {
    std::env::var(var).map(|v| normalize_tags(v.split([',', '|', ';']))).unwrap_or_default()
}

/// Tags set in the UI for connections from the environment (they are not in connections.json).
pub fn env_tags_path(data_dir: &Path) -> PathBuf {
    data_dir.join("env-connection-tags.json")
}

pub fn load_env_tag_overrides(path: &Path) -> std::collections::BTreeMap<String, Vec<String>> {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_env_tag_overrides(path: &Path, conns: &[ConnConfig]) -> Result<()> {
    let map: std::collections::BTreeMap<&str, &Vec<String>> =
        conns.iter().filter(|c| c.source == "env").map(|c| (c.id.as_str(), &c.tags)).collect();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&map)?).with_context(|| format!("Cannot write {}", path.display()))
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
        assert_eq!(name_from_url("redis://:p%40ss@cache.local:6379/2"), "cache.local");
        assert_eq!(name_from_url("rediss://[::1]:6380"), "::1");
        assert_eq!(name_from_url("bigquery://my-proj/sales?location=EU"), "my-proj");
    }

    #[test]
    fn cleans_tags() {
        assert_eq!(normalize_tags([" dev ", "PROD", "Dev", "", "eu-west 1!"]), ["DEV", "PROD", "EU-WEST 1"]);
        assert_eq!(normalize_tags((0..20).map(|i| format!("t{i}"))).len(), 8);
        let old: ConnConfig = serde_json::from_str(r#"{"id":"a","name":"n","url":"sqlite://x.db"}"#).unwrap();
        assert!(old.tags.is_empty());
    }
}
