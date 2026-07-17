//! The indexer: streams decoded messages from a `chat.db` into the index.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use better_im_core::{ChatReader, ScannedMessage};
use regex::Regex;

use crate::db::IndexDb;
use crate::model::{IndexedMessage, SearchOpts, SearchResult, SyncReport};
use crate::query::parse_query;

/// URL detector for the `has:link` flag (computed once at index time).
static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:https?://|www\.)\S+").expect("valid URL regex")
});

/// Builds and maintains the search index for a single source `chat.db`.
///
/// Reading + decoding lives in [`better_im_core::ChatReader`] (which owns the
/// GPL-licensed `imessage-database` dependency); this type is our own code and
/// only orchestrates the write side.
pub struct Indexer {
    source_path: PathBuf,
    db: IndexDb,
}

impl Indexer {
    /// Open the index at `index_path` (creating/migrating it) for the source
    /// `chat.db` at `source_path`.
    ///
    /// # Errors
    /// Returns an error if the index database cannot be opened.
    pub fn open(
        source_path: impl AsRef<Path>,
        index_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let db = IndexDb::open(index_path)?;
        Ok(Self {
            source_path: source_path.as_ref().to_path_buf(),
            db,
        })
    }

    /// Borrow the index database (for search/context without re-opening).
    #[must_use]
    pub fn db(&self) -> &IndexDb {
        &self.db
    }

    /// Rebuild the entire index from the source `chat.db`: one streaming decode
    /// pass over all messages, replacing the index contents.
    ///
    /// # Errors
    /// Returns an error if the source cannot be read or the writes fail.
    pub fn full_reindex(&self) -> anyhow::Result<SyncReport> {
        let reader = ChatReader::open(&self.source_path)?;
        let scanned = reader.scan_messages(None)?;
        let watermark = reader.max_message_rowid()?;
        let records: Vec<IndexedMessage> = scanned.iter().map(to_indexed).collect();

        self.db.clear()?;
        let indexed = self.db.upsert_messages(&records)?;
        self.db.set_watermark(watermark, true)?;

        Ok(SyncReport { indexed, watermark })
    }

    /// Index only messages newer than the stored watermark, advancing it.
    ///
    /// # Errors
    /// Returns an error if the source cannot be read or the writes fail.
    pub fn incremental_sync(&self) -> anyhow::Result<SyncReport> {
        let current = self.db.watermark()?;
        let reader = ChatReader::open(&self.source_path)?;
        let scanned = reader.scan_messages(Some(current))?;
        let records: Vec<IndexedMessage> = scanned.iter().map(to_indexed).collect();

        let indexed = self.db.upsert_messages(&records)?;
        let watermark = records
            .iter()
            .map(|m| m.id)
            .max()
            .map_or(current, |max_id| max_id.max(current));
        self.db.set_watermark(watermark, false)?;

        Ok(SyncReport { indexed, watermark })
    }

    /// Parse `raw` (operators + free text) and run the search.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn search(&self, raw: &str, opts: SearchOpts) -> anyhow::Result<Vec<SearchResult>> {
        self.db.search(&parse_query(raw), opts)
    }

    /// Fetch one indexed message by source `message.ROWID`.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn get_message(&self, id: i64) -> anyhow::Result<Option<IndexedMessage>> {
        self.db.get_message(id)
    }

    /// Fetch conversational context around a message (see
    /// [`IndexDb::message_context`]).
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn message_context(
        &self,
        id: i64,
        before: usize,
        after: usize,
    ) -> anyhow::Result<Vec<IndexedMessage>> {
        self.db.message_context(id, before, after)
    }
}

/// Convert a decoded [`ScannedMessage`] into an index row, computing the
/// `has_link` flag from the body text.
fn to_indexed(scanned: &ScannedMessage) -> IndexedMessage {
    let m = &scanned.message;
    let has_link = m.text.as_deref().is_some_and(|t| URL_RE.is_match(t));
    IndexedMessage {
        id: m.id,
        guid: m.guid.clone(),
        chat_id: m.chat_id,
        canonical_chat_id: scanned.canonical_chat_id,
        chat_identifier: scanned.chat_identifier.clone(),
        chat_name: scanned.chat_name.clone(),
        handle_id: m.handle_id,
        sender: m.sender.clone(),
        is_from_me: m.is_from_me,
        text: m.text.clone(),
        timestamp_millis: m.timestamp.map_or(0, |t| t.timestamp_millis()),
        timestamp: m.timestamp,
        has_attachment: m.num_attachments > 0,
        has_photo: scanned.has_photo,
        has_link,
        service: m.service.clone(),
        msg_type: m.item_type,
    }
}
