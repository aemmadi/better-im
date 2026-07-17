//! End-to-end tests: build a synthetic `chat.db`, index it, and exercise the
//! query engine (keyword ranking + snippets, every operator filter, canonical
//! collapsing, and incremental sync). All fixtures are synthetic — no real
//! `chat.db` is required.

mod common;

use std::path::PathBuf;

use better_im_index::{Indexer, SearchOpts};

/// Build a synthetic source db + a fresh index, run a full reindex, and return
/// the (indexer, tempdir) pair. The tempdir must be kept alive for the db files.
fn indexed() -> (Indexer, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("chat.db");
    let index = dir.path().join("index.db");
    common::build_db(&source);

    let indexer = Indexer::open(&source, &index).expect("open indexer");
    let report = indexer.full_reindex().expect("full reindex");
    assert_eq!(report.indexed, 8, "8 messages indexed");
    assert_eq!(report.watermark, 8, "watermark at max ROWID");
    (indexer, dir)
}

fn search(indexer: &Indexer, query: &str) -> Vec<better_im_index::SearchResult> {
    indexer
        .search(query, SearchOpts::default())
        .expect("search ok")
}

#[test]
fn full_reindex_indexes_every_message() {
    let (indexer, _dir) = indexed();
    assert_eq!(indexer.db().message_count().unwrap(), 8);
}

#[test]
fn keyword_search_is_ranked_with_snippets() {
    let (indexer, _dir) = indexed();
    let hits = search(&indexer, "dinner");
    assert_eq!(hits.len(), 2, "two messages mention dinner");
    for hit in &hits {
        assert!(
            hit.snippet.contains("[dinner]"),
            "snippet should highlight the match: {}",
            hit.snippet
        );
    }
    // Results are BM25-ordered (best/lowest score first).
    assert!(hits[0].score <= hits[1].score);
    let guids: Vec<&str> = hits.iter().map(|h| h.message.guid.as_str()).collect();
    assert!(guids.contains(&"g3") && guids.contains(&"g7"));
}

#[test]
fn attributed_body_text_is_searchable() {
    let (indexer, _dir) = indexed();
    // Message 2's text lives only in the attributedBody blob ("Noter test").
    let hits = search(&indexer, "Noter");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message.guid, "g2");
    assert_eq!(hits[0].message.text.as_deref(), Some("Noter test"));
    assert!(hits[0].message.is_from_me);
}

#[test]
fn operator_from_filters_by_sender() {
    let (indexer, _dir) = indexed();
    let hits = search(&indexer, "from:alice");
    // Messages 1, 3, 7, 8 are received from alice@example.com.
    assert_eq!(hits.len(), 4);
    assert!(hits
        .iter()
        .all(|h| h.message.sender.as_deref().unwrap().contains("alice")));
}

#[test]
fn operator_is_from_me() {
    let (indexer, _dir) = indexed();
    let hits = search(&indexer, "is:from-me");
    assert_eq!(hits.len(), 2, "messages 2 and 4 are from me");
    assert!(hits.iter().all(|h| h.message.is_from_me));
}

#[test]
fn operator_has_link_photo_attachment() {
    let (indexer, _dir) = indexed();

    let links = search(&indexer, "has:link");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].message.guid, "g4");
    assert!(links[0].message.has_link);

    let photos = search(&indexer, "has:photo");
    assert_eq!(photos.len(), 1);
    assert_eq!(photos[0].message.guid, "g5");
    assert!(photos[0].message.has_photo && photos[0].message.has_attachment);

    let attachments = search(&indexer, "has:attachment");
    assert_eq!(attachments.len(), 2, "image + pdf");
    assert!(attachments.iter().all(|h| h.message.has_attachment));
}

#[test]
fn operator_before_and_after_dates() {
    let (indexer, _dir) = indexed();

    let before = search(&indexer, "before:2023-02-01");
    assert_eq!(before.len(), 2, "only the two January messages");

    let after = search(&indexer, "after:2023-06-01");
    // June 20 (x2) + Sept 10; the May 1 duplicate-chat message is excluded.
    assert_eq!(after.len(), 3);

    let window = search(&indexer, "after:2023-03-01 before:2023-04-01");
    assert_eq!(window.len(), 2, "the two March messages");
}

#[test]
fn operator_combined_with_free_text() {
    let (indexer, _dir) = indexed();
    let hits = search(&indexer, "from:alice dinner");
    assert_eq!(hits.len(), 2, "dinner messages, all from alice");
    assert!(hits
        .iter()
        .all(|h| h.message.sender.as_deref().unwrap().contains("alice")));
}

#[test]
fn operator_in_chat_matches_name() {
    let (indexer, _dir) = indexed();
    let hits = search(&indexer, "in:Work");
    // Only the two group-chat messages (5, 6) are in "Work".
    assert_eq!(hits.len(), 2);
    assert!(hits
        .iter()
        .all(|h| h.message.chat_name.as_deref() == Some("Work")));
}

#[test]
fn canonical_collapsing_merges_duplicate_chats() {
    let (indexer, _dir) = indexed();
    // Message 1 (chat 1) and message 8 (chat 2) live in duplicate chats with the
    // same participant set -> same canonical id, different raw chat id.
    let m1 = indexer.get_message(1).unwrap().unwrap();
    let m8 = indexer.get_message(8).unwrap().unwrap();
    assert_ne!(m1.chat_id, m8.chat_id);
    assert_eq!(m1.canonical_chat_id, m8.canonical_chat_id);

    // The group chat is a distinct canonical group.
    let m5 = indexer.get_message(5).unwrap().unwrap();
    assert_ne!(m5.canonical_chat_id, m1.canonical_chat_id);
}

#[test]
fn incremental_sync_picks_up_new_message() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("chat.db");
    let index = dir.path().join("index.db");
    common::build_db(&source);

    let indexer = Indexer::open(&source, &index).unwrap();
    indexer.full_reindex().unwrap();
    assert!(search(&indexer, "incremental").is_empty());

    // A new message arrives after the initial index.
    common::append_message(&source, 9, "g9", "a brand new incremental message", 1);

    let report = indexer.incremental_sync().unwrap();
    assert_eq!(report.indexed, 1, "exactly one new message");
    assert_eq!(report.watermark, 9);

    let hits = search(&indexer, "incremental");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message.guid, "g9");
    assert_eq!(indexer.db().message_count().unwrap(), 9);
}

#[test]
fn incremental_sync_is_noop_when_nothing_new() {
    let (indexer, _dir) = indexed();
    let report = indexer.incremental_sync().unwrap();
    assert_eq!(report.indexed, 0);
    assert_eq!(report.watermark, 8);
}

#[test]
fn message_context_returns_chronological_neighbors() {
    let (indexer, _dir) = indexed();
    // Around message 3 ("dinner ... beach") within its canonical chat.
    let context = indexer.message_context(3, 2, 2).unwrap();
    assert!(context.iter().any(|m| m.id == 3), "target included");
    // Chronological, non-decreasing timestamps.
    for pair in context.windows(2) {
        assert!(pair[0].timestamp_millis <= pair[1].timestamp_millis);
    }
}

#[test]
fn default_index_path_is_under_app_support() {
    // Smoke test the path helper without touching the real chat.db.
    let path: PathBuf = better_im_index::default_index_path().unwrap();
    assert!(path.ends_with("better-im/index.db"));
}
