//! Live-watcher integration test: a write to the source `chat.db` should, after
//! debounce, trigger an incremental sync and surface an "index updated" signal.
//!
//! This drives real file-system (FSEvents) notifications, so it uses a generous
//! timeout. The debounce/dispatch logic itself is unit-tested deterministically
//! in `watcher.rs`.

mod common;

use std::time::Duration;

use better_im_index::{watcher, IndexDb, Indexer};

#[test]
fn file_change_triggers_incremental_sync() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("chat.db");
    let index = dir.path().join("index.db");
    common::build_db(&source);

    // Initial full index.
    let indexer = Indexer::open(&source, &index).unwrap();
    indexer.full_reindex().unwrap();

    // Start watching (the watcher takes ownership of this indexer + its own
    // connection). A second connection is used below to read results.
    let (_watch, rx) =
        watcher::watch_channel(indexer, &source, Duration::from_millis(200)).unwrap();

    // A new message lands on disk.
    common::append_message(&source, 9, "g9", "watcher saw this new message", 1);

    // Wait for the debounced sync to report back.
    let report = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("watcher should report an index update within the timeout");
    assert!(report.indexed >= 1, "at least the new message was indexed");
    assert_eq!(report.watermark, 9);

    // Verify via an independent reader connection to the same index file.
    let reader = IndexDb::open(&index).unwrap();
    let hits = reader
        .search(&better_im_index::parse_query("watcher"), Default::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message.guid, "g9");
}
