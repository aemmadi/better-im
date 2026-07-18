//! better-im index (Phase 1): a headless search index + query engine over an
//! iMessage `chat.db`.
//!
//! Reading and text decoding live in [`better_im_core`] (which owns the
//! GPL-licensed `imessage-database` dependency). This crate is our own code: it
//! builds a denormalized, full-text-searchable index and answers ranked queries
//! with an operator mini-language, so search never re-decodes messages.
//!
//! ## Storage & the libSQL-vs-rusqlite decision
//!
//! The index is a standard **SQLite** database written through `rusqlite` (with
//! the *bundled* SQLite that also carries FTS5). Full-text search uses FTS5 in
//! external-content mode; see [`schema`].
//!
//! We initially preferred the `libsql` crate (for its native `F32_BLOB` vector
//! search, so Phase 5 would need no migration), and verified it works. But
//! `libsql` **bundles its own copy of SQLite**, which collides at link time with
//! the bundled SQLite that `core` already pulls in through `imessage-database`'s
//! `rusqlite` (duplicate `sqlite3_*` symbols; two SQLite engines in one binary).
//! That is genuine integration friction, so we use `rusqlite` here to keep a
//! single SQLite in the workspace.
//!
//! This costs us nothing for Phase 5: the index is an ordinary SQLite file, and
//! libSQL opens existing SQLite databases in place. Phase 5 can point libSQL at
//! this same `index.db`, migrate the reserved [`message_vectors`](schema) table
//! to an `F32_BLOB` column, and build a vector index — no data migration, no
//! re-index.
//!
//! ## Typical use
//!
//! ```no_run
//! use better_im_index::{Indexer, SearchOpts};
//!
//! # fn run() -> anyhow::Result<()> {
//! let indexer = Indexer::open("/path/to/chat.db", "/path/to/index.db")?;
//! indexer.full_reindex()?;
//! let hits = indexer.search("from:alice dinner after:2023-01-01", SearchOpts::default())?;
//! for hit in hits {
//!     println!("{:.3}  {}", hit.score, hit.snippet);
//! }
//! # Ok(())
//! # }
//! ```

pub mod db;
pub mod indexer;
pub mod model;
pub mod paths;
pub mod query;
pub mod schema;
pub mod urls;
pub mod watcher;

pub use db::IndexDb;
pub use indexer::Indexer;
pub use model::{
    ContactCount, DayCount, HourCount, IndexedMessage, InsightsData, LinkRow, SearchOpts,
    SearchResult, SyncReport,
};
pub use paths::default_index_path;
pub use query::{parse_query, Filters, ParsedQuery};
pub use urls::{extract_urls, has_url};
pub use watcher::{watch, watch_channel, IndexWatcher, DEFAULT_DEBOUNCE};
