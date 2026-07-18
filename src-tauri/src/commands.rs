//! Tauri IPC commands. Every command returns `Result<T, String>`; the sync
//! `core`/`index` calls are wrapped in `spawn_blocking` so the webview thread is
//! never blocked. Failures to read `chat.db` surface as `FDA_DENIED:`-prefixed
//! errors the frontend routes to the onboarding screen.

use std::collections::HashMap;
use std::sync::Arc;

use better_im_core::{ChatReader, MessageActionProvider, ReadOnlyProvider};
use better_im_index::SearchOpts;
use chrono::{DateTime, Utc};
use tauri::{AppHandle, Emitter, State};

use crate::contacts::{self, ContactIndex, PermissionStatus};
use crate::dto::{
    semantic_status_dto, ContactInfoDto, ConversationDto, FdaStatus, IndexStatusDto, MessageDto,
    SearchResultDto, SemanticIndexReportDto, SemanticProgressDto, SemanticStatusDto, SyncReportDto,
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

/// The action capabilities the current backend advertises, as stable string
/// tags (see [`Capability::as_str`](better_im_core::Capability::as_str)).
///
/// # ⇽ The send-layer drop-in point
///
/// Today the app is backed by [`ReadOnlyProvider`], whose capability set is
/// **empty**, so this returns `[]`. The frontend reads this list once and, seeing
/// no `"SendText"`, renders a polished but disabled, read-only composer.
///
/// To add sending later, this is the *only* backend line that changes: swap
/// `ReadOnlyProvider` below for a send-capable provider — e.g. an `IMCoreProvider`
/// (private `IMCore` framework) or an `AppleScriptProvider` (Messages.app
/// Automation) — that returns `Capability::SendText` (and friends). Because the
/// UI gates purely on the returned tags, the composer enables itself with no
/// other frontend change. (A real send path also requires the user to opt into a
/// lower-security tier — disabling SIP / granting Automation — which is why it is
/// a deliberately separate, future opt-in rather than the default.)
#[tauri::command]
pub fn capabilities() -> Vec<String> {
    // ── SEND-LAYER SEAM ──────────────────────────────────────────────────────
    // Replace `ReadOnlyProvider` with a future send-capable provider to light up
    // the composer. Nothing else here (or in the UI) needs to change.
    let provider = ReadOnlyProvider;
    let mut caps: Vec<String> = provider
        .capabilities()
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();
    caps.sort(); // stable order for the UI / any snapshot testing
    caps
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

/// Phase 5 **smart** (semantic + keyword hybrid) search over the index. Returns
/// the same [`SearchResultDto`] shape as [`search`], so the UI reuses it. The
/// `score` here is a fused Reciprocal-Rank-Fusion score (higher is better),
/// rather than the raw BM25 score of keyword `search`. Degrades gracefully to
/// keyword ranking when the semantic index has not been built yet.
#[tauri::command]
pub async fn smart_search(
    state: State<'_, AppState>,
    query: String,
    limit: usize,
    offset: usize,
) -> Result<Vec<SearchResultDto>, String> {
    let indexer = state.indexer.clone();
    run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let results = guard
            .smart_search(&query, SearchOpts { limit, offset })
            .map_err(|e| format!("{e:#}"))?;
        Ok(results.iter().map(SearchResultDto::from).collect())
    })
    .await
}

/// Semantic-index health: whether embeddings exist yet (so the UI can offer the
/// "build semantic index" affordance) and how many messages remain to embed.
#[tauri::command]
pub async fn semantic_status(state: State<'_, AppState>) -> Result<SemanticStatusDto, String> {
    let indexer = state.indexer.clone();
    run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let status = guard.semantic_status().map_err(|e| format!("{e:#}"))?;
        Ok(semantic_status_dto(status, guard.has_embedder()))
    })
    .await
}

/// Build (or top up) the semantic index: embed every message with text that
/// lacks a vector, in batches, emitting a `semantic-progress` event
/// ([`SemanticProgressDto`]) after each batch. Opt-in and potentially expensive.
#[tauri::command]
pub async fn build_semantic_index(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SemanticIndexReportDto, String> {
    let indexer = state.indexer.clone();
    run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let report = guard
            .build_semantic_index(|progress| {
                let _ = app.emit("semantic-progress", SemanticProgressDto::from(progress));
            })
            .map_err(|e| format!("{e:#}"))?;
        Ok(SemanticIndexReportDto::from(report))
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

/// Current Contacts authorization status, as one of
/// `authorized` / `denied` / `restricted` / `notDetermined`. Does **not** prompt
/// (reading the status is side-effect-free); the prompt is triggered lazily by
/// the first `resolve_contacts` call when the status is `notDetermined`.
#[tauri::command]
pub async fn contacts_permission_status() -> Result<String, String> {
    run_blocking(|| Ok(contacts::store::permission_status().as_str().to_string())).await
}

/// Resolve a batch of `chat.db` handles (phones/emails) to Contacts identities.
///
/// Returns a map keyed by the *requested* identifier. Best-effort: unmatched
/// identifiers still get a formatted `displayName` with `matched: false`, so the
/// frontend can render every entry uniformly. Graceful when Contacts is denied —
/// everything simply comes back unmatched.
///
/// Caching is two-tiered (both in `AppState`): the built [`ContactIndex`] is
/// cached so the store is enumerated at most once, and each handle's resolved
/// [`ContactInfoDto`] is memoized. All Contacts work happens on the blocking
/// pool, never the webview thread.
#[tauri::command]
pub async fn resolve_contacts(
    state: State<'_, AppState>,
    identifiers: Vec<String>,
) -> Result<HashMap<String, ContactInfoDto>, String> {
    let index_slot = state.contact_index.clone();
    let cache = state.resolved_contacts.clone();
    run_blocking(move || {
        // 1. Get the cached index, or build it once (holding the lock so two
        //    concurrent first-calls can't both enumerate the store). The index is
        //    only cached when authorized; otherwise we use an empty, non-cached
        //    index so a later permission grant re-enumerates.
        let (index, authoritative) = {
            let mut guard = index_slot.lock().map_err(|e| e.to_string())?;
            match guard.as_ref() {
                Some(idx) => (idx.clone(), true),
                None => {
                    let loaded = contacts::store::load_contacts();
                    if loaded.status == PermissionStatus::Authorized {
                        let idx = Arc::new(ContactIndex::build(loaded.records));
                        *guard = Some(idx.clone());
                        (idx, true)
                    } else {
                        (Arc::new(ContactIndex::empty()), false)
                    }
                }
            }
        };

        // 2. Resolve each handle, memoizing authoritative results.
        let mut cache = cache.lock().map_err(|e| e.to_string())?;
        let mut out = HashMap::with_capacity(identifiers.len());
        for id in identifiers {
            if let Some(hit) = cache.get(&id) {
                out.insert(id, hit.clone());
                continue;
            }
            let info = index.resolve(&id);
            if authoritative {
                cache.insert(id.clone(), info.clone());
            }
            out.insert(id, info);
        }
        Ok(out)
    })
    .await
}

/// Open the macOS Contacts privacy pane in System Settings (for the denied hint).
#[tauri::command]
pub async fn open_contacts_settings() -> Result<(), String> {
    run_blocking(|| {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Contacts")
            .status()
            .map_err(|e| format!("could not open System Settings: {e}"))?;
        Ok(())
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
