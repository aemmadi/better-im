//! `better-im-index`: headless CLI to build and query the search index.
//!
//! ```text
//! better-im-index index                    # build / update the index (incremental)
//! better-im-index reindex                   # full rebuild from the source chat.db
//! better-im-index search "from:alice pizza after:2023-01-01"
//! better-im-index --db /path/chat.db --index /path/index.db search "hello"
//! ```

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};

use better_im_index::{default_index_path, Indexer, SearchOpts};

#[derive(Parser)]
#[command(
    name = "better-im-index",
    about = "Build and query the better-im search index (Phase 1)",
    version
)]
struct Cli {
    /// Source iMessage `chat.db`. Defaults to `~/Library/Messages/chat.db`.
    #[arg(long, global = true, value_name = "PATH")]
    db: Option<PathBuf>,

    /// Index database path. Defaults to the app-support location.
    #[arg(long, global = true, value_name = "PATH")]
    index: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build or update the index from the source db (incremental).
    Index,
    /// Rebuild the whole index from scratch.
    Reindex,
    /// Search the index and print ranked snippets.
    Search {
        /// Query string (supports `from:`, `before:`/`after:`, `has:*`, `in:`, `is:from-me`).
        query: String,
        /// Maximum number of results.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let source = cli.db.unwrap_or_else(default_source_db);
    let index_path = match cli.index {
        Some(p) => p,
        None => default_index_path()?,
    };

    let indexer = Indexer::open(&source, &index_path)
        .with_context(|| format!("opening index at {}", index_path.display()))?;

    match cli.command {
        Command::Index => {
            let report = indexer
                .incremental_sync()
                .context("incremental sync (on macOS the source db needs Full Disk Access)")?;
            println!(
                "indexed {} new message(s); watermark = {}",
                report.indexed, report.watermark
            );
        }
        Command::Reindex => {
            let report = indexer
                .full_reindex()
                .context("full reindex (on macOS the source db needs Full Disk Access)")?;
            println!(
                "reindexed {} message(s); watermark = {}",
                report.indexed, report.watermark
            );
        }
        Command::Search { query, limit } => {
            let opts = SearchOpts { limit, offset: 0 };
            let results = indexer.search(&query, opts)?;
            println!("{} result(s) for {query:?}:", results.len());
            for hit in results {
                let when = hit
                    .message
                    .timestamp
                    .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "????-??-?? ??:??".to_string());
                let who = if hit.message.is_from_me {
                    "me".to_string()
                } else {
                    hit.message
                        .sender
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string())
                };
                let chat = hit
                    .message
                    .chat_name
                    .clone()
                    .or_else(|| hit.message.chat_identifier.clone())
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "  [{:>8.3}] {when}  {who} in {chat}\n            {}",
                    hit.score, hit.snippet
                );
            }
        }
    }

    Ok(())
}

/// Default source `chat.db`: `~/Library/Messages/chat.db`.
fn default_source_db() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/Messages/chat.db")
}
