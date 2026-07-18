//! Links & shared-content hub endpoint.

use better_im_index::LinkRow;
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

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

impl From<&LinkRow> for LinkItemDto {
    fn from(r: &LinkRow) -> Self {
        Self {
            message_id: r.message_id,
            chat_id: r.chat_id,
            url: r.url.clone(),
            timestamp: r.timestamp.map(|t| t.to_rfc3339()),
            sender: r.sender.clone(),
            is_from_me: r.is_from_me,
            chat_name: r.chat_name.clone(),
        }
    }
}

/// List shared links, newest first. `chat_id = None` spans all conversations.
///
/// Scans the index for `has_link` messages and extracts one entry per URL from
/// the body text; `limit`/`offset` paginate at URL granularity. No Full Disk
/// Access needed — everything comes from the local index.
#[tauri::command]
pub async fn list_links(
    state: State<'_, AppState>,
    chat_id: Option<i64>,
    limit: usize,
    offset: usize,
) -> Result<Vec<LinkItemDto>, String> {
    let indexer = state.indexer.clone();
    super::run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let rows = guard
            .db()
            .list_links(chat_id, limit, offset)
            .map_err(|e| format!("{e:#}"))?;
        Ok(rows.iter().map(LinkItemDto::from).collect())
    })
    .await
}
