//! Contacts resolution (Phase 3).
//!
//! Splits into two halves:
//! - **This module** — the *pure*, platform-independent matching logic: phone /
//!   email normalization, the handle → key canonicalization, display-name
//!   assembly, and the in-memory [`ContactIndex`]. All of it is unit-tested
//!   (see `#[cfg(test)]` below) and needs no Contacts framework.
//! - [`store`] — the macOS-only `CNContactStore` enumeration that produces the
//!   [`ContactRecord`]s this module indexes. Behind a `cfg` so the workspace
//!   still builds on non-macOS targets (returning "denied" + no contacts).
//!
//! A `chat.db` handle is a raw phone/email string. Contacts store the same
//! endpoints but formatted differently ("+1 (555) 123-4567" vs "+15551234567"),
//! so both sides are pushed through the *same* [`normalize_handle_key`] before
//! comparison — that is what makes the match tolerant of formatting.

pub mod store;

use std::collections::HashMap;

use crate::dto::ContactInfoDto;

/// Where the user stands on the Contacts permission (TCC) prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    Authorized,
    Denied,
    Restricted,
    NotDetermined,
}

impl PermissionStatus {
    /// Stable lowercase-ish string mirrored by the frontend.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionStatus::Authorized => "authorized",
            PermissionStatus::Denied => "denied",
            PermissionStatus::Restricted => "restricted",
            PermissionStatus::NotDetermined => "notDetermined",
        }
    }
}

/// A single contact as enumerated from the store, before indexing. Plain data
/// (no Objective-C types) so it crosses the FFI boundary cleanly and is trivial
/// to build in tests.
#[derive(Debug, Clone, Default)]
pub struct ContactRecord {
    /// Assembled display name (may be empty if the card had no name fields).
    pub display_name: String,
    /// Raw phone-number strings exactly as stored in Contacts.
    pub phones: Vec<String>,
    /// Raw email strings exactly as stored in Contacts.
    pub emails: Vec<String>,
    /// `thumbnailImageData` already encoded as a `data:` URL, if present.
    pub avatar_data_url: Option<String>,
}

/// The resolved identity for one handle: what the UI ultimately renders.
#[derive(Debug, Clone)]
pub struct ResolvedContact {
    pub display_name: String,
    pub avatar_data_url: Option<String>,
}

/// A prebuilt lookup from normalized handle key → contact. Built once from the
/// full store enumeration and cached in `AppState`, so per-handle resolution is
/// just a `HashMap` hit.
#[derive(Debug, Default)]
pub struct ContactIndex {
    by_key: HashMap<String, ResolvedContact>,
}

impl ContactIndex {
    /// An index that matches nothing (used when permission is not granted so the
    /// app degrades to raw/formatted handles).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build the index from enumerated records. Every phone and email of every
    /// contact becomes a key; on collision the later contact wins (rare, and not
    /// worth resolving perfectly).
    #[must_use]
    pub fn build(records: Vec<ContactRecord>) -> Self {
        let mut by_key = HashMap::new();
        for rec in records {
            let resolved = ResolvedContact {
                display_name: rec.display_name,
                avatar_data_url: rec.avatar_data_url,
            };
            for handle in rec.phones.iter().chain(rec.emails.iter()) {
                if let Some(key) = normalize_handle_key(handle) {
                    by_key.insert(key, resolved.clone());
                }
            }
        }
        Self { by_key }
    }

    /// Look up the contact for a raw `chat.db` handle, if any.
    #[must_use]
    pub fn lookup(&self, handle: &str) -> Option<&ResolvedContact> {
        let key = normalize_handle_key(handle)?;
        self.by_key.get(&key)
    }

    /// Resolve a handle into the DTO the frontend consumes. Unmatched handles (or
    /// matched contacts with no name) fall back to a nicely [`format_handle`]d
    /// version of the raw handle.
    #[must_use]
    pub fn resolve(&self, handle: &str) -> ContactInfoDto {
        match self.lookup(handle) {
            Some(c) if !c.display_name.trim().is_empty() => ContactInfoDto {
                display_name: c.display_name.clone(),
                avatar_data_url: c.avatar_data_url.clone(),
                matched: true,
            },
            // Matched a card, but it has no name: keep the avatar, format the handle.
            Some(c) => ContactInfoDto {
                display_name: format_handle(handle),
                avatar_data_url: c.avatar_data_url.clone(),
                matched: true,
            },
            None => ContactInfoDto {
                display_name: format_handle(handle),
                avatar_data_url: None,
                matched: false,
            },
        }
    }
}

/// Reduce a phone number to its comparable core: digits only, and — when there
/// are more than 10 — just the trailing 10. This strips spaces, dashes, parens,
/// a leading `+`, and any country-code prefix, so "+1 (555) 123-4567",
/// "1-555-123-4567", and "5551234567" all collapse to "5551234567". Numbers with
/// fewer than 10 digits (short codes, local 7-digit numbers) are kept whole and
/// matched exactly. Returns `None` when there are no digits at all.
#[must_use]
pub fn normalize_phone(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    if digits.len() > 10 {
        // Keep the last 10 digits, dropping country code / trunk prefixes.
        Some(digits[digits.len() - 10..].to_string())
    } else {
        Some(digits)
    }
}

/// Normalize an email: trim surrounding whitespace and lowercase. Returns `None`
/// for strings without an `@` (not an email).
#[must_use]
pub fn normalize_email(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.contains('@') {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

/// Canonical lookup key for any handle (phone or email), prefixed so a
/// phone-shaped local part can never collide with a phone number. Both the
/// contact side and the `chat.db` side go through this, guaranteeing they agree.
#[must_use]
pub fn normalize_handle_key(handle: &str) -> Option<String> {
    let trimmed = handle.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('@') {
        normalize_email(trimmed).map(|e| format!("e:{e}"))
    } else {
        normalize_phone(trimmed).map(|p| format!("p:{p}"))
    }
}

/// Human-friendly rendering of a raw handle for the *unmatched* case: US-style
/// phone formatting when the shape is recognizable, otherwise the handle as-is.
/// Emails and anything unrecognized are returned unchanged.
#[must_use]
pub fn format_handle(handle: &str) -> String {
    let h = handle.trim();
    if h.is_empty() {
        return "Unknown".to_string();
    }
    if h.contains('@') {
        return h.to_string();
    }
    let digits: String = h.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        10 => format!("({}) {}-{}", &digits[0..3], &digits[3..6], &digits[6..10]),
        11 if digits.starts_with('1') => {
            format!("+1 ({}) {}-{}", &digits[1..4], &digits[4..7], &digits[7..11])
        }
        _ => h.to_string(),
    }
}

/// Assemble a display name from the Contacts name fields, in priority order:
/// "given family" (either side may be blank), then nickname, then organization.
/// Returns an empty string when the card carries no usable name.
#[must_use]
pub fn display_name_from_parts(given: &str, family: &str, nickname: &str, organization: &str) -> String {
    let full = format!("{} {}", given.trim(), family.trim());
    let full = full.trim();
    if !full.is_empty() {
        return full.to_string();
    }
    let nick = nickname.trim();
    if !nick.is_empty() {
        return nick.to_string();
    }
    organization.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_phone_strips_formatting_and_country_code() {
        // Same US number in three different formats collapses to one key.
        assert_eq!(normalize_phone("+1 (555) 123-4567").as_deref(), Some("5551234567"));
        assert_eq!(normalize_phone("1-555-123-4567").as_deref(), Some("5551234567"));
        assert_eq!(normalize_phone("5551234567").as_deref(), Some("5551234567"));
        assert_eq!(normalize_phone(" +15551234567 ").as_deref(), Some("5551234567"));
    }

    #[test]
    fn normalize_phone_keeps_short_numbers_whole() {
        assert_eq!(normalize_phone("262966").as_deref(), Some("262966")); // short code
        assert_eq!(normalize_phone("123-4567").as_deref(), Some("1234567")); // 7-digit local
        assert_eq!(normalize_phone("no digits here"), None);
        assert_eq!(normalize_phone(""), None);
    }

    #[test]
    fn normalize_phone_international_uses_trailing_ten() {
        // UK: +44 20 7946 0958 -> digits 442079460958 -> last 10.
        assert_eq!(normalize_phone("+44 20 7946 0958").as_deref(), Some("2079460958"));
        assert_eq!(normalize_phone("+442079460958").as_deref(), Some("2079460958"));
    }

    #[test]
    fn normalize_email_trims_and_lowercases() {
        assert_eq!(normalize_email("  Alice@Example.COM ").as_deref(), Some("alice@example.com"));
        assert_eq!(normalize_email("not-an-email"), None);
    }

    #[test]
    fn handle_key_is_stable_across_formats() {
        let a = normalize_handle_key("+1 (555) 123-4567");
        let b = normalize_handle_key("5551234567");
        let c = normalize_handle_key("1.555.123.4567");
        assert!(a.is_some());
        assert_eq!(a, b);
        assert_eq!(b, c);
        // Emails and phones live in disjoint key spaces.
        assert_ne!(normalize_handle_key("5551234567"), normalize_handle_key("55512345@67"));
        assert_eq!(normalize_handle_key("  Bob@Work.io "), Some("e:bob@work.io".to_string()));
        assert_eq!(normalize_handle_key("   "), None);
    }

    #[test]
    fn format_handle_pretty_prints_phones() {
        assert_eq!(format_handle("5551234567"), "(555) 123-4567");
        assert_eq!(format_handle("+15551234567"), "+1 (555) 123-4567");
        assert_eq!(format_handle("alice@example.com"), "alice@example.com");
        assert_eq!(format_handle("+442079460958"), "+442079460958"); // unrecognized shape: as-is
        assert_eq!(format_handle(""), "Unknown");
    }

    #[test]
    fn display_name_precedence() {
        assert_eq!(display_name_from_parts("Ada", "Lovelace", "Countess", "Analytical"), "Ada Lovelace");
        assert_eq!(display_name_from_parts("", "Lovelace", "", ""), "Lovelace");
        assert_eq!(display_name_from_parts("Ada", "", "", ""), "Ada");
        assert_eq!(display_name_from_parts("", "", "Ada!", "Analytical"), "Ada!");
        assert_eq!(display_name_from_parts("  ", " ", "  ", "Analytical Engine Co"), "Analytical Engine Co");
        assert_eq!(display_name_from_parts("", "", "", ""), "");
    }

    #[test]
    fn index_matches_across_formatting_and_falls_back() {
        let records = vec![ContactRecord {
            display_name: "Grace Hopper".to_string(),
            phones: vec!["+1 (555) 123-4567".to_string()],
            emails: vec!["Grace@Navy.mil".to_string()],
            avatar_data_url: Some("data:image/png;base64,AAA".to_string()),
        }];
        let index = ContactIndex::build(records);

        // Phone matches despite different formatting on the chat.db side.
        let hit = index.resolve("5551234567");
        assert!(hit.matched);
        assert_eq!(hit.display_name, "Grace Hopper");
        assert_eq!(hit.avatar_data_url.as_deref(), Some("data:image/png;base64,AAA"));

        // Email matches case-insensitively.
        let hit = index.resolve("grace@navy.mil");
        assert!(hit.matched);
        assert_eq!(hit.display_name, "Grace Hopper");

        // Unknown handle: unmatched + formatted fallback + no avatar.
        let miss = index.resolve("+15559999999");
        assert!(!miss.matched);
        assert_eq!(miss.display_name, "+1 (555) 999-9999");
        assert!(miss.avatar_data_url.is_none());
    }

    #[test]
    fn empty_index_matches_nothing() {
        let index = ContactIndex::empty();
        assert!(index.lookup("alice@example.com").is_none());
        let r = index.resolve("alice@example.com");
        assert!(!r.matched);
        assert_eq!(r.display_name, "alice@example.com");
    }

    #[test]
    fn nameless_card_keeps_avatar_but_formats_handle() {
        let records = vec![ContactRecord {
            display_name: String::new(),
            phones: vec!["5551234567".to_string()],
            emails: vec![],
            avatar_data_url: Some("data:image/jpeg;base64,ZZZ".to_string()),
        }];
        let index = ContactIndex::build(records);
        let hit = index.resolve("5551234567");
        assert!(hit.matched);
        assert_eq!(hit.display_name, "(555) 123-4567");
        assert_eq!(hit.avatar_data_url.as_deref(), Some("data:image/jpeg;base64,ZZZ"));
    }
}
