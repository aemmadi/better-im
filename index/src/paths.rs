//! Filesystem locations for the index database.

use std::path::PathBuf;

use anyhow::Context;

/// Default index database path: `~/Library/Application Support/better-im/index.db`
/// on macOS (via [`dirs::data_dir`]). The parent directory is created if needed.
///
/// # Errors
/// Returns an error if the platform data directory cannot be resolved or the
/// parent directory cannot be created.
pub fn default_index_path() -> anyhow::Result<PathBuf> {
    let dir = dirs::data_dir()
        .context("could not resolve the platform data directory (Application Support)")?
        .join("better-im");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating index directory {}", dir.display()))?;
    Ok(dir.join("index.db"))
}
