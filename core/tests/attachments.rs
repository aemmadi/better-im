//! Exercise `ChatReader::list_attachments` over a synthetic `chat.db` with real
//! `attachment` / `message_attachment_join` rows (the data path behind the
//! Phase 4 `list_media` command). Path `~`-expansion, existence checks, and
//! `kind` classification are pure and unit-tested in the app layer.

use std::path::Path;

use chrono::{TimeZone, Utc};
use rusqlite::{params, Connection};

use better_im_core::reader::{utc_to_apple_nanos, ChatReader};

/// Minimal subset of the iMessage schema the queries touch (mirrors the Phase 0
/// helper), sufficient for `ChatReader::open` + `list_attachments`.
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
    date INTEGER,
    is_from_me INTEGER DEFAULT 0
);

CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER, message_date INTEGER);
CREATE TABLE message_attachment_join (message_id INTEGER, attachment_id INTEGER);
CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
";

fn build_db(path: &Path) {
    let conn = Connection::open(path).expect("open writable synthetic db");
    conn.execute_batch(SCHEMA).expect("create schema");

    conn.execute(
        "INSERT INTO handle (ROWID, id) VALUES (1, '+15551234567')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat (ROWID, chat_identifier, service_name, display_name) \
         VALUES (1, '+15551234567', 'iMessage', 'Alice')",
        [],
    )
    .unwrap();

    let t1 = utc_to_apple_nanos(Utc.with_ymd_and_hms(2023, 1, 1, 12, 0, 0).unwrap());
    let t2 = utc_to_apple_nanos(Utc.with_ymd_and_hms(2023, 1, 1, 12, 5, 0).unwrap());

    // Message 1: received image. Message 2: sent (from me) video.
    conn.execute(
        "INSERT INTO message (ROWID, guid, service, handle_id, date, is_from_me) \
         VALUES (1, 'g1', 'iMessage', 1, ?1, 0), (2, 'g2', 'iMessage', 1, ?2, 1)",
        params![t1, t2],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_message_join (chat_id, message_id, message_date) \
         VALUES (1, 1, ?1), (1, 2, ?2)",
        params![t1, t2],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO attachment (ROWID, guid, filename, mime_type, transfer_name, total_bytes) VALUES \
         (1, 'a1', '~/Library/Messages/Attachments/ab/01/photo.jpg', 'image/jpeg', 'photo.jpg', 1000), \
         (2, 'a2', '~/Library/Messages/Attachments/cd/02/clip.mov',  'video/quicktime', 'clip.mov', 2000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message_attachment_join (message_id, attachment_id) VALUES (1, 1), (2, 2)",
        [],
    )
    .unwrap();

    conn.close().expect("close synthetic db");
}

#[test]
fn list_attachments_newest_first_with_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("chat.db");
    build_db(&db_path);

    let reader = ChatReader::open(&db_path).unwrap();
    let items = reader.list_attachments(None, 50, 0).unwrap();
    assert_eq!(items.len(), 2);

    // Newest-first: message 2 (12:05, sent) precedes message 1 (12:00, received).
    let sent = &items[0];
    assert_eq!(sent.message_id, 2);
    assert_eq!(sent.chat_id, Some(1));
    assert_eq!(sent.mime_type.as_deref(), Some("video/quicktime"));
    assert_eq!(sent.transfer_name.as_deref(), Some("clip.mov"));
    assert!(sent.path.as_deref().unwrap().contains("Attachments"));
    assert!(sent.is_from_me);
    assert_eq!(sent.sender, None, "from-me rows carry no sender");
    assert!(sent.timestamp.is_some());

    let received = &items[1];
    assert_eq!(received.message_id, 1);
    assert_eq!(received.mime_type.as_deref(), Some("image/jpeg"));
    assert!(!received.is_from_me);
    assert_eq!(received.sender.as_deref(), Some("+15551234567"));
}

#[test]
fn list_attachments_filters_by_chat_and_paginates() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("chat.db");
    build_db(&db_path);

    let reader = ChatReader::open(&db_path).unwrap();

    // Per-chat scoping.
    assert_eq!(reader.list_attachments(Some(1), 50, 0).unwrap().len(), 2);
    assert_eq!(reader.list_attachments(Some(999), 50, 0).unwrap().len(), 0);

    // Pagination: one per page, newest-first.
    let page1 = reader.list_attachments(None, 1, 0).unwrap();
    assert_eq!(page1.len(), 1);
    assert_eq!(page1[0].message_id, 2);
    let page2 = reader.list_attachments(None, 1, 1).unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].message_id, 1);
}
