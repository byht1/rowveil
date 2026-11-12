//! Connection form fields <-> connection URL (the same rules as the stable UI).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DbType {
    pub id: &'static str,
    pub label: &'static str,
    pub port: &'static str,
    pub user: &'static str,
    /// Query key for SSL and its values for (disable, require).
    pub ssl: Option<(&'static str, &'static str, &'static str)>,
}

pub const DB_TYPES: &[DbType] = &[
    DbType { id: "postgres", label: "PostgreSQL", port: "5432", user: "postgres", ssl: Some(("sslmode", "disable", "require")) },
    DbType { id: "mysql", label: "MySQL", port: "3306", user: "root", ssl: Some(("ssl-mode", "disabled", "required")) },
    DbType { id: "mariadb", label: "MariaDB", port: "3306", user: "root", ssl: Some(("ssl-mode", "disabled", "required")) },
    DbType { id: "sqlite", label: "SQLite", port: "", user: "", ssl: None },
    DbType { id: "redis", label: "Redis", port: "6379", user: "default", ssl: None },
    DbType { id: "bigquery", label: "BigQuery", port: "", user: "", ssl: None },
];

pub fn db_type(id: &str) -> DbType {
    DB_TYPES.iter().copied().find(|t| t.id == id).unwrap_or(DB_TYPES[0])
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fields {
    pub kind: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
    pub database: String,
    /// "default", "disable" or "require".
    pub ssl: String,
    pub options: String,
    pub file: String,
    pub project: String,
    pub dataset: String,
    pub key_file: String,
    pub location: String,
    pub max_bytes: String,
    pub endpoint: String,
}

/// encodeURIComponent
pub fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// decodeURIComponent; returns the input when it is not valid.
pub fn dec(s: &str) -> String {
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
            return s.to_string();
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

pub fn build(f: &Fields, mask_password: bool) -> String {
    let t = db_type(&f.kind);
    if t.id == "bigquery" {
        let project = f.project.trim();
        if project.is_empty() {
            return String::new();
        }
        let mut q = Vec::new();
        for (k, v) in [("credentials", &f.key_file), ("location", &f.location), ("maximumBytesBilled", &f.max_bytes), ("endpoint", &f.endpoint)] {
            if !v.trim().is_empty() {
                q.push(format!("{k}={}", enc(v.trim())));
            }
        }
        let ds = f.dataset.trim();
        return format!(
            "bigquery://{}{}{}",
            enc(project),
            if ds.is_empty() { String::new() } else { format!("/{}", enc(ds)) },
            if q.is_empty() { String::new() } else { format!("?{}", q.join("&")) }
        );
    }
    if t.id == "sqlite" {
        let p = f.file.trim().replace('\\', "/").replace('%', "%25").replace('?', "%3F").replace('#', "%23");
        return if p.is_empty() { String::new() } else { format!("sqlite://{p}") };
    }
    let host = f.host.trim();
    if host.is_empty() {
        return String::new();
    }
    let mut auth = String::new();
    if !f.user.is_empty() || (t.id == "redis" && !f.password.is_empty()) {
        auth = enc(&f.user);
        if !f.password.is_empty() {
            auth.push(':');
            auth.push_str(&if mask_password { "••••".to_string() } else { enc(&f.password) });
        }
        auth.push('@');
    }
    let host_part = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_string() };
    let port = f.port.trim();
    let port = if port.is_empty() { String::new() } else { format!(":{port}") };
    let mut params = Vec::new();
    if let Some((key, dis, req)) = t.ssl {
        match f.ssl.as_str() {
            "disable" => params.push(format!("{key}={dis}")),
            "require" => params.push(format!("{key}={req}")),
            _ => {}
        }
    }
    let extra = f.options.trim().trim_start_matches('?');
    if !extra.is_empty() {
        params.push(extra.to_string());
    }
    let query = if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) };
    if t.id == "redis" {
        let scheme = if f.ssl == "require" { "rediss" } else { "redis" };
        let db = f.database.trim();
        let db = if db.is_empty() { String::new() } else { format!("/{}", enc(db)) };
        return format!("{scheme}://{auth}{host_part}{port}{db}{query}");
    }
    format!("{}://{auth}{host_part}{port}/{}{query}", t.id, enc(f.database.trim()))
}

pub fn parse(raw: &str) -> Fields {
    let url = raw.trim();
    let mut f = Fields { kind: "postgres".into(), ssl: "default".into(), ..Default::default() };
    let scheme_end = url.find(':').filter(|&i| {
        let s = &url[..i];
        !s.is_empty() && s.chars().next().unwrap().is_ascii_alphabetic() && s.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)) && i > 1
    });
    let Some(se) = scheme_end else {
        if !url.is_empty() {
            f.kind = "sqlite".into();
            f.file = url.to_string();
        }
        return f;
    };
    let scheme = url[..se].to_ascii_lowercase();
    let mut rest = url[se + 1..].trim_start_matches("//").to_string();
    if scheme == "sqlite" || scheme == "file" {
        f.kind = "sqlite".into();
        let p = rest.split('?').next().unwrap_or("").trim_start_matches('@').to_string();
        let p = if p.len() > 2 && p.starts_with('/') && p.as_bytes()[2] == b':' { p[1..].to_string() } else { p };
        f.file = dec(&p);
        return f;
    }
    if scheme == "bigquery" || scheme == "bq" {
        f.kind = "bigquery".into();
        let (path, query) = rest.split_once('?').unwrap_or((&rest, ""));
        let mut it = path.split('/');
        f.project = dec(it.next().unwrap_or(""));
        f.dataset = dec(it.next().unwrap_or(""));
        for kv in query.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            let v = dec(v);
            match k.to_ascii_lowercase().as_str() {
                "credentials" | "keyfile" => f.key_file = v,
                "location" => f.location = v,
                "maximumbytesbilled" => f.max_bytes = v,
                "endpoint" => f.endpoint = v,
                _ => {}
            }
        }
        return f;
    }
    if ["redis", "rediss", "valkey", "valkeys"].contains(&scheme.as_str()) {
        f.kind = "redis".into();
        if scheme.ends_with('s') {
            f.ssl = "require".into();
        }
    } else {
        f.kind = if scheme == "postgresql" { "postgres".into() } else if DB_TYPES.iter().any(|t| t.id == scheme) { scheme.clone() } else { "postgres".into() };
    }
    let t = db_type(&f.kind);
    if let Some(at) = rest.rfind('@') {
        let creds = rest[..at].to_string();
        rest = rest[at + 1..].to_string();
        match creds.split_once(':') {
            Some((u, p)) => {
                f.user = dec(u);
                f.password = dec(p);
            }
            None => f.user = dec(&creds),
        }
    }
    let (main, query) = rest.split_once('?').unwrap_or((&rest, ""));
    let (host_port, db) = main.split_once('/').unwrap_or((main, ""));
    f.database = dec(db);
    if let Some(v6) = host_port.strip_prefix('[') {
        let (h, after) = v6.split_once(']').unwrap_or((v6, ""));
        f.host = h.to_string();
        f.port = after.trim_start_matches(':').to_string();
    } else {
        match host_port.rsplit_once(':') {
            Some((h, p)) => {
                f.host = h.to_string();
                f.port = p.to_string();
            }
            None => f.host = host_port.to_string(),
        }
    }
    f.host = dec(&f.host);
    let mut opts = Vec::new();
    for kv in query.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        match t.ssl {
            Some((key, dis, req)) if k.eq_ignore_ascii_case(key) && (v.eq_ignore_ascii_case(dis) || v.eq_ignore_ascii_case(req)) => {
                f.ssl = if v.eq_ignore_ascii_case(dis) { "disable".into() } else { "require".into() };
            }
            _ => opts.push(kv.to_string()),
        }
    }
    f.options = opts.join("&");
    f
}

/// Returns an error message, or None when the fields make a valid URL.
pub fn validate(f: &Fields) -> Option<&'static str> {
    match f.kind.as_str() {
        "sqlite" if f.file.trim().is_empty() => Some("Enter the database file"),
        "bigquery" if f.project.trim().is_empty() => Some("Enter the project ID"),
        "bigquery" if !f.max_bytes.trim().is_empty() && f.max_bytes.trim().parse::<u64>().is_err() => Some("Max bytes billed: enter a number"),
        "sqlite" | "bigquery" => None,
        _ if f.host.trim().is_empty() => Some("Enter the host"),
        "redis" if !f.database.trim().is_empty() && f.database.trim().parse::<u32>().is_err() => Some("Database number: enter a number"),
        _ if !f.port.trim().is_empty() && !matches!(f.port.trim().parse::<u32>(), Ok(1..=65535)) => Some("Port: enter a number from 1 to 65535"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for url in [
            "postgres://postgres:p%40ss@localhost:5432/app?sslmode=require",
            "mysql://root@db:3306/shop?ssl-mode=disabled&charset=utf8mb4",
            "rediss://default:secret@cache:6380/2",
            "bigquery://proj/ds?credentials=C%3A%2Fkey.json&location=EU",
            "sqlite://C:/data/app.db",
            "postgres://u@[::1]:5432/db",
        ] {
            assert_eq!(build(&parse(url), false), url, "{url}");
        }
    }

    #[test]
    fn parses_raw_password_and_masks() {
        let f = parse("postgres://u:p@ss@h:1/d");
        assert_eq!(f.password, "p@ss");
        assert_eq!(build(&f, true), "postgres://u:••••@h:1/d");
    }

    #[test]
    fn validates() {
        assert_eq!(validate(&Fields { kind: "postgres".into(), ..Default::default() }), Some("Enter the host"));
        assert_eq!(validate(&Fields { kind: "postgres".into(), host: "h".into(), port: "99999".into(), ..Default::default() }), Some("Port: enter a number from 1 to 65535"));
        assert_eq!(validate(&Fields { kind: "sqlite".into(), file: "a.db".into(), ..Default::default() }), None);
    }
}
