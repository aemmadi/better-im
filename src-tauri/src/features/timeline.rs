//! Global unified timeline endpoint — a merged, chronological feed across every
//! conversation.

use better_im_index::IndexedMessage;
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

/// One row in the cross-conversation timeline.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItemDto {
    /// Source `message.ROWID`.
    pub id: i64,
    pub chat_id: Option<i64>,
    /// Chat display label (custom name / identifier) for the row's origin.
    pub chat_label: Option<String>,
    pub sender: Option<String>,
    pub is_from_me: bool,
    pub text: Option<String>,
    pub timestamp: Option<String>,
    pub has_attachment: bool,
    pub has_photo: bool,
}

impl From<&IndexedMessage> for TimelineItemDto {
    fn from(m: &IndexedMessage) -> Self {
        // Prefer the custom chat name, fall back to the raw identifier.
        let chat_label = m
            .chat_name
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| m.chat_identifier.clone().filter(|s| !s.is_empty()));
        Self {
            id: m.id,
            chat_id: m.chat_id,
            chat_label,
            sender: m.sender.clone(),
            is_from_me: m.is_from_me,
            text: m.text.clone(),
            timestamp: m.timestamp.map(|t| t.to_rfc3339()),
            has_attachment: m.has_attachment,
            has_photo: m.has_photo,
        }
    }
}

/// Newest-first merged feed across all conversations. `before` is an exclusive
/// ISO-8601 cursor for pagination (`None` = start at the most recent). No Full
/// Disk Access needed — everything comes from the local index.
#[tauri::command]
pub async fn timeline_feed(
    state: State<'_, AppState>,
    before: Option<String>,
    limit: usize,
) -> Result<Vec<TimelineItemDto>, String> {
    let indexer = state.indexer.clone();
    super::run_blocking(move || {
        let before_millis = match before {
            Some(s) => Some(super::parse_iso_millis(&s)?),
            None => None,
        };
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let msgs = guard
            .db()
            .timeline(before_millis, limit)
            .map_err(|e| format!("{e:#}"))?;
        Ok(msgs.iter().map(TimelineItemDto::from).collect())
    })
    .await
}
