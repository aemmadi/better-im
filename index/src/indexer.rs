//! The indexer: streams decoded messages from a `chat.db` into the index.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use better_im_core::{ChatReader, ScannedMessage};

use crate::db::IndexDb;
use crate::embeddings::Embedder;
use crate::model::{
    IndexedMessage, SearchOpts, SearchResult, SemanticIndexReport, SemanticProgress, SemanticStatus,
    SyncReport,
};
use crate::query::parse_query;

/// Batch size for embedding backfill (messages embedded per model call).
const EMBED_BATCH: usize = 128;

/// Builds and maintains the search index for a single source `chat.db`.
///
/// Reading + decoding lives in [`better_im_core::ChatReader`] (which owns the
/// GPL-licensed `imessage-database` dependency); this type is our own code and
/// only orchestrates the write side.
///
/// An optional [`Embedder`] powers Phase 5 semantic search. When present and the
/// semantic index has been built at least once, incremental syncs also embed
/// newly-indexed messages so vectors stay current; when absent (or the semantic
/// index is empty) all embedding work is skipped, keeping keyword-only builds
/// cheap.
pub struct Indexer {
    source_path: PathBuf,
    db: IndexDb,
    embedder: Option<Arc<dyn Embedder>>,
}

impl Indexer {
    /// Open the index at `index_path` (creating/migrating it) for the source
    /// `chat.db` at `source_path`. No embedder is attached (keyword search only).
    ///
    /// # Errors
    /// Returns an error if the index database cannot be opened.
    pub fn open(
        source_path: impl AsRef<Path>,
        index_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        Self::open_with_embedder(source_path, index_path, None)
    }

    /// Like [`open`](Self::open), attaching an [`Embedder`] for semantic /
    /// hybrid search and incremental embedding.
    ///
    /// # Errors
    /// Returns an error if the index database cannot be opened.
    pub fn open_with_embedder(
        source_path: impl AsRef<Path>,
        index_path: impl AsRef<Path>,
        embedder: Option<Arc<dyn Embedder>>,
    ) -> anyhow::Result<Self> {
        let db = IndexDb::open(index_path)?;
        Ok(Self {
            source_path: source_path.as_ref().to_path_buf(),
            db,
            embedder,
        })
    }

    /// Borrow the index database (for search/context without re-opening).
    #[must_use]
    pub fn db(&self) -> &IndexDb {
        &self.db
    }

    /// Whether an embedder is attached (semantic search is available).
    #[must_use]
    pub fn has_embedder(&self) -> bool {
        self.embedder.is_some()
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
        self.embed_new_if_enabled()?;

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
        self.embed_new_if_enabled()?;

        Ok(SyncReport { indexed, watermark })
    }

    /// Parse `raw` (operators + free text) and run keyword search (unchanged).
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn search(&self, raw: &str, opts: SearchOpts) -> anyhow::Result<Vec<SearchResult>> {
        self.db.search(&parse_query(raw), opts)
    }

    // ── Phase 5: semantic / hybrid search + embedding backfill ──────────────

    /// Parse `raw`, embed its free text, and run hybrid (keyword + semantic)
    /// search fused with Reciprocal Rank Fusion. Falls back to keyword search
    /// when no embedder is attached or the query has no free text. Operator
    /// filters apply to both arms.
    ///
    /// # Errors
    /// Returns an error if embedding or either search arm fails.
    pub fn smart_search(&self, raw: &str, opts: SearchOpts) -> anyhow::Result<Vec<SearchResult>> {
        let query = parse_query(raw);
        let Some(embedder) = self.embedder.as_ref() else {
            return self.db.search(&query, opts); // no model: keyword only
        };
        // Embed only the free-text terms (operators like `from:` are applied as
        // filters, not embedded). `fts` holds the quoted free-text phrases; strip
        // the FTS quoting to recover plain text for the model.
        let query_vec = match query.fts.as_deref() {
            Some(fts) => {
                let plain = fts.replace('"', " ");
                embedder
                    .embed(std::slice::from_ref(&plain))?
                    .into_iter()
                    .next()
            }
            None => None,
        };
        self.db
            .hybrid_search(query_vec.as_deref(), &query, opts)
    }

    /// Semantic-index health (stored vectors, embeddable messages, model tag).
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn semantic_status(&self) -> anyhow::Result<SemanticStatus> {
        self.db.semantic_status()
    }

    /// Build (or top up) the semantic index: embed every message with text that
    /// lacks a vector, in batches, invoking `progress` after each batch. This is
    /// the opt-in backfill; it may be expensive on a large corpus.
    ///
    /// # Errors
    /// Returns an error if no embedder is attached, or if embedding / writes fail.
    pub fn build_semantic_index<F: FnMut(SemanticProgress)>(
        &self,
        mut progress: F,
    ) -> anyhow::Result<SemanticIndexReport> {
        let embedder = self
            .embedder
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no embedder configured for semantic indexing"))?;

        let total = usize::try_from(self.db.missing_vector_count()?).unwrap_or(usize::MAX);
        let mut done = 0usize;
        progress(SemanticProgress { done, total });

        loop {
            let batch = self.db.messages_missing_vectors(EMBED_BATCH)?;
            if batch.is_empty() {
                break;
            }
            let embedded = embed_and_store(&self.db, embedder.as_ref(), &batch)?;
            done += embedded;
            progress(SemanticProgress { done, total });
            // A short batch means we have drained all missing rows.
            if batch.len() < EMBED_BATCH {
                break;
            }
        }

        Ok(SemanticIndexReport {
            embedded: done,
            total_vectors: self.db.vector_count()?,
        })
    }

    /// After a sync, embed newly-indexed messages — but only when an embedder is
    /// attached *and* the semantic index is already non-empty (i.e. the user has
    /// opted in by building it). No-op otherwise, so keyword-only use pays
    /// nothing.
    fn embed_new_if_enabled(&self) -> anyhow::Result<()> {
        let Some(embedder) = self.embedder.as_ref() else {
            return Ok(());
        };
        if self.db.vector_count()? == 0 {
            return Ok(()); // semantic index not built yet — stay cheap
        }
        loop {
            let batch = self.db.messages_missing_vectors(EMBED_BATCH)?;
            if batch.is_empty() {
                break;
            }
            embed_and_store(&self.db, embedder.as_ref(), &batch)?;
            if batch.len() < EMBED_BATCH {
                break;
            }
        }
        Ok(())
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

/// Embed a batch of `(id, text)` rows and upsert the vectors. Returns the number
/// of vectors written.
fn embed_and_store(
    db: &IndexDb,
    embedder: &dyn Embedder,
    batch: &[(i64, String)],
) -> anyhow::Result<usize> {
    let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
    let vectors = embedder.embed(&texts)?;
    let rows: Vec<(i64, Vec<f32>)> = batch
        .iter()
        .map(|(id, _)| *id)
        .zip(vectors)
        .collect();
    db.upsert_vectors(&rows, &embedder.model_tag())
}

/// Convert a decoded [`ScannedMessage`] into an index row, computing the
/// `has_link` flag from the body text.
fn to_indexed(scanned: &ScannedMessage) -> IndexedMessage {
    let m = &scanned.message;
    let has_link = m.text.as_deref().is_some_and(crate::urls::has_url);
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
