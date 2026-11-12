//! SQL autocomplete for the editor: columns of the tables the statement uses (with aliases),
//! `alias.` / `table.` / `schema.` qualification, tables, schemas, keywords and functions.

use crate::model::Schema;
use crate::sqltext;
use gpui_kit::component::input::{CompletionProvider, Rope, RopeExt as _};
use gpui_kit::*;
use lsp_types::{CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, Range as LspRange, TextEdit};
use std::collections::HashMap;
use std::rc::Rc;

const KEYWORDS: &str = "select from where and or not in is null like ilike between as on join left right inner outer full cross group by order having limit offset \
insert into values update set delete create table view index drop alter add column primary key foreign references unique default distinct union all \
except intersect case when then else end returning with recursive exists asc desc nulls first last true false begin commit rollback \
explain analyze show truncate cascade if replace using natural lateral window over partition rows range conflict do nothing constraint check schema \
grant revoke to only materialized refresh returning";

const PHRASES: &[&str] = &["order by", "group by", "left join", "right join", "inner join", "full join", "cross join", "insert into", "delete from", "primary key", "foreign key", "is not null", "is null", "not in", "not exists", "union all", "on conflict", "do nothing"];

const COMMON: &str = "count sum avg min max coalesce nullif lower upper length trim substring replace round abs cast concat greatest least row_number rank dense_rank lag lead";

fn functions(dialect: &str) -> &'static str {
    match dialect {
        "postgres" => "now date_trunc to_char to_timestamp to_date extract age json_agg jsonb_agg array_agg string_agg json_build_object jsonb_build_object jsonb_array_elements jsonb_each jsonb_set gen_random_uuid generate_series unnest array_length regexp_replace split_part position initcap left right lpad rpad md5 random ceil floor date_part current_date current_timestamp",
        "mysql" => "now curdate curtime date_format str_to_date date_add date_sub datediff timestampdiff unix_timestamp from_unixtime ifnull if group_concat json_extract json_unquote json_object json_array json_arrayagg uuid left right lpad rpad locate instr md5 rand ceil floor year month day",
        "sqlite" => "ifnull iif datetime date time strftime julianday unixepoch json json_extract json_object json_array json_group_array group_concat typeof random printf instr hex quote total ltrim rtrim",
        "bigquery" => "current_date current_timestamp current_datetime date_trunc timestamp_trunc datetime_trunc date_diff timestamp_diff date_add date_sub timestamp_add format_date format_timestamp parse_date parse_timestamp extract safe_cast safe_divide ifnull if countif count_if approx_count_distinct array_agg array_length array_to_string string_agg split unnest struct generate_array generate_date_array json_value json_query json_extract json_extract_scalar to_json_string parse_json regexp_contains regexp_extract regexp_replace starts_with ends_with farm_fingerprint md5 sha256 any_value logical_and logical_or st_geogpoint st_distance",
        _ => "",
    }
}

const TABLE_CONTEXT: &[&str] = &["from", "join", "into", "update", "table", "describe"];
const NOT_ALIAS: &[&str] = &["where", "on", "join", "left", "right", "inner", "outer", "full", "cross", "natural", "group", "order", "limit", "offset", "set", "values", "using", "having", "union", "except", "intersect", "returning", "window", "as"];

pub struct SqlCompletion {
    pub schema: Rc<Schema>,
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('`') && t.ends_with('`')) || (t.starts_with('[') && t.ends_with(']')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Splits SQL into identifier-ish tokens (quoted identifiers kept whole), lower-case words.
fn tokens(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' || c == '`' {
            let mut q = String::from(c);
            for d in chars.by_ref() {
                q.push(d);
                if d == c {
                    break;
                }
            }
            cur.push_str(&q);
        } else if c.is_alphanumeric() || c == '_' || c == '$' || c == '.' {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            if c == ',' || c == '(' || c == ')' {
                out.push(c.to_string());
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl SqlCompletion {
    /// Tables the statement references: alias (lower case) → (schema, table).
    fn referenced(&self, stmt: &str) -> HashMap<String, (String, String)> {
        let toks = tokens(stmt);
        let mut out = HashMap::new();
        let mut i = 0;
        while i < toks.len() {
            let w = toks[i].to_lowercase();
            if TABLE_CONTEXT.contains(&w.as_str()) {
                let mut j = i + 1;
                loop {
                    let Some(name) = toks.get(j) else { break };
                    if name == "(" {
                        break;
                    }
                    let parts: Vec<String> = name.split('.').map(unquote).collect();
                    let (s, t) = if parts.len() >= 2 { (parts[parts.len() - 2].clone(), parts[parts.len() - 1].clone()) } else { (String::new(), parts[0].clone()) };
                    if let Some(table) = self.find_table(&s, &t) {
                        out.insert(t.to_lowercase(), table.clone());
                        // alias: "t AS a" or "t a"
                        let mut k = j + 1;
                        if toks.get(k).is_some_and(|x| x.eq_ignore_ascii_case("as")) {
                            k += 1;
                        }
                        if let Some(alias) = toks.get(k) {
                            let al = alias.to_lowercase();
                            if !NOT_ALIAS.contains(&al.as_str()) && !TABLE_CONTEXT.contains(&al.as_str()) && al != "," && al != "(" && al != ")" {
                                out.insert(unquote(&al), table);
                                j = k;
                            }
                        }
                    }
                    // FROM a, b
                    if toks.get(j + 1).map(String::as_str) == Some(",") {
                        j += 2;
                    } else {
                        break;
                    }
                }
            }
            i += 1;
        }
        out
    }

    fn find_table(&self, schema: &str, name: &str) -> Option<(String, String)> {
        let def = &self.schema.default_schema;
        self.schema
            .tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name) && (schema.is_empty() && (t.schema == *def || t.schema.is_empty()) || t.schema.eq_ignore_ascii_case(schema)))
            .or_else(|| self.schema.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name) && schema.is_empty()))
            .map(|t| (t.schema.clone(), t.name.clone()))
    }

    fn columns(&self, schema: &str, table: &str) -> Vec<(String, String)> {
        self.schema.table(schema, table).map(|t| t.columns.iter().map(|c| (c.name.clone(), c.data_type.clone())).collect()).unwrap_or_default()
    }

    fn suggest(&self, text: &str, offset: usize) -> (usize, Vec<CompletionItem>) {
        let before = &text[..offset];
        // The word being typed, with an optional "qualifier." part.
        let start = before.rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$' || c == '.' || c == '"' || c == '`')).map(|i| i + 1).unwrap_or(0);
        let word = &before[start..];
        let (qualifier, prefix, replace_from) = match word.rfind('.') {
            Some(d) => (Some(unquote(&word[..d])), word[d + 1..].to_string(), start + d + 1),
            None => (None, word.to_string(), start),
        };
        let p = unquote(&prefix).to_lowercase();
        let stmt_range = sqltext::statement_at(text, offset).unwrap_or(0..text.len());
        let stmt = &text[stmt_range.start.min(text.len())..stmt_range.end.max(offset).min(text.len())];
        let refs = self.referenced(stmt);
        let mut items: Vec<(u8, CompletionItem)> = Vec::new();
        let mut push = |rank: u8, label: String, kind: CompletionItemKind, detail: Option<String>| {
            items.push((rank, CompletionItem { label, kind: Some(kind), detail, ..Default::default() }));
        };
        if let Some(q) = qualifier {
            let ql = q.to_lowercase();
            if let Some((s, t)) = refs.get(&ql).cloned().or_else(|| self.find_table("", &q)) {
                for (c, ty) in self.columns(&s, &t) {
                    push(0, c, CompletionItemKind::FIELD, Some(ty));
                }
            } else if self.schema.schemas.iter().any(|s| s.eq_ignore_ascii_case(&q)) {
                for t in self.schema.tables.iter().filter(|t| t.schema.eq_ignore_ascii_case(&q)) {
                    push(0, t.name.clone(), CompletionItemKind::CLASS, Some(t.kind.clone()));
                }
            }
        } else {
            // After FROM / JOIN / INTO … tables come first.
            let prev = tokens(&before[..start]).last().map(|s| s.to_lowercase()).unwrap_or_default();
            let table_ctx = TABLE_CONTEXT.contains(&prev.as_str()) || prev == ",";
            let mut seen = std::collections::HashSet::new();
            for (s, t) in refs.values() {
                if seen.insert((s.clone(), t.clone())) {
                    for (c, ty) in self.columns(s, t) {
                        push(if table_ctx { 3 } else { 0 }, c, CompletionItemKind::FIELD, Some(format!("{t} · {ty}")));
                    }
                }
            }
            let def = &self.schema.default_schema;
            for t in &self.schema.tables {
                let label = if t.schema.is_empty() || t.schema == *def || self.schema.dialect == "sqlite" { t.name.clone() } else { format!("{}.{}", t.schema, t.name) };
                push(if table_ctx { 0 } else { 2 }, label, CompletionItemKind::CLASS, Some(t.kind.clone()));
            }
            if self.schema.schemas.len() > 1 {
                for s in &self.schema.schemas {
                    push(3, s.clone(), CompletionItemKind::MODULE, Some("schema".into()));
                }
            }
            for f in COMMON.split(' ').chain(functions(&self.schema.dialect).split(' ')).filter(|f| !f.is_empty()) {
                push(3, format!("{f}()"), CompletionItemKind::FUNCTION, None);
            }
            for k in KEYWORDS.split_whitespace() {
                push(4, k.to_uppercase(), CompletionItemKind::KEYWORD, None);
            }
            for ph in PHRASES {
                push(4, ph.to_uppercase(), CompletionItemKind::KEYWORD, None);
            }
        }
        // Ranking: prefix match, then a word start ("_" or space), then a substring.
        let mut scored: Vec<(u8, u8, CompletionItem)> = items
            .into_iter()
            .filter_map(|(rank, it)| {
                let l = it.label.to_lowercase();
                let m = if p.is_empty() || l.starts_with(&p) {
                    0
                } else if l.split(['_', ' ', '.']).any(|w| w.starts_with(&p)) {
                    1
                } else if l.contains(&p) {
                    2
                } else {
                    return None;
                };
                Some((m, rank, it))
            })
            .collect();
        scored.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)).then(a.2.label.len().cmp(&b.2.label.len())));
        scored.dedup_by(|a, b| a.2.label == b.2.label);
        (replace_from, scored.into_iter().take(200).map(|x| x.2).collect())
    }
}

impl CompletionProvider for SqlCompletion {
    fn completions(&self, text: &Rope, offset: usize, _trigger: CompletionContext, _: &mut Window, _: &mut App) -> Task<anyhow::Result<CompletionResponse>> {
        let s = text.to_string();
        let offset = offset.min(s.len());
        let (from, mut items) = self.suggest(&s, offset);
        let range = LspRange { start: text.offset_to_position(from), end: text.offset_to_position(offset) };
        for it in &mut items {
            // A function goes in with "()".
            it.text_edit = Some(CompletionTextEdit::Edit(TextEdit { range, new_text: it.label.clone() }));
            it.filter_text = Some(it.label.clone());
        }
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _offset: usize, new_text: &str, _: &mut App) -> bool {
        new_text.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
    }
}

#[cfg(test)]
mod tests {
    use super::SqlCompletion;
    use crate::model::{Column, Schema, Table};
    use std::rc::Rc;

    fn schema() -> Schema {
        let col = |n: &str| Column { name: n.into(), data_type: "int".into(), ..Default::default() };
        Schema {
            dialect: "postgres".into(),
            schemas: vec!["public".into()],
            default_schema: "public".into(),
            tables: vec![
                Table { schema: "public".into(), name: "users".into(), kind: "table".into(), columns: vec![col("id"), col("email")], ..Default::default() },
                Table { schema: "public".into(), name: "orders".into(), kind: "table".into(), columns: vec![col("id"), col("user_id"), col("total")], ..Default::default() },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn suggests_columns_of_aliases() {
        let c = SqlCompletion { schema: Rc::new(schema()) };
        let sql = "select o.to from orders o";
        let (_, items) = c.suggest(sql, 10);
        assert_eq!(items.first().map(|i| i.label.as_str()), Some("total"));
        let sql = "select em from users";
        let (from, items) = c.suggest(sql, 9);
        assert_eq!(from, 7);
        assert_eq!(items.first().map(|i| i.label.as_str()), Some("email"));
        let sql = "select * from ord";
        let (_, items) = c.suggest(sql, sql.len());
        assert_eq!(items.first().map(|i| i.label.as_str()), Some("orders"));
    }
}
