//! End-to-end proof through `ChatReader`: build a tiny synthetic `chat.db`
//! whose schema `imessage-database` accepts, insert a message whose `text`
//! column is `NULL` and whose content lives only in a real `attributedBody`
//! typedstream blob, then read it back and assert the text is extracted.
//!
//! The synthetic DB is generated at test time (no heavy fixture is vendored);
//! only the small typedstream blob under `tests/fixtures/` is checked in.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use rusqlite::{params, Connection};

use better_im_core::reader::{utc_to_apple_nanos, ChatReader};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()))
}

/// Minimal subset of the iMessage schema the crate's queries touch.
///
/// The `message` table lists the 26 body/metadata columns first (matching the
/// crate's indexed row decoder) followed by the BLOB columns the body parser
/// reads out of band.
const SCHEMA: &str = "
CREATE TABLE handle (
    ROWID INTEGER PRIMARY KEY,
    id TEXT NOT NULL,
    person_centric_id TEXT
);

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

CREATE TABLE chat_message_join (
    chat_id INTEGER,
    message_id INTEGER,
    message_date INTEGER
);

CREATE TABLE message_attachment_join (
    message_id INTEGER,
    attachment_id INTEGER
);

CREATE TABLE chat_handle_join (
    chat_id INTEGER,
    handle_id INTEGER
);
";

/// Build the synthetic database at `path` and return the two message row ids
/// we insert: (plain-text message, NULL-text/attributedBody-only message).
fn build_db(path: &Path) {
    let conn = Connection::open(path).expect("open writable synthetic db");
    conn.execute_batch(SCHEMA).expect("create schema");

    // One handle + one direct chat.
    conn.execute(
        "INSERT INTO handle (ROWID, id, person_centric_id) VALUES (1, '+15551234567', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat (ROWID, chat_identifier, service_name, display_name) \
         VALUES (1, '+15551234567', 'iMessage', 'Alice')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_handle_join (chat_id, handle_id) VALUES (1, 1)",
        [],
    )
    .unwrap();

    let t1 = utc_to_apple_nanos(Utc.with_ymd_and_hms(2023, 1, 1, 12, 0, 0).unwrap());
    let t2 = utc_to_apple_nanos(Utc.with_ymd_and_hms(2023, 1, 1, 12, 5, 0).unwrap());

    // Message 1: ordinary populated `text` column, received from the handle.
    conn.execute(
        "INSERT INTO message (ROWID, guid, text, service, handle_id, date, is_from_me) \
         VALUES (1, 'guid-plain', 'hello from the text column', 'iMessage', 1, ?1, 0)",
        params![t1],
    )
    .unwrap();

    // Message 2: THE important case. `text` is NULL; the content lives only in
    // the real captured `attributedBody` typedstream blob (decodes to
    // "Noter test"). Sent by me.
    let attributed_body = fixture("AttributedBodyTextOnly");
    conn.execute(
        "INSERT INTO message (ROWID, guid, text, service, handle_id, date, is_from_me, attributedBody) \
         VALUES (2, 'guid-attr', NULL, 'iMessage', 1, ?1, 1, ?2)",
        params![t2, attributed_body],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO chat_message_join (chat_id, message_id, message_date) VALUES (1, 1, ?1)",
        params![t1],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chat_message_join (chat_id, message_id, message_date) VALUES (1, 2, ?1)",
        params![t2],
    )
    .unwrap();

    // Ensure everything is flushed before the read-only reader opens the file.
    conn.close().expect("close synthetic db");
}

#[test]
fn reads_thread_and_extracts_null_text_body_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("chat.db");
    build_db(&db_path);

    let reader = ChatReader::open(&db_path).expect("open synthetic db read-only");

    // stats
    let stats = reader.stats().unwrap();
    assert_eq!(stats.chats, 1);
    assert_eq!(stats.messages, 2);
    assert_eq!(stats.attachments, 0);

    // conversations
    let conversations = reader.list_conversations().unwrap();
    assert_eq!(conversations.len(), 1);
    let convo = &conversations[0];
    assert_eq!(convo.id, 1);
    assert_eq!(convo.label(), "Alice");
    assert_eq!(convo.participants, vec!["+15551234567".to_string()]);

    // thread (oldest-first)
    let messages = reader.get_thread(1, 50, None).unwrap();
    assert_eq!(messages.len(), 2);

    let plain = &messages[0];
    assert_eq!(plain.text.as_deref(), Some("hello from the text column"));
    assert!(!plain.is_from_me);
    assert_eq!(plain.sender.as_deref(), Some("+15551234567"));

    // The critical assertion: text recovered from `attributedBody` when the
    // `text` column was NULL.
    let attr = &messages[1];
    assert_eq!(attr.text.as_deref(), Some("Noter test"));
    assert!(attr.is_from_me);
    assert_eq!(attr.sender, None);
    assert!(attr.timestamp.is_some());
}

#[test]
fn pagination_before_cursor_limits_results() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("chat.db");
    build_db(&db_path);

    let reader = ChatReader::open(&db_path).unwrap();

    // Only the first message occurs strictly before 12:02.
    let cursor = Utc.with_ymd_and_hms(2023, 1, 1, 12, 2, 0).unwrap();
    let messages = reader.get_thread(1, 50, Some(cursor)).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].guid, "guid-plain");

    // limit caps the newest page.
    let limited = reader.get_thread(1, 1, None).unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].guid, "guid-attr");
}
