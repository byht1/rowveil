//! Google BigQuery over the REST API.
//!
//! URL: `bigquery://PROJECT[/DATASET]?credentials=PATH&location=EU&maximumBytesBilled=N&endpoint=URL`
//!
//! Credentials, in order: `credentials=` (a service-account or `gcloud auth application-default
//! login` JSON file), `GOOGLE_APPLICATION_CREDENTIALS`, the gcloud ADC file, the GCE/GKE/Cloud Run
//! metadata server. With `endpoint=` (an emulator) and no credentials, requests are anonymous.
//!
//! Browsing a table without filters or sorting uses `tabledata.list`, which does not run a query
//! and costs nothing. Filters, sorting and the console run query jobs.

use crate::db::{ResultColumn, ResultSet, Sql, CONSOLE_ROW_LIMIT};
use crate::ddl::{Catalog, ColumnDef, ConstraintDef, Features, TableDetail, TableItem, ViewDetail, ViewItem};
use crate::schema::{Column, DbSchema, ForeignKey, Table};
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use futures::StreamExt;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SCOPE: &str = "https://www.googleapis.com/auth/bigquery";
const API: &str = "https://bigquery.googleapis.com";
const QUERY_TIMEOUT: Duration = Duration::from_secs(600);

enum Auth {
    None,
    ServiceAccount { email: String, key: Box<ring::signature::RsaKeyPair>, key_id: String, token_uri: String },
    User { client_id: String, client_secret: String, refresh_token: String },
    Metadata,
}

pub struct Bq {
    http: reqwest::Client,
    pub project: String,
    location: Option<String>,
    base: String,
    max_bytes: Option<String>,
    auth: Auth,
    /// Billing project header for user credentials (`quota_project_id`).
    quota_project: Option<String>,
    token: tokio::sync::Mutex<Option<(String, Instant)>>,
    /// Dataset from the URL, or the first dataset found; unqualified names resolve in it.
    dataset: Mutex<Option<String>>,
}

pub fn is_bigquery_url(lower: &str) -> bool {
    lower.starts_with("bigquery://") || lower.starts_with("bq://")
}

struct Parsed {
    project: String,
    dataset: Option<String>,
    credentials: Option<PathBuf>,
    location: Option<String>,
    endpoint: Option<String>,
    max_bytes: Option<String>,
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
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

fn parse_url(url: &str) -> Result<Parsed> {
    let rest = url.split_once("://").map(|x| x.1).unwrap_or("");
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let project = decode(parts.next().unwrap_or(""));
    if project.is_empty() {
        bail!("Enter the Google Cloud project: bigquery://PROJECT_ID");
    }
    let dataset = parts.next().map(decode).filter(|d| !d.is_empty());
    let mut p = Parsed { project, dataset, credentials: None, location: None, endpoint: None, max_bytes: None };
    for kv in query.split('&').filter(|x| !x.is_empty()) {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        let v = decode(v);
        if v.is_empty() {
            continue;
        }
        match k.to_ascii_lowercase().as_str() {
            "credentials" | "keyfile" | "key_file" => p.credentials = Some(PathBuf::from(v)),
            "location" => p.location = Some(v),
            "endpoint" => p.endpoint = Some(v.trim_end_matches('/').to_string()),
            "maximumbytesbilled" | "maxbytesbilled" | "max_bytes_billed" => {
                v.parse::<u64>().map_err(|_| anyhow!("maximumBytesBilled must be a number of bytes"))?;
                p.max_bytes = Some(v);
            }
            "dataset" => p.dataset = Some(v),
            _ => {}
        }
    }
    Ok(p)
}

/// The gcloud Application Default Credentials file.
fn adc_file() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CLOUDSDK_CONFIG") {
        return Some(PathBuf::from(dir).join("application_default_credentials.json"));
    }
    #[cfg(windows)]
    let dir = dirs::config_dir().map(|d| d.join("gcloud"));
    #[cfg(not(windows))]
    let dir = dirs::home_dir().map(|d| d.join(".config").join("gcloud"));
    dir.map(|d| d.join("application_default_credentials.json"))
}

fn pem_to_der(pem: &str) -> Result<Vec<u8>> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).map(str::trim).collect();
    base64::engine::general_purpose::STANDARD.decode(body).context("Invalid private key in the credentials file")
}

fn auth_from_file(path: &std::path::Path) -> Result<(Auth, Option<String>)> {
    let text = std::fs::read_to_string(path).with_context(|| format!("Cannot read the credentials file {}", path.display()))?;
    let v: Value = serde_json::from_str(&text).with_context(|| format!("{} is not a JSON credentials file", path.display()))?;
    let get = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let quota = v.get("quota_project_id").and_then(|x| x.as_str()).map(str::to_string);
    match get("type").as_str() {
        "service_account" => {
            let der = pem_to_der(&get("private_key"))?;
            let key = ring::signature::RsaKeyPair::from_pkcs8(&der)
                .map_err(|e| anyhow!("Cannot load the service account key: {e}"))?;
            let token_uri = Some(get("token_uri")).filter(|u| !u.is_empty()).unwrap_or_else(|| "https://oauth2.googleapis.com/token".into());
            Ok((Auth::ServiceAccount { email: get("client_email"), key: Box::new(key), key_id: get("private_key_id"), token_uri }, None))
        }
        "authorized_user" => Ok((
            Auth::User { client_id: get("client_id"), client_secret: get("client_secret"), refresh_token: get("refresh_token") },
            quota,
        )),
        other => bail!(
            "Credentials of type \"{other}\" are not supported. Use a service account key or `gcloud auth application-default login`"
        ),
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn b64url(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Result of one query job.
pub struct QueryOutput {
    pub fields: Vec<Value>,
    pub rows: Vec<Vec<Value>>,
    pub total_rows: Option<i64>,
    pub affected: Option<i64>,
    pub bytes_processed: Option<i64>,
    pub cache_hit: bool,
    pub job_id: Option<String>,
    pub job_location: Option<String>,
    pub truncated: bool,
}

impl Bq {
    pub async fn connect(url: &str) -> Result<Bq> {
        let p = parse_url(url)?;
        let (auth, quota_project) = if let Some(path) = &p.credentials {
            auth_from_file(path)?
        } else if let Some(path) = std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS").filter(|v| !v.is_empty()) {
            auth_from_file(std::path::Path::new(&path))?
        } else if p.endpoint.is_some() {
            (Auth::None, None)
        } else if let Some(path) = adc_file().filter(|p| p.is_file()) {
            auth_from_file(&path)?
        } else {
            (Auth::Metadata, None)
        };
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .user_agent(concat!("rowveil/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let bq = Bq {
            http,
            project: p.project,
            location: p.location,
            base: p.endpoint.unwrap_or_else(|| API.to_string()),
            max_bytes: p.max_bytes,
            auth,
            quota_project,
            token: tokio::sync::Mutex::new(None),
            dataset: Mutex::new(p.dataset),
        };
        // Check access: the project must be readable.
        bq.get(&format!("/projects/{}/datasets?maxResults=1", enc(&bq.project))).await.map_err(|e| {
            if matches!(bq.auth, Auth::Metadata) {
                anyhow!("{e:#}. No credentials found: set \"Key file\", GOOGLE_APPLICATION_CREDENTIALS, or run `gcloud auth application-default login`")
            } else {
                e
            }
        })?;
        Ok(bq)
    }

    pub fn default_dataset(&self) -> Option<String> {
        self.dataset.lock().ok().and_then(|d| d.clone())
    }

    async fn token(&self) -> Result<Option<String>> {
        if matches!(self.auth, Auth::None) {
            return Ok(None);
        }
        let mut cached = self.token.lock().await;
        if let Some((t, until)) = cached.as_ref() {
            if Instant::now() < *until {
                return Ok(Some(t.clone()));
            }
        }
        let resp: Value = match &self.auth {
            Auth::None => unreachable!(),
            Auth::ServiceAccount { email, key, key_id, token_uri } => {
                let iat = now_secs();
                let header = json!({ "alg": "RS256", "typ": "JWT", "kid": key_id });
                let claims = json!({ "iss": email, "scope": SCOPE, "aud": token_uri, "iat": iat, "exp": iat + 3600 });
                let signing_input = format!("{}.{}", b64url(header.to_string().as_bytes()), b64url(claims.to_string().as_bytes()));
                let mut sig = vec![0u8; key.public().modulus_len()];
                key.sign(&ring::signature::RSA_PKCS1_SHA256, &ring::rand::SystemRandom::new(), signing_input.as_bytes(), &mut sig)
                    .map_err(|_| anyhow!("Cannot sign the token request"))?;
                let jwt = format!("{signing_input}.{}", b64url(&sig));
                let r = self
                    .http
                    .post(token_uri)
                    .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", jwt.as_str())])
                    .send()
                    .await
                    .context("Google sign-in failed")?;
                token_json(r).await?
            }
            Auth::User { client_id, client_secret, refresh_token } => {
                let r = self
                    .http
                    .post("https://oauth2.googleapis.com/token")
                    .form(&[
                        ("grant_type", "refresh_token"),
                        ("client_id", client_id.as_str()),
                        ("client_secret", client_secret.as_str()),
                        ("refresh_token", refresh_token.as_str()),
                    ])
                    .send()
                    .await
                    .context("Google sign-in failed")?;
                token_json(r).await.map_err(|e| anyhow!("{e:#}. Run `gcloud auth application-default login` again"))?
            }
            Auth::Metadata => {
                let r = self
                    .http
                    .get("http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/token")
                    .query(&[("scopes", SCOPE)])
                    .header("Metadata-Flavor", "Google")
                    .timeout(Duration::from_secs(3))
                    .send()
                    .await
                    .map_err(|_| anyhow!("Google credentials not found"))?;
                token_json(r).await?
            }
        };
        let token = resp["access_token"].as_str().ok_or_else(|| anyhow!("Google sign-in returned no access token"))?.to_string();
        let ttl = resp["expires_in"].as_u64().unwrap_or(3600).saturating_sub(120).max(60);
        *cached = Some((token.clone(), Instant::now() + Duration::from_secs(ttl)));
        Ok(Some(token))
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value> {
        let mut req = req;
        if let Some(t) = self.token().await? {
            req = req.bearer_auth(t);
        }
        if let Some(q) = &self.quota_project {
            req = req.header("x-goog-user-project", q);
        }
        let r = req.send().await.context("BigQuery request failed")?;
        let status = r.status();
        let text = r.text().await.unwrap_or_default();
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = v["error"]["message"].as_str().map(str::to_string).unwrap_or_else(|| text.chars().take(300).collect());
            bail!("BigQuery: {msg} ({})", status.as_u16());
        }
        Ok(v)
    }

    async fn get(&self, path: &str) -> Result<Value> {
        self.send(self.http.get(format!("{}/bigquery/v2{path}", self.base))).await
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(self.http.post(format!("{}/bigquery/v2{path}", self.base)).json(body)).await
    }

    async fn paged(&self, path: &str, list_key: &str) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let sep = if path.contains('?') { '&' } else { '?' };
            let url = match &token {
                Some(t) => format!("{path}{sep}maxResults=1000&pageToken={}", enc(t)),
                None => format!("{path}{sep}maxResults=1000"),
            };
            let v = self.get(&url).await?;
            if let Some(a) = v[list_key].as_array() {
                out.extend(a.iter().cloned());
            }
            match v["nextPageToken"].as_str() {
                Some(t) if !t.is_empty() && out.len() < 50_000 => token = Some(t.to_string()),
                _ => break,
            }
        }
        Ok(out)
    }

    pub async fn datasets(&self) -> Result<Vec<String>> {
        let list = self.paged(&format!("/projects/{}/datasets", enc(&self.project)), "datasets").await?;
        let mut names: Vec<String> =
            list.iter().filter_map(|d| d["datasetReference"]["datasetId"].as_str().map(str::to_string)).collect();
        names.sort();
        Ok(names)
    }

    async fn table_list(&self, dataset: &str) -> Result<Vec<Value>> {
        self.paged(&format!("/projects/{}/datasets/{}/tables", enc(&self.project), enc(dataset)), "tables").await
    }

    pub async fn table_meta(&self, dataset: &str, table: &str) -> Result<Value> {
        self.get(&format!("/projects/{}/datasets/{}/tables/{}", enc(&self.project), enc(dataset), enc(table))).await
    }

    // ------------------------------------------------------------ queries

    /// Runs a GoogleSQL query with positional (`?`) STRING parameters.
    pub async fn query(&self, sql: &str, params: &[Option<String>], max_rows: usize, dry_run: bool) -> Result<QueryOutput> {
        let mut body = json!({
            "query": sql,
            "useLegacySql": false,
            "timeoutMs": 20000,
            "maxResults": max_rows.min(10_000),
            "formatOptions": { "useInt64Timestamp": true },
            "dryRun": dry_run,
        });
        if let Some(l) = &self.location {
            body["location"] = json!(l);
        }
        if let Some(m) = &self.max_bytes {
            body["maximumBytesBilled"] = json!(m);
        }
        if let Some(ds) = self.default_dataset() {
            body["defaultDataset"] = json!({ "projectId": self.project, "datasetId": ds });
        }
        if !params.is_empty() {
            body["parameterMode"] = json!("POSITIONAL");
            body["queryParameters"] = Value::Array(
                params
                    .iter()
                    .map(|p| match p {
                        Some(v) => json!({ "parameterType": { "type": "STRING" }, "parameterValue": { "value": v } }),
                        None => json!({ "parameterType": { "type": "STRING" }, "parameterValue": {} }),
                    })
                    .collect(),
            );
        }
        let started = Instant::now();
        let mut v = self.post(&format!("/projects/{}/queries", enc(&self.project)), &body).await?;
        let job_id = v["jobReference"]["jobId"].as_str().map(str::to_string);
        let job_location = v["jobReference"]["location"].as_str().map(str::to_string).or_else(|| self.location.clone());
        // Wait for the job.
        while !dry_run && v["jobComplete"].as_bool() == Some(false) {
            if started.elapsed() > QUERY_TIMEOUT {
                bail!("The query is still running after {} s; check it in the Google Cloud console", QUERY_TIMEOUT.as_secs());
            }
            let id = job_id.as_deref().ok_or_else(|| anyhow!("BigQuery returned no job id"))?;
            v = self.results(id, job_location.as_deref(), None, max_rows).await?;
        }
        check_errors(&v)?;
        let fields = v["schema"]["fields"].as_array().cloned().unwrap_or_default();
        let mut rows: Vec<Vec<Value>> = v["rows"].as_array().map(|r| r.iter().map(|row| row_values(row, &fields)).collect()).unwrap_or_default();
        let total_rows = v["totalRows"].as_str().and_then(|x| x.parse().ok());
        // Further pages.
        let mut page = v["pageToken"].as_str().map(str::to_string);
        while let (Some(token), Some(id)) = (page.clone(), job_id.as_deref()) {
            if rows.len() >= max_rows {
                break;
            }
            let next = self.results(id, job_location.as_deref(), Some(&token), max_rows - rows.len()).await?;
            if let Some(r) = next["rows"].as_array() {
                rows.extend(r.iter().map(|row| row_values(row, &fields)));
            }
            page = next["pageToken"].as_str().map(str::to_string);
        }
        let truncated = rows.len() > max_rows || page.is_some() || total_rows.is_some_and(|t: i64| t as usize > rows.len().min(max_rows));
        rows.truncate(max_rows);
        Ok(QueryOutput {
            fields,
            rows,
            total_rows,
            affected: v["numDmlAffectedRows"].as_str().and_then(|x| x.parse().ok()),
            bytes_processed: v["totalBytesProcessed"].as_str().and_then(|x| x.parse().ok()),
            cache_hit: v["cacheHit"].as_bool().unwrap_or(false),
            job_id,
            job_location,
            truncated,
        })
    }

    async fn results(&self, job_id: &str, location: Option<&str>, page: Option<&str>, max_rows: usize) -> Result<Value> {
        let mut path = format!(
            "/projects/{}/queries/{}?timeoutMs=20000&maxResults={}&formatOptions.useInt64Timestamp=true",
            enc(&self.project),
            enc(job_id),
            max_rows.clamp(1, 10_000)
        );
        if let Some(l) = location {
            path.push_str(&format!("&location={}", enc(l)));
        }
        if let Some(p) = page {
            path.push_str(&format!("&pageToken={}", enc(p)));
        }
        self.get(&path).await
    }

    async fn job(&self, job_id: &str, location: Option<&str>) -> Result<Value> {
        let mut path = format!("/projects/{}/jobs/{}", enc(&self.project), enc(job_id));
        if let Some(l) = location {
            path.push_str(&format!("?location={}", enc(l)));
        }
        self.get(&path).await
    }

    /// `Db::fetch` for BigQuery: rows as JSON values (scalars as strings).
    pub async fn fetch(&self, sql: &Sql) -> Result<Vec<Vec<Value>>> {
        Ok(self.query(&sql.text, &sql.params, 10_000, false).await?.rows)
    }

    /// DML statements one by one (BigQuery has no client-side transactions over REST).
    pub async fn execute_all(&self, stmts: &[Sql]) -> Result<u64> {
        let mut total = 0u64;
        for (i, s) in stmts.iter().enumerate() {
            let out = self.query(&s.text, &s.params, 1, false).await.map_err(|e| anyhow!("Change #{} failed: {e:#}", i + 1))?;
            total += out.affected.unwrap_or(0).max(0) as u64;
        }
        Ok(total)
    }

    /// Browses table rows without a query (free): `tabledata.list`.
    pub async fn browse(&self, dataset: &str, table: &str, offset: i64, limit: i64) -> Result<(Vec<Vec<Value>>, Option<i64>)> {
        let meta = self.table_meta(dataset, table).await?;
        let fields = meta["schema"]["fields"].as_array().cloned().unwrap_or_default();
        let path = format!(
            "/projects/{}/datasets/{}/tables/{}/data?startIndex={}&maxResults={}&formatOptions.useInt64Timestamp=true",
            enc(&self.project),
            enc(dataset),
            enc(table),
            offset.max(0),
            limit.clamp(1, 10_000)
        );
        let v = self.get(&path).await?;
        let rows = v["rows"].as_array().map(|r| r.iter().map(|row| row_values(row, &fields)).collect()).unwrap_or_default();
        let total = v["totalRows"].as_str().and_then(|x| x.parse().ok()).or_else(|| meta["numRows"].as_str().and_then(|x| x.parse().ok()));
        Ok((rows, total))
    }

    /// The console. `EXPLAIN <query>` runs a dry run and reports the bytes the query would scan.
    pub async fn run_console(&self, sql: &str) -> Result<Vec<ResultSet>> {
        let trimmed = sql.trim_start();
        if trimmed.len() > 8 && trimmed[..8].eq_ignore_ascii_case("explain ") {
            let out = self.query(&trimmed[8..], &[], 0, true).await?;
            let bytes = out.bytes_processed.unwrap_or(0);
            return Ok(vec![ResultSet {
                columns: vec![
                    ResultColumn { name: "estimate".into(), type_name: "STRING".into(), kind: "text".into() },
                    ResultColumn { name: "bytes".into(), type_name: "INT64".into(), kind: "number".into() },
                ],
                rows: vec![vec![json!(format!("Dry run: this query will process {}", human_bytes(bytes))), json!(bytes)]],
                rows_affected: 0,
                truncated: false,
                info: Some("Dry run: nothing was billed".into()),
            }]);
        }
        let out = self.query(sql, &[], CONSOLE_ROW_LIMIT, false).await?;
        let mut info = vec![];
        let job = match out.job_id.as_deref() {
            Some(id) => self.job(id, out.job_location.as_deref()).await.ok(),
            None => None,
        };
        let stats = job.as_ref().map(|j| &j["statistics"]);
        let billed = stats.and_then(|s| s["query"]["totalBytesBilled"].as_str()).and_then(|x| x.parse::<i64>().ok());
        if out.cache_hit {
            info.push("cached result".to_string());
        } else if let Some(p) = out.bytes_processed.or_else(|| stats.and_then(|s| s["totalBytesProcessed"].as_str()).and_then(|x| x.parse().ok())) {
            info.push(format!("processed {}", human_bytes(p)));
            if let Some(b) = billed {
                info.push(format!("billed {}", human_bytes(b)));
            }
        }
        let info = if info.is_empty() { None } else { Some(info.join(" · ")) };

        // A script: show the result of every SELECT, in order.
        let is_script = stats.and_then(|s| s["query"]["statementType"].as_str()) == Some("SCRIPT");
        if is_script {
            if let Some(id) = out.job_id.as_deref() {
                if let Ok(children) = self.child_results(id, out.job_location.as_deref()).await {
                    if !children.is_empty() {
                        let mut sets = children;
                        if let Some(last) = sets.last_mut() {
                            last.info = info;
                        }
                        return Ok(sets);
                    }
                }
            }
        }
        Ok(vec![result_set(out, info)])
    }

    async fn child_results(&self, parent: &str, location: Option<&str>) -> Result<Vec<ResultSet>> {
        let mut jobs = self
            .paged(&format!("/projects/{}/jobs?parentJobId={}&projection=full", enc(&self.project), enc(parent)), "jobs")
            .await?;
        jobs.sort_by_key(|j| j["statistics"]["creationTime"].as_str().and_then(|x| x.parse::<i64>().ok()).unwrap_or(0));
        let mut sets = Vec::new();
        for j in jobs {
            let st = j["statistics"]["query"]["statementType"].as_str().unwrap_or("");
            let Some(id) = j["jobReference"]["jobId"].as_str() else { continue };
            if st == "SELECT" {
                let v = self.results(id, location, None, CONSOLE_ROW_LIMIT).await?;
                let fields = v["schema"]["fields"].as_array().cloned().unwrap_or_default();
                let rows: Vec<Vec<Value>> =
                    v["rows"].as_array().map(|r| r.iter().map(|row| row_values(row, &fields)).collect()).unwrap_or_default();
                let total: Option<i64> = v["totalRows"].as_str().and_then(|x| x.parse().ok());
                let truncated = total.is_some_and(|t| t as usize > rows.len());
                sets.push(result_set(
                    QueryOutput { fields, rows, total_rows: total, affected: None, bytes_processed: None, cache_hit: false, job_id: None, job_location: None, truncated },
                    None,
                ));
            } else if let Some(n) = j["statistics"]["query"]["numDmlAffectedRows"].as_str().and_then(|x| x.parse::<u64>().ok()) {
                sets.push(ResultSet { columns: vec![], rows: vec![], rows_affected: n, truncated: false, info: None });
            }
        }
        Ok(sets)
    }

    // ------------------------------------------------------------ schema

    pub async fn introspect(&self) -> Result<DbSchema> {
        let datasets = self.datasets().await?;
        {
            let mut d = self.dataset.lock().map_err(|_| anyhow!("lock"))?;
            if d.as_ref().is_none_or(|x| !datasets.contains(x)) {
                *d = datasets.first().cloned().or(d.clone());
            }
        }
        let mut refs: Vec<(String, String)> = Vec::new();
        for ds in &datasets {
            for t in self.table_list(ds).await? {
                if let Some(name) = t["tableReference"]["tableId"].as_str() {
                    refs.push((ds.clone(), name.to_string()));
                }
            }
        }
        let futs: Vec<_> = refs
            .iter()
            .cloned()
            .map(|(ds, t)| async move { self.table_meta(&ds, &t).await })
            .collect();
        let metas: Vec<Result<Value>> = futures::stream::iter(futs).buffered(12).collect().await;
        let mut tables = Vec::new();
        for ((ds, name), meta) in refs.into_iter().zip(metas) {
            let meta = match meta {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("BigQuery: cannot read {ds}.{name}: {e:#}");
                    continue;
                }
            };
            tables.push(table_from_meta(&ds, &name, &meta));
        }
        Ok(DbSchema {
            dialect: crate::db::Dialect::Bigquery,
            default_schema: self.default_dataset().unwrap_or_default(),
            schemas: datasets,
            tables,
        })
    }

    pub async fn catalog(&self) -> Result<Catalog> {
        let datasets = self.datasets().await?;
        let mut tables = Vec::new();
        let mut views = Vec::new();
        for ds in &datasets {
            for t in self.table_list(ds).await? {
                let name = t["tableReference"]["tableId"].as_str().unwrap_or("").to_string();
                match t["type"].as_str().unwrap_or("TABLE") {
                    "VIEW" => views.push(ViewItem { schema: ds.clone(), name, materialized: false }),
                    "MATERIALIZED_VIEW" => views.push(ViewItem { schema: ds.clone(), name, materialized: true }),
                    _ => tables.push(TableItem { schema: ds.clone(), name, rls: false }),
                }
            }
        }
        Ok(Catalog {
            dialect: crate::db::Dialect::Bigquery,
            features: Features {
                schemas: true,
                rename_schema: false,
                enums: false,
                roles: false,
                policies: false,
                rls: false,
                alter_column: true,
                constraints: false,
                index_methods: false,
                materialized_views: true,
            },
            default_schema: self.default_dataset().or_else(|| datasets.first().cloned()).unwrap_or_default(),
            schemas: datasets,
            tables,
            views,
            enums: vec![],
            roles: vec![],
            policies: vec![],
        })
    }

    pub async fn table_detail(&self, dataset: &str, name: &str) -> Result<TableDetail> {
        let meta = self.table_meta(dataset, name).await?;
        let fields = meta["schema"]["fields"].as_array().cloned().unwrap_or_default();
        let pk: Vec<String> = string_list(&meta["tableConstraints"]["primaryKey"]["columns"]);
        let columns = fields
            .iter()
            .map(|f| {
                let name = f["name"].as_str().unwrap_or("").to_string();
                ColumnDef {
                    type_name: field_type(f),
                    not_null: f["mode"].as_str() == Some("REQUIRED"),
                    default: f["defaultValueExpression"].as_str().map(str::to_string),
                    primary: pk.contains(&name),
                    identity: None,
                    generated: None,
                    auto_increment: false,
                    definition: column_sql(f),
                    name,
                }
            })
            .collect();
        let mut constraints = Vec::new();
        if !pk.is_empty() {
            constraints.push(ConstraintDef {
                name: "PRIMARY KEY".into(),
                kind: "primary".into(),
                definition: format!("PRIMARY KEY ({}) NOT ENFORCED", pk.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")),
            });
        }
        for fk in meta["tableConstraints"]["foreignKeys"].as_array().into_iter().flatten() {
            let cols: Vec<String> = fk["columnReferences"].as_array().into_iter().flatten().filter_map(|c| c["referencingColumn"].as_str().map(qi)).collect();
            let refs: Vec<String> = fk["columnReferences"].as_array().into_iter().flatten().filter_map(|c| c["referencedColumn"].as_str().map(qi)).collect();
            let rt = &fk["referencedTable"];
            constraints.push(ConstraintDef {
                name: fk["name"].as_str().unwrap_or("").to_string(),
                kind: "foreign".into(),
                definition: format!(
                    "FOREIGN KEY ({}) REFERENCES {}.{}({}) NOT ENFORCED",
                    cols.join(", "),
                    qi(rt["datasetId"].as_str().unwrap_or("")),
                    qi(rt["tableId"].as_str().unwrap_or("")),
                    refs.join(", ")
                ),
            });
        }
        Ok(TableDetail {
            schema: dataset.to_string(),
            name: name.to_string(),
            rls: false,
            columns,
            constraints,
            indexes: vec![],
            policies: vec![],
            triggers: vec![],
            ddl: self.table_ddl(dataset, name, &meta),
        })
    }

    fn table_ddl(&self, dataset: &str, name: &str, meta: &Value) -> String {
        let full = format!("{}.{}", qi(dataset), qi(name));
        match meta["type"].as_str().unwrap_or("TABLE") {
            "VIEW" => return format!("CREATE VIEW {full} AS\n{};", meta["view"]["query"].as_str().unwrap_or("").trim()),
            "MATERIALIZED_VIEW" => {
                return format!("CREATE MATERIALIZED VIEW {full} AS\n{};", meta["materializedView"]["query"].as_str().unwrap_or("").trim())
            }
            _ => {}
        }
        let fields = meta["schema"]["fields"].as_array().cloned().unwrap_or_default();
        let mut lines: Vec<String> = fields.iter().map(|f| format!("  {}", column_sql(f))).collect();
        let pk = string_list(&meta["tableConstraints"]["primaryKey"]["columns"]);
        if !pk.is_empty() {
            lines.push(format!("  PRIMARY KEY ({}) NOT ENFORCED", pk.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")));
        }
        let mut out = format!("CREATE TABLE {full} (\n{}\n)", lines.join(",\n"));
        let tp = &meta["timePartitioning"];
        if tp.is_object() {
            let unit = tp["type"].as_str().unwrap_or("DAY");
            out.push_str(&match tp["field"].as_str() {
                Some(f) => format!("\nPARTITION BY TIMESTAMP_TRUNC({}, {unit})", qi(f)).replace("TIMESTAMP_TRUNC", trunc_fn(&fields, f)),
                None => format!("\nPARTITION BY _PARTITIONDATE{}", if unit == "DAY" { String::new() } else { format!(" -- {unit}") }),
            });
        } else if let Some(rp) = meta["rangePartitioning"].as_object() {
            let r = &rp["range"];
            out.push_str(&format!(
                "\nPARTITION BY RANGE_BUCKET({}, GENERATE_ARRAY({}, {}, {}))",
                qi(rp["field"].as_str().unwrap_or("")),
                r["start"].as_str().unwrap_or("0"),
                r["end"].as_str().unwrap_or("0"),
                r["interval"].as_str().unwrap_or("1")
            ));
        }
        let cluster = string_list(&meta["clustering"]["fields"]);
        if !cluster.is_empty() {
            out.push_str(&format!("\nCLUSTER BY {}", cluster.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")));
        }
        if let Some(d) = meta["description"].as_str().filter(|d| !d.is_empty()) {
            out.push_str(&format!("\nOPTIONS (description = {})", lit(d)));
        }
        out.push(';');
        out
    }

    pub async fn view_detail(&self, dataset: &str, name: &str) -> Result<ViewDetail> {
        let meta = self.table_meta(dataset, name).await?;
        let materialized = meta["type"].as_str() == Some("MATERIALIZED_VIEW");
        let definition = if materialized { meta["materializedView"]["query"].as_str() } else { meta["view"]["query"].as_str() }
            .unwrap_or("")
            .trim()
            .to_string();
        Ok(ViewDetail { schema: dataset.into(), name: name.into(), materialized, definition, ddl: self.table_ddl(dataset, name, &meta) })
    }

    pub async fn dump(&self) -> Result<String> {
        let mut out = vec![format!("-- BigQuery project {}", self.project)];
        for ds in self.datasets().await? {
            out.push(format!("\nCREATE SCHEMA IF NOT EXISTS {};", qi(&ds)));
            let list = self.table_list(&ds).await?;
            let mut views = Vec::new();
            for t in list {
                let Some(name) = t["tableReference"]["tableId"].as_str() else { continue };
                let meta = self.table_meta(&ds, name).await?;
                let ddl = self.table_ddl(&ds, name, &meta);
                if t["type"].as_str().is_some_and(|k| k.contains("VIEW")) {
                    views.push(ddl);
                } else {
                    out.push(format!("\n{ddl}"));
                }
            }
            for v in views {
                out.push(format!("\n{v}"));
            }
        }
        Ok(out.join("\n") + "\n")
    }

    /// Schema changes from the Schema view: each statement is one job, in order.
    pub async fn execute_ddl(&self, statements: &[String]) -> Result<()> {
        for (i, s) in statements.iter().enumerate() {
            self.query(s, &[], 0, false).await.map_err(|e| anyhow!("Statement #{} failed: {e:#}", i + 1))?;
        }
        Ok(())
    }
}

async fn token_json(r: reqwest::Response) -> Result<Value> {
    let status = r.status();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = v["error_description"].as_str().or_else(|| v["error"].as_str()).unwrap_or("unknown error");
        bail!("Google sign-in failed: {msg}");
    }
    Ok(v)
}

fn check_errors(v: &Value) -> Result<()> {
    if let Some(e) = v["errors"].as_array().and_then(|a| a.first()) {
        if v["jobComplete"].as_bool() != Some(true) || v["rows"].is_null() {
            bail!("BigQuery: {}", e["message"].as_str().unwrap_or("query failed"));
        }
    }
    Ok(())
}

fn result_set(out: QueryOutput, info: Option<String>) -> ResultSet {
    let columns = out
        .fields
        .iter()
        .map(|f| ResultColumn { name: f["name"].as_str().unwrap_or("").to_string(), type_name: field_type(f), kind: field_kind(f) })
        .collect::<Vec<_>>();
    let kinds: Vec<String> = columns.iter().map(|c| c.kind.clone()).collect();
    let rows = out
        .rows
        .into_iter()
        .map(|r| r.into_iter().enumerate().map(|(i, v)| crate::db::typed_value(kinds.get(i).map(String::as_str).unwrap_or("text"), v)).collect())
        .collect();
    ResultSet { columns, rows, rows_affected: out.affected.unwrap_or(0).max(0) as u64, truncated: out.truncated, info }
}

fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn qi(name: &str) -> String {
    format!("`{}`", name.replace('\\', "\\\\").replace('`', "\\`"))
}

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

pub fn human_bytes(n: i64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", units[u])
    }
}

fn string_list(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn trunc_fn(fields: &[Value], col: &str) -> &'static str {
    match fields.iter().find(|f| f["name"].as_str() == Some(col)).and_then(|f| f["type"].as_str()) {
        Some("DATE") => "DATE_TRUNC",
        Some("DATETIME") => "DATETIME_TRUNC",
        _ => "TIMESTAMP_TRUNC",
    }
}

/// Scalar type name in GoogleSQL (legacy REST names mapped).
fn base_type(t: &str) -> &str {
    match t {
        "INTEGER" => "INT64",
        "FLOAT" => "FLOAT64",
        "BOOLEAN" => "BOOL",
        "RECORD" => "STRUCT",
        other => other,
    }
}

/// Full GoogleSQL type of a schema field: `ARRAY<STRUCT<a INT64, b STRING>>`, `NUMERIC(10, 2)`…
pub fn field_type(f: &Value) -> String {
    let t = base_type(f["type"].as_str().unwrap_or("STRING"));
    let mut s = if t == "STRUCT" {
        let inner: Vec<String> = f["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| format!("{} {}", x["name"].as_str().unwrap_or(""), field_type(x)))
            .collect();
        format!("STRUCT<{}>", inner.join(", "))
    } else if let (Some(p), Some(sc)) = (f["precision"].as_str(), f["scale"].as_str()) {
        format!("{t}({p}, {sc})")
    } else if let Some(p) = f["precision"].as_str() {
        format!("{t}({p})")
    } else if let Some(l) = f["maxLength"].as_str() {
        format!("{t}({l})")
    } else if t == "RANGE" {
        format!("RANGE<{}>", f["rangeElementType"]["type"].as_str().unwrap_or("DATE"))
    } else {
        t.to_string()
    };
    if f["mode"].as_str() == Some("REPEATED") {
        s = format!("ARRAY<{s}>");
    }
    s
}

fn column_sql(f: &Value) -> String {
    let mut s = format!("{} {}", qi(f["name"].as_str().unwrap_or("")), field_type(f));
    if f["mode"].as_str() == Some("REQUIRED") {
        s.push_str(" NOT NULL");
    }
    if let Some(d) = f["defaultValueExpression"].as_str() {
        s.push_str(&format!(" DEFAULT {d}"));
    }
    if let Some(d) = f["description"].as_str().filter(|d| !d.is_empty()) {
        s.push_str(&format!(" OPTIONS (description = {})", lit(d)));
    }
    s
}

/// UI kind of a field (see `typed_value`).
pub fn field_kind(f: &Value) -> String {
    if f["mode"].as_str() == Some("REPEATED") {
        return "json".into();
    }
    match base_type(f["type"].as_str().unwrap_or("STRING")) {
        "INT64" | "NUMERIC" | "BIGNUMERIC" | "BIGDECIMAL" | "DECIMAL" => "bigint",
        "FLOAT64" => "number",
        "BOOL" => "boolean",
        "DATE" | "DATETIME" | "TIME" | "TIMESTAMP" => "date",
        "JSON" | "STRUCT" => "json",
        "BYTES" => "binary",
        _ => "text",
    }
    .into()
}

fn table_from_meta(dataset: &str, name: &str, meta: &Value) -> Table {
    let fields = meta["schema"]["fields"].as_array().cloned().unwrap_or_default();
    let primary_key = string_list(&meta["tableConstraints"]["primaryKey"]["columns"]);
    let columns = fields
        .iter()
        .map(|f| {
            let t = base_type(f["type"].as_str().unwrap_or("STRING")).to_string();
            Column {
                name: f["name"].as_str().unwrap_or("").to_string(),
                data_type: field_type(f),
                cast_type: t,
                kind: field_kind(f),
                nullable: f["mode"].as_str() != Some("REQUIRED"),
                default: f["defaultValueExpression"].as_str().map(str::to_string),
                primary: false,
                auto: false,
                generated: false,
                enum_values: None,
                references: None,
            }
        })
        .collect();
    let foreign_keys = meta["tableConstraints"]["foreignKeys"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|fk| {
            let refs = fk["columnReferences"].as_array().cloned().unwrap_or_default();
            ForeignKey {
                name: fk["name"].as_str().unwrap_or("").to_string(),
                columns: refs.iter().filter_map(|c| c["referencingColumn"].as_str().map(str::to_string)).collect(),
                ref_schema: fk["referencedTable"]["datasetId"].as_str().unwrap_or("").to_string(),
                ref_table: fk["referencedTable"]["tableId"].as_str().unwrap_or("").to_string(),
                ref_columns: refs.iter().filter_map(|c| c["referencedColumn"].as_str().map(str::to_string)).collect(),
            }
        })
        .collect();
    let kind = match meta["type"].as_str().unwrap_or("TABLE") {
        "VIEW" => "view",
        "MATERIALIZED_VIEW" => "materialized_view",
        "EXTERNAL" => "foreign_table",
        _ => "table",
    };
    Table {
        schema: dataset.to_string(),
        name: name.to_string(),
        kind: kind.into(),
        columns,
        primary_key,
        foreign_keys,
        indexes: vec![],
        row_estimate: meta["numRows"].as_str().and_then(|x| x.parse().ok()),
    }
}

// ------------------------------------------------------------ values

fn row_values(row: &Value, fields: &[Value]) -> Vec<Value> {
    let cells = row["f"].as_array().cloned().unwrap_or_default();
    cells.iter().enumerate().map(|(i, c)| cell_value(&c["v"], fields.get(i).unwrap_or(&Value::Null), true)).collect()
}

/// A tabledata/query cell as JSON. At the top level scalars stay strings (the grid types them by
/// column kind); STRUCT and ARRAY become JSON text with typed values inside.
fn cell_value(v: &Value, field: &Value, top: bool) -> Value {
    if v.is_null() {
        return Value::Null;
    }
    // The emulator omits `mode`, so an array value also marks a repeated field.
    let repeated = field["mode"].as_str() == Some("REPEATED") || v.is_array();
    let t = base_type(field["type"].as_str().unwrap_or("STRING"));
    let nested = if repeated {
        let single = {
            let mut f = field.clone();
            f["mode"] = json!("NULLABLE");
            f
        };
        Value::Array(
            v.as_array()
                .into_iter()
                .flatten()
                .map(|x| cell_value(if x.get("v").is_some() { &x["v"] } else { x }, &single, false))
                .collect(),
        )
    } else if t == "STRUCT" {
        let sub = field["fields"].as_array().cloned().unwrap_or_default();
        let mut o = serde_json::Map::new();
        for (i, c) in v["f"].as_array().into_iter().flatten().enumerate() {
            let f = sub.get(i).cloned().unwrap_or(Value::Null);
            o.insert(f["name"].as_str().unwrap_or("").to_string(), cell_value(&c["v"], &f, false));
        }
        Value::Object(o)
    } else {
        let s = match v.as_str() {
            Some(s) => s.to_string(),
            None => v.to_string(),
        };
        let s = if t == "TIMESTAMP" { format_timestamp(&s) } else { s };
        if top {
            return Value::String(s);
        }
        return match t {
            "INT64" => s.parse::<i64>().ok().filter(|n| n.unsigned_abs() <= 1 << 53).map(Value::from).unwrap_or(Value::String(s)),
            "FLOAT64" => s.parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::String(s)),
            "BOOL" => Value::Bool(s == "true"),
            "JSON" => serde_json::from_str(&s).unwrap_or(Value::String(s)),
            _ => Value::String(s),
        };
    };
    if top {
        Value::String(nested.to_string())
    } else {
        nested
    }
}

/// `1700000000123456` (µs) or `1.700000000123456E9` (s) → `2023-11-14 22:13:20.123456 UTC`.
pub fn format_timestamp(s: &str) -> String {
    let micros: i64 = if let Ok(n) = s.parse::<i64>() {
        n
    } else if let Ok(f) = s.parse::<f64>() {
        (f * 1_000_000.0).round() as i64
    } else {
        return s.to_string();
    };
    let secs = micros.div_euclid(1_000_000);
    let frac = micros.rem_euclid(1_000_000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    let base = format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", tod / 3600, tod % 3600 / 60, tod % 60);
    if frac == 0 {
        format!("{base} UTC")
    } else {
        format!("{base}.{} UTC", format!("{frac:06}").trim_end_matches('0'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls() {
        let p = parse_url("bigquery://my-proj/sales?credentials=C%3A%5Ckeys%5Csa.json&location=EU&maximumBytesBilled=1000").unwrap();
        assert_eq!(p.project, "my-proj");
        assert_eq!(p.dataset.as_deref(), Some("sales"));
        assert_eq!(p.credentials.unwrap(), PathBuf::from("C:\\keys\\sa.json"));
        assert_eq!(p.location.as_deref(), Some("EU"));
        assert_eq!(p.max_bytes.as_deref(), Some("1000"));
        assert!(parse_url("bigquery://").is_err());
        assert!(parse_url("bigquery://p?maximumBytesBilled=x").is_err());
    }

    #[test]
    fn formats_timestamps() {
        assert_eq!(format_timestamp("0"), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_timestamp("1700000000123456"), "2023-11-14 22:13:20.123456 UTC");
        assert_eq!(format_timestamp("1.7E9"), "2023-11-14 22:13:20 UTC");
        assert_eq!(format_timestamp("-1000000"), "1969-12-31 23:59:59 UTC");
    }

    #[test]
    fn nested_values() {
        let field = json!({ "name": "s", "type": "RECORD", "mode": "NULLABLE", "fields": [
            { "name": "a", "type": "INTEGER" }, { "name": "tags", "type": "STRING", "mode": "REPEATED" } ] });
        let v = json!({ "f": [ { "v": "7" }, { "v": [ { "v": "x" }, { "v": "y" } ] } ] });
        assert_eq!(cell_value(&v, &field, true), Value::String(r#"{"a":7,"tags":["x","y"]}"#.into()));
        assert_eq!(field_type(&field), "STRUCT<a INT64, tags ARRAY<STRING>>");
    }
}
