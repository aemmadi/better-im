//! Phase 4 read-only feature endpoints: media galleries, links hub, insights,
//! and the global unified timeline.
//!
//! Each submodule owns its command(s) + DTO(s). The command **signatures** and
//! DTO **shapes** here are the frozen contract mirrored by `frontend/src/types.ts`
//! and `frontend/src/api.ts`; the Phase 4 feature agents fill in the command
//! bodies (and may add private helpers / `AppState` access) without changing the
//! JS-facing shape.

pub mod insights;
pub mod links;
pub mod media;
pub mod timeline;

/// Run a closure on the blocking pool, collapsing the join error into a `String`.
/// Mirrors the helper in `commands.rs` so feature bodies keep the sync
/// `core`/`index` calls off the webview thread.
pub(crate) async fn run_blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(inner) => inner,
        Err(e) => Err(e.to_string()),
    }
}

/// Parse an ISO-8601 (RFC3339) pagination cursor into unix-epoch milliseconds.
pub(crate) fn parse_iso_millis(s: &str) -> Result<i64, String> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.timestamp_millis())
        .map_err(|e| format!("invalid ISO-8601 timestamp {s:?}: {e}"))
}

/// Open a shared link (or any web URL) in the user's default browser.
///
/// Restricted to `http`/`https`/`mailto` so an odd string from message text can
/// never be interpreted by `open` as a flag or a local file path.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    let allowed = url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:");
    if !allowed {
        return Err(format!("refusing to open non-web URL: {url:?}"));
    }
    run_blocking(move || {
        std::process::Command::new("open")
            .arg(&url)
            .status()
            .map_err(|e| format!("could not open URL: {e}"))?;
        Ok(())
    })
    .await
}
