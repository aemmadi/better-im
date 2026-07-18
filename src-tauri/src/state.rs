//! Application state, path resolution, and the background startup/sync task.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use better_im_core::ChatReader;
use better_im_index::{watch, Embedder, IndexWatcher, Indexer, DEFAULT_DEBOUNCE};
use tauri::{AppHandle, Emitter, Manager};

use crate::contacts::ContactIndex;
use crate::dto::{ContactInfoDto, SyncReportDto};

/// Shared application state managed by Tauri.
pub struct AppState {
    /// Source iMessage database (`~/Library/Messages/chat.db`).
    pub chat_db_path: PathBuf,
    /// Search index database (`~/Library/Application Support/better-im/index.db`).
    pub index_path: PathBuf,
    /// The command-side indexer (search / context / reindex / status). Behind a
    /// mutex because `rusqlite::Connection` is `Send` but not `Sync`, and
    /// commands run on `spawn_blocking` threads.
    pub indexer: Arc<Mutex<Indexer>>,
    /// The live FSEvents watcher. Kept here so it is not dropped (dropping it
    /// stops watching). A second, dedicated indexer lives inside it.
    pub watcher: Arc<Mutex<Option<IndexWatcher>>>,
    /// Guards one-shot startup (initial sync + watcher). Reset to `false` if the
    /// startup task aborts because Full Disk Access was denied, so a later
    /// successful `fda_status` can retry without an app relaunch.
    pub sync_started: AtomicBool,
    /// The built Contacts lookup index (Phase 3). `None` until the first
    /// successful (authorized) enumeration, then cached so `resolve_contacts`
    /// never re-enumerates the whole store. Stays `None` while permission is not
    /// granted, so a later grant re-enumerates.
    pub contact_index: Arc<Mutex<Option<Arc<ContactIndex>>>>,
    /// Per-handle resolution cache, keyed by the raw `chat.db` identifier. Only
    /// populated from an authoritative (authorized) index.
    pub resolved_contacts: Arc<Mutex<HashMap<String, ContactInfoDto>>>,
    /// The on-device embedder powering Phase 5 semantic search. Shared with both
    /// the command-side indexer and the watcher-side indexer so vectors stay
    /// current. See [`make_embedder`].
    pub embedder: Arc<dyn Embedder>,
}

impl AppState {
    /// Build state, opening the (always-local) index database. Does **not** touch
    /// `chat.db`, so this succeeds even without Full Disk Access.
    pub fn new(chat_db_path: PathBuf, index_path: PathBuf) -> anyhow::Result<Self> {
        let embedder = make_embedder();
        let indexer =
            Indexer::open_with_embedder(&chat_db_path, &index_path, Some(embedder.clone()))?;
        Ok(Self {
            chat_db_path,
            index_path,
            indexer: Arc::new(Mutex::new(indexer)),
            watcher: Arc::new(Mutex::new(None)),
            sync_started: AtomicBool::new(false),
            contact_index: Arc::new(Mutex::new(None)),
            resolved_contacts: Arc::new(Mutex::new(HashMap::new())),
            embedder,
        })
    }
}

/// Construct the semantic-search embedder for this build.
///
/// With the `fastembed` feature, this is the production `BAAI/bge-small-en-v1.5`
/// model (its ONNX weights download on first use). Without it — the default build
/// used by CI and this sandbox — it is the deterministic [`MockEmbedder`], so the
/// full Smart-search flow still works end-to-end (with toy vectors) and the
/// workspace never needs the onnxruntime toolchain. The model tag is stored with
/// each vector, so switching builds is detectable.
#[cfg(feature = "fastembed")]
fn make_embedder() -> Arc<dyn Embedder> {
    Arc::new(better_im_index::FastEmbedEmbedder::new())
}

#[cfg(not(feature = "fastembed"))]
fn make_embedder() -> Arc<dyn Embedder> {
    // Match bge-small's dimensionality so switching to the real model keeps the
    // stored `dim` consistent for freshly-built indexes.
    Arc::new(better_im_index::MockEmbedder::new(384))
}

/// Default source `chat.db`: `~/Library/Messages/chat.db`.
pub fn default_chat_db_path() -> PathBuf {
    dirs_home()
        .unwrap_or_default()
        .join("Library/Messages/chat.db")
}

/// `dirs::home_dir()` without adding a direct `dirs` dependency: fall back to
/// the `HOME` environment variable.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Whether an error chain looks like a macOS Full Disk Access / permission
/// denial (as opposed to, say, a malformed query). Used to decide when to route
/// the UI to the FDA onboarding screen.
pub fn is_permission_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        let msg = cause.to_string().to_ascii_lowercase();
        msg.contains("authorization denied")
            || msg.contains("not authorized")
            || msg.contains("operation not permitted")
            || msg.contains("unable to open database file")
            || msg.contains("permission denied")
            || msg.contains("access denied")
    })
}

/// Open the source reader, mapping *any* open failure to an `FDA_DENIED:`-prefixed
/// error string. On macOS a failure to open `chat.db` is, in practice, always a
/// Full Disk Access problem, and the onboarding screen is the actionable fix.
pub fn open_reader(path: &Path) -> Result<ChatReader, String> {
    ChatReader::open(path).map_err(|e| format!("FDA_DENIED: {e:#}"))
}

/// Map a sync/reindex error, prefixing `FDA_DENIED:` only when it looks like a
/// permission problem (these paths open the reader internally).
pub fn map_sync_err(e: anyhow::Error) -> String {
    if is_permission_error(&e) {
        format!("FDA_DENIED: {e:#}")
    } else {
        format!("{e:#}")
    }
}

/// Ensure the background startup task (initial sync + watcher) has been kicked
/// off exactly once. Safe to call repeatedly (e.g. from `fda_status` after the
/// user grants access): the atomic guard makes all but the first call a no-op.
pub fn ensure_started(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.sync_started.swap(true, Ordering::SeqCst) {
        return; // already started (or in-flight)
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { startup_sync(handle).await });
}

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

/// Background startup: probe FDA, run the initial sync, then start the watcher.
/// Emits `index-updated` with a [`SyncReportDto`] after the initial sync and on
/// every subsequent watcher-driven sync.
async fn startup_sync(app: AppHandle) {
    let (chat_db_path, index_path, indexer, watcher_slot, embedder) = {
        let s = app.state::<AppState>();
        (
            s.chat_db_path.clone(),
            s.index_path.clone(),
            s.indexer.clone(),
            s.watcher.clone(),
            s.embedder.clone(),
        )
    };

    // 1. Probe Full Disk Access. If denied, reset the guard so a later
    //    `fda_status` (after the user grants access) can retry.
    let probe_path = chat_db_path.clone();
    let granted = run_blocking(move || Ok(ChatReader::open(&probe_path).is_ok()))
        .await
        .unwrap_or(false);
    if !granted {
        app.state::<AppState>()
            .sync_started
            .store(false, Ordering::SeqCst);
        return;
    }

    // 2. Initial sync: full reindex when the index is empty, else incremental.
    let idx = indexer.clone();
    let initial = run_blocking(move || {
        let guard = idx.lock().map_err(|e| e.to_string())?;
        let empty = guard.db().message_count().map_err(map_sync_err)? == 0;
        let report = if empty {
            guard.full_reindex()
        } else {
            guard.incremental_sync()
        };
        report.map_err(map_sync_err)
    })
    .await;
    if let Ok(report) = initial {
        let _ = app.emit("index-updated", SyncReportDto::from(report));
    }

    // 3. Start the FSEvents watcher on a second, dedicated indexer. Each
    //    debounced sync is forwarded to the webview as `index-updated`.
    let src = chat_db_path.clone();
    let ip = index_path.clone();
    let emit_handle = app.clone();
    let watcher = run_blocking(move || {
        let watcher_indexer =
            Indexer::open_with_embedder(&src, &ip, Some(embedder)).map_err(map_sync_err)?;
        watch(watcher_indexer, &src, DEFAULT_DEBOUNCE, move |res| {
            if let Ok(report) = res {
                let _ = emit_handle.emit("index-updated", SyncReportDto::from(report));
            }
        })
        .map_err(map_sync_err)
    })
    .await;
    if let Ok(watcher) = watcher {
        if let Ok(mut slot) = watcher_slot.lock() {
            *slot = Some(watcher);
        }
    }
}
