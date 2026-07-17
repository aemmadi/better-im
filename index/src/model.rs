//! Public value types returned by the query engine.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// One indexed message row (the denormalized `messages` record).
///
/// Carries enough identity (`id`, `chat_id`, `canonical_chat_id`, `timestamp`)
/// for a caller to jump back to full context in the source thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexedMessage {
    /// Source `message.ROWID` (stable, unique).
    pub id: i64,
    /// Source `message.guid`.
    pub guid: String,
    /// One of the chats this message joins (`chat.ROWID`).
    pub chat_id: Option<i64>,
    /// Deduplicated chat group id (collapses duplicate/split chats).
    pub canonical_chat_id: Option<i64>,
    /// Raw chat identifier (phone/email/group id).
    pub chat_identifier: Option<String>,
    /// Chat display label.
    pub chat_name: Option<String>,
    /// Sender `handle.ROWID`, when known.
    pub handle_id: Option<i64>,
    /// Sender identifier (phone/email); `None` when [`is_from_me`](Self::is_from_me).
    pub sender: Option<String>,
    /// Whether the database owner sent this message.
    pub is_from_me: bool,
    /// Decoded body text.
    pub text: Option<String>,
    /// Timestamp as unix epoch milliseconds (`0` when unknown).
    pub timestamp_millis: i64,
    /// Timestamp as a UTC datetime, when known.
    pub timestamp: Option<DateTime<Utc>>,
    /// Whether the message has any attachment.
    pub has_attachment: bool,
    /// Whether the message has an image/video attachment.
    pub has_photo: bool,
    /// Whether the message body contains a URL.
    pub has_link: bool,
    /// Raw service name (`iMessage`, `SMS`, …).
    pub service: Option<String>,
    /// Source `message.item_type`.
    pub msg_type: i32,
}

impl IndexedMessage {
    /// Rebuild the [`timestamp`](Self::timestamp) datetime from
    /// [`timestamp_millis`](Self::timestamp_millis).
    #[must_use]
    pub fn datetime_from_millis(millis: i64) -> Option<DateTime<Utc>> {
        if millis == 0 {
            None
        } else {
            Utc.timestamp_millis_opt(millis).single()
        }
    }
}

/// A ranked search hit: an [`IndexedMessage`] plus its highlighted excerpt and
/// BM25 relevance score (more negative = more relevant, FTS5 convention).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// The matched message and its jump-to-context references.
    pub message: IndexedMessage,
    /// `snippet()`-highlighted excerpt (matches wrapped in `[` … `]`).
    pub snippet: String,
    /// BM25 score (lower is better; `0.0` for filter-only queries).
    pub score: f64,
}

/// Options controlling a search: result window.
#[derive(Debug, Clone, Copy)]
pub struct SearchOpts {
    /// Maximum number of results to return.
    pub limit: usize,
    /// Number of leading results to skip (pagination).
    pub offset: usize,
}

impl Default for SearchOpts {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

/// Outcome of a `full_reindex` / `incremental_sync` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncReport {
    /// Number of messages written (inserted or updated) this run.
    pub indexed: usize,
    /// The new watermark (highest indexed `message.ROWID`).
    pub watermark: i64,
}
