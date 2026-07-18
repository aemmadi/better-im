//! Index database schema (SQLite / FTS5, via `rusqlite`).
//!
//! # Design choices
//!
//! - **Denormalized `messages` table.** One row per iMessage message, keyed by
//!   the source `message.ROWID` (which becomes this table's `rowid`). Decoding
//!   happens once at index time so search never re-decodes `attributedBody`.
//! - **FTS5 in *external-content* mode** (`content='messages'`). The FTS index
//!   references the `messages` table for column text instead of storing its own
//!   copy. Tradeoffs:
//!   - vs **standalone**: no duplicated text storage (text lives once, in
//!     `messages`).
//!   - vs **contentless** (`content=''`): `snippet()`/`highlight()` still work,
//!     because FTS can read the original text back from the content table —
//!     contentless tables return `NULL` for column text and cannot snippet.
//!   - cost: the index must be kept in sync with the content table. We do that
//!     with the standard `AFTER INSERT/DELETE/UPDATE` trigger trio below, so
//!     every write to `messages` (including upserts) maintains the index.
//! - **`sync_state`** holds a single row (`id = 1`) with the last-indexed
//!   `message.ROWID` watermark that drives incremental sync.
//! - **`message_vectors`** (Phase 5) stores one on-device embedding per message,
//!   keyed by `messages.id`. The vector is a little-endian `f32` BLOB plus its
//!   `dim` and a `model` tag (so a model swap is detectable). Search reads these
//!   BLOBs directly and ranks by cosine similarity in Rust (see
//!   [`db::IndexDb::semantic_search`](crate::db::IndexDb::semantic_search)); a
//!   native ANN index (e.g. `sqlite-vec`'s `vec0`) can replace the scan later
//!   without changing this storage. Older indexes created before Phase 5 have a
//!   2-column `(id, embedding)` table; [`db::IndexDb::open`](crate::db::IndexDb)
//!   migrates them by adding the `dim`/`model` columns in place.

/// Full, idempotent schema. Safe to run on every open (all `IF NOT EXISTS`).
pub const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Denormalized message rows. `id` aliases rowid and stores the source
-- `message.ROWID` (stable + unique), which is also the FTS content_rowid.
CREATE TABLE IF NOT EXISTS messages (
    id                INTEGER PRIMARY KEY,
    guid              TEXT NOT NULL,
    chat_id           INTEGER,
    canonical_chat_id INTEGER,
    chat_identifier   TEXT,
    chat_name         TEXT,
    handle_id         INTEGER,
    sender            TEXT,
    text              TEXT,
    ts_millis         INTEGER NOT NULL DEFAULT 0,  -- unix epoch millis (numeric, sortable)
    ts_utc            TEXT,                        -- RFC3339 UTC (lexicographically sortable)
    is_from_me        INTEGER NOT NULL DEFAULT 0,
    has_attachment    INTEGER NOT NULL DEFAULT 0,
    has_photo         INTEGER NOT NULL DEFAULT 0,
    has_link          INTEGER NOT NULL DEFAULT 0,
    service           TEXT,
    msg_type          INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_messages_ts     ON messages(ts_millis);
CREATE INDEX IF NOT EXISTS idx_messages_chat   ON messages(chat_id);
CREATE INDEX IF NOT EXISTS idx_messages_canon  ON messages(canonical_chat_id);
CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender);

-- Full-text index over message body, external-content against `messages`.
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    text,
    content='messages',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

-- Keep the external-content FTS index in sync with `messages`.
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO messages_fts(rowid, text) VALUES (new.id, new.text);
END;

-- Incremental-sync watermark (single row).
CREATE TABLE IF NOT EXISTS sync_state (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    last_rowid           INTEGER NOT NULL DEFAULT 0,
    last_full_reindex_at TEXT,
    last_sync_at         TEXT
);
INSERT OR IGNORE INTO sync_state (id, last_rowid) VALUES (1, 0);

-- Phase 5: one on-device embedding per message (keyed by messages.id).
-- `embedding` is a little-endian f32 BLOB of length `dim`; `model` tags the
-- producing embedder. Fresh indexes get all columns here; pre-Phase-5 indexes
-- (2-column) are upgraded by the ALTER migration in `IndexDb::open`.
CREATE TABLE IF NOT EXISTS message_vectors (
    id        INTEGER PRIMARY KEY,
    embedding BLOB,
    dim       INTEGER,
    model     TEXT
);
"#;
