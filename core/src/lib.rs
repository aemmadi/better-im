//! better-im core (Phase 0): headless, read-only iMessage `chat.db` reader.
//!
//! This crate wraps [`imessage-database`](https://crates.io/crates/imessage-database)
//! to solve the #1 correctness problem before any UI exists: reliably
//! extracting message body text, including messages whose `text` column is
//! `NULL` and whose real content lives in the `attributedBody` typedstream blob.
//!
//! - [`models`] — source-agnostic domain types (`Conversation`, `Message`,
//!   `Handle`, `Attachment`) plus the [`models::MessageActionProvider`]
//!   extensibility seam for a future send layer.
//! - [`reader`] — [`reader::ChatReader`], which opens `chat.db` read-only and
//!   exposes conversations, threads, and aggregate stats.
//!
//! ```no_run
//! use better_im_core::reader::ChatReader;
//!
//! let reader = ChatReader::open("/path/to/chat.db")?;
//! for convo in reader.list_conversations()? {
//!     println!("{}: {}", convo.id, convo.label());
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

pub mod models;
pub mod reader;

pub use models::{
    Attachment, Capability, Conversation, Handle, Message, MessageActionProvider, ReadOnlyProvider,
};
pub use reader::{apple_time_to_utc, ChatReader, ScannedMessage, Stats};
