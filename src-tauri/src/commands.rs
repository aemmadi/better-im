//! Tauri IPC commands. Every command returns `Result<T, String>`; the sync
//! `core`/`index` calls are wrapped in `spawn_blocking` so the webview thread is
//! never blocked. Failures to read `chat.db` surface as `FDA_DENIED:`-prefixed
//! errors the frontend routes to the onboarding screen.

use better_im_core::ChatReader;
use better_im_index::SearchOpts;
use chrono::{DateTime, Utc};
use tauri::{AppHandle, State};

use crate::dto::{
    ConversationDto, FdaStatus, IndexStatusDto, MessageDto, SearchResultDto, SyncReportDto,
};
use crate::state::{ensure_started, map_sync_err, open_reader, AppState};

/// Run a closure on the blocking pool, collapsing the join error into a `String`.
async fn run_blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(inner) => inner,
        Err(e) => Err(e.to_string()),
    }
}

/// Parse an ISO-8601 pagination cursor into a UTC datetime.
fn parse_iso(s: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| format!("invalid ISO-8601 timestamp {s:?}: {e}"))
}

/// Probe Full Disk Access with a lightweight `chat.db` read. Also (re)triggers
/// the background startup task when access is available, so granting FDA to a
/// running app self-heals the initial sync + watcher.
#[tauri::command]
pub async fn fda_status(app: AppHandle, state: State<'_, AppState>) -> Result<FdaStatus, String> {
    let path = state.chat_db_path.clone();
    let granted = run_blocking(move || {
        // Open read-only and touch the smallest possible query.
        let ok = ChatReader::open(&path)
            .and_then(|r| r.max_message_rowid())
            .is_ok();
        Ok(ok)
    })
    .await?;

    if granted {
        ensure_started(&app);
    }
    Ok(FdaStatus { granted })
}

/// Open the macOS Full Disk Access settings pane.
#[tauri::command]
pub async fn open_fda_settings() -> Result<(), String> {
    run_blocking(|| {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFilesAccess")
            .status()
            .map_err(|e| format!("could not open System Settings: {e}"))?;
        Ok(())
    })
    .await
}

/// List all conversations (from `chat.db`; requires FDA).
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
) -> Result<Vec<ConversationDto>, String> {
    let path = state.chat_db_path.clone();
    run_blocking(move || {
        let reader = open_reader(&path)?;
        let convos = reader.list_conversations().map_err(map_sync_err)?;
        Ok(convos.iter().map(ConversationDto::from).collect())
    })
    .await
}

/// Fetch a page of a thread, oldest-first. `before` is an ISO-8601 cursor: the
/// page returned is the most recent `limit` messages strictly before it.
#[tauri::command]
pub async fn get_thread(
    state: State<'_, AppState>,
    chat_id: i64,
    limit: usize,
    before: Option<String>,
) -> Result<Vec<MessageDto>, String> {
    let path = state.chat_db_path.clone();
    run_blocking(move || {
        let before_dt = match before {
            Some(s) => Some(parse_iso(&s)?),
            None => None,
        };
        let reader = open_reader(&path)?;
        let msgs = reader
            .get_thread(chat_id, limit, before_dt)
            .map_err(map_sync_err)?;
        Ok(msgs.iter().map(MessageDto::from).collect())
    })
    .await
}

/// Ranked full-text search over the index (no FDA needed once indexed).
#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    query: String,
    limit: usize,
    offset: usize,
) -> Result<Vec<SearchResultDto>, String> {
    let indexer = state.indexer.clone();
    run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let results = guard
            .search(&query, SearchOpts { limit, offset })
            .map_err(|e| format!("{e:#}"))?;
        Ok(results.iter().map(SearchResultDto::from).collect())
    })
    .await
}

/// Fetch conversational context around a message (from the index): up to
/// `before` preceding and `after` following, chronological, target included.
#[tauri::command]
pub async fn get_message_context(
    state: State<'_, AppState>,
    id: i64,
    before: usize,
    after: usize,
) -> Result<Vec<MessageDto>, String> {
    let indexer = state.indexer.clone();
    run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let msgs = guard
            .message_context(id, before, after)
            .map_err(|e| format!("{e:#}"))?;
        Ok(msgs.iter().map(MessageDto::from).collect())
    })
    .await
}

/// Force a full reindex from the source `chat.db` (requires FDA).
#[tauri::command]
pub async fn reindex(state: State<'_, AppState>) -> Result<SyncReportDto, String> {
    let indexer = state.indexer.clone();
    run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let report = guard.full_reindex().map_err(map_sync_err)?;
        Ok(SyncReportDto::from(report))
    })
    .await
}

/// Index health: total indexed messages and last successful sync time.
#[tauri::command]
pub async fn index_status(state: State<'_, AppState>) -> Result<IndexStatusDto, String> {
    let indexer = state.indexer.clone();
    run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let count = guard.db().message_count().map_err(|e| format!("{e:#}"))?;
        let last_synced: Option<String> = guard
            .db()
            .connection()
            .query_row(
                "SELECT last_sync_at FROM sync_state WHERE id = 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(IndexStatusDto { count, last_synced })
    })
    .await
}
