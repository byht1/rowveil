//! SQL highlighting for read-only SQL text (schema definitions, DDL), the same token classes
//! as the stable UI's sqlTokens(): comment, string, quoted identifier, number, parameter,
//! operator, punctuation, constant, type, function, keyword.

use crate::ui::Pal;
use gpui_kit::*;
use std::ops::Range;

const KEYWORDS: &str = "select from where and or not in is null like ilike between as on join left right inner outer full cross group by order having limit offset \
insert into values update set delete create table view index drop alter add column primary key foreign references unique default distinct union all \
except intersect case when then else end returning with recursive exists asc desc nulls first last true false begin commit rollback transaction \
explain analyze show pragma truncate cascade restrict if replace using natural lateral window over partition rows range count sum avg min max coalesce cast \
conflict do nothing constraint check schema database grant revoke to of only similar any some array filter within materialized refresh concurrently \
vacuum auto_increment engine charset collate unsigned temporary temp trigger function procedure returns language declare return each row for execute \
comment rename owner type extension lock share nowait skip locked fetch next generated always identity stored virtual without zone at local interval \
describe use ignore duplicate preceding following unbounded current groups exclude ties sequence start increment savepoint release isolation level \
read write committed serializable deferrable initially deferred immediate match partial simple action no enable disable after before instead";

const TYPES: &str = "int integer smallint bigint serial bigserial smallserial real double precision float numeric decimal money boolean bool char \
character varchar varying nchar nvarchar text citext bytea blob clob date time timestamp timestamptz timetz interval uuid json jsonb xml inet cidr macaddr \
point line polygon tinyint mediumint tinytext mediumtext longtext tinyblob mediumblob longblob varbinary binary datetime year enum bit varbit tsvector \
tsquery oid regclass int2 int4 int8 float4 float8 hstore geometry geography";

const PARAM_TYPES: &str = "varchar char character numeric decimal float bit varbit varbinary binary nvarchar nchar timestamp time interval enum int tinyint smallint mediumint bigint double";
const CONSTANTS: &str = "null true false unknown current_date current_time current_timestamp localtime localtimestamp current_user session_user";
const KW_PAREN: &str = "in exists values using over filter within any some all array as on not and or into table check primary key unique references foreign conflict when then else";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tok {
    Com,
    Str,
    Id,
    Num,
    Param,
    Op,
    Punct,
    Const,
    Type,
    Fn,
    Kw,
}

fn has(list: &str, w: &str) -> bool {
    list.split_whitespace().any(|x| x == w)
}

/// Token ranges (byte offsets) with their class; plain identifiers and spaces are left out.
pub fn tokens(src: &str, mysql: bool) -> Vec<(Range<usize>, Tok)> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    let word_start = |c: u8| c.is_ascii_alphabetic() || c == b'_';
    let word_char = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
    while i < n {
        let c = b[i];
        let start = i;
        // comments
        if c == b'-' && b.get(i + 1) == Some(&b'-') || (mysql && c == b'#') {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            out.push((start..i, Tok::Com));
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < n && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(n);
            out.push((start..i, Tok::Com));
            continue;
        }
        // dollar quoting: $tag$ … $tag$
        if c == b'$' && !mysql {
            let mut j = i + 1;
            while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_') && !b[i + 1].is_ascii_digit() {
                j += 1;
            }
            if j < n && b[j] == b'$' {
                let tag = &src[i..=j];
                let body = j + 1;
                let end = src[body..].find(tag).map(|e| body + e + tag.len()).unwrap_or(n);
                out.push((start..end, Tok::Str));
                i = end;
                continue;
            }
            if b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                i += 1;
                while i < n && b[i].is_ascii_digit() {
                    i += 1;
                }
                out.push((start..i, Tok::Param));
                continue;
            }
        }
        // strings: [EeNnBbXx]'…'
        let prefixed = matches!(c, b'E' | b'e' | b'N' | b'n' | b'B' | b'b' | b'X' | b'x') && b.get(i + 1) == Some(&b'\'');
        if c == b'\'' || prefixed || (mysql && c == b'"') {
            let q = if prefixed { b'\'' } else { c };
            let escapes = mysql || (prefixed && (c == b'E' || c == b'e'));
            i += if prefixed { 2 } else { 1 };
            while i < n {
                if escapes && b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == q {
                    if b.get(i + 1) == Some(&q) {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push((start..i.min(n), Tok::Str));
            continue;
        }
        // quoted identifiers
        if (c == b'"' && !mysql) || c == b'`' {
            i += 1;
            while i < n {
                if b[i] == c {
                    if b.get(i + 1) == Some(&c) {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push((start..i.min(n), Tok::Id));
            continue;
        }
        // numbers
        let prev_word = i > 0 && word_char(b[i - 1]);
        if !prev_word && (c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit()))) {
            if c == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X')) {
                i += 2;
                while i < n && b[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else {
                while i < n && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                if i < n && (b[i] == b'e' || b[i] == b'E') && b.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == b'+' || *d == b'-') {
                    i += 2;
                    while i < n && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            if i < n && word_char(b[i]) {
                // part of an identifier such as 1abc: skip it as a word
                while i < n && word_char(b[i]) {
                    i += 1;
                }
                continue;
            }
            out.push((start..i, Tok::Num));
            continue;
        }
        // parameters: ?, :name, @var
        if c == b'?' && !matches!(b.get(i + 1), Some(b'|' | b'&')) {
            out.push((i..i + 1, Tok::Param));
            i += 1;
            continue;
        }
        if c == b':' && b.get(i + 1).is_some_and(|d| word_start(*d)) && (i == 0 || b[i - 1] != b':') {
            i += 1;
            while i < n && word_char(b[i]) {
                i += 1;
            }
            out.push((start..i, Tok::Param));
            continue;
        }
        if c == b'@' && b.get(i + 1).is_some_and(|d| word_start(*d) || *d == b'@') {
            i += 1;
            while i < n && (word_char(b[i]) || b[i] == b'.' || b[i] == b'@') {
                i += 1;
            }
            out.push((start..i, Tok::Param));
            continue;
        }
        // words
        if word_start(c) {
            while i < n && word_char(b[i]) {
                i += 1;
            }
            let w = src[start..i].to_ascii_lowercase();
            let prev = src[..start].trim_end();
            let after_dot = prev.ends_with('.') && !prev.ends_with("..");
            let call_next = src[i..].trim_start().starts_with('(');
            let prev_lower = prev.to_ascii_lowercase();
            let after_obj_kw = ["into", "table", "references", "from", "join", "update", "view", "index", "on"].iter().any(|k| prev_lower.ends_with(k) && prev_lower[..prev_lower.len() - k.len()].chars().last().is_none_or(|ch| !ch.is_alphanumeric() && ch != '_'));
            let tok = if after_dot {
                None
            } else if has(CONSTANTS, &w) {
                Some(Tok::Const)
            } else if has(TYPES, &w) && (!call_next || has(PARAM_TYPES, &w)) {
                Some(Tok::Type)
            } else if call_next && !has(KW_PAREN, &w) && !after_obj_kw {
                Some(Tok::Fn)
            } else if has(KEYWORDS, &w) {
                Some(Tok::Kw)
            } else {
                None
            };
            if let Some(t) = tok {
                out.push((start..i, t));
            }
            continue;
        }
        // operators
        let ops3 = ["->>", "#>>"];
        let ops2 = ["::", "->", "#>", "@>", "<@", "?|", "?&", "<=", ">=", "<>", "!=", "||", "&&"];
        if let Some(op) = ops3.iter().chain(ops2.iter()).find(|o| src[i..].starts_with(**o)) {
            i += op.len();
            out.push((start..i, Tok::Op));
            continue;
        }
        if b"=<>+-*/%~^!|&#@".contains(&c) {
            i += 1;
            out.push((start..i, Tok::Op));
            continue;
        }
        if b"(),;.[]{}".contains(&c) {
            i += 1;
            out.push((start..i, Tok::Punct));
            continue;
        }
        // any other character (UTF-8 aware)
        i += src[i..].chars().next().map(char::len_utf8).unwrap_or(1);
    }
    out
}

/// The color of a token class in the palette (op and punct use the muted tones of the stable UI).
pub fn color(t: Tok, p: &Pal) -> Hsla {
    match t {
        Tok::Com => p.t_com,
        Tok::Str => p.t_str,
        Tok::Id => p.t_id,
        Tok::Num => p.t_num,
        Tok::Param => p.danger,
        Tok::Op => p.muted,
        Tok::Punct => p.dim,
        Tok::Const => p.t_const,
        Tok::Type => p.t_type,
        Tok::Fn => p.t_fn,
        Tok::Kw => p.t_kw,
    }
}

/// Highlighted SQL text.
pub fn styled(src: &str, mysql: bool, p: &Pal) -> StyledText {
    let hl: Vec<(Range<usize>, HighlightStyle)> = tokens(src, mysql)
        .into_iter()
        .map(|(r, t)| (r, HighlightStyle { color: Some(color(t, p)), font_style: (t == Tok::Com).then_some(FontStyle::Italic), ..Default::default() }))
        .collect();
    StyledText::new(src.to_string()).with_highlights(hl)
}

#[cfg(test)]
mod tests {
    use super::{tokens, Tok};

    fn kinds(s: &str) -> Vec<(String, Tok)> {
        tokens(s, false).into_iter().map(|(r, t)| (s[r].to_string(), t)).collect()
    }

    #[test]
    fn classifies_a_column_definition() {
        let k = kinds(r#""id" integer PRIMARY KEY DEFAULT nextval('users_id_seq'::regclass)"#);
        assert_eq!(k[0], ("\"id\"".into(), Tok::Id));
        assert_eq!(k[1], ("integer".into(), Tok::Type));
        assert_eq!(k[2], ("PRIMARY".into(), Tok::Kw));
        assert!(k.contains(&("nextval".into(), Tok::Fn)));
        assert!(k.contains(&("'users_id_seq'".into(), Tok::Str)));
        assert!(k.contains(&("::".into(), Tok::Op)));
        assert!(k.contains(&("regclass".into(), Tok::Type)));
    }

    #[test]
    fn types_with_length_and_constants() {
        let k = kinds("varchar(255) NOT NULL DEFAULT 0 -- note");
        assert_eq!(k[0], ("varchar".into(), Tok::Type));
        assert!(k.contains(&("255".into(), Tok::Num)));
        assert!(k.contains(&("NULL".into(), Tok::Const)));
        assert!(k.contains(&("-- note".into(), Tok::Com)));
    }

    #[test]
    fn references_table_is_not_a_function() {
        let k = kinds("REFERENCES organizations(id)");
        assert!(!k.iter().any(|(w, t)| w == "organizations" && *t == Tok::Fn));
    }
}
