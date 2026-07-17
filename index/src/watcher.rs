//! Live file watcher: react to `chat.db` writes with a debounced incremental
//! sync, and surface an "index updated" signal to callers (e.g. Phase 2 Tauri).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};

use crate::indexer::Indexer;
use crate::model::SyncReport;

/// Default debounce window: coalesce a burst of `chat.db` / `-wal` writes into a
/// single sync.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(400);

/// A running watcher. Dropping it stops watching and joins the worker thread.
///
/// Keep it alive for as long as you want live updates.
pub struct IndexWatcher {
    watcher: Option<notify::RecommendedWatcher>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for IndexWatcher {
    fn drop(&mut self) {
        // Drop the OS watcher first: this drops the event sender, so the worker
        // loop sees a disconnect and exits, then we join it.
        self.watcher.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Watch `source_db` (and its `-wal`/`-shm` sidecars) and run
/// [`Indexer::incremental_sync`] after each debounced burst of writes. The
/// `on_update` callback receives the result of every sync (so callers learn the
/// index changed, and see any error).
///
/// The `indexer` is moved onto a dedicated worker thread that runs the
/// (synchronous) syncs after each debounced burst.
///
/// # Errors
/// Returns an error if the source path has no parent directory or the OS
/// watcher cannot be created/registered.
pub fn watch<F>(
    indexer: Indexer,
    source_db: impl AsRef<Path>,
    debounce: Duration,
    on_update: F,
) -> anyhow::Result<IndexWatcher>
where
    F: FnMut(anyhow::Result<SyncReport>) + Send + 'static,
{
    let source_db = source_db.as_ref().to_path_buf();
    let watch_dir = source_db
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("source db path has no parent directory"))?;
    let target_name = source_db
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow::anyhow!("source db path has no file name"))?;

    let (raw_tx, raw_rx) = mpsc::channel::<()>();

    // OS watcher: forward only events touching our db files into the channel.
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let relevant = event.paths.iter().any(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(&target_name))
                    .unwrap_or(false)
            });
            if relevant {
                let _ = raw_tx.send(());
            }
        }
    })?;
    watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

    // Worker: debounce bursts, then sync and report.
    let worker = thread::spawn(move || {
        let mut on_update = on_update;
        debounce_loop(&raw_rx, debounce, || {
            on_update(indexer.incremental_sync());
        });
    });

    Ok(IndexWatcher {
        watcher: Some(watcher),
        worker: Some(worker),
    })
}

/// Convenience wrapper around [`watch`] that reports successful syncs over a
/// channel instead of a callback (errors are dropped). Handy for tests and
/// simple consumers.
///
/// # Errors
/// See [`watch`].
pub fn watch_channel(
    indexer: Indexer,
    source_db: impl AsRef<Path>,
    debounce: Duration,
) -> anyhow::Result<(IndexWatcher, Receiver<SyncReport>)> {
    let (tx, rx) = mpsc::channel::<SyncReport>();
    let handle = watch(indexer, source_db, debounce, move |res| {
        if let Ok(report) = res {
            let _ = tx.send(report);
        }
    })?;
    Ok((handle, rx))
}

/// Drive a debounce loop over a raw-event channel: block for the first event,
/// coalesce any further events arriving within `debounce` of each other, then
/// invoke `on_flush` once. Repeats until all senders disconnect.
///
/// Factored out (and free of any file-system or async coupling) so the
/// coalescing behavior can be unit-tested deterministically.
fn debounce_loop(raw_rx: &Receiver<()>, debounce: Duration, mut on_flush: impl FnMut()) {
    while raw_rx.recv().is_ok() {
        loop {
            match raw_rx.recv_timeout(debounce) {
                Ok(()) => {} // more activity within the window; keep waiting
                Err(RecvTimeoutError::Timeout) => break, // quiet period reached
                Err(RecvTimeoutError::Disconnected) => return, // shutting down
            }
        }
        on_flush();
    }
}

/// A path plus its libSQL/WAL sidecars, for callers that want to register extra
/// watch targets explicitly.
#[must_use]
pub fn sidecar_paths(db: &Path) -> Vec<PathBuf> {
    let mut out = vec![db.to_path_buf()];
    if let Some(name) = db.file_name().and_then(|n| n.to_str()) {
        if let Some(dir) = db.parent() {
            out.push(dir.join(format!("{name}-wal")));
            out.push(dir.join(format!("{name}-shm")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn debounce_coalesces_bursts_into_single_flush() {
        let (tx, rx) = mpsc::channel::<()>();
        let flushes = Arc::new(AtomicUsize::new(0));
        let flushes_worker = Arc::clone(&flushes);

        let worker = thread::spawn(move || {
            debounce_loop(&rx, Duration::from_millis(50), || {
                flushes_worker.fetch_add(1, Ordering::SeqCst);
            });
        });

        // Burst 1: five events in quick succession -> one flush.
        for _ in 0..5 {
            tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(5));
        }
        // Let the debounce window elapse.
        thread::sleep(Duration::from_millis(120));

        // Burst 2: another event -> a second flush.
        tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(120));

        // Disconnect -> loop ends.
        drop(tx);
        worker.join().unwrap();

        assert_eq!(flushes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn sidecar_paths_includes_wal_and_shm() {
        let paths = sidecar_paths(Path::new("/tmp/chat.db"));
        assert_eq!(paths.len(), 3);
        assert!(paths.iter().any(|p| p.ends_with("chat.db")));
        assert!(paths.iter().any(|p| p.ends_with("chat.db-wal")));
        assert!(paths.iter().any(|p| p.ends_with("chat.db-shm")));
    }
}
