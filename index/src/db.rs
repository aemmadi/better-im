//! SQLite/FTS5-backed index database (via `rusqlite`): schema, writes, and
//! read/search queries.

use std::path::Path;

use anyhow::Context;
use chrono::Utc;
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection, Row};

use crate::embeddings::{cosine_similarity, decode_vector, encode_vector};
use crate::model::{
    ContactCount, DayCount, HourCount, IndexedMessage, InsightsData, LinkRow, SearchOpts,
    SearchResult, SemanticStatus,
};
use crate::query::{Filters, ParsedQuery};
use crate::schema::SCHEMA;

/// Reciprocal Rank Fusion constant. The standard value from the RRF paper;
/// larger `k` flattens the contribution of rank differences.
const RRF_K: f64 = 60.0;

/// Per-arm candidate pool size for hybrid fusion. Each arm (FTS + vector)
/// contributes up to this many ranked hits before fusion; large enough that a
/// message ranked well by one arm is not dropped before it can be fused.
const RRF_POOL: usize = 200;

/// The message columns selected (in order) for row decoding. Indices 0..=16.
const MESSAGE_SELECT: &str = "\
    m.id, m.guid, m.chat_id, m.canonical_chat_id, m.chat_identifier, m.chat_name, \
    m.handle_id, m.sender, m.is_from_me, m.text, m.ts_millis, m.ts_utc, \
    m.has_attachment, m.has_photo, m.has_link, m.service, m.msg_type";

/// Upsert statement for the denormalized `messages` table (17 bound params).
const UPSERT_SQL: &str = "\
    INSERT INTO messages \
      (id, guid, chat_id, canonical_chat_id, chat_identifier, chat_name, handle_id, sender, text, \
       ts_millis, ts_utc, is_from_me, has_attachment, has_photo, has_link, service, msg_type) \
    VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17) \
    ON CONFLICT(id) DO UPDATE SET \
      guid=excluded.guid, chat_id=excluded.chat_id, canonical_chat_id=excluded.canonical_chat_id, \
      chat_identifier=excluded.chat_identifier, chat_name=excluded.chat_name, \
      handle_id=excluded.handle_id, sender=excluded.sender, text=excluded.text, \
      ts_millis=excluded.ts_millis, ts_utc=excluded.ts_utc, is_from_me=excluded.is_from_me, \
      has_attachment=excluded.has_attachment, has_photo=excluded.has_photo, \
      has_link=excluded.has_link, service=excluded.service, msg_type=excluded.msg_type";

/// A handle to the index database (a standard SQLite file with an FTS5 index).
pub struct IndexDb {
    conn: Connection,
}

impl IndexDb {
    /// Open (creating if needed) the index database at `path` and apply the
    /// schema. Idempotent — safe to call on an existing index.
    ///
    /// # Errors
    /// Returns an error if the database cannot be opened or the schema fails.
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let conn = Connection::open(path.as_ref())
            .with_context(|| format!("opening index db at {}", path.as_ref().display()))?;
        conn.execute_batch(SCHEMA).context("applying index schema")?;
        migrate_message_vectors(&conn).context("migrating message_vectors table")?;
        Ok(Self { conn })
    }

    /// Borrow the underlying connection (escape hatch for Phase 5 vector queries
    /// and ad-hoc reads).
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Remove every indexed message (and, via triggers, its FTS entries). Used
    /// by a full reindex before repopulating.
    ///
    /// # Errors
    /// Returns an error if the delete fails.
    pub fn clear(&self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM messages", [])?;
        Ok(())
    }

    /// Upsert a batch of messages in one transaction. Returns the number of rows
    /// written.
    ///
    /// # Errors
    /// Returns an error if any statement fails; the transaction is rolled back.
    pub fn upsert_messages(&self, messages: &[IndexedMessage]) -> anyhow::Result<usize> {
        if messages.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(UPSERT_SQL)?;
            for msg in messages {
                stmt.execute(params_from_iter(upsert_params(msg)))?;
            }
        }
        tx.commit()?;
        Ok(messages.len())
    }

    /// Current incremental-sync watermark (highest indexed `message.ROWID`).
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn watermark(&self) -> anyhow::Result<i64> {
        let v: i64 = self.conn.query_row(
            "SELECT last_rowid FROM sync_state WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(v)
    }

    /// Record a new watermark. When `full` is set, also stamps the
    /// last-full-reindex time; always stamps the last-sync time.
    ///
    /// # Errors
    /// Returns an error if the update fails.
    pub fn set_watermark(&self, watermark: i64, full: bool) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        if full {
            self.conn.execute(
                "UPDATE sync_state SET last_rowid = ?1, last_full_reindex_at = ?2, last_sync_at = ?2 WHERE id = 1",
                rusqlite::params![watermark, now],
            )?;
        } else {
            self.conn.execute(
                "UPDATE sync_state SET last_rowid = ?1, last_sync_at = ?2 WHERE id = 1",
                rusqlite::params![watermark, now],
            )?;
        }
        Ok(())
    }

    /// Total number of indexed messages.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn message_count(&self) -> anyhow::Result<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;
        Ok(n)
    }

    /// Run a parsed query: FTS5 BM25 ranking when there is free text, otherwise a
    /// chronological filter-only listing. Filters are applied as SQL `WHERE`
    /// clauses in both paths.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn search(
        &self,
        query: &ParsedQuery,
        opts: SearchOpts,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let (filter_sql, filter_params) = build_filter_clauses(&query.filters);

        let (sql, params) = if let Some(ref fts) = query.fts {
            let sql = format!(
                "SELECT {cols}, \
                     snippet(messages_fts, 0, '[', ']', '…', 12) AS snip, \
                     bm25(messages_fts) AS score \
                 FROM messages_fts \
                 JOIN messages m ON m.id = messages_fts.rowid \
                 WHERE messages_fts MATCH ?1 {filters} \
                 ORDER BY score \
                 LIMIT ? OFFSET ?",
                cols = MESSAGE_SELECT,
                filters = filter_sql,
            );
            let mut params = vec![Value::Text(fts.clone())];
            params.extend(filter_params);
            params.push(Value::Integer(opts.limit as i64));
            params.push(Value::Integer(opts.offset as i64));
            (sql, params)
        } else {
            let sql = format!(
                "SELECT {cols}, \
                     substr(COALESCE(m.text, ''), 1, 160) AS snip, \
                     0.0 AS score \
                 FROM messages m \
                 WHERE 1 = 1 {filters} \
                 ORDER BY m.ts_millis DESC \
                 LIMIT ? OFFSET ?",
                cols = MESSAGE_SELECT,
                filters = filter_sql,
            );
            let mut params = filter_params;
            params.push(Value::Integer(opts.limit as i64));
            params.push(Value::Integer(opts.offset as i64));
            (sql, params)
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            let message = row_to_message(row)?;
            let snippet: Option<String> = row.get(17)?;
            let score: f64 = row.get(18)?;
            Ok(SearchResult {
                message,
                snippet: snippet.unwrap_or_default(),
                score,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ── Phase 5: vector storage + semantic / hybrid search ──────────────────

    /// Upsert a batch of `(message_id, embedding)` vectors, tagging each with
    /// `model`. The embedding is stored as a little-endian `f32` BLOB plus its
    /// dimensionality. Returns the number of rows written.
    ///
    /// # Errors
    /// Returns an error if any statement fails; the transaction is rolled back.
    pub fn upsert_vectors(
        &self,
        vectors: &[(i64, Vec<f32>)],
        model: &str,
    ) -> anyhow::Result<usize> {
        if vectors.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO message_vectors (id, embedding, dim, model) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(id) DO UPDATE SET \
                   embedding = excluded.embedding, dim = excluded.dim, model = excluded.model",
            )?;
            for (id, vec) in vectors {
                stmt.execute(rusqlite::params![
                    id,
                    encode_vector(vec),
                    vec.len() as i64,
                    model
                ])?;
            }
        }
        tx.commit()?;
        Ok(vectors.len())
    }

    /// Number of stored embedding vectors.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn vector_count(&self) -> anyhow::Result<i64> {
        let n: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM message_vectors", [], |row| row.get(0))?;
        Ok(n)
    }

    /// Count of messages with embeddable text that still lack a vector (the
    /// remaining backfill work). Cheaper than fetching the rows just to count.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn missing_vector_count(&self) -> anyhow::Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages m \
             LEFT JOIN message_vectors v ON v.id = m.id \
             WHERE v.id IS NULL AND m.text IS NOT NULL AND m.text <> ''",
            [],
            |row| row.get(0),
        )?;
        Ok(n)
    }

    /// Semantic-index health: stored vectors, embeddable messages, and the model
    /// tag of the stored vectors (if any).
    ///
    /// # Errors
    /// Returns an error if any query fails.
    pub fn semantic_status(&self) -> anyhow::Result<SemanticStatus> {
        let vector_count = self.vector_count()?;
        let embeddable_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE text IS NOT NULL AND text <> ''",
            [],
            |row| row.get(0),
        )?;
        let model: Option<String> = self
            .conn
            .query_row(
                "SELECT model FROM message_vectors WHERE model IS NOT NULL LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(SemanticStatus {
            vector_count,
            embeddable_count,
            model,
        })
    }

    /// Fetch up to `limit` messages that have embeddable text but no stored
    /// vector yet, as `(id, text)` pairs (oldest source ROWID first). Drives the
    /// backfill: embed these, upsert, repeat until empty.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn messages_missing_vectors(&self, limit: usize) -> anyhow::Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.text FROM messages m \
             LEFT JOIN message_vectors v ON v.id = m.id \
             WHERE v.id IS NULL AND m.text IS NOT NULL AND m.text <> '' \
             ORDER BY m.id ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Rank stored vectors by cosine similarity to `query_vec`, applying the same
    /// operator [`Filters`] as keyword search, and return the top `limit` as
    /// `(message_id, similarity)` (best first).
    ///
    /// This is an exact brute-force KNN over the stored BLOBs (see the crate docs
    /// for the sqlite-vec-vs-in-Rust rationale). Only vectors whose dimensionality
    /// matches the query are considered.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn semantic_search(
        &self,
        query_vec: &[f32],
        filters: &Filters,
        limit: usize,
    ) -> anyhow::Result<Vec<(i64, f32)>> {
        let (filter_sql, filter_params) = build_filter_clauses(filters);
        let sql = format!(
            "SELECT v.id, v.embedding FROM message_vectors v \
             JOIN messages m ON m.id = v.id \
             WHERE v.embedding IS NOT NULL {filters}",
            filters = filter_sql,
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(filter_params), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;

        let mut scored: Vec<(i64, f32)> = Vec::new();
        for r in rows {
            let (id, blob) = r?;
            let vec = decode_vector(&blob);
            if vec.len() != query_vec.len() {
                continue; // dimensionality mismatch (e.g. a different model)
            }
            scored.push((id, cosine_similarity(query_vec, &vec)));
        }
        // Best (highest) similarity first; stable tiebreak on id.
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        scored.truncate(limit);
        Ok(scored)
    }

    /// Hybrid search: fuse keyword (FTS/BM25) and semantic (vector) rankings with
    /// Reciprocal Rank Fusion, returning results in the shared [`SearchResult`]
    /// shape (so the UI reuses it). Operator filters are applied to **both** arms.
    ///
    /// `query_vec` is the embedding of the query's free text. When there is no
    /// free text to match (filters-only query) or no query vector, this degrades
    /// to plain keyword/filter search. When the semantic index is empty, the
    /// vector arm contributes nothing and results equal FTS ranking.
    ///
    /// The returned [`SearchResult::score`] is the fused RRF score (higher is
    /// better), unlike the raw BM25 score of [`search`](Self::search).
    ///
    /// # Errors
    /// Returns an error if either arm's query fails.
    pub fn hybrid_search(
        &self,
        query_vec: Option<&[f32]>,
        query: &ParsedQuery,
        opts: SearchOpts,
    ) -> anyhow::Result<Vec<SearchResult>> {
        // Without free text (or a query vector) there is nothing to embed against;
        // fall back to the plain keyword/filter path.
        let (Some(vec), true) = (query_vec, query.fts.is_some()) else {
            return self.search(query, opts);
        };

        let pool = opts.offset.saturating_add(opts.limit).max(RRF_POOL);
        let fts_hits = self.search(
            query,
            SearchOpts {
                limit: pool,
                offset: 0,
            },
        )?;
        let vec_hits = self.semantic_search(vec, &query.filters, pool)?;

        // Fuse: RRF score per message id, plus a representative row per id. The
        // FTS hit is preferred as the representative (it carries a highlighted
        // snippet); a vector-only hit falls back to a plain text snippet.
        let mut fused: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
        let mut rep: std::collections::HashMap<i64, SearchResult> = std::collections::HashMap::new();

        for (rank, hit) in fts_hits.iter().enumerate() {
            let id = hit.message.id;
            *fused.entry(id).or_insert(0.0) += rrf_contribution(rank);
            rep.entry(id).or_insert_with(|| hit.clone());
        }
        for (rank, (id, _sim)) in vec_hits.iter().enumerate() {
            *fused.entry(*id).or_insert(0.0) += rrf_contribution(rank);
            if !rep.contains_key(id) {
                if let Some(msg) = self.get_message(*id)? {
                    let snippet = plain_snippet(msg.text.as_deref());
                    rep.insert(
                        *id,
                        SearchResult {
                            message: msg,
                            snippet,
                            score: 0.0,
                        },
                    );
                }
            }
        }

        // Order by fused score (desc), stable tiebreak on id (asc).
        let mut ranked: Vec<(i64, f64)> = fused.into_iter().collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });

        let out = ranked
            .into_iter()
            .skip(opts.offset)
            .take(opts.limit)
            .filter_map(|(id, score)| {
                rep.remove(&id).map(|mut r| {
                    r.score = score;
                    r
                })
            })
            .collect();
        Ok(out)
    }

    /// Fetch a single indexed message by source `message.ROWID`.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn get_message(&self, id: i64) -> anyhow::Result<Option<IndexedMessage>> {
        let sql = format!("SELECT {MESSAGE_SELECT} FROM messages m WHERE m.id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(rusqlite::params![id], row_to_message)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Fetch conversational context around a message: up to `before` messages
    /// preceding it and `after` following it within the same canonical chat,
    /// returned in chronological order (the target included).
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn message_context(
        &self,
        id: i64,
        before: usize,
        after: usize,
    ) -> anyhow::Result<Vec<IndexedMessage>> {
        let Some(target) = self.get_message(id)? else {
            return Ok(Vec::new());
        };
        // Prefer the canonical chat grouping; fall back to raw chat id.
        let (chat_col, chat_val) = match target.canonical_chat_id {
            Some(c) => ("canonical_chat_id", c),
            None => match target.chat_id {
                Some(c) => ("chat_id", c),
                None => return Ok(vec![target]),
            },
        };

        let preceding_sql = format!(
            "SELECT {MESSAGE_SELECT} FROM messages m \
             WHERE m.{chat_col} = ?1 AND (m.ts_millis < ?2 OR (m.ts_millis = ?2 AND m.id < ?3)) \
             ORDER BY m.ts_millis DESC, m.id DESC LIMIT ?4"
        );
        let following_sql = format!(
            "SELECT {MESSAGE_SELECT} FROM messages m \
             WHERE m.{chat_col} = ?1 AND (m.ts_millis > ?2 OR (m.ts_millis = ?2 AND m.id > ?3)) \
             ORDER BY m.ts_millis ASC, m.id ASC LIMIT ?4"
        );

        let mut preceding = self.collect_messages(
            &preceding_sql,
            rusqlite::params![chat_val, target.timestamp_millis, target.id, before as i64],
        )?;
        preceding.reverse();

        let following = self.collect_messages(
            &following_sql,
            rusqlite::params![chat_val, target.timestamp_millis, target.id, after as i64],
        )?;

        let mut out = preceding;
        out.push(target);
        out.extend(following);
        Ok(out)
    }

    /// Run a `SELECT {MESSAGE_SELECT} ...` query and decode all rows.
    fn collect_messages(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> anyhow::Result<Vec<IndexedMessage>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, row_to_message)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// List shared links, newest-first, one [`LinkRow`] per URL. `chat_id = None`
    /// spans all conversations; `limit`/`offset` paginate at **URL** granularity.
    ///
    /// Every `has_link` message yields at least one URL (the same regex sets the
    /// flag and extracts here), so fetching the first `offset + limit` such
    /// messages is enough to satisfy any URL window without scanning the corpus.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn list_links(
        &self,
        chat_id: Option<i64>,
        limit: usize,
        offset: usize,
    ) -> anyhow::Result<Vec<LinkRow>> {
        let mut sql = String::from(
            "SELECT m.id, m.chat_id, m.text, m.ts_millis, m.sender, m.is_from_me, m.chat_name \
             FROM messages m \
             WHERE m.has_link = 1 AND m.text IS NOT NULL",
        );
        let mut params: Vec<Value> = Vec::new();
        if let Some(cid) = chat_id {
            sql.push_str(" AND m.chat_id = ?");
            params.push(Value::Integer(cid));
        }
        sql.push_str(" ORDER BY m.ts_millis DESC, m.id DESC LIMIT ?");
        // Over-fetch enough messages to cover the requested URL window.
        params.push(Value::Integer(offset.saturating_add(limit) as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)? != 0,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;

        let mut out: Vec<LinkRow> = Vec::new();
        for row in rows {
            let (id, chat_id, text, ts_millis, sender, is_from_me, chat_name) = row?;
            let Some(text) = text else { continue };
            for url in crate::urls::extract_urls(&text) {
                out.push(LinkRow {
                    message_id: id,
                    chat_id,
                    url,
                    timestamp: IndexedMessage::datetime_from_millis(ts_millis),
                    sender: sender.clone(),
                    is_from_me,
                    chat_name: chat_name.clone(),
                });
            }
        }
        // URL-level pagination over the flattened, newest-first list.
        Ok(out.into_iter().skip(offset).take(limit).collect())
    }

    /// Merged newest-first feed across every conversation, keyset-paginated on an
    /// exclusive `before_millis` cursor (`None` starts at the most recent).
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn timeline(
        &self,
        before_millis: Option<i64>,
        limit: usize,
    ) -> anyhow::Result<Vec<IndexedMessage>> {
        let mut sql = format!("SELECT {MESSAGE_SELECT} FROM messages m WHERE m.ts_millis > 0");
        let mut params: Vec<Value> = Vec::new();
        if let Some(before) = before_millis {
            sql.push_str(" AND m.ts_millis < ?");
            params.push(Value::Integer(before));
        }
        sql.push_str(" ORDER BY m.ts_millis DESC, m.id DESC LIMIT ?");
        params.push(Value::Integer(limit as i64));
        self.collect_messages(&sql, params_from_iter(params))
    }

    /// Aggregate stats for one conversation (`chat_id = Some`) or the whole
    /// corpus (`None`): totals, sent/received, first/last timestamp, per-day and
    /// per-hour histograms (local time), and the top inbound correspondents.
    ///
    /// # Errors
    /// Returns an error if any aggregate query fails.
    pub fn insights(&self, chat_id: Option<i64>) -> anyhow::Result<InsightsData> {
        // Optional `AND m.chat_id = ?1` shared by every aggregate below.
        let (chat_clause, chat_param): (&str, Vec<Value>) = match chat_id {
            Some(cid) => (" AND m.chat_id = ?1", vec![Value::Integer(cid)]),
            None => ("", Vec::new()),
        };

        // Totals + sent/received + min/max timestamp in a single scan.
        let totals_sql = format!(
            "SELECT COUNT(*), \
                    COALESCE(SUM(CASE WHEN m.is_from_me = 1 THEN 1 ELSE 0 END), 0), \
                    COALESCE(SUM(CASE WHEN m.is_from_me = 0 THEN 1 ELSE 0 END), 0), \
                    MIN(CASE WHEN m.ts_millis > 0 THEN m.ts_millis END), \
                    MAX(CASE WHEN m.ts_millis > 0 THEN m.ts_millis END) \
             FROM messages m WHERE 1 = 1{chat_clause}"
        );
        let (total_messages, sent_count, received_count, min_ts, max_ts) = self.conn.query_row(
            &totals_sql,
            params_from_iter(chat_param.iter().cloned()),
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )?;

        // Per-day histogram (local calendar day).
        let by_day_sql = format!(
            "SELECT strftime('%Y-%m-%d', m.ts_millis / 1000, 'unixepoch', 'localtime') AS day, \
                    COUNT(*) \
             FROM messages m WHERE m.ts_millis > 0{chat_clause} \
             GROUP BY day ORDER BY day"
        );
        let mut by_day_stmt = self.conn.prepare(&by_day_sql)?;
        let by_day = by_day_stmt
            .query_map(params_from_iter(chat_param.iter().cloned()), |row| {
                Ok(DayCount {
                    date: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // Per-hour histogram (local hour, 0..=23).
        let by_hour_sql = format!(
            "SELECT CAST(strftime('%H', m.ts_millis / 1000, 'unixepoch', 'localtime') AS INTEGER) \
                    AS hour, COUNT(*) \
             FROM messages m WHERE m.ts_millis > 0{chat_clause} \
             GROUP BY hour ORDER BY hour"
        );
        let mut by_hour_stmt = self.conn.prepare(&by_hour_sql)?;
        let by_hour = by_hour_stmt
            .query_map(params_from_iter(chat_param.iter().cloned()), |row| {
                Ok(HourCount {
                    hour: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // Top inbound correspondents.
        let top_sql = format!(
            "SELECT m.sender, COUNT(*) AS c \
             FROM messages m \
             WHERE m.is_from_me = 0 AND m.sender IS NOT NULL{chat_clause} \
             GROUP BY m.sender ORDER BY c DESC, m.sender ASC LIMIT 10"
        );
        let mut top_stmt = self.conn.prepare(&top_sql)?;
        let top_contacts = top_stmt
            .query_map(params_from_iter(chat_param.iter().cloned()), |row| {
                Ok(ContactCount {
                    handle: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(InsightsData {
            total_messages,
            sent_count,
            received_count,
            first_message: min_ts.and_then(IndexedMessage::datetime_from_millis),
            last_message: max_ts.and_then(IndexedMessage::datetime_from_millis),
            by_day,
            by_hour,
            top_contacts,
        })
    }
}

/// Build the params for one upsert (matches `UPSERT_SQL` order).
fn upsert_params(m: &IndexedMessage) -> Vec<Value> {
    vec![
        Value::Integer(m.id),
        Value::Text(m.guid.clone()),
        opt_i64(m.chat_id),
        opt_i64(m.canonical_chat_id),
        opt_str(&m.chat_identifier),
        opt_str(&m.chat_name),
        opt_i64(m.handle_id),
        opt_str(&m.sender),
        opt_str(&m.text),
        Value::Integer(m.timestamp_millis),
        m.timestamp
            .map(|t| Value::Text(t.to_rfc3339()))
            .unwrap_or(Value::Null),
        Value::Integer(i64::from(m.is_from_me)),
        Value::Integer(i64::from(m.has_attachment)),
        Value::Integer(i64::from(m.has_photo)),
        Value::Integer(i64::from(m.has_link)),
        opt_str(&m.service),
        Value::Integer(i64::from(m.msg_type)),
    ]
}

/// Build ` AND ...` filter SQL and its bound params from [`Filters`].
fn build_filter_clauses(filters: &Filters) -> (String, Vec<Value>) {
    let mut sql = String::new();
    let mut params: Vec<Value> = Vec::new();

    for who in &filters.from {
        sql.push_str(" AND m.sender IS NOT NULL AND lower(m.sender) LIKE ?");
        params.push(Value::Text(contains(who)));
    }
    for chat in &filters.in_chat {
        sql.push_str(
            " AND (lower(COALESCE(m.chat_name,'')) LIKE ? \
                   OR lower(COALESCE(m.chat_identifier,'')) LIKE ? \
                   OR m.chat_id = ? OR m.canonical_chat_id = ?)",
        );
        let like = contains(chat);
        let numeric = chat.parse::<i64>().unwrap_or(i64::MIN);
        params.push(Value::Text(like.clone()));
        params.push(Value::Text(like));
        params.push(Value::Integer(numeric));
        params.push(Value::Integer(numeric));
    }
    if let Some(before) = filters.before {
        sql.push_str(" AND m.ts_millis > 0 AND m.ts_millis < ?");
        params.push(Value::Integer(before));
    }
    if let Some(after) = filters.after {
        sql.push_str(" AND m.ts_millis >= ?");
        params.push(Value::Integer(after));
    }
    if filters.has_attachment {
        sql.push_str(" AND m.has_attachment = 1");
    }
    if filters.has_photo {
        sql.push_str(" AND m.has_photo = 1");
    }
    if filters.has_link {
        sql.push_str(" AND m.has_link = 1");
    }
    if let Some(from_me) = filters.is_from_me {
        sql.push_str(" AND m.is_from_me = ?");
        params.push(Value::Integer(i64::from(from_me)));
    }

    (sql, params)
}

/// Decode the leading 17 columns (`MESSAGE_SELECT`) of a row into a message.
fn row_to_message(row: &Row) -> rusqlite::Result<IndexedMessage> {
    let timestamp_millis: i64 = row.get(10)?;
    Ok(IndexedMessage {
        id: row.get(0)?,
        guid: row.get(1)?,
        chat_id: row.get(2)?,
        canonical_chat_id: row.get(3)?,
        chat_identifier: row.get(4)?,
        chat_name: row.get(5)?,
        handle_id: row.get(6)?,
        sender: row.get(7)?,
        is_from_me: row.get::<_, i64>(8)? != 0,
        text: row.get(9)?,
        timestamp_millis,
        timestamp: IndexedMessage::datetime_from_millis(timestamp_millis),
        has_attachment: row.get::<_, i64>(12)? != 0,
        has_photo: row.get::<_, i64>(13)? != 0,
        has_link: row.get::<_, i64>(14)? != 0,
        service: row.get(15)?,
        msg_type: row.get::<_, i64>(16)? as i32,
    })
}

fn opt_str(o: &Option<String>) -> Value {
    o.as_ref()
        .map(|s| Value::Text(s.clone()))
        .unwrap_or(Value::Null)
}

fn opt_i64(o: Option<i64>) -> Value {
    o.map(Value::Integer).unwrap_or(Value::Null)
}

/// `%value%` (lowercased) for a case-insensitive `LIKE` contains-match.
fn contains(value: &str) -> String {
    format!("%{}%", value.to_lowercase())
}

/// One arm's Reciprocal Rank Fusion contribution for a hit at 0-based `rank`:
/// `1 / (RRF_K + rank + 1)`.
fn rrf_contribution(rank: usize) -> f64 {
    1.0 / (RRF_K + (rank as f64) + 1.0)
}

/// A plain, unhighlighted snippet (first ~160 chars) for a vector-only hit that
/// has no FTS `snippet()` output. Mirrors the filter-only search path.
fn plain_snippet(text: Option<&str>) -> String {
    let text = text.unwrap_or("");
    let truncated: String = text.chars().take(160).collect();
    truncated
}

/// Bring a `message_vectors` table created before Phase 5 up to the current
/// shape by adding the `dim` / `model` columns when missing. Idempotent: fresh
/// indexes already have both columns (from `SCHEMA`), so nothing is altered.
fn migrate_message_vectors(conn: &Connection) -> anyhow::Result<()> {
    let mut existing: Vec<String> = Vec::new();
    {
        let mut stmt = conn.prepare("PRAGMA table_info(message_vectors)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for r in rows {
            existing.push(r?);
        }
    }
    if !existing.iter().any(|c| c == "dim") {
        conn.execute("ALTER TABLE message_vectors ADD COLUMN dim INTEGER", [])?;
    }
    if !existing.iter().any(|c| c == "model") {
        conn.execute("ALTER TABLE message_vectors ADD COLUMN model TEXT", [])?;
    }
    Ok(())
}
