//! Media / attachment gallery endpoint.

use std::path::{Path, PathBuf};

use better_im_core::MediaAttachment;
use serde::Serialize;
use tauri::State;

use crate::state::{map_sync_err, open_reader, AppState};

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
/// Attachment rows (path / MIME) live in the source `chat.db`, so this reads via
/// [`better_im_core::ChatReader`] and surfaces a `FDA_DENIED:`-prefixed error if
/// the database can't be opened (missing Full Disk Access).
#[tauri::command]
pub async fn list_media(
    state: State<'_, AppState>,
    chat_id: Option<i64>,
    limit: usize,
    offset: usize,
) -> Result<Vec<MediaItemDto>, String> {
    let path = state.chat_db_path.clone();
    super::run_blocking(move || {
        let reader = open_reader(&path)?;
        let items = reader
            .list_attachments(chat_id, limit, offset)
            .map_err(map_sync_err)?;
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Ok(items
            .iter()
            .map(|a| media_item(a, home.as_deref()))
            .collect())
    })
    .await
}

/// Map a core [`MediaAttachment`] into the JS-facing DTO: expand `~`, verify the
/// file exists, and classify the media `kind`.
fn media_item(a: &MediaAttachment, home: Option<&Path>) -> MediaItemDto {
    let absolute_path = a
        .path
        .as_deref()
        .and_then(|p| resolve_existing(p, home));
    let filename = a
        .transfer_name
        .clone()
        .or_else(|| a.path.as_deref().and_then(basename));
    MediaItemDto {
        message_id: a.message_id,
        chat_id: a.chat_id,
        filename,
        mime_type: a.mime_type.clone(),
        absolute_path,
        kind: kind_from_mime(a.mime_type.as_deref()),
        timestamp: a.timestamp.map(|t| t.to_rfc3339()),
        sender: a.sender.clone(),
        is_from_me: a.is_from_me,
    }
}

/// Coarse media bucket from a MIME type. One of `image` / `video` / `audio` /
/// `file` (the fallback for anything else, including a missing MIME).
fn kind_from_mime(mime: Option<&str>) -> String {
    match mime {
        Some(m) if m.starts_with("image/") => "image",
        Some(m) if m.starts_with("video/") => "video",
        Some(m) if m.starts_with("audio/") => "audio",
        _ => "file",
    }
    .to_string()
}

/// Expand a leading `~` (relative to `home`) into an absolute path. Leaves
/// already-absolute paths untouched, and leaves `~` paths as-is when `home` is
/// unknown (best effort).
fn expand_tilde(path: &str, home: Option<&Path>) -> PathBuf {
    match home {
        Some(home) => {
            if let Some(rest) = path.strip_prefix("~/") {
                home.join(rest)
            } else if path == "~" {
                home.to_path_buf()
            } else {
                PathBuf::from(path)
            }
        }
        None => PathBuf::from(path),
    }
}

/// Expand `~`, then return the absolute path string only when the file actually
/// exists on disk (`None` otherwise, so the gallery can render a placeholder).
fn resolve_existing(path: &str, home: Option<&Path>) -> Option<String> {
    let expanded = expand_tilde(path, home);
    if expanded.exists() {
        Some(expanded.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// The final path component, as a display filename.
fn basename(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_classification() {
        assert_eq!(kind_from_mime(Some("image/jpeg")), "image");
        assert_eq!(kind_from_mime(Some("video/mp4")), "video");
        assert_eq!(kind_from_mime(Some("audio/mpeg")), "audio");
        assert_eq!(kind_from_mime(Some("application/pdf")), "file");
        assert_eq!(kind_from_mime(None), "file");
    }

    #[test]
    fn tilde_expansion() {
        let home = Path::new("/Users/test");
        assert_eq!(
            expand_tilde("~/Library/Messages/Attachments/x.jpg", Some(home)),
            PathBuf::from("/Users/test/Library/Messages/Attachments/x.jpg")
        );
        assert_eq!(
            expand_tilde("/absolute/path.jpg", Some(home)),
            PathBuf::from("/absolute/path.jpg")
        );
        // Unknown home: left untouched (best effort).
        assert_eq!(expand_tilde("~/x.jpg", None), PathBuf::from("~/x.jpg"));
    }

    #[test]
    fn basename_extraction() {
        assert_eq!(
            basename("~/Library/Messages/Attachments/ab/cd/IMG_1.HEIC").as_deref(),
            Some("IMG_1.HEIC")
        );
    }

    #[test]
    fn absolute_path_none_when_missing() {
        assert!(
            resolve_existing("~/definitely/not/here/xyz.jpg", Some(Path::new("/no-such-home")))
                .is_none()
        );
    }

    #[test]
    fn absolute_path_some_when_present() {
        // A real, existing file resolves to its absolute path string.
        let file = std::env::temp_dir().join(format!("better-im-media-{}.bin", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        let got = resolve_existing(file.to_str().unwrap(), None);
        assert_eq!(got.as_deref(), Some(file.to_string_lossy().as_ref()));
        let _ = std::fs::remove_file(&file);
    }
}
