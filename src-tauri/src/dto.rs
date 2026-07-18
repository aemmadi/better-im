//! Serde DTOs exposed to the webview.
//!
//! All fields serialize as `camelCase`; timestamps are ISO-8601 (RFC3339)
//! strings so the TypeScript layer can `new Date(...)` them directly. These are
//! the single source of truth mirrored by `frontend/src/types.ts`.

use better_im_core::{Attachment, Conversation, Message};
use better_im_index::{IndexedMessage, SearchResult, SyncReport};
use chrono::{DateTime, Utc};
use serde::Serialize;

/// ISO-8601 helper for an optional timestamp.
fn iso(ts: Option<DateTime<Utc>>) -> Option<String> {
    ts.map(|t| t.to_rfc3339())
}

/// Result of the Full Disk Access probe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FdaStatus {
    pub granted: bool,
}

/// A conversation row for the sidebar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDto {
    pub id: i64,
    pub identifier: String,
    pub display_name: Option<String>,
    /// Best display label today (custom name, else identifier). Phase 3 replaces
    /// this with a Contacts-resolved name on the frontend.
    pub label: String,
    pub service: Option<String>,
    pub participants: Vec<String>,
}

impl From<&Conversation> for ConversationDto {
    fn from(c: &Conversation) -> Self {
        Self {
            id: c.id,
            identifier: c.identifier.clone(),
            display_name: c.display_name.clone(),
            label: c.label().to_string(),
            service: c.service.clone(),
            participants: c.participants.clone(),
        }
    }
}

/// An attachment reference (media resolved for thread rows; empty for index
/// context rows, which only carry the boolean flags).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDto {
    pub id: i64,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub total_bytes: i64,
    pub is_sticker: bool,
}

impl From<&Attachment> for AttachmentDto {
    fn from(a: &Attachment) -> Self {
        Self {
            id: a.id,
            filename: a.filename.clone(),
            mime_type: a.mime_type.clone(),
            total_bytes: a.total_bytes,
            is_sticker: a.is_sticker,
        }
    }
}

/// A message for the thread / context views. Constructible from both the core
/// [`Message`] (full thread rows, with resolved attachments) and the index
/// [`IndexedMessage`] (search-context rows, attachment flags only).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    pub id: i64,
    pub guid: String,
    pub chat_id: Option<i64>,
    /// Sender identifier (phone/email); `None` when `isFromMe`.
    pub sender: Option<String>,
    pub is_from_me: bool,
    pub service: Option<String>,
    pub text: Option<String>,
    /// ISO-8601 timestamp, when known.
    pub timestamp: Option<String>,
    pub num_attachments: i64,
    pub attachments: Vec<AttachmentDto>,
    pub is_edited: bool,
    pub is_reply: bool,
    pub has_attachment: bool,
    pub has_photo: bool,
    /// `message.item_type` (0 = ordinary message; non-zero = group action, etc.).
    pub item_type: i32,
}

impl From<&Message> for MessageDto {
    fn from(m: &Message) -> Self {
        let has_photo = m.attachments.iter().any(|a| {
            a.mime_type
                .as_deref()
                .is_some_and(|t| t.starts_with("image/") || t.starts_with("video/"))
        });
        Self {
            id: m.id,
            guid: m.guid.clone(),
            chat_id: m.chat_id,
            sender: m.sender.clone(),
            is_from_me: m.is_from_me,
            service: m.service.clone(),
            text: m.text.clone(),
            timestamp: iso(m.timestamp),
            num_attachments: m.num_attachments,
            attachments: m.attachments.iter().map(AttachmentDto::from).collect(),
            is_edited: m.is_edited,
            is_reply: m.is_reply,
            has_attachment: m.num_attachments > 0,
            has_photo,
            item_type: m.item_type,
        }
    }
}

impl From<&IndexedMessage> for MessageDto {
    fn from(m: &IndexedMessage) -> Self {
        Self {
            id: m.id,
            guid: m.guid.clone(),
            chat_id: m.chat_id,
            sender: m.sender.clone(),
            is_from_me: m.is_from_me,
            service: m.service.clone(),
            text: m.text.clone(),
            timestamp: iso(m.timestamp),
            // The index stores flags, not resolved attachment rows.
            num_attachments: i64::from(m.has_attachment),
            attachments: Vec::new(),
            is_edited: false,
            is_reply: false,
            has_attachment: m.has_attachment,
            has_photo: m.has_photo,
            item_type: m.msg_type,
        }
    }
}

/// A ranked search hit. Carries the highlighted snippet plus the message refs
/// (`id` / `chatId` / `timestamp`) needed to jump into context.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    pub id: i64,
    pub chat_id: Option<i64>,
    pub canonical_chat_id: Option<i64>,
    pub timestamp: Option<String>,
    pub sender: Option<String>,
    pub is_from_me: bool,
    pub chat_name: Option<String>,
    pub chat_identifier: Option<String>,
    /// FTS `snippet()` output: matched spans wrapped in `[` … `]`. The frontend
    /// renders those markers as `<mark>`.
    pub snippet: String,
    pub score: f64,
}

impl From<&SearchResult> for SearchResultDto {
    fn from(r: &SearchResult) -> Self {
        let m = &r.message;
        Self {
            id: m.id,
            chat_id: m.chat_id,
            canonical_chat_id: m.canonical_chat_id,
            timestamp: iso(m.timestamp),
            sender: m.sender.clone(),
            is_from_me: m.is_from_me,
            chat_name: m.chat_name.clone(),
            chat_identifier: m.chat_identifier.clone(),
            snippet: r.snippet.clone(),
            score: r.score,
        }
    }
}

/// Outcome of a sync/reindex run (also the `index-updated` event payload).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReportDto {
    pub indexed: usize,
    pub watermark: i64,
}

impl From<SyncReport> for SyncReportDto {
    fn from(r: SyncReport) -> Self {
        Self {
            indexed: r.indexed,
            watermark: r.watermark,
        }
    }
}

/// Index health for the status bar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatusDto {
    pub count: i64,
    /// ISO-8601 timestamp of the last successful sync, when known.
    pub last_synced: Option<String>,
}

/// The resolved identity for a single `chat.db` handle (phone/email). Returned
/// by `resolve_contacts`, keyed by the raw identifier that was requested.
///
/// Best-effort: an unmatched identifier still comes back with a nicely formatted
/// `display_name` (and `matched = false`), so the UI never has to special-case a
/// missing entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactInfoDto {
    /// Contacts display name when matched, else a formatted version of the handle.
    pub display_name: String,
    /// `thumbnailImageData` as a `data:` URL, when the matched contact has a photo.
    pub avatar_data_url: Option<String>,
    /// Whether this identifier matched a card in the user's Contacts.
    pub matched: bool,
}
