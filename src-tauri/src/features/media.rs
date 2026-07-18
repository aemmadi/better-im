//! Media / attachment gallery endpoint.

use serde::Serialize;

/// One attachment surfaced in a gallery grid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItemDto {
    /// Source `message.ROWID` the attachment belongs to.
    pub message_id: i64,
    pub chat_id: Option<i64>,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    /// Absolute path under `~/Library/Messages/Attachments/` (serve via the Tauri
    /// asset protocol / `convertFileSrc`). `None` if the file is missing.
    pub absolute_path: Option<String>,
    /// Coarse bucket for filtering / rendering: `"image" | "video" | "audio" | "file"`.
    pub kind: String,
    pub timestamp: Option<String>,
    pub sender: Option<String>,
    pub is_from_me: bool,
}

/// List media attachments, newest first. `chat_id = None` spans all conversations.
///
/// TODO(phase4-media): implement over `better-im-index` / `ChatReader` + the
/// `attachment` table. Resolve absolute paths (expand `~`), classify `kind` from
/// the MIME type, and paginate with `limit`/`offset`.
#[tauri::command]
pub async fn list_media(
    chat_id: Option<i64>,
    limit: usize,
    offset: usize,
) -> Result<Vec<MediaItemDto>, String> {
    let _ = (chat_id, limit, offset);
    Ok(Vec::new())
}
