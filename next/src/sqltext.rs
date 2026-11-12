//! SQL text helpers: statement splitting (quotes, comments, $tags$), the statement at the
//! cursor, DDL detection and a suggested name for a saved query.

use std::ops::Range;

/// Byte ranges of the statements (without the ";"), skipping empty ones.
pub fn split(sql: &str) -> Vec<Range<usize>> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'\'' | b'"' | b'`' => {
                i += 1;
                while i < b.len() {
                    if b[i] == c {
                        if i + 1 < b.len() && b[i + 1] == c {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    if b[i] == b'\\' && c == b'\'' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'#' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            b'$' => {
                // $tag$ ... $tag$ (PostgreSQL)
                if let Some(end) = sql[i + 1..].find('$') {
                    let tag = &sql[i..i + 2 + end];
                    if tag[1..tag.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '_') {
                        if let Some(close) = sql[i + tag.len()..].find(tag) {
                            i += tag.len() + close + tag.len();
                            continue;
                        }
                    }
                }
            }
            b';' => {
                push(sql, start..i, &mut out);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    push(sql, start..sql.len(), &mut out);
    out
}

fn push(sql: &str, r: Range<usize>, out: &mut Vec<Range<usize>>) {
    let r = r.start.min(sql.len())..r.end.min(sql.len());
    let text = &sql[r.clone()];
    if strip_comments(text).trim().is_empty() {
        return;
    }
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    out.push(r.start + lead..r.end - trail);
}

/// Text without -- and /* */ comments (used to detect empty statements).
pub fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.char_indices().peekable();
    let b = s.as_bytes();
    while let Some((i, c)) = it.next() {
        if c == '-' && b.get(i + 1) == Some(&b'-') {
            for (_, c) in it.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            out.push('\n');
        } else if c == '/' && b.get(i + 1) == Some(&b'*') {
            it.next();
            let mut prev = ' ';
            for (_, c) in it.by_ref() {
                if prev == '*' && c == '/' {
                    break;
                }
                prev = c;
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The statement under the cursor, or the nearest one above it.
pub fn statement_at(sql: &str, cursor: usize) -> Option<Range<usize>> {
    let parts = split(sql);
    parts.iter().find(|r| cursor >= r.start && cursor <= r.end + 1).cloned().or_else(|| parts.iter().rev().find(|r| r.end <= cursor).cloned()).or_else(|| parts.first().cloned())
}

pub fn is_ddl(sql: &str) -> bool {
    let s = strip_comments(sql).to_ascii_lowercase();
    s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').any(|w| matches!(w, "create" | "drop" | "alter" | "rename"))
}

/// The first non-comment line, cut to 60 characters.
pub fn suggest_name(sql: &str) -> String {
    let line = strip_comments(sql).lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Query").to_string();
    line.chars().take(60).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_statements() {
        let sql = "select 1; select ';' as x -- c;\n; /* ; */ select $a$ ; $a$;";
        let parts: Vec<&str> = split(sql).into_iter().map(|r| &sql[r]).collect();
        assert_eq!(parts, vec!["select 1", "select ';' as x -- c;", "/* ; */ select $a$ ; $a$"]);
    }

    #[test]
    fn finds_statement_at_cursor() {
        let sql = "select 1;\n\nselect 2;";
        assert_eq!(&sql[statement_at(sql, 0).unwrap()], "select 1");
        assert_eq!(&sql[statement_at(sql, 10).unwrap()], "select 1");
        assert_eq!(&sql[statement_at(sql, 13).unwrap()], "select 2");
    }

    #[test]
    fn detects_ddl() {
        assert!(is_ddl("CREATE TABLE t (id int)"));
        assert!(!is_ddl("select created from t -- drop"));
    }
}
