//! Read-only access to an iMessage `chat.db`.
//!
//! [`ChatReader`] wraps the `imessage-database` crate: it opens the database
//! read-only, decodes message bodies (including the `attributedBody`
//! typedstream blobs used when `message.text` is `NULL`), and maps the upstream
//! rows into the source-agnostic [`crate::models`] types.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use imessage_database::tables::{
    attachment::Attachment as DbAttachment,
    chat::Chat as DbChat,
    chat_handle::ChatToHandle,
    handle::Handle as DbHandle,
    messages::Message as DbMessage,
    table::{get_connection, Cacheable, Table},
};
use rusqlite::Connection;

use crate::models::{Attachment, Conversation, Message};

/// Number of whole seconds between the Unix epoch (`1970-01-01T00:00:00Z`) and
/// the Apple/Cocoa reference epoch (`2001-01-01T00:00:00Z`).
pub const APPLE_EPOCH_OFFSET_SECONDS: i64 = 978_307_200;

/// One second expressed in nanoseconds.
const NANOS_PER_SEC: i64 = 1_000_000_000;

/// Below this magnitude, a raw Messages timestamp is interpreted as *seconds*
/// since the Apple epoch (legacy rows); at or above it, as *nanoseconds*
/// (modern rows). Mirrors the heuristic used by `imessage-database`.
const NANOSECOND_THRESHOLD: i64 = 1_000_000_000_000;

/// Convert a raw Messages timestamp (Apple epoch) to a UTC datetime.
///
/// Messages stores dates relative to `2001-01-01T00:00:00Z`. Modern databases
/// use nanosecond precision; older rows store plain seconds. A raw value of `0`
/// (the "unset" sentinel) yields `None`.
///
/// # Examples
///
/// ```
/// use better_im_core::reader::apple_time_to_utc;
/// use chrono::{TimeZone, Utc};
///
/// // 2022-05-18T00:00:00Z expressed as nanoseconds since 2001-01-01.
/// let dt = apple_time_to_utc(674_524_800_000_000_000).unwrap();
/// assert_eq!(dt, Utc.with_ymd_and_hms(2022, 5, 18, 0, 0, 0).unwrap());
/// assert_eq!(apple_time_to_utc(0), None);
/// ```
#[must_use]
pub fn apple_time_to_utc(raw: i64) -> Option<DateTime<Utc>> {
    if raw == 0 {
        return None;
    }
    let (seconds_since_2001, nanos) = if raw.abs() >= NANOSECOND_THRESHOLD {
        (raw / NANOS_PER_SEC, (raw % NANOS_PER_SEC) as u32)
    } else {
        (raw, 0)
    };
    Utc.timestamp_opt(seconds_since_2001 + APPLE_EPOCH_OFFSET_SECONDS, nanos)
        .single()
}

/// Convert a UTC datetime back to a raw Messages nanosecond timestamp (Apple
/// epoch). Used to build `before` pagination cursors for modern databases.
#[must_use]
pub fn utc_to_apple_nanos(dt: DateTime<Utc>) -> i64 {
    let seconds_since_2001 = dt.timestamp() - APPLE_EPOCH_OFFSET_SECONDS;
    seconds_since_2001 * NANOS_PER_SEC + i64::from(dt.timestamp_subsec_nanos())
}

/// Aggregate counts for a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Number of rows in the `chat` table.
    pub chats: i64,
    /// Number of rows in the `message` table.
    pub messages: i64,
    /// Number of rows in the `attachment` table.
    pub attachments: i64,
}

/// A read-only handle to an iMessage `chat.db`.
///
/// Construct with [`ChatReader::open`]. All queries are read-only; the
/// underlying SQLite connection is opened with `SQLITE_OPEN_READ_ONLY`.
pub struct ChatReader {
    db: Connection,
    /// `handle.ROWID` -> handle identifier (phone/email). Includes `0 -> "Me"`.
    handles: HashMap<i32, String>,
}

/// Explicit column list matching `imessage-database`'s indexed row decoder
/// (`Message::from_row`). Order is load-bearing: the first 26 columns, then
/// `chat_id`, `num_attachments`, `deleted_from`, `num_replies`.
const MESSAGE_COLS: &str = "\
    m.rowid, m.guid, m.text, m.service, m.handle_id, m.destination_caller_id, m.subject, \
    m.date, m.date_read, m.date_delivered, m.is_from_me, m.is_read, m.item_type, \
    m.other_handle, m.share_status, m.share_direction, m.group_title, m.group_action_type, \
    m.associated_message_guid, m.associated_message_type, m.balloon_bundle_id, \
    m.expressive_send_style_id, m.thread_originator_guid, m.thread_originator_part, \
    m.date_edited, m.associated_message_emoji";

impl ChatReader {
    /// Open a `chat.db` at `path` read-only and warm the handle cache.
    ///
    /// # Errors
    /// Returns an error if the file is missing, is not a file, or cannot be
    /// opened read-only (e.g. without Full Disk Access on macOS).
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db = get_connection(path.as_ref())?;
        let handles = DbHandle::cache(&db)?;
        Ok(Self { db, handles })
    }

    /// Resolve a `handle_id` to its identifier string, if known.
    fn handle_identifier(&self, handle_id: Option<i32>) -> Option<String> {
        handle_id.and_then(|id| self.handles.get(&id).cloned())
    }

    /// List all conversations with their participant handles.
    ///
    /// Ordered by chat id for stable output. Note that iMessage can store
    /// duplicate `chat` rows for the same logical conversation; de-duplication
    /// is left to a later phase (Phase 1's index is the right place for it).
    ///
    /// # Errors
    /// Returns an error if the underlying tables cannot be read.
    pub fn list_conversations(&self) -> anyhow::Result<Vec<Conversation>> {
        let chats = DbChat::cache(&self.db)?;
        let chat_participants = ChatToHandle::cache(&self.db)?;

        let mut out: Vec<Conversation> = chats
            .into_values()
            .map(|chat| {
                let participants = chat_participants
                    .get(&chat.rowid)
                    .map(|handle_ids| {
                        handle_ids
                            .iter()
                            .filter_map(|hid| self.handles.get(hid).cloned())
                            .collect()
                    })
                    .unwrap_or_default();
                Conversation {
                    id: i64::from(chat.rowid),
                    identifier: chat.chat_identifier,
                    display_name: chat.display_name.filter(|n| !n.is_empty()),
                    service: chat.service_name,
                    participants,
                }
            })
            .collect();

        out.sort_by_key(|c| c.id);
        Ok(out)
    }

    /// Fetch a page of messages for a chat, oldest-first.
    ///
    /// Returns up to `limit` messages, taking the most recent ones that occur
    /// strictly before `before` (or the most recent overall when `before` is
    /// `None`), then returned in chronological (ascending) order. Each
    /// message's body text is extracted via `parse_body`/`apply_body`, which
    /// decodes the `attributedBody` typedstream blob when `message.text` is
    /// `NULL`.
    ///
    /// # Errors
    /// Returns an error if the query cannot be prepared or executed.
    pub fn get_thread(
        &self,
        chat_id: i64,
        limit: usize,
        before: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Vec<Message>> {
        // Explicit-column query aligned with `Message::from_row`, filtered to a
        // single chat and paginated by date. `ORDER BY ... DESC LIMIT` grabs the
        // newest page; we reverse to ascending below.
        let where_before = if before.is_some() {
            "AND m.date < ?2"
        } else {
            ""
        };
        let sql = format!(
            "SELECT {cols}, \
                 c.chat_id, \
                 (SELECT COUNT(*) FROM message_attachment_join a WHERE m.ROWID = a.message_id) AS num_attachments, \
                 NULL AS deleted_from, \
                 (SELECT COUNT(*) FROM message m2 WHERE m2.thread_originator_guid = m.guid) AS num_replies \
             FROM message AS m \
             JOIN chat_message_join AS c ON m.ROWID = c.message_id \
             WHERE c.chat_id = ?1 {where_before} \
             ORDER BY m.date DESC \
             LIMIT {limit}",
            cols = MESSAGE_COLS,
        );

        let mut stmt = self.db.prepare(&sql)?;

        // Collect the raw upstream rows first (the borrow of `stmt` ends here),
        // then decode bodies which requires an immutable borrow of `self.db`.
        let chat_id_i32 = i32::try_from(chat_id).unwrap_or(i32::MAX);
        let mut db_messages: Vec<DbMessage> = Vec::new();
        {
            let rows = if let Some(before) = before {
                let before_raw = utc_to_apple_nanos(before);
                DbMessage::rows(&mut stmt, rusqlite::params![chat_id_i32, before_raw])?
            } else {
                DbMessage::rows(&mut stmt, rusqlite::params![chat_id_i32])?
            };
            for row in rows {
                db_messages.push(row?);
            }
        }

        // Newest-first from SQL -> chronological.
        db_messages.reverse();

        let mut out = Vec::with_capacity(db_messages.len());
        for db_msg in &mut db_messages {
            out.push(self.map_message(db_msg)?);
        }
        Ok(out)
    }

    /// Map one upstream [`DbMessage`] into the domain [`Message`], decoding its
    /// body text and resolving attachments.
    fn map_message(&self, db_msg: &mut DbMessage) -> anyhow::Result<Message> {
        // Decode the body: pulls plain text out of the `attributedBody`
        // typedstream blob when the `text` column is NULL.
        if let Ok(body) = db_msg.parse_body(&self.db) {
            db_msg.apply_body(body);
        }

        let attachments = self.resolve_attachments(db_msg);

        let sender = if db_msg.is_from_me {
            None
        } else {
            self.handle_identifier(db_msg.handle_id)
        };

        Ok(Message {
            id: i64::from(db_msg.rowid),
            guid: db_msg.guid.clone(),
            chat_id: db_msg.chat_id.map(i64::from),
            sender,
            is_from_me: db_msg.is_from_me,
            text: db_msg.text.clone(),
            timestamp: apple_time_to_utc(db_msg.date),
            date_read: apple_time_to_utc(db_msg.date_read),
            num_attachments: i64::from(db_msg.num_attachments),
            attachments,
            is_edited: db_msg.is_edited(),
            is_reply: db_msg.is_reply(),
            thread_originator_guid: db_msg.thread_originator_guid.clone(),
        })
    }

    /// Resolve a message's attachment rows into domain [`Attachment`]s.
    ///
    /// Best-effort: attachment resolution failures are swallowed (returning an
    /// empty list) so a single bad row never sinks a whole thread.
    fn resolve_attachments(&self, db_msg: &DbMessage) -> Vec<Attachment> {
        DbAttachment::from_message(&self.db, db_msg)
            .map(|rows| rows.into_iter().map(map_attachment).collect())
            .unwrap_or_default()
    }

    /// Compute aggregate counts for the database.
    ///
    /// # Errors
    /// Returns an error if any count query fails.
    pub fn stats(&self) -> anyhow::Result<Stats> {
        Ok(Stats {
            chats: self.count("chat")?,
            messages: self.count("message")?,
            attachments: self.count("attachment")?,
        })
    }

    fn count(&self, table: &str) -> anyhow::Result<i64> {
        // Table name is from a fixed internal allowlist, never user input.
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count: i64 = self.db.query_row(&sql, [], |row| row.get(0))?;
        Ok(count)
    }

    /// Borrow the underlying read-only connection (escape hatch for callers
    /// that need a query this reader does not yet expose).
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.db
    }
}

/// Map an upstream attachment row into the domain [`Attachment`].
fn map_attachment(a: DbAttachment) -> Attachment {
    Attachment {
        id: i64::from(a.rowid),
        guid: a.guid,
        filename: a.filename,
        mime_type: a.mime_type,
        total_bytes: a.total_bytes,
        is_sticker: a.is_sticker,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_epoch_nanoseconds_round_trips_to_known_utc() {
        // 2022-05-18T00:00:00Z as nanoseconds since 2001-01-01.
        let raw = 674_524_800_000_000_000;
        let expected = Utc.with_ymd_and_hms(2022, 5, 18, 0, 0, 0).unwrap();
        assert_eq!(apple_time_to_utc(raw), Some(expected));
        assert_eq!(utc_to_apple_nanos(expected), raw);
    }

    #[test]
    fn apple_epoch_preserves_subsecond_nanos() {
        let raw = 674_524_800_500_000_000; // +0.5s
        let dt = apple_time_to_utc(raw).unwrap();
        assert_eq!(dt.timestamp_subsec_nanos(), 500_000_000);
    }

    #[test]
    fn apple_epoch_legacy_seconds_path() {
        // Same instant, stored as plain seconds (legacy rows).
        let secs = 674_524_800;
        let expected = Utc.with_ymd_and_hms(2022, 5, 18, 0, 0, 0).unwrap();
        assert_eq!(apple_time_to_utc(secs), Some(expected));
    }

    #[test]
    fn apple_epoch_zero_is_none() {
        assert_eq!(apple_time_to_utc(0), None);
    }
}
