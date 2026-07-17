//! `better-im-cli`: a headless dump/inspection tool for an iMessage `chat.db`.
//!
//! Phase 0 has no UI; this CLI exists to exercise and eyeball the reader.
//!
//! ```text
//! better-im-cli list-chats
//! better-im-cli thread 42 --limit 20
//! better-im-cli stats
//! better-im-cli --db /path/to/chat.db stats
//! ```

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};

use better_im_core::reader::ChatReader;
use imessage_database::util::dirs::default_db_path;

#[derive(Parser)]
#[command(
    name = "better-im-cli",
    about = "Headless read-only iMessage chat.db reader (better-im Phase 0)",
    version
)]
struct Cli {
    /// Path to the iMessage `chat.db`. Defaults to the macOS location.
    #[arg(long, global = true, value_name = "PATH")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List every conversation with its participants.
    ListChats,
    /// Dump a single thread's messages (oldest-first).
    Thread {
        /// Chat id (`chat.ROWID`), as shown by `list-chats`.
        chat_id: i64,
        /// Maximum number of messages to show.
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Print counts of chats, messages, and attachments.
    Stats,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.unwrap_or_else(default_db_path);

    let reader = ChatReader::open(&db_path).with_context(|| {
        format!(
            "failed to open chat.db at {} (on macOS this needs Full Disk Access)",
            db_path.display()
        )
    })?;

    match cli.command {
        Command::ListChats => list_chats(&reader)?,
        Command::Thread { chat_id, limit } => thread(&reader, chat_id, limit)?,
        Command::Stats => stats(&reader)?,
    }

    Ok(())
}

fn list_chats(reader: &ChatReader) -> anyhow::Result<()> {
    let conversations = reader.list_conversations()?;
    println!("{} conversation(s):", conversations.len());
    for convo in conversations {
        let participants = if convo.participants.is_empty() {
            String::from("-")
        } else {
            convo.participants.join(", ")
        };
        println!(
            "  [{:>5}] {:<28} service={:<8} participants=[{}]",
            convo.id,
            convo.label(),
            convo.service.as_deref().unwrap_or("?"),
            participants,
        );
    }
    Ok(())
}

fn thread(reader: &ChatReader, chat_id: i64, limit: usize) -> anyhow::Result<()> {
    let messages = reader.get_thread(chat_id, limit, None)?;
    println!("chat {chat_id}: {} message(s)", messages.len());
    for msg in messages {
        let when = msg
            .timestamp
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "????-??-?? ??:??:??".to_string());
        let who = if msg.is_from_me {
            "me".to_string()
        } else {
            msg.sender.clone().unwrap_or_else(|| "unknown".to_string())
        };
        let text = msg.text.as_deref().unwrap_or("<no text>");
        let attachments = if msg.num_attachments > 0 {
            format!(" [+{} attachment(s)]", msg.num_attachments)
        } else {
            String::new()
        };
        let edited = if msg.is_edited { " (edited)" } else { "" };
        println!("  {when}  {who:>20}: {text}{attachments}{edited}");
    }
    Ok(())
}

fn stats(reader: &ChatReader) -> anyhow::Result<()> {
    let stats = reader.stats()?;
    println!("chats:       {}", stats.chats);
    println!("messages:    {}", stats.messages);
    println!("attachments: {}", stats.attachments);
    Ok(())
}
