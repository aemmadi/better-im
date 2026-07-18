//! Links & shared-content hub endpoint.

use serde::Serialize;

/// One URL shared in a conversation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkItemDto {
    /// Source `message.ROWID` the link came from.
    pub message_id: i64,
    pub chat_id: Option<i64>,
    pub url: String,
    pub timestamp: Option<String>,
    pub sender: Option<String>,
    pub is_from_me: bool,
    pub chat_name: Option<String>,
}

/// List shared links, newest first. `chat_id = None` spans all conversations.
///
/// TODO(phase4-links): implement by scanning indexed message text for URLs
/// (reuse the index's `has_link` flag to prefilter, then extract with a URL
/// regex). Paginate with `limit`/`offset`.
#[tauri::command]
pub async fn list_links(
    chat_id: Option<i64>,
    limit: usize,
    offset: usize,
) -> Result<Vec<LinkItemDto>, String> {
    let _ = (chat_id, limit, offset);
    Ok(Vec::new())
}
