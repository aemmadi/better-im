//! Shared synthetic `chat.db` builder for the index integration tests.
//!
//! Extends the Phase 0 fixture (`core/tests/synthetic_db.rs`) with the extra
//! surface Phase 1 exercises: multiple chats (including a duplicate chat with an
//! identical participant set, to test canonical collapsing), image vs non-image
//! attachments, links, from-me/received messages, and an `attributedBody`-only
//! message. It also adds `chat_recoverable_message_join` so `imessage-database`'s
//! primary (macOS Ventura+) full-scan query path is used.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use rusqlite::{params, Connection};

use better_im_core::reader::utc_to_apple_nanos;

/// Read a checked-in typedstream fixture blob.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()))
}

/// Minimal iMessage schema (superset of the Phase 0 one) the crate's queries
/// touch, including `chat_recoverable_message_join`.
const SCHEMA: &str = "
CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT NOT NULL, person_centric_id TEXT);

CREATE TABLE chat (
    ROWID INTEGER PRIMARY KEY,
    chat_identifier TEXT NOT NULL,
    service_name TEXT,
    display_name TEXT,
    properties BLOB
);

CREATE TABLE attachment (
    ROWID INTEGER PRIMARY KEY,
    guid TEXT,
    filename TEXT,
    uti TEXT,
    mime_type TEXT,
    transfer_name TEXT,
    total_bytes INTEGER DEFAULT 0,
    is_sticker INTEGER DEFAULT 0,
    hide_attachment INTEGER DEFAULT 0,
    emoji_image_short_description TEXT
);

CREATE TABLE message (
    ROWID INTEGER PRIMARY KEY,
    guid TEXT NOT NULL,
    text TEXT,
    service TEXT,
    handle_id INTEGER,
    destination_caller_id TEXT,
    subject TEXT,
    date INTEGER,
    date_read INTEGER DEFAULT 0,
    date_delivered INTEGER DEFAULT 0,
    is_from_me INTEGER DEFAULT 0,
    is_read INTEGER DEFAULT 0,
    item_type INTEGER DEFAULT 0,
    other_handle INTEGER DEFAULT 0,
    share_status INTEGER DEFAULT 0,
    share_direction INTEGER DEFAULT 0,
    group_title TEXT,
    group_action_type INTEGER DEFAULT 0,
    associated_message_guid TEXT,
    associated_message_type INTEGER DEFAULT 0,
    balloon_bundle_id TEXT,
    expressive_send_style_id TEXT,
    thread_originator_guid TEXT,
    thread_originator_part TEXT,
    date_edited INTEGER DEFAULT 0,
    associated_message_emoji TEXT,
    attributedBody BLOB,
    payload_data BLOB,
    message_summary_info BLOB
);

CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER, message_date INTEGER);
CREATE TABLE message_attachment_join (message_id INTEGER, attachment_id INTEGER);
CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
CREATE TABLE chat_recoverable_message_join (chat_id INTEGER, message_id INTEGER, delete_date INTEGER);
";

/// Convert a `YYYY-MM-DD HH:MM` UTC wall-clock to a raw Apple-epoch nanos value.
fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
    utc_to_apple_nanos(Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap())
}

/// Insert one message row. `attributed` overrides `text` with a decoded blob.
#[allow(clippy::too_many_arguments)]
fn insert_message(
    conn: &Connection,
    rowid: i64,
    guid: &str,
    text: Option<&str>,
    handle_id: i64,
    date: i64,
    is_from_me: i64,
    attributed: Option<&[u8]>,
) {
    conn.execute(
        "INSERT INTO message (ROWID, guid, text, service, handle_id, date, is_from_me, attributedBody) \
         VALUES (?1, ?2, ?3, 'iMessage', ?4, ?5, ?6, ?7)",
        params![rowid, guid, text, handle_id, date, is_from_me, attributed],
    )
    .unwrap();
}

fn join_chat(conn: &Connection, chat_id: i64, message_id: i64, date: i64) {
    conn.execute(
        "INSERT INTO chat_message_join (chat_id, message_id, message_date) VALUES (?1, ?2, ?3)",
        params![chat_id, message_id, date],
    )
    .unwrap();
}

/// Build the full synthetic database at `path`.
///
/// Message row ids and their meaning are stable and referenced by the tests:
/// 1 plain received, 2 attributedBody-only (from me), 3 "dinner" received,
/// 4 link (from me), 5 image attachment, 6 pdf attachment, 7 "dinner" received,
/// 8 message in a duplicate chat (canonical-collapse fixture).
pub fn build_db(path: &Path) {
    let conn = Connection::open(path).expect("open writable synthetic db");
    conn.execute_batch(SCHEMA).expect("create schema");

    // Handles (emails, so `from:<name>` operators are meaningful).
    conn.execute(
        "INSERT INTO handle (ROWID, id) VALUES (1, 'alice@example.com'), (2, 'bob@example.com')",
        [],
    )
    .unwrap();

    // Chats: 1 = Alice direct; 2 = a duplicate of Alice (same participant set);
    // 3 = a group with Alice + Bob.
    conn.execute(
        "INSERT INTO chat (ROWID, chat_identifier, service_name, display_name) VALUES \
         (1, 'alice@example.com', 'iMessage', 'Alice'), \
         (2, 'alice@example.com', 'iMessage', ''), \
         (3, 'chat-work',        'iMessage', 'Work')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_handle_join (chat_id, handle_id) VALUES (1,1), (2,1), (3,1), (3,2)",
        [],
    )
    .unwrap();

    let t1 = at(2023, 1, 1, 12, 0);
    let t2 = at(2023, 1, 1, 12, 5);
    let t3 = at(2023, 3, 15, 18, 0);
    let t4 = at(2023, 3, 15, 18, 5);
    let t5 = at(2023, 6, 20, 9, 0);
    let t6 = at(2023, 6, 20, 9, 5);
    let t7 = at(2023, 9, 10, 20, 0);
    let t8 = at(2023, 5, 1, 10, 0);

    insert_message(&conn, 1, "g1", Some("hello from the text column"), 1, t1, 0, None);
    let attr = fixture("AttributedBodyTextOnly"); // decodes to "Noter test"
    insert_message(&conn, 2, "g2", None, 1, t2, 1, Some(&attr));
    insert_message(&conn, 3, "g3", Some("let's grab dinner tonight at the beach"), 1, t3, 0, None);
    insert_message(&conn, 4, "g4", Some("check this out https://example.com/cool"), 1, t4, 1, None);
    insert_message(&conn, 5, "g5", Some("quarterly report attached"), 2, t5, 0, None);
    insert_message(&conn, 6, "g6", Some("here is a pdf document"), 2, t6, 0, None);
    insert_message(&conn, 7, "g7", Some("dinner was great yesterday"), 1, t7, 0, None);
    insert_message(&conn, 8, "g8", Some("message in the duplicate chat"), 1, t8, 0, None);

    join_chat(&conn, 1, 1, t1);
    join_chat(&conn, 1, 2, t2);
    join_chat(&conn, 1, 3, t3);
    join_chat(&conn, 1, 4, t4);
    join_chat(&conn, 3, 5, t5);
    join_chat(&conn, 3, 6, t6);
    join_chat(&conn, 1, 7, t7);
    join_chat(&conn, 2, 8, t8); // the duplicate chat

    // Attachments: message 5 has an image, message 6 has a pdf.
    conn.execute(
        "INSERT INTO attachment (ROWID, guid, filename, mime_type, total_bytes) VALUES \
         (1, 'a1', 'photo.jpg', 'image/jpeg', 1000), \
         (2, 'a2', 'doc.pdf',   'application/pdf', 2000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message_attachment_join (message_id, attachment_id) VALUES (5,1), (6,2)",
        [],
    )
    .unwrap();

    conn.close().expect("close synthetic db");
}

/// Append a new message (for exercising incremental sync). Uses a fresh writable
/// connection so the change is committed and visible to a read-only reader.
pub fn append_message(path: &Path, rowid: i64, guid: &str, text: &str, chat_id: i64) {
    let conn = Connection::open(path).expect("reopen synthetic db for append");
    let date = at(2023, 10, 1, 8, 0);
    insert_message(&conn, rowid, guid, Some(text), 1, date, 0, None);
    join_chat(&conn, chat_id, rowid, date);
    conn.close().expect("close after append");
}
