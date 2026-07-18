//! URL detection + extraction.
//!
//! A single regex backs both the index-time `has_link` flag (via [`has_url`])
//! and the links hub's per-URL expansion (via [`extract_urls`]), so detection
//! and extraction can never disagree: every message flagged `has_link` yields at
//! least one URL here.

use std::sync::LazyLock;

use regex::Regex;

/// Matches `http(s)://…` URLs and bare `www.` hosts. Kept intentionally simple
/// (a run of non-whitespace after the scheme); trailing sentence punctuation is
/// trimmed by [`normalize`].
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:https?://|www\.)\S+").expect("valid URL regex"));

/// Whether `text` contains at least one URL (drives the `has_link` flag).
#[must_use]
pub fn has_url(text: &str) -> bool {
    URL_RE.is_match(text)
}

/// Extract every URL from `text`, in the order they appear. Trailing sentence
/// punctuation is trimmed and bare `www.` hosts are normalized to an absolute
/// `https://` URL so callers (e.g. `open_url`) can open them directly.
#[must_use]
pub fn extract_urls(text: &str) -> Vec<String> {
    URL_RE
        .find_iter(text)
        .filter_map(|m| normalize(m.as_str()))
        .collect()
}

/// Trim trailing punctuation a URL commonly picks up from surrounding prose and
/// promote a bare `www.` host to `https://www.…`.
fn normalize(raw: &str) -> Option<String> {
    let trimmed = raw.trim_end_matches(|c: char| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\'' | '>'
        )
    });
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() >= 4 && trimmed[..4].eq_ignore_ascii_case("www.") {
        Some(format!("https://{trimmed}"))
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_extracts_a_single_url() {
        let text = "check this out https://example.com/cool";
        assert!(has_url(text));
        assert_eq!(extract_urls(text), vec!["https://example.com/cool"]);
    }

    #[test]
    fn extracts_multiple_urls_in_order() {
        let text = "first https://a.example second http://b.example/x third";
        assert_eq!(
            extract_urls(text),
            vec!["https://a.example", "http://b.example/x"]
        );
    }

    #[test]
    fn trims_trailing_punctuation() {
        assert_eq!(
            extract_urls("see (https://example.com/page)."),
            vec!["https://example.com/page"]
        );
        assert_eq!(
            extract_urls("done: https://example.com!"),
            vec!["https://example.com"]
        );
    }

    #[test]
    fn normalizes_bare_www_host_to_https() {
        assert_eq!(
            extract_urls("visit www.example.com today"),
            vec!["https://www.example.com"]
        );
    }

    #[test]
    fn no_urls_in_plain_text() {
        assert!(!has_url("just some ordinary words"));
        assert!(extract_urls("just some ordinary words").is_empty());
    }
}
