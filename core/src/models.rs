//! Source-agnostic domain model for better-im.
//!
//! These structs are intentionally decoupled from the `imessage-database`
//! crate's row types. The reader maps the upstream rows into these plain
//! `serde`-friendly types so the rest of the app (indexer, search, future UI,
//! and a possible send layer) never depends on the on-disk iMessage schema.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A conversation / chat thread.
///
/// Maps from the iMessage `chat` table plus its participant handles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    /// Stable chat row id (the iMessage `chat.ROWID`).
    pub id: i64,
    /// Phone number, email, or group identifier (`chat.chat_identifier`).
    pub identifier: String,
    /// User-provided display name, when set (`chat.display_name`).
    pub display_name: Option<String>,
    /// Service name (e.g. `iMessage`, `SMS`, `RCS`), when known.
    pub service: Option<String>,
    /// Participant handles (phone numbers / emails) in this chat.
    pub participants: Vec<String>,
}

impl Conversation {
    /// Best display label: the custom name, else the raw identifier.
    #[must_use]
    pub fn label(&self) -> &str {
        match self.display_name.as_deref() {
            Some(name) if !name.is_empty() => name,
            _ => &self.identifier,
        }
    }
}

/// A single contact endpoint (phone number or email).
///
/// Maps from the iMessage `handle` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handle {
    /// Handle row id (`handle.ROWID`).
    pub id: i64,
    /// Phone number, email, or service identifier (`handle.id`).
    pub identifier: String,
}

/// An attachment (media/file) associated with a message.
///
/// Maps from the iMessage `attachment` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment row id (`attachment.ROWID`).
    pub id: i64,
    /// Attachment GUID, if present.
    pub guid: Option<String>,
    /// On-disk filename, if present.
    pub filename: Option<String>,
    /// MIME type, if known.
    pub mime_type: Option<String>,
    /// Total size in bytes as recorded by Messages.
    pub total_bytes: i64,
    /// Whether the attachment is a sticker.
    pub is_sticker: bool,
}

/// A single message.
///
/// The [`text`](Message::text) field carries the *extracted* body text. This is
/// the critical correctness concern the reader solves: when the `message.text`
/// column is `NULL`, the real content lives in the `attributedBody` typedstream
/// blob, which the reader decodes for us.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Message row id (`message.ROWID`).
    pub id: i64,
    /// Globally-unique message id (`message.guid`).
    pub guid: String,
    /// Chat this row belongs to, if joined to one.
    pub chat_id: Option<i64>,
    /// Sender handle identifier. `None` when [`is_from_me`](Message::is_from_me).
    pub sender: Option<String>,
    /// Whether the database owner sent this message.
    pub is_from_me: bool,
    /// Raw service name (`iMessage`, `SMS`, `RCS`, …), when recorded.
    pub service: Option<String>,
    /// Sender `handle.ROWID`, when known (useful for stable joins in the index).
    pub handle_id: Option<i64>,
    /// Message `item_type` code (0 = ordinary message, non-zero = group action,
    /// etc.). Stored as-is so downstream layers can classify without re-reading.
    pub item_type: i32,
    /// Extracted plain body text (from `text` **or** decoded `attributedBody`).
    pub text: Option<String>,
    /// Message timestamp, converted from the Apple epoch to UTC.
    pub timestamp: Option<DateTime<Utc>>,
    /// When the message was read, converted to UTC (if recorded).
    pub date_read: Option<DateTime<Utc>>,
    /// Number of attachments joined to this message.
    pub num_attachments: i64,
    /// Resolved attachment rows (populated by the reader on demand).
    pub attachments: Vec<Attachment>,
    /// Whether the message has been edited.
    pub is_edited: bool,
    /// Whether the message is a reply within a thread.
    pub is_reply: bool,
    /// The GUID of the thread root this message replies to, if any.
    pub thread_originator_guid: Option<String>,
}

// MARK: Extensibility seam (future send layer)

/// Actions a message provider might support.
///
/// Phase 0 is strictly read-only, but this enum is the vocabulary a future
/// send/interaction layer will speak. Keeping it here lets callers reason about
/// what a provider can do without depending on a concrete implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Read conversations and messages.
    ReadMessages,
    /// Send a new text message.
    SendText,
    /// Send a message with attachments.
    SendAttachment,
    /// Add a tapback / reaction to a message.
    React,
    /// Edit a previously sent message.
    Edit,
    /// Retract / unsend a message.
    Unsend,
    /// Mark a message or thread as read.
    MarkRead,
}

impl Capability {
    /// Stable string tag for this capability.
    ///
    /// This is the *contract* the UI gates on: the Tauri `capabilities` command
    /// serializes a provider's set into these tags, and the frontend composer
    /// enables sending only when it sees `"SendText"`. The tags intentionally
    /// match the serde variant names, so keep them in lockstep and stable.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::ReadMessages => "ReadMessages",
            Capability::SendText => "SendText",
            Capability::SendAttachment => "SendAttachment",
            Capability::React => "React",
            Capability::Edit => "Edit",
            Capability::Unsend => "Unsend",
            Capability::MarkRead => "MarkRead",
        }
    }
}

/// The extensibility seam for a future send layer.
///
/// A provider advertises the [`Capability`] set it supports. Phase 0 ships only
/// the read path, so the only provider today is [`ReadOnlyProvider`].
pub trait MessageActionProvider {
    /// The set of actions this provider supports.
    fn capabilities(&self) -> HashSet<Capability>;

    /// Convenience: whether a specific capability is supported.
    fn supports(&self, capability: Capability) -> bool {
        self.capabilities().contains(&capability)
    }
}

/// A provider that can do nothing but be read from.
///
/// Returns an empty capability set: it exposes no *actions*. The read path is
/// provided directly by [`ChatReader`](crate::reader::ChatReader).
///
/// # ⇽ Send-layer drop-in point
///
/// This is the exact seam where a future *send* layer plugs in. To make the app
/// send, add a sibling provider — e.g. `IMCoreProvider` (linking Apple's private
/// `IMCore` framework) or `AppleScriptProvider` (driving Messages.app via
/// Automation) — that implements [`MessageActionProvider`] and returns the
/// actions it supports (starting with [`Capability::SendText`]). The Tauri
/// `capabilities` command exposes that set to the UI, whose composer is already
/// gated on it, so lighting up sending is purely additive.
///
/// Enabling send is deliberately *not* a drop-in for the default distribution:
/// programmatic sending on modern macOS requires a lower-security posture
/// (disabling System Integrity Protection to link `IMCore`, or granting
/// Automation control of Messages.app). That is a separate, user-opted-in tier —
/// which is why the shipping build stays [`ReadOnlyProvider`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ReadOnlyProvider;

impl MessageActionProvider for ReadOnlyProvider {
    fn capabilities(&self) -> HashSet<Capability> {
        HashSet::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_provider_advertises_no_actions() {
        let provider = ReadOnlyProvider;
        assert!(provider.capabilities().is_empty());
        assert!(!provider.supports(Capability::SendText));
        assert!(!provider.supports(Capability::ReadMessages));
    }

    #[test]
    fn capability_tags_are_stable() {
        // These string tags are the IPC contract the UI gates on; if this test
        // needs updating, the frontend composer's gate must change in lockstep.
        assert_eq!(Capability::SendText.as_str(), "SendText");
        assert_eq!(Capability::ReadMessages.as_str(), "ReadMessages");
        assert_eq!(Capability::MarkRead.as_str(), "MarkRead");
    }

    #[test]
    fn conversation_label_prefers_display_name() {
        let mut c = Conversation {
            id: 1,
            identifier: "+15551234567".into(),
            display_name: Some("Family".into()),
            service: Some("iMessage".into()),
            participants: vec![],
        };
        assert_eq!(c.label(), "Family");
        c.display_name = Some(String::new());
        assert_eq!(c.label(), "+15551234567");
        c.display_name = None;
        assert_eq!(c.label(), "+15551234567");
    }
}
