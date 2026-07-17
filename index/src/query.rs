//! Operator mini-language parser.
//!
//! Splits a raw query string into structured [`Filters`] and a free-text FTS5
//! `MATCH` expression. Supported operators:
//!
//! | operator            | effect                                             |
//! |---------------------|----------------------------------------------------|
//! | `from:<who>`        | sender identifier contains `<who>`                 |
//! | `in:<chat>`         | chat name/identifier contains `<chat>` (or chat id)|
//! | `before:<date>`     | timestamp strictly before `<date>`                 |
//! | `after:<date>`      | timestamp at or after `<date>`                     |
//! | `has:photo`         | has an image/video attachment                      |
//! | `has:link`          | body contains a URL                                |
//! | `has:attachment`    | has any attachment                                 |
//! | `is:from-me`        | sent by the database owner                          |
//! | `is:from-them`      | received (not from me)                             |
//!
//! Everything else is free text. Double-quoted runs are preserved as phrases,
//! both as operator values (`from:"John Doe"`) and as FTS phrases (`"exact
//! phrase"`). A bare `word` is matched as an FTS token; `word1 word2` ANDs them.
//! Tokens that merely *contain* a colon (e.g. a pasted `https://…` URL) are
//! treated as free text — only the known operator keys above are special.

use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Structured filters extracted from a query, applied as SQL `WHERE` clauses
/// alongside the FTS match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// `from:` values; sender must contain each (case-insensitive).
    pub from: Vec<String>,
    /// `in:` values; chat name/identifier/id must match each.
    pub in_chat: Vec<String>,
    /// `before:` — exclusive upper bound, unix epoch millis.
    pub before: Option<i64>,
    /// `after:` — inclusive lower bound, unix epoch millis.
    pub after: Option<i64>,
    /// `has:attachment`.
    pub has_attachment: bool,
    /// `has:photo`.
    pub has_photo: bool,
    /// `has:link`.
    pub has_link: bool,
    /// `is:from-me` (`Some(true)`) / `is:from-them` (`Some(false)`).
    pub is_from_me: Option<bool>,
}

impl Filters {
    /// Whether any filter is active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.from.is_empty()
            && self.in_chat.is_empty()
            && self.before.is_none()
            && self.after.is_none()
            && !self.has_attachment
            && !self.has_photo
            && !self.has_link
            && self.is_from_me.is_none()
    }
}

/// A parsed query: an optional FTS `MATCH` expression plus structured filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedQuery {
    /// FTS5 `MATCH` expression, or `None` when the query is filters-only.
    pub fts: Option<String>,
    /// Structured filters.
    pub filters: Filters,
    /// The original raw query, retained for display/debugging.
    pub raw: String,
}

/// Parse a raw query string into [`ParsedQuery`].
#[must_use]
pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut filters = Filters::default();
    let mut fts_parts: Vec<String> = Vec::new();

    for token in tokenize(raw) {
        match split_operator(&token) {
            Some((key, value)) => apply_operator(&mut filters, &key, &value, &mut fts_parts),
            None => {
                if let Some(part) = fts_term(&token) {
                    fts_parts.push(part);
                }
            }
        }
    }

    let fts = if fts_parts.is_empty() {
        None
    } else {
        Some(fts_parts.join(" "))
    };

    ParsedQuery {
        fts,
        filters,
        raw: raw.to_string(),
    }
}

/// The recognized operator keys.
const OPERATOR_KEYS: &[&str] = &["from", "in", "before", "after", "has", "is"];

/// Apply a recognized operator to the filter set. Unknown `has:`/`is:` values
/// fall back to free text so a typo never silently drops the term.
fn apply_operator(filters: &mut Filters, key: &str, value: &str, fts_parts: &mut Vec<String>) {
    match key {
        "from" => filters.from.push(value.to_string()),
        "in" => filters.in_chat.push(value.to_string()),
        "before" => filters.before = parse_date_boundary(value),
        "after" => filters.after = parse_date_boundary(value),
        "has" => match value.to_ascii_lowercase().as_str() {
            "photo" | "image" | "video" => filters.has_photo = true,
            "link" | "url" => filters.has_link = true,
            "attachment" | "file" => filters.has_attachment = true,
            _ => {
                if let Some(part) = fts_term(value) {
                    fts_parts.push(part);
                }
            }
        },
        "is" => match value.to_ascii_lowercase().as_str() {
            "from-me" | "fromme" | "sent" => filters.is_from_me = Some(true),
            "from-them" | "fromthem" | "received" => filters.is_from_me = Some(false),
            _ => {
                if let Some(part) = fts_term(value) {
                    fts_parts.push(part);
                }
            }
        },
        _ => {}
    }
}

/// Split a token into `(key, value)` when it begins with a known operator key
/// followed by `:`. Returns `None` for anything else (including colon-bearing
/// free text like URLs).
fn split_operator(token: &str) -> Option<(String, String)> {
    let colon = token.find(':')?;
    if colon == 0 {
        return None;
    }
    let key = token[..colon].to_ascii_lowercase();
    if !OPERATOR_KEYS.contains(&key.as_str()) {
        return None;
    }
    let value = strip_quotes(&token[colon + 1..]);
    if value.is_empty() {
        return None;
    }
    Some((key, value))
}

/// Turn a free-text token into an FTS5 phrase, or `None` if it has no indexable
/// content. Bare words are wrapped in quotes so punctuation can never inject FTS
/// syntax; already-quoted phrases are re-emitted as a clean phrase.
fn fts_term(token: &str) -> Option<String> {
    let inner = strip_quotes(token);
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Escape embedded double quotes per FTS5 (double them), then wrap.
    let escaped = trimmed.replace('"', "\"\"");
    Some(format!("\"{escaped}\""))
}

/// Remove one layer of surrounding double quotes, if present.
fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Whitespace-split a query while keeping double-quoted runs (including any
/// `key:"value"` form) intact as single tokens.
fn tokenize(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in raw.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Parse a date/datetime boundary into unix epoch millis (UTC). Accepts
/// RFC3339, `YYYY-MM-DD[THH:MM[:SS]]` (space or `T` separator), and bare
/// `YYYY-MM-DD` (midnight UTC).
#[must_use]
pub fn parse_date_boundary(value: &str) -> Option<i64> {
    let v = value.trim();

    // RFC3339 (with timezone), e.g. 2023-01-02T03:04:05Z / +05:00.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(v) {
        return Some(dt.with_timezone(&Utc).timestamp_millis());
    }

    // Naive datetime forms, interpreted as UTC.
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(v, fmt) {
            return Some(Utc.from_utc_datetime(&naive).timestamp_millis());
        }
    }

    // Bare date -> midnight UTC.
    if let Ok(date) = NaiveDate::parse_from_str(v, "%Y-%m-%d") {
        let naive = date.and_hms_opt(0, 0, 0)?;
        return Some(Utc.from_utc_datetime(&naive).timestamp_millis());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(date: &str) -> i64 {
        parse_date_boundary(date).unwrap()
    }

    #[test]
    fn plain_free_text_becomes_anded_phrases() {
        let q = parse_query("hello world");
        assert_eq!(q.fts.as_deref(), Some("\"hello\" \"world\""));
        assert!(q.filters.is_empty());
    }

    #[test]
    fn quoted_phrase_is_preserved() {
        let q = parse_query("\"exact phrase\"");
        assert_eq!(q.fts.as_deref(), Some("\"exact phrase\""));
    }

    #[test]
    fn from_operator_extracts_sender_and_leaves_free_text() {
        let q = parse_query("from:alice dinner");
        assert_eq!(q.filters.from, vec!["alice".to_string()]);
        assert_eq!(q.fts.as_deref(), Some("\"dinner\""));
    }

    #[test]
    fn quoted_operator_value() {
        let q = parse_query("from:\"John Doe\" pizza");
        assert_eq!(q.filters.from, vec!["John Doe".to_string()]);
        assert_eq!(q.fts.as_deref(), Some("\"pizza\""));
    }

    #[test]
    fn multiple_operators_and_flags() {
        let q = parse_query("has:photo is:from-me from:bob after:2023-01-01 before:2023-12-31 beach");
        assert!(q.filters.has_photo);
        assert_eq!(q.filters.is_from_me, Some(true));
        assert_eq!(q.filters.from, vec!["bob".to_string()]);
        assert_eq!(q.filters.after, Some(ts("2023-01-01")));
        assert_eq!(q.filters.before, Some(ts("2023-12-31")));
        assert_eq!(q.fts.as_deref(), Some("\"beach\""));
    }

    #[test]
    fn has_variants_and_is_from_them() {
        let q = parse_query("has:attachment has:link is:from-them");
        assert!(q.filters.has_attachment);
        assert!(q.filters.has_link);
        assert_eq!(q.filters.is_from_me, Some(false));
        assert!(q.fts.is_none());
    }

    #[test]
    fn filters_only_query_has_no_fts() {
        let q = parse_query("in:Family is:from-me");
        assert_eq!(q.filters.in_chat, vec!["Family".to_string()]);
        assert_eq!(q.filters.is_from_me, Some(true));
        assert!(q.fts.is_none());
    }

    #[test]
    fn url_with_colon_is_free_text_not_operator() {
        let q = parse_query("https://example.com/path");
        assert!(q.filters.is_empty());
        assert_eq!(q.fts.as_deref(), Some("\"https://example.com/path\""));
    }

    #[test]
    fn unknown_operatorish_token_is_free_text() {
        let q = parse_query("foo:bar");
        assert!(q.filters.is_empty());
        assert_eq!(q.fts.as_deref(), Some("\"foo:bar\""));
    }

    #[test]
    fn datetime_boundaries_parse() {
        assert_eq!(
            parse_date_boundary("2023-06-15"),
            Some(Utc.with_ymd_and_hms(2023, 6, 15, 0, 0, 0).unwrap().timestamp_millis())
        );
        assert_eq!(
            parse_date_boundary("2023-06-15T12:30:00"),
            Some(Utc.with_ymd_and_hms(2023, 6, 15, 12, 30, 0).unwrap().timestamp_millis())
        );
        assert_eq!(
            parse_date_boundary("2023-06-15T12:30:00Z"),
            Some(Utc.with_ymd_and_hms(2023, 6, 15, 12, 30, 0).unwrap().timestamp_millis())
        );
        assert!(parse_date_boundary("not-a-date").is_none());
    }

    #[test]
    fn embedded_quote_is_escaped() {
        let q = parse_query("say \"hi\"\"there\"");
        // Inner quotes are doubled for FTS safety.
        assert!(q.fts.as_deref().unwrap().contains("\"\""));
    }

    #[test]
    fn operator_with_empty_value_is_ignored_as_operator() {
        // `from:` with no value should not create a filter.
        let q = parse_query("from: hello");
        assert!(q.filters.from.is_empty());
    }
}
