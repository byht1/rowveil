//! UI settings of Rowveil Next, in next-prefs.json next to connections.json.
//! A separate file: the alpha never changes the stable app's ui-prefs.json.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    /// "light", "dark" or "system".
    pub theme: String,
    /// Open the last connection on start (instead of the start page).
    pub open_last: bool,
    pub last_conn: Option<String>,
    /// Start page / connection menu tag filter.
    pub tag_filter: Vec<String>,
    /// Sidebar width in pixels (0: default).
    pub sidebar_width: f32,
    /// JSON viewer mode: "formatted" or "raw".
    pub json_mode: String,
    /// Per connection: open tabs and other state (free-form JSON).
    pub conn_state: Map<String, Value>,
    /// SQL history per connection (newest first).
    pub history: Map<String, Value>,
}

fn path() -> PathBuf {
    crate::backend::data_dir().join("next-prefs.json")
}

impl Prefs {
    pub fn load() -> Self {
        std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| Prefs { theme: "system".into(), ..Default::default() })
    }

    pub fn save(&self) {
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, &p);
            }
        }
    }
}
