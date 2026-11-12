//! The bridge to the Rowveil backend: the same axum router as the stable app, called in-process
//! (no socket, no port). Every endpoint keeps its validation, read-only checks and errors.
//! Database work runs on a tokio runtime; the UI awaits the result from its own executor.

use anyhow::{anyhow, Result};
use axum::body::Body;
use axum::http::{Method, Request};
use axum::Router;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tower::ServiceExt as _;

/// The runtime for sqlx, redis and reqwest (they need tokio).
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("rowveil-db")
            .build()
            .expect("tokio runtime")
    })
}

/// Runs any future on the tokio runtime (reqwest needs it); await the result from the UI.
pub async fn run<F, T>(f: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    runtime().spawn(f).await.map_err(|e| anyhow!("Task failed: {e}"))?
}

pub struct Backend {
    router: Router,
    pub data_dir: PathBuf,
}

/// The data directory: STUDIO_DATA_DIR, or the stable app's folder (shared connections).
pub fn data_dir() -> PathBuf {
    std::env::var_os("STUDIO_DATA_DIR").map(PathBuf::from).unwrap_or_else(rowveil::default_data_dir)
}

impl Backend {
    pub fn new() -> Arc<Self> {
        let data_dir = data_dir();
        let opts = rowveil::Options { url: None, data_dir: data_dir.clone(), read_only: false, basic_auth: None, app_token: None };
        // The router builds its state on the runtime (it may spawn tasks).
        let (router, _) = runtime().block_on(async { rowveil::router(&opts) });
        Arc::new(Self { router, data_dir })
    }

    /// Calls an API endpoint ("/api" is added). Errors carry the server's message.
    pub async fn call(self: Arc<Self>, method: Method, path: String, body: Option<Value>) -> Result<Value> {
        let router = self.router.clone();
        let fut = async move {
            let mut req = Request::builder().method(method).uri(format!("/api{path}"));
            let body = match body {
                Some(v) => {
                    req = req.header("content-type", "application/json");
                    Body::from(serde_json::to_vec(&v)?)
                }
                None => Body::empty(),
            };
            let res = router.oneshot(req.body(body)?).await.map_err(|e| anyhow!("{e}"))?;
            let status = res.status();
            let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024 * 1024).await?;
            let v: Value = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())) };
            if !status.is_success() {
                let msg = v.get("error").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("HTTP {status}"));
                return Err(anyhow!(msg));
            }
            Ok(v)
        };
        runtime().spawn(fut).await.map_err(|e| anyhow!("Task failed: {e}"))?
    }

    pub async fn get(self: Arc<Self>, path: impl Into<String>) -> Result<Value> {
        self.call(Method::GET, path.into(), None).await
    }

    pub async fn post(self: Arc<Self>, path: impl Into<String>, body: Value) -> Result<Value> {
        self.call(Method::POST, path.into(), Some(body)).await
    }
}

/// Percent-encodes one path segment or query value.
pub fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The text of a cell: NULL, strings as they are, other JSON values in compact form.
pub fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
