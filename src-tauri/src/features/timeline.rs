//! Global unified timeline endpoint — a merged, chronological feed across every
//! conversation.

use serde::Serialize;

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

/// Newest-first merged feed across all conversations. `before` is an exclusive
/// ISO-8601 cursor for pagination (`None` = start at the most recent).
///
/// TODO(phase4-timeline): implement over the index (all chats, ORDER BY
/// timestamp DESC, keyset-paginate on `before`). Support optional filtering by
/// reusing the query operators later.
#[tauri::command]
pub async fn timeline_feed(
    before: Option<String>,
    limit: usize,
) -> Result<Vec<TimelineItemDto>, String> {
    let _ = (before, limit);
    Ok(Vec::new())
}
