//! Rowveil: database explorer and query tool (server + embedded web UI).

pub mod access;
pub mod config;
pub mod data;
pub mod ddl;
pub mod db;
pub mod schema;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use config::ConnConfig;
use db::Db;
use schema::DbSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};

/// Options for building the HTTP application.
pub struct Options {
    /// Connection opened on start (from DATABASE_URL / --url).
    pub url: Option<String>,
    /// Directory for saved connections.
    pub data_dir: PathBuf,
    /// Block all data changes.
    pub read_only: bool,
    /// Basic auth (user, password).
    pub basic_auth: Option<(String, String)>,
    /// Desktop mode: a per-launch secret. Only the app window, which receives it, can use the API.
    pub app_token: Option<String>,
}

/// Default directory for saved connections. On first start, copies `connections.json`
/// from the directories of the app's earlier names ("roostbase", "querynest", "rust-studio").
pub fn default_data_dir() -> PathBuf {
    let Some(base) = dirs::config_dir() else { return PathBuf::from(".rowveil") };
    let dir = base.join("rowveil");
    let new = dir.join("connections.json");
    if !new.exists() {
        for old_name in ["roostbase", "querynest", "rust-studio"] {
            let old = base.join(old_name).join("connections.json");
            if old.exists() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::copy(&old, &new);
                break;
            }
        }
    }
    dir
}

struct AppState {
    conns: RwLock<Vec<ConnConfig>>,
    pools: Mutex<HashMap<String, Db>>,
    schemas: RwLock<HashMap<String, Arc<DbSchema>>>,
    store: PathBuf,
    read_only: bool,
    auth: Option<String>,
    app_token: Option<String>,
}

type Shared = Arc<AppState>;

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        let e: anyhow::Error = e.into();
        ApiError(StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}

fn not_found(what: &str) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, format!("{what} not found"))
}

type ApiResult<T> = Result<T, ApiError>;

impl AppState {
    async fn conn(&self, id: &str) -> ApiResult<ConnConfig> {
        self.conns.read().await.iter().find(|c| c.id == id).cloned().ok_or_else(|| not_found("Connection"))
    }

    async fn db(&self, id: &str) -> ApiResult<Db> {
        let cfg = self.conn(id).await?;
        let mut pools = self.pools.lock().await;
        if let Some(db) = pools.get(id) {
            return Ok(db.clone());
        }
        let db = db::connect(&cfg.url).await?;
        pools.insert(id.to_string(), db.clone());
        Ok(db)
    }

    async fn schema(&self, id: &str, refresh: bool) -> ApiResult<Arc<DbSchema>> {
        if !refresh {
            if let Some(s) = self.schemas.read().await.get(id) {
                return Ok(s.clone());
            }
        }
        let db = self.db(id).await?;
        let s = Arc::new(schema::introspect(&db).await?);
        self.schemas.write().await.insert(id.to_string(), s.clone());
        Ok(s)
    }

    async fn is_read_only(&self, id: &str) -> ApiResult<bool> {
        Ok(self.read_only_reason(id).await?.is_some())
    }

    /// Why the connection cannot change data, or None when it can.
    async fn read_only_reason(&self, id: &str) -> ApiResult<Option<String>> {
        if self.read_only {
            return Ok(Some("Rowveil runs in read-only mode".into()));
        }
        if self.conn(id).await?.read_only {
            return Ok(Some("Connection is read-only".into()));
        }
        if self.db(id).await?.is_read_only() {
            if self.sqlite_write_blockers(id).await?.is_empty() {
                return Ok(Some("The connection URL opens the SQLite file read-only (mode=ro)".into()));
            }
            return Ok(Some(if access::CAN_GRANT {
                "The database file is write-protected for the current user. Click \"Grant write access\" to fix it once".into()
            } else {
                "The database file is write-protected for the current user. Change its owner or mode (chown / chmod) to edit it".into()
            }));
        }
        Ok(None)
    }

    /// For a SQLite connection: the files and folders that block writes for the current OS user.
    async fn sqlite_write_blockers(&self, id: &str) -> ApiResult<Vec<PathBuf>> {
        let cfg = self.conn(id).await?;
        let (dialect, url) = db::normalize_url(&cfg.url)?;
        if dialect != db::Dialect::Sqlite {
            return Ok(Vec::new());
        }
        let (path, _) = db::sqlite_target(&url);
        if path.as_os_str() == ":memory:" {
            return Ok(Vec::new());
        }
        Ok(access::write_blockers(&path))
    }

    /// Drops the cached pool and schema so the next request reconnects.
    async fn reset_connection(&self, id: &str) {
        if let Some(db) = self.pools.lock().await.remove(id) {
            db.close().await;
        }
        self.schemas.write().await.remove(id);
    }
}

// ---------- static files ----------

macro_rules! asset {
    ($name:literal, $mime:literal) => {{
        async fn handler() -> Response {
            #[cfg(debug_assertions)]
            if let Ok(body) = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/", $name)) {
                return ([(header::CONTENT_TYPE, $mime), (header::CACHE_CONTROL, "no-cache")], body).into_response();
            }
            (
                [(header::CONTENT_TYPE, $mime), (header::CACHE_CONTROL, "no-cache")],
                include_str!(concat!("../static/", $name)),
            )
                .into_response()
        }
        get(handler)
    }};
}

macro_rules! asset_bytes {
    ($name:literal, $mime:literal) => {{
        async fn handler() -> Response {
            #[cfg(debug_assertions)]
            if let Ok(body) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/static/", $name)) {
                return ([(header::CONTENT_TYPE, $mime), (header::CACHE_CONTROL, "no-cache")], body).into_response();
            }
            (
                [(header::CONTENT_TYPE, $mime), (header::CACHE_CONTROL, "no-cache")],
                include_bytes!(concat!("../static/", $name)).as_slice(),
            )
                .into_response()
        }
        get(handler)
    }};
}

// ---------- handlers ----------

async fn info(State(st): State<Shared>) -> Json<Value> {
    Json(json!({ "version": env!("CARGO_PKG_VERSION"), "readOnly": st.read_only, "desktop": st.app_token.is_some() }))
}

async fn list_connections(State(st): State<Shared>) -> Json<Value> {
    let conns = st.conns.read().await;
    Json(json!(conns.iter().map(|c| c.info(st.read_only)).collect::<Vec<_>>()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewConnection {
    #[serde(default)]
    name: String,
    url: String,
    #[serde(default)]
    read_only: bool,
}

async fn test_connection(Json(body): Json<NewConnection>) -> ApiResult<Json<Value>> {
    let db = db::connect(&body.url).await?;
    let dialect = db.dialect();
    db.close().await;
    Ok(Json(json!({ "ok": true, "dialect": dialect })))
}

async fn add_connection(State(st): State<Shared>, Json(body): Json<NewConnection>) -> ApiResult<Json<Value>> {
    let db = db::connect(&body.url).await?;
    let name = if body.name.trim().is_empty() { config::name_from_url(&body.url) } else { body.name.trim().to_string() };
    let cfg = ConnConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        url: body.url.trim().to_string(),
        read_only: body.read_only,
        source: "user".into(),
    };
    st.pools.lock().await.insert(cfg.id.clone(), db);
    let mut conns = st.conns.write().await;
    conns.push(cfg.clone());
    config::save_user_connections(&st.store, &conns)?;
    Ok(Json(json!(cfg.info(st.read_only))))
}

async fn delete_connection(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut conns = st.conns.write().await;
    let pos = conns.iter().position(|c| c.id == id).ok_or_else(|| not_found("Connection"))?;
    if conns[pos].source == "env" {
        return Err(ApiError(StatusCode::BAD_REQUEST, "Connections from environment cannot be removed".into()));
    }
    conns.remove(pos);
    config::save_user_connections(&st.store, &conns)?;
    drop(conns);
    st.reset_connection(&id).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct SchemaQuery {
    #[serde(default)]
    refresh: Option<String>,
}

async fn get_schema(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Query(q): Query<SchemaQuery>,
) -> ApiResult<Json<Value>> {
    let refresh = q.refresh.is_some_and(|v| v != "0" && v != "false");
    let s = st.schema(&id, refresh).await?;
    let reason = st.read_only_reason(&id).await?;
    let mut v = serde_json::to_value(&*s)?;
    v["readOnly"] = json!(reason.is_some());
    v["readOnlyReason"] = json!(reason);
    let can_grant = access::CAN_GRANT
        && !st.read_only
        && !st.conn(&id).await?.read_only
        && st.db(&id).await?.is_read_only()
        && !st.sqlite_write_blockers(&id).await?.is_empty();
    v["canGrantWrite"] = json!(can_grant);
    Ok(Json(v))
}

/// Asks the OS (one UAC prompt on Windows) to give the current user write access to the
/// SQLite file, then reconnects.
async fn grant_write(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let blockers = st.sqlite_write_blockers(&id).await?;
    if !blockers.is_empty() {
        tracing::info!("granting write access to {:?}", blockers);
        access::grant_write(&blockers).await?;
    }
    st.reset_connection(&id).await;
    let left = st.sqlite_write_blockers(&id).await?;
    if !left.is_empty() {
        let list = left.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!("Access was granted, but these paths are still write-protected: {list}. Check the file's read-only attribute"),
        ));
    }
    Ok(Json(json!({ "ok": true, "granted": blockers.iter().map(|p| p.display().to_string()).collect::<Vec<_>>() })))
}

async fn find_table(st: &AppState, id: &str, schema_name: &str, table: &str) -> ApiResult<schema::Table> {
    let s = st.schema(id, false).await?;
    if let Some(t) = s.table(schema_name, table) {
        return Ok(t.clone());
    }
    // The table may be new: refresh once.
    let s = st.schema(id, true).await?;
    s.table(schema_name, table)
        .cloned()
        .ok_or_else(|| not_found(&format!("Table {schema_name}.{table}")))
}

async fn get_rows(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<data::RowsRequest>,
) -> ApiResult<Json<data::RowsResponse>> {
    let t = find_table(&st, &id, &req.schema, &req.table).await?;
    let db = st.db(&id).await?;
    Ok(Json(data::fetch_rows(&db, &t, &req).await?))
}

async fn apply_changes(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<data::ApplyRequest>,
) -> ApiResult<Json<Value>> {
    if let Some(reason) = st.read_only_reason(&id).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, reason));
    }
    let t = find_table(&st, &id, &req.schema, &req.table).await?;
    let db = st.db(&id).await?;
    let stmts = data::build_changes(db.dialect(), &t, &req.changes)?;
    let affected = db.execute_tx(&stmts).await?;
    Ok(Json(json!({ "ok": true, "rowsAffected": affected })))
}

#[derive(Deserialize)]
struct SqlRequest {
    sql: String,
}

/// Rough check that a script only reads data (used in read-only mode).
fn is_read_query(sql: &str) -> bool {
    const READ: &[&str] = &["select", "with", "show", "explain", "pragma", "describe", "desc", "values", "table"];
    const WRITE: &[&str] = &[
        "insert", "update", "delete", "drop", "alter", "create", "truncate", "grant", "revoke", "replace", "merge",
        "attach", "detach", "vacuum", "reindex", "call", "do", "copy",
    ];
    sql.split(';').map(str::trim).filter(|s| !s.is_empty()).all(|stmt| {
        let lower = stmt.to_ascii_lowercase();
        let first = lower.split_whitespace().next().unwrap_or("");
        READ.contains(&first) && !lower.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').any(|w| WRITE.contains(&w))
    })
}

async fn run_sql(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<SqlRequest>,
) -> ApiResult<Json<Value>> {
    if st.is_read_only(&id).await? && !is_read_query(&req.sql) {
        return Err(ApiError(StatusCode::FORBIDDEN, "Connection is read-only: only SELECT-like queries are allowed".into()));
    }
    let db = st.db(&id).await?;
    let started = Instant::now();
    let results = db.run_console(&req.sql).await?;
    Ok(Json(json!({ "results": results, "durationMs": started.elapsed().as_secs_f64() * 1000.0 })))
}

// ---------- schema management ----------

#[derive(Deserialize)]
struct ObjQuery {
    #[serde(default)]
    schema: String,
    name: String,
}

async fn ddl_catalog(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let db = st.db(&id).await?;
    let mut v = serde_json::to_value(ddl::catalog(&db).await?)?;
    v["readOnlyReason"] = json!(st.read_only_reason(&id).await?);
    Ok(Json(v))
}

async fn ddl_table(State(st): State<Shared>, Path(id): Path<String>, Query(q): Query<ObjQuery>) -> ApiResult<Json<Value>> {
    let db = st.db(&id).await?;
    Ok(Json(serde_json::to_value(ddl::table(&db, &q.schema, &q.name).await?)?))
}

async fn ddl_view(State(st): State<Shared>, Path(id): Path<String>, Query(q): Query<ObjQuery>) -> ApiResult<Json<Value>> {
    let db = st.db(&id).await?;
    Ok(Json(serde_json::to_value(ddl::view(&db, &q.schema, &q.name).await?)?))
}

async fn ddl_role(State(st): State<Shared>, Path(id): Path<String>, Query(q): Query<ObjQuery>) -> ApiResult<Json<Value>> {
    let db = st.db(&id).await?;
    Ok(Json(serde_json::to_value(ddl::role(&db, &q.name).await?)?))
}

async fn ddl_dump(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let db = st.db(&id).await?;
    Ok(Json(json!({ "sql": ddl::dump(&db).await? })))
}

#[derive(Deserialize)]
struct DdlRequest {
    statements: Vec<String>,
}

async fn ddl_apply(State(st): State<Shared>, Path(id): Path<String>, Json(req): Json<DdlRequest>) -> ApiResult<Json<Value>> {
    if let Some(reason) = st.read_only_reason(&id).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, reason));
    }
    let db = st.db(&id).await?;
    let started = Instant::now();
    ddl::execute(&db, &req.statements).await?;
    st.schemas.write().await.remove(&id);
    Ok(Json(json!({ "ok": true, "durationMs": started.elapsed().as_secs_f64() * 1000.0 })))
}

// ---------- desktop: native file dialogs ----------

#[derive(Deserialize)]
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
struct SaveFileRequest {
    filename: String,
    content: String,
}

/// Desktop mode: "Save as" dialog for exports. Returns the chosen path, or `cancelled`.
async fn desktop_save(State(st): State<Shared>, Json(req): Json<SaveFileRequest>) -> ApiResult<Json<Value>> {
    if st.app_token.is_none() {
        return Err(not_found("Endpoint"));
    }
    #[cfg(feature = "desktop")]
    {
        let ext = std::path::Path::new(&req.filename).extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let picked = tokio::task::spawn_blocking(move || {
            // The async dialog hands the panel to the UI (main) thread where the OS needs it
            // (macOS); this worker thread only waits for the answer.
            let mut dialog = rfd::AsyncFileDialog::new().set_title("Save as").set_file_name(&req.filename);
            if let Some(dir) = dirs::download_dir() {
                dialog = dialog.set_directory(dir);
            }
            if !ext.is_empty() {
                dialog = dialog.add_filter(ext.to_uppercase(), &[ext.as_str()]);
            }
            futures::executor::block_on(dialog.save_file()).map(|h| (h.path().to_path_buf(), req.content))
        })
        .await
        .map_err(|e| anyhow::anyhow!("dialog failed: {e}"))?;
        match picked {
            None => Ok(Json(json!({ "cancelled": true }))),
            Some((path, content)) => {
                tokio::fs::write(&path, content).await.map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", path.display()))?;
                Ok(Json(json!({ "path": path.display().to_string() })))
            }
        }
    }
    #[cfg(not(feature = "desktop"))]
    {
        let _ = req;
        Err(not_found("Endpoint"))
    }
}

#[derive(Deserialize)]
struct RevealRequest {
    path: String,
}

/// Desktop mode: show a saved file in Explorer (or the file manager).
async fn desktop_reveal(State(st): State<Shared>, Json(req): Json<RevealRequest>) -> ApiResult<Json<Value>> {
    if st.app_token.is_none() {
        return Err(not_found("Endpoint"));
    }
    let path = PathBuf::from(&req.path);
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(&path).spawn();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = open::that_detached(path.parent().unwrap_or(&path));
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------- auth ----------

const TOKEN_COOKIE: &str = "rv_token";

/// Desktop mode: the window opens `/?token=…` once; the server answers with an HttpOnly
/// cookie, and every later request must carry it. Other local processes get 403.
async fn app_token_guard(State(st): State<Shared>, headers: HeaderMap, req: Request, next: Next) -> Response {
    let Some(token) = &st.app_token else { return next.run(req).await };
    let from_query = req
        .uri()
        .query()
        .is_some_and(|q| q.split('&').any(|kv| kv.strip_prefix("token=") == Some(token.as_str())));
    let from_cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .any(|c| c.trim().strip_prefix("rv_token=") == Some(token.as_str()));
    if from_query {
        let mut res = next.run(req).await;
        if let Ok(v) = HeaderValue::from_str(&format!("{TOKEN_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/")) {
            res.headers_mut().append(header::SET_COOKIE, v);
        }
        return res;
    }
    if from_cookie {
        return next.run(req).await;
    }
    (StatusCode::FORBIDDEN, "Rowveil: this server belongs to a desktop window").into_response()
}


async fn basic_auth(State(st): State<Shared>, headers: HeaderMap, req: Request, next: Next) -> Response {
    let Some(expected) = &st.auth else { return next.run(req).await };
    let ok = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == expected);
    if ok {
        return next.run(req).await;
    }
    let mut res = StatusCode::UNAUTHORIZED.into_response();
    res.headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic realm=\"Rowveil\""));
    res
}

/// Builds the HTTP application. Returns the router and the path of the saved-connections file.
pub fn router(opts: &Options) -> (Router, PathBuf) {
    let store = config::store_path(&opts.data_dir);
    let mut conns = config::env_connections(opts.url.as_deref());
    conns.extend(config::load_user_connections(&store));
    let auth = opts
        .basic_auth
        .as_ref()
        .filter(|(_, p)| !p.is_empty())
        .map(|(u, p)| format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"))));
    let state: Shared = Arc::new(AppState {
        conns: RwLock::new(conns),
        pools: Mutex::new(HashMap::new()),
        schemas: RwLock::new(HashMap::new()),
        store: store.clone(),
        read_only: opts.read_only,
        auth,
        app_token: opts.app_token.clone(),
    });

    let api = Router::new()
        .route("/info", get(info))
        .route("/connections", get(list_connections).post(add_connection))
        .route("/connections/test", post(test_connection))
        .route("/connections/{id}", axum::routing::delete(delete_connection))
        .route("/connections/{id}/schema", get(get_schema))
        .route("/connections/{id}/rows", post(get_rows))
        .route("/connections/{id}/apply", post(apply_changes))
        .route("/connections/{id}/sql", post(run_sql))
        .route("/connections/{id}/grant-write", post(grant_write))
        .route("/connections/{id}/ddl", post(ddl_apply))
        .route("/connections/{id}/ddl/catalog", get(ddl_catalog))
        .route("/connections/{id}/ddl/table", get(ddl_table))
        .route("/connections/{id}/ddl/view", get(ddl_view))
        .route("/connections/{id}/ddl/role", get(ddl_role))
        .route("/connections/{id}/ddl/dump", get(ddl_dump))
        .route("/desktop/save", post(desktop_save))
        .route("/desktop/reveal", post(desktop_reveal));

    let app = Router::new()
        .route("/", asset!("index.html", "text/html; charset=utf-8"))
        .route("/app.js", asset!("app.js", "text/javascript; charset=utf-8"))
        .route("/style.css", asset!("style.css", "text/css; charset=utf-8"))
        .route("/vendor/codemirror.js", asset!("vendor/codemirror.js", "text/javascript; charset=utf-8"))
        .route("/favicon.svg", asset!("favicon.svg", "image/svg+xml"))
        .route("/favicon.ico", asset_bytes!("favicon.ico", "image/x-icon"))
        .route("/brand/symbol.svg", asset!("brand/symbol.svg", "image/svg+xml"))
        .route("/brand/app-icon.svg", asset!("brand/app-icon.svg", "image/svg+xml"))
        .nest("/api", api)
        .layer(middleware::from_fn_with_state(state.clone(), basic_auth))
        .layer(middleware::from_fn_with_state(state.clone(), app_token_guard))
        // A panic in one request returns a JSON error instead of dropping the connection.
        .layer(tower_http::catch_panic::CatchPanicLayer::custom(|err: Box<dyn std::any::Any + Send>| {
            let msg = err
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".into());
            tracing::error!("request panicked: {msg}");
            ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("Internal error: {msg}")).into_response()
        }))
        .with_state(state);
    (app, store)
}

#[cfg(test)]
mod tests {
    use super::is_read_query;

    #[test]
    fn read_query_detection() {
        assert!(is_read_query("select * from users; select 1"));
        assert!(is_read_query("WITH a AS (SELECT 1) SELECT * FROM a"));
        assert!(!is_read_query("delete from users"));
        assert!(!is_read_query("with x as (delete from users returning *) select * from x"));
        assert!(!is_read_query("select 1; drop table users"));
    }
}
