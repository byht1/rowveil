//! Redis: key browser, typed values, edits and a command console.

use anyhow::{anyhow, bail, Context, Result};
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{Client, Value as RV};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Mutex;

/// SCAN COUNT per step when a pattern or a type filter is set.
const SCAN_STEP_FILTERED: usize = 5000;

/// Max items of a collection (hash, list, set, …) returned per request.
const PAGE_MAX: i64 = 1000;
/// Max bytes of a string value shown in the UI.
const STRING_MAX: usize = 512 * 1024;

pub struct Redis {
    client: Client,
    /// Database from the URL (`redis://host/3`).
    pub default_db: i64,
    conns: Mutex<HashMap<i64, ConnectionManager>>,
}

pub fn is_redis_url(lower: &str) -> bool {
    lower.starts_with("redis://") || lower.starts_with("rediss://") || lower.starts_with("valkey://") || lower.starts_with("valkeys://")
}

/// `valkey://` is an alias of `redis://`.
pub fn normalize(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("valkeys://") {
        return format!("rediss://{}", &url["valkeys://".len()..]);
    }
    if lower.starts_with("valkey://") {
        return format!("redis://{}", &url["valkey://".len()..]);
    }
    url.to_string()
}

impl Redis {
    pub async fn connect(url: &str) -> Result<Redis> {
        let client = Client::open(normalize(url)).context("Invalid Redis URL")?;
        let default_db = client.get_connection_info().redis_settings().db();
        let r = Redis { client, default_db, conns: Mutex::new(HashMap::new()) };
        let mut c = r.conn(default_db).await?;
        let _: String = redis::cmd("PING").query_async(&mut c).await.map_err(|e| anyhow!("Redis PING failed: {}", redis_error(&e)))?;
        Ok(r)
    }

    /// A connection bound to database `db` (one multiplexed connection per database).
    async fn conn(&self, db: i64) -> Result<ConnectionManager> {
        let mut conns = self.conns.lock().await;
        if let Some(c) = conns.get(&db) {
            return Ok(c.clone());
        }
        let info = self.client.get_connection_info().clone();
        let settings = info.redis_settings().clone().set_db(db);
        let client = Client::open(info.set_redis_settings(settings))?;
        let cfg = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(10)))
            .set_response_timeout(Some(Duration::from_secs(60)))
            .set_number_of_retries(1);
        let c = tokio::time::timeout(Duration::from_secs(12), client.get_connection_manager_with_config(cfg))
            .await
            .map_err(|_| anyhow!("Redis connection timed out"))?
            .map_err(|e| anyhow!("Redis connection failed: {}", redis_error(&e)))?;
        conns.insert(db, c.clone());
        Ok(c)
    }

    /// Server info and the databases that hold keys.
    pub async fn overview(&self) -> Result<Value> {
        let mut c = self.conn(self.default_db).await?;
        let info: String = redis::cmd("INFO").query_async(&mut c).await?;
        let mut fields = Map::new();
        let mut dbs: Vec<Value> = Vec::new();
        for line in info.lines() {
            let Some((k, v)) = line.split_once(':') else { continue };
            if let Some(n) = k.strip_prefix("db").and_then(|n| n.parse::<i64>().ok()) {
                let keys = v.split(',').find_map(|kv| kv.strip_prefix("keys=")).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
                dbs.push(json!({ "db": n, "keys": keys }));
                continue;
            }
            const KEEP: &[&str] = &[
                "redis_version", "valkey_version", "server_name", "redis_mode", "os", "uptime_in_seconds", "connected_clients",
                "used_memory_human", "maxmemory_human", "role", "total_commands_processed", "keyspace_hits", "keyspace_misses",
            ];
            if KEEP.contains(&k) {
                fields.insert(k.to_string(), Value::String(v.trim().to_string()));
            }
        }
        // CONFIG GET may be disabled (managed services); 16 is the default.
        let count: i64 = match redis::cmd("CONFIG").arg("GET").arg("databases").query_async::<Vec<String>>(&mut c).await {
            Ok(v) => v.get(1).and_then(|x| x.parse().ok()).unwrap_or(16),
            Err(_) => 16,
        };
        let modules: Vec<String> = match redis::cmd("MODULE").arg("LIST").query_async::<RV>(&mut c).await {
            Ok(RV::Array(items)) => items
                .iter()
                .filter_map(|m| {
                    let v = reply_json(m);
                    v.get("name").and_then(|n| n.as_str()).map(str::to_string).or_else(|| {
                        v.as_array().and_then(|a| a.get(1)).and_then(|n| n.as_str()).map(str::to_string)
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        Ok(json!({
            "server": fields,
            "databases": count.max(self.default_db + 1),
            "keyspace": dbs,
            "defaultDb": self.default_db,
            "modules": modules,
        }))
    }

    pub async fn scan(&self, req: &ScanRequest) -> Result<Value> {
        let mut c = self.conn(req.db).await?;
        let pattern = if req.pattern.trim().is_empty() { "*".to_string() } else { req.pattern.clone() };
        let want = req.count.clamp(10, 5000) as usize;
        let mut cursor: u64 = req.cursor.parse().unwrap_or(0);
        let mut keys: Vec<Vec<u8>> = Vec::new();
        let type_filter = req.key_type.as_deref().filter(|t| !t.is_empty());
        // Exact-key fast path: a filter without glob characters may name a key directly; show it
        // first, before SCAN walks a possibly huge keyspace.
        let mut exact: Option<Vec<u8>> = None;
        if cursor == 0 {
            if let Some(k) = req.exact.as_deref().filter(|k| !k.is_empty()) {
                let n: i64 = redis::cmd("EXISTS").arg(k).query_async(&mut c).await?;
                if n > 0 {
                    exact = Some(k.as_bytes().to_vec());
                    keys.push(k.as_bytes().to_vec());
                }
            }
        }
        // A filter matches few keys per SCAN step, so each step walks more of the keyspace
        // (one round trip over a VPN costs ~100 ms).
        let step = if pattern != "*" || type_filter.is_some() { SCAN_STEP_FILTERED } else { want.max(200) };
        // SCAN may return few keys per call: loop until the page is full or the scan ends,
        // but never longer than about a second.
        let started = std::time::Instant::now();
        loop {
            let mut cmd = redis::cmd("SCAN");
            cmd.arg(cursor).arg("MATCH").arg(&pattern).arg("COUNT").arg(step);
            if let Some(t) = type_filter {
                cmd.arg("TYPE").arg(t);
            }
            let (next, batch): (u64, Vec<Vec<u8>>) = cmd.query_async(&mut c).await?;
            keys.extend(batch.into_iter().filter(|k| exact.as_ref() != Some(k)));
            cursor = next;
            if cursor == 0 || keys.len() >= want || started.elapsed() > Duration::from_millis(1200) {
                break;
            }
        }
        let mut pipe = redis::pipe();
        for k in &keys {
            pipe.cmd("TYPE").arg(k).cmd("PTTL").arg(k);
        }
        let meta: Vec<RV> = if keys.is_empty() { Vec::new() } else { pipe.query_async(&mut c).await? };
        let items: Vec<Value> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let t = meta.get(i * 2).map(reply_text).unwrap_or_default();
                let ttl = meta.get(i * 2 + 1).and_then(|v| if let RV::Int(n) = v { Some(*n) } else { None }).unwrap_or(-1);
                let mut v = json!({ "key": key_text(k), "type": t, "ttl": ttl });
                if exact.as_ref() == Some(k) {
                    v["exact"] = json!(true);
                }
                v
            })
            .filter(|v| type_filter.is_none_or(|t| v["type"] == t || v["exact"].is_null()))
            .collect();
        Ok(json!({ "cursor": cursor.to_string(), "keys": items }))
    }

    pub async fn key(&self, req: &KeyRequest) -> Result<Value> {
        let mut c = self.conn(req.db).await?;
        let key = req.key.as_bytes();
        let t: String = redis::cmd("TYPE").arg(key).query_async(&mut c).await?;
        if t == "none" {
            bail!("Key \"{}\" does not exist", req.key);
        }
        let ttl: i64 = redis::cmd("PTTL").arg(key).query_async(&mut c).await?;
        let memory: Option<i64> = redis::cmd("MEMORY").arg("USAGE").arg(key).query_async(&mut c).await.ok();
        let encoding: Option<String> = redis::cmd("OBJECT").arg("ENCODING").arg(key).query_async(&mut c).await.ok();
        let limit = req.limit.clamp(1, PAGE_MAX);
        let offset = req.offset.max(0);
        let cursor: u64 = req.cursor.parse().unwrap_or(0);
        let mut out = json!({ "key": req.key, "type": t, "ttl": ttl, "memory": memory, "encoding": encoding });
        match t.as_str() {
            "string" => {
                let len: i64 = redis::cmd("STRLEN").arg(key).query_async(&mut c).await?;
                let bytes: Vec<u8> = redis::cmd("GETRANGE").arg(key).arg(0).arg(STRING_MAX as i64 - 1).query_async(&mut c).await?;
                let (text, binary) = bytes_text(&bytes);
                out["length"] = json!(len);
                out["value"] = json!(text);
                out["binary"] = json!(binary);
                out["truncated"] = json!(len as usize > STRING_MAX);
            }
            "hash" => {
                let len: i64 = redis::cmd("HLEN").arg(key).query_async(&mut c).await?;
                let (next, items) = scan_collection(&mut c, "HSCAN", key, cursor, limit, &req.filter).await?;
                let pairs: Vec<Value> = items.chunks(2).map(|p| json!([key_text(&p[0]), value_text(p.get(1))])).collect();
                out["length"] = json!(len);
                out["items"] = json!(pairs);
                out["cursor"] = json!(next.to_string());
            }
            "set" => {
                let len: i64 = redis::cmd("SCARD").arg(key).query_async(&mut c).await?;
                let (next, items) = scan_collection(&mut c, "SSCAN", key, cursor, limit, &req.filter).await?;
                out["length"] = json!(len);
                out["items"] = json!(items.iter().map(|m| json!(key_text(m))).collect::<Vec<_>>());
                out["cursor"] = json!(next.to_string());
            }
            "list" => {
                let len: i64 = redis::cmd("LLEN").arg(key).query_async(&mut c).await?;
                let items: Vec<Vec<u8>> = redis::cmd("LRANGE").arg(key).arg(offset).arg(offset + limit - 1).query_async(&mut c).await?;
                out["length"] = json!(len);
                out["offset"] = json!(offset);
                out["items"] = json!(items.iter().map(|m| json!(key_text(m))).collect::<Vec<_>>());
            }
            "zset" => {
                let len: i64 = redis::cmd("ZCARD").arg(key).query_async(&mut c).await?;
                let mut cmd = redis::cmd("ZRANGE");
                cmd.arg(key).arg(offset).arg(offset + limit - 1);
                if req.desc {
                    cmd.arg("REV");
                }
                let items: Vec<(Vec<u8>, String)> = cmd.arg("WITHSCORES").query_async(&mut c).await?;
                out["length"] = json!(len);
                out["offset"] = json!(offset);
                out["items"] = json!(items.iter().map(|(m, s)| json!([key_text(m), s])).collect::<Vec<_>>());
            }
            "stream" => {
                let len: i64 = redis::cmd("XLEN").arg(key).query_async(&mut c).await?;
                // Newest first; `cursor` is the id to continue below (exclusive).
                let end = if req.cursor.is_empty() || req.cursor == "0" { "+".to_string() } else { format!("({}", req.cursor) };
                let raw: RV = redis::cmd("XREVRANGE").arg(key).arg(&end).arg("-").arg("COUNT").arg(limit).query_async(&mut c).await?;
                let mut entries = Vec::new();
                let mut last = String::new();
                if let RV::Array(list) = raw {
                    for e in list {
                        if let RV::Array(parts) = e {
                            let id = parts.first().map(reply_text).unwrap_or_default();
                            let mut fields = Map::new();
                            if let Some(RV::Array(kv)) = parts.get(1) {
                                for p in kv.chunks(2) {
                                    fields.insert(reply_text(&p[0]), json!(p.get(1).map(reply_text).unwrap_or_default()));
                                }
                            }
                            last = id.clone();
                            entries.push(json!({ "id": id, "fields": fields }));
                        }
                    }
                }
                // RESP2 returns each group as [name, value, name, value, …].
                let groups: Vec<Value> = match redis::cmd("XINFO").arg("GROUPS").arg(key).query_async::<RV>(&mut c).await {
                    Ok(RV::Array(list)) => list
                        .iter()
                        .map(|g| match g {
                            RV::Array(kv) => Value::Object(kv.chunks(2).map(|p| (reply_text(&p[0]), p.get(1).map(reply_json).unwrap_or(Value::Null))).collect()),
                            other => reply_json(other),
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                out["length"] = json!(len);
                out["items"] = json!(entries);
                out["cursor"] = json!(if entries.len() as i64 == limit { last } else { "0".into() });
                out["groups"] = json!(groups);
            }
            "ReJSON-RL" => {
                let v: Option<String> = redis::cmd("JSON.GET").arg(key).query_async(&mut c).await?;
                out["type"] = json!("json");
                out["value"] = json!(v.unwrap_or_default());
            }
            other => {
                out["unsupported"] = json!(format!("Values of type \"{other}\" are shown in the console only"));
            }
        }
        Ok(out)
    }

    pub async fn edit(&self, req: &EditRequest) -> Result<Value> {
        let mut c = self.conn(req.db).await?;
        let key = req.key.as_bytes();
        let s = |name: &str| -> Result<String> {
            req.args.get(name).and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            }).ok_or_else(|| anyhow!("Missing \"{name}\""))
        };
        let exists = || async {
            let n: i64 = redis::cmd("EXISTS").arg(key).query_async(&mut self.conn(req.db).await?).await?;
            if n == 0 {
                bail!("Key \"{}\" no longer exists", req.key);
            }
            Ok(())
        };
        match req.op.as_str() {
            "create" => {
                let t = s("type")?;
                let n: i64 = redis::cmd("EXISTS").arg(key).query_async(&mut c).await?;
                if n > 0 {
                    bail!("Key \"{}\" already exists", req.key);
                }
                let cmd = match t.as_str() {
                    "string" => redis::cmd("SET").arg(key).arg(s("value")?).clone(),
                    "hash" => redis::cmd("HSET").arg(key).arg(s("field")?).arg(s("value")?).clone(),
                    "list" => redis::cmd("RPUSH").arg(key).arg(s("value")?).clone(),
                    "set" => redis::cmd("SADD").arg(key).arg(s("value")?).clone(),
                    "zset" => redis::cmd("ZADD").arg(key).arg(score(&s("score")?)?).arg(s("value")?).clone(),
                    "stream" => {
                        let mut cmd = redis::cmd("XADD");
                        cmd.arg(key).arg("*");
                        for (f, v) in stream_fields(&req.args)? {
                            cmd.arg(f).arg(v);
                        }
                        cmd
                    }
                    "json" => redis::cmd("JSON.SET").arg(key).arg("$").arg(s("value")?).clone(),
                    other => bail!("Unknown key type: {other}"),
                };
                let _: RV = cmd.query_async(&mut c).await?;
                if let Ok(ttl) = s("ttl") {
                    if let Ok(secs) = ttl.trim().parse::<i64>() {
                        if secs > 0 {
                            let _: RV = redis::cmd("EXPIRE").arg(key).arg(secs).query_async(&mut c).await?;
                        }
                    }
                }
            }
            "set" => {
                exists().await?;
                let _: RV = redis::cmd("SET").arg(key).arg(s("value")?).arg("KEEPTTL").query_async(&mut c).await?;
            }
            "json_set" => {
                exists().await?;
                let _: RV = redis::cmd("JSON.SET").arg(key).arg("$").arg(s("value")?).query_async(&mut c).await?;
            }
            "hset" => {
                exists().await?;
                if let Ok(old) = s("oldField") {
                    let new = s("field")?;
                    if old != new {
                        let taken: i64 = redis::cmd("HEXISTS").arg(key).arg(&new).query_async(&mut c).await?;
                        if taken > 0 {
                            bail!("Field \"{new}\" already exists");
                        }
                        let _: RV = redis::pipe().atomic().cmd("HDEL").arg(key).arg(&old).cmd("HSET").arg(key).arg(&new).arg(s("value")?).query_async(&mut c).await?;
                        return Ok(json!({ "ok": true }));
                    }
                }
                let _: RV = redis::cmd("HSET").arg(key).arg(s("field")?).arg(s("value")?).query_async(&mut c).await?;
            }
            "hdel" => {
                let _: RV = redis::cmd("HDEL").arg(key).arg(s("field")?).query_async(&mut c).await?;
            }
            "lpush" | "rpush" => {
                let _: RV = redis::cmd(&req.op.to_uppercase()).arg(key).arg(s("value")?).query_async(&mut c).await?;
            }
            "lset" => {
                exists().await?;
                let index: i64 = s("index")?.parse()?;
                let _: RV = redis::cmd("LSET").arg(key).arg(index).arg(s("value")?).query_async(&mut c).await?;
            }
            "lrem_index" => {
                // Lists cannot delete by index: replace the element with a unique marker, then remove it.
                let index: i64 = s("index")?.parse()?;
                let expected = s("value")?;
                let current: Option<String> = redis::cmd("LINDEX").arg(key).arg(index).query_async(&mut c).await?;
                if current.as_deref() != Some(expected.as_str()) {
                    bail!("The list changed; refresh and try again");
                }
                let marker = format!("__rowveil_delete_{}", uuid::Uuid::new_v4());
                let _: RV = redis::pipe()
                    .atomic()
                    .cmd("LSET").arg(key).arg(index).arg(&marker)
                    .cmd("LREM").arg(key).arg(1).arg(&marker)
                    .query_async(&mut c)
                    .await?;
            }
            "sadd" => {
                if let Ok(old) = s("oldValue") {
                    let _: RV = redis::pipe().atomic().cmd("SREM").arg(key).arg(&old).cmd("SADD").arg(key).arg(s("value")?).query_async(&mut c).await?;
                } else {
                    let _: RV = redis::cmd("SADD").arg(key).arg(s("value")?).query_async(&mut c).await?;
                }
            }
            "srem" => {
                let _: RV = redis::cmd("SREM").arg(key).arg(s("value")?).query_async(&mut c).await?;
            }
            "zadd" => {
                let sc = score(&s("score")?)?;
                if let Ok(old) = s("oldValue") {
                    let _: RV = redis::pipe().atomic().cmd("ZREM").arg(key).arg(&old).cmd("ZADD").arg(key).arg(sc).arg(s("value")?).query_async(&mut c).await?;
                } else {
                    let _: RV = redis::cmd("ZADD").arg(key).arg(sc).arg(s("value")?).query_async(&mut c).await?;
                }
            }
            "zrem" => {
                let _: RV = redis::cmd("ZREM").arg(key).arg(s("value")?).query_async(&mut c).await?;
            }
            "xadd" => {
                let mut cmd = redis::cmd("XADD");
                cmd.arg(key).arg(s("id").unwrap_or_else(|_| "*".into()));
                for (f, v) in stream_fields(&req.args)? {
                    cmd.arg(f).arg(v);
                }
                let id: String = cmd.query_async(&mut c).await?;
                return Ok(json!({ "ok": true, "id": id }));
            }
            "xdel" => {
                let _: RV = redis::cmd("XDEL").arg(key).arg(s("id")?).query_async(&mut c).await?;
            }
            "expire" => {
                exists().await?;
                let ttl = s("ttl").unwrap_or_default();
                if ttl.trim().is_empty() || ttl.trim() == "-1" {
                    let _: RV = redis::cmd("PERSIST").arg(key).query_async(&mut c).await?;
                } else {
                    let secs: i64 = ttl.trim().parse().map_err(|_| anyhow!("TTL must be a whole number of seconds"))?;
                    if secs <= 0 {
                        bail!("TTL must be greater than 0. Leave it empty to remove the expiry");
                    }
                    let _: RV = redis::cmd("EXPIRE").arg(key).arg(secs).query_async(&mut c).await?;
                }
            }
            "rename" => {
                let to = s("newKey")?;
                if to.is_empty() {
                    bail!("Enter the new key name");
                }
                let n: i64 = redis::cmd("RENAMENX").arg(key).arg(&to).query_async(&mut c).await?;
                if n == 0 {
                    bail!("Key \"{to}\" already exists");
                }
            }
            "delete" => {
                let _: RV = redis::cmd("UNLINK").arg(key).query_async(&mut c).await?;
            }
            other => bail!("Unknown operation: {other}"),
        }
        Ok(json!({ "ok": true }))
    }

    /// Runs one console command. Returns the reply as typed JSON (see [`reply_json_typed`]).
    pub async fn command(&self, db: i64, line: &str, read_only: bool) -> Result<Value> {
        let args = split_command(line)?;
        let Some(first) = args.first() else { bail!("Empty command") };
        let name = String::from_utf8_lossy(first).to_ascii_uppercase();
        const BLOCKED: &[&str] = &[
            "SUBSCRIBE", "PSUBSCRIBE", "SSUBSCRIBE", "MONITOR", "SYNC", "PSYNC", "QUIT", "RESET", "SELECT", "HELLO", "AUTH",
            "CLIENT|REPLY", "SHUTDOWN", "DEBUG",
        ];
        if BLOCKED.contains(&name.as_str()) {
            if name == "SELECT" {
                bail!("Use the database picker in the sidebar to switch databases");
            }
            bail!("{name} is not supported in the console");
        }
        if read_only && !is_read_command(&args) {
            bail!("Connection is read-only: only read commands are allowed");
        }
        let mut cmd = redis::cmd(&name);
        for a in &args[1..] {
            cmd.arg(a.as_slice());
        }
        let mut c = self.conn(db).await?;
        let reply: RV = cmd.query_async(&mut c).await?;
        Ok(reply_json_typed(&reply))
    }
}

/// Error text without the repeated detail (some errors print "X: X").
fn redis_error(e: &redis::RedisError) -> String {
    let s = e.to_string();
    if s.len() > 2 && (s.len() - 2).is_multiple_of(2) {
        let half = (s.len() - 2) / 2;
        if s.is_char_boundary(half) && s.is_char_boundary(half + 2) && s[..half] == s[half + 2..] && &s[half..half + 2] == ": " {
            return s[..half].to_string();
        }
    }
    s
}

async fn scan_collection(
    c: &mut ConnectionManager,
    cmd_name: &str,
    key: &[u8],
    cursor: u64,
    limit: i64,
    filter: &str,
) -> Result<(u64, Vec<Vec<u8>>)> {
    let mut items: Vec<Vec<u8>> = Vec::new();
    let mut cur = cursor;
    let per_item = if cmd_name == "HSCAN" { 2 } else { 1 };
    let started = std::time::Instant::now();
    loop {
        let mut cmd = redis::cmd(cmd_name);
        cmd.arg(key).arg(cur);
        if !filter.is_empty() {
            cmd.arg("MATCH").arg(filter);
        }
        cmd.arg("COUNT").arg(limit.max(100));
        let (next, batch): (u64, Vec<Vec<u8>>) = cmd.query_async(c).await?;
        items.extend(batch);
        cur = next;
        if cur == 0 || items.len() as i64 >= limit * per_item || started.elapsed() > Duration::from_millis(1200) {
            break;
        }
    }
    Ok((cur, items))
}

fn score(s: &str) -> Result<String> {
    let t = s.trim();
    match t.to_ascii_lowercase().as_str() {
        "inf" | "+inf" | "-inf" => Ok(t.to_string()),
        _ => {
            t.parse::<f64>().map_err(|_| anyhow!("Score must be a number"))?;
            Ok(t.to_string())
        }
    }
}

fn stream_fields(args: &Map<String, Value>) -> Result<Vec<(String, String)>> {
    let Some(Value::Array(pairs)) = args.get("fields") else { bail!("Missing \"fields\"") };
    let out: Vec<(String, String)> = pairs
        .iter()
        .filter_map(|p| {
            let a = p.as_array()?;
            let f = a.first()?.as_str()?.to_string();
            let v = a.get(1).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
            Some((f, v))
        })
        .filter(|(f, _)| !f.is_empty())
        .collect();
    if out.is_empty() {
        bail!("A stream entry needs at least one field");
    }
    Ok(out)
}

fn key_text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn value_text(v: Option<&Vec<u8>>) -> String {
    v.map(|b| key_text(b)).unwrap_or_default()
}

/// UTF-8 text, or `\xHH` escapes (as redis-cli shows) when the value is binary.
fn bytes_text(b: &[u8]) -> (String, bool) {
    match std::str::from_utf8(b) {
        Ok(s) => (s.to_string(), false),
        Err(_) => {
            let mut out = String::with_capacity(b.len() * 2);
            for &c in b {
                if c.is_ascii_graphic() || c == b' ' {
                    if c == b'\\' {
                        out.push_str("\\\\");
                    } else {
                        out.push(c as char);
                    }
                } else {
                    out.push_str(&format!("\\x{c:02x}"));
                }
            }
            (out, true)
        }
    }
}

fn reply_text(v: &RV) -> String {
    match v {
        RV::BulkString(b) => key_text(b),
        RV::SimpleString(s) => s.clone(),
        RV::Okay => "OK".into(),
        RV::Int(n) => n.to_string(),
        RV::Double(d) => d.to_string(),
        RV::Boolean(b) => b.to_string(),
        RV::Nil => String::new(),
        other => reply_json(other).to_string(),
    }
}

/// A reply as plain JSON (maps become objects when keys are strings).
fn reply_json(v: &RV) -> Value {
    match v {
        RV::Nil => Value::Null,
        RV::Int(n) => json!(n),
        RV::BulkString(b) => json!(bytes_text(b).0),
        RV::SimpleString(s) => json!(s),
        RV::Okay => json!("OK"),
        RV::Double(d) => json!(d),
        RV::Boolean(b) => json!(b),
        RV::Array(a) | RV::Set(a) => {
            // RESP2 replies of INFO-like commands come as [k, v, k, v]; keep them as arrays.
            Value::Array(a.iter().map(reply_json).collect())
        }
        RV::Map(m) => {
            let mut o = Map::new();
            for (k, v) in m {
                o.insert(reply_text(k), reply_json(v));
            }
            Value::Object(o)
        }
        RV::ServerError(e) => json!(format!("(error) {e:?}")),
        other => json!(format!("{other:?}")),
    }
}

/// A reply with its RESP type, so the console can print it like redis-cli:
/// `{"t": "string"|"status"|"int"|"double"|"bool"|"nil"|"error"|"array"|"map"|"set", "v": …}`.
pub fn reply_json_typed(v: &RV) -> Value {
    match v {
        RV::Nil => json!({ "t": "nil" }),
        RV::Int(n) => json!({ "t": "int", "v": n }),
        RV::BulkString(b) => {
            let (s, bin) = bytes_text(b);
            json!({ "t": "string", "v": s, "binary": bin })
        }
        RV::SimpleString(s) => json!({ "t": "status", "v": s }),
        RV::Okay => json!({ "t": "status", "v": "OK" }),
        RV::Double(d) => json!({ "t": "double", "v": d }),
        RV::Boolean(b) => json!({ "t": "bool", "v": b }),
        RV::Array(a) => json!({ "t": "array", "v": a.iter().map(reply_json_typed).collect::<Vec<_>>() }),
        RV::Set(a) => json!({ "t": "set", "v": a.iter().map(reply_json_typed).collect::<Vec<_>>() }),
        RV::Map(m) => json!({ "t": "map", "v": m.iter().map(|(k, v)| json!([reply_json_typed(k), reply_json_typed(v)])).collect::<Vec<_>>() }),
        RV::ServerError(e) => json!({ "t": "error", "v": format!("{e}") }),
        other => json!({ "t": "string", "v": format!("{other:?}") }),
    }
}

/// Splits a console line into arguments like redis-cli: spaces separate,
/// "double quotes" support \n \r \t \" \\ \xHH escapes, 'single quotes' are literal.
pub fn split_command(line: &str) -> Result<Vec<Vec<u8>>> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let mut arg = Vec::new();
        match b[i] {
            b'"' => {
                i += 1;
                loop {
                    if i >= b.len() {
                        bail!("Unbalanced quotes");
                    }
                    match b[i] {
                        b'"' => {
                            i += 1;
                            break;
                        }
                        b'\\' if i + 1 < b.len() => {
                            i += 1;
                            match b[i] {
                                b'n' => arg.push(b'\n'),
                                b'r' => arg.push(b'\r'),
                                b't' => arg.push(b'\t'),
                                b'b' => arg.push(8),
                                b'a' => arg.push(7),
                                b'x' if i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() => {
                                    arg.push(u8::from_str_radix(&line[i + 1..i + 3], 16)?);
                                    i += 2;
                                }
                                c => arg.push(c),
                            }
                            i += 1;
                        }
                        c => {
                            arg.push(c);
                            i += 1;
                        }
                    }
                }
            }
            b'\'' => {
                i += 1;
                loop {
                    if i >= b.len() {
                        bail!("Unbalanced quotes");
                    }
                    if b[i] == b'\\' && i + 1 < b.len() && b[i + 1] == b'\'' {
                        arg.push(b'\'');
                        i += 2;
                        continue;
                    }
                    if b[i] == b'\'' {
                        i += 1;
                        break;
                    }
                    arg.push(b[i]);
                    i += 1;
                }
            }
            _ => {
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    arg.push(b[i]);
                    i += 1;
                }
            }
        }
        if i < b.len() && !b[i].is_ascii_whitespace() {
            bail!("A closing quote must be followed by a space");
        }
        out.push(arg);
    }
    Ok(out)
}

/// True for commands that do not change data (used in read-only mode).
fn is_read_command(args: &[Vec<u8>]) -> bool {
    const READ: &[&str] = &[
        "GET", "MGET", "GETRANGE", "STRLEN", "EXISTS", "TYPE", "TTL", "PTTL", "EXPIRETIME", "PEXPIRETIME", "KEYS", "SCAN",
        "RANDOMKEY", "DBSIZE", "INFO", "PING", "ECHO", "TIME", "LASTSAVE", "DUMP", "OBJECT", "MEMORY", "HGET", "HMGET",
        "HGETALL", "HKEYS", "HVALS", "HLEN", "HEXISTS", "HSTRLEN", "HSCAN", "HRANDFIELD", "LRANGE", "LINDEX", "LLEN", "LPOS",
        "SMEMBERS", "SISMEMBER", "SMISMEMBER", "SCARD", "SSCAN", "SRANDMEMBER", "SINTER", "SUNION", "SDIFF", "SINTERCARD",
        "ZRANGE", "ZRANGEBYSCORE", "ZREVRANGEBYSCORE", "ZRANGEBYLEX", "ZREVRANGEBYLEX", "ZREVRANGE", "ZSCORE", "ZMSCORE",
        "ZRANK", "ZREVRANK", "ZCARD", "ZCOUNT", "ZLEXCOUNT", "ZSCAN", "ZRANDMEMBER", "ZINTER", "ZUNION", "ZDIFF", "XRANGE",
        "XREVRANGE", "XLEN", "XINFO", "XPENDING", "XREAD", "GETBIT", "BITCOUNT", "BITPOS", "PFCOUNT", "GEOPOS", "GEODIST",
        "GEOHASH", "GEOSEARCH", "GEORADIUS_RO", "GEORADIUSBYMEMBER_RO", "JSON.GET", "JSON.MGET", "JSON.TYPE", "JSON.STRLEN",
        "JSON.ARRLEN", "JSON.OBJKEYS", "JSON.OBJLEN", "JSON.RESP", "FT.SEARCH", "FT.AGGREGATE", "FT.INFO", "FT._LIST",
        "TS.GET", "TS.MGET", "TS.RANGE", "TS.REVRANGE", "TS.MRANGE", "TS.INFO", "COMMAND", "LOLWUT", "ROLE", "SLOWLOG",
        "LATENCY", "MODULE", "SORT_RO", "EVAL_RO", "EVALSHA_RO", "FCALL_RO", "LCS", "SUBSTR", "TOUCH",
    ];
    let name = String::from_utf8_lossy(&args[0]).to_ascii_uppercase();
    if !READ.contains(&name.as_str()) {
        return false;
    }
    // Subcommands that change state.
    let sub = args.get(1).map(|a| String::from_utf8_lossy(a).to_ascii_uppercase()).unwrap_or_default();
    !matches!((name.as_str(), sub.as_str()), ("SLOWLOG", "RESET") | ("LATENCY", "RESET") | ("MODULE", "LOAD" | "UNLOAD" | "LOADEX") | ("MEMORY", "PURGE"))
}

// ---------------------------------------------------------------- requests

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    #[serde(default)]
    pub db: i64,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub cursor: String,
    #[serde(default = "default_count")]
    pub count: i64,
    #[serde(default, rename = "type")]
    pub key_type: Option<String>,
    /// A literal key to check with EXISTS on the first page (the filter text without globs).
    #[serde(default)]
    pub exact: Option<String>,
}

fn default_count() -> i64 {
    200
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRequest {
    #[serde(default)]
    pub db: i64,
    pub key: String,
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_page")]
    pub limit: i64,
    #[serde(default)]
    pub cursor: String,
    #[serde(default)]
    pub filter: String,
    #[serde(default)]
    pub desc: bool,
}

fn default_page() -> i64 {
    200
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    #[serde(default)]
    pub db: i64,
    pub key: String,
    pub op: String,
    #[serde(default)]
    pub args: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    #[serde(default)]
    pub db: i64,
    pub command: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[Vec<u8>]) -> Vec<String> {
        v.iter().map(|a| String::from_utf8_lossy(a).into_owned()).collect()
    }

    #[test]
    fn splits_like_redis_cli() {
        assert_eq!(s(&split_command(r#"SET "a b" 'c d'"#).unwrap()), ["SET", "a b", "c d"]);
        assert_eq!(split_command(r#"SET k "\x00\n""#).unwrap()[2], vec![0u8, b'\n']);
        assert_eq!(s(&split_command("  get   key  ").unwrap()), ["get", "key"]);
        assert!(split_command(r#"SET "unclosed"#).is_err());
        assert!(split_command(r#"SET "a"b"#).is_err());
        assert_eq!(s(&split_command(r#"SET k 'it\'s'"#).unwrap()), ["SET", "k", "it's"]);
    }

    #[test]
    fn read_only_commands() {
        let c = |l: &str| is_read_command(&split_command(l).unwrap());
        assert!(c("get a"));
        assert!(c("HGETALL h"));
        assert!(!c("set a 1"));
        assert!(!c("FLUSHALL"));
        assert!(!c("slowlog reset"));
        assert!(c("slowlog get 10"));
    }

    #[test]
    fn binary_values_are_escaped() {
        assert_eq!(bytes_text(b"ok"), ("ok".to_string(), false));
        assert_eq!(bytes_text(&[0xff, b'a', b'\\']), ("\\xffa\\\\".to_string(), true));
    }
}
