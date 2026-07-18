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

/// One extracted shared link, produced by [`IndexDb::list_links`](crate::db::IndexDb::list_links).
/// A message with several URLs yields one `LinkRow` per URL.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRow {
    /// Source `message.ROWID` the link came from.
    pub message_id: i64,
    /// Chat the message belongs to (`chat.ROWID`), when known.
    pub chat_id: Option<i64>,
    /// The extracted (and normalized) URL.
    pub url: String,
    /// Message timestamp (UTC), when known.
    pub timestamp: Option<DateTime<Utc>>,
    /// Sender identifier; `None` when the message is from the database owner.
    pub sender: Option<String>,
    /// Whether the database owner sent the message.
    pub is_from_me: bool,
    /// Chat display label (custom name, else identifier).
    pub chat_name: Option<String>,
}

/// Messages sent on one calendar day (local time, `YYYY-MM-DD`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayCount {
    pub date: String,
    pub count: i64,
}

/// Messages sent in one hour of the day (local time, `0..=23`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourCount {
    pub hour: i32,
    pub count: i64,
}

/// A correspondent ranked by inbound (received) message volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactCount {
    /// Raw sender handle (phone/email).
    pub handle: String,
    pub count: i64,
}

/// Aggregate stats for one conversation or the whole corpus, produced by
/// [`IndexDb::insights`](crate::db::IndexDb::insights).
#[derive(Debug, Clone, PartialEq)]
pub struct InsightsData {
    pub total_messages: i64,
    pub sent_count: i64,
    pub received_count: i64,
    /// Earliest message timestamp (UTC), when any.
    pub first_message: Option<DateTime<Utc>>,
    /// Latest message timestamp (UTC), when any.
    pub last_message: Option<DateTime<Utc>>,
    pub by_day: Vec<DayCount>,
    pub by_hour: Vec<HourCount>,
    pub top_contacts: Vec<ContactCount>,
}

/// Outcome of a `full_reindex` / `incremental_sync` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncReport {
    /// Number of messages written (inserted or updated) this run.
    pub indexed: usize,
    /// The new watermark (highest indexed `message.ROWID`).
    pub watermark: i64,
}
