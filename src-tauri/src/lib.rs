//! Better iMessage — Tauri application shell (Phase 2).
//!
//! Wires the headless `better-im-core` reader and `better-im-index` search index
//! to a 3-pane webview UI, with Full Disk Access onboarding and live index
//! updates over the `index-updated` event.

mod commands;
mod contacts;
mod dto;
mod state;

use tauri::Manager;

use state::{default_chat_db_path, ensure_started, AppState};

/// Build and run the Tauri application. Shared by the desktop binary (and any
/// future mobile entrypoint).
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let chat_db_path = default_chat_db_path();
            let index_path = better_im_index::default_index_path().map_err(|e| e.to_string())?;
            let app_state = AppState::new(chat_db_path, index_path).map_err(|e| e.to_string())?;
            app.manage(app_state);

            // Probe FDA off the main thread; if granted, run the initial sync and
            // start the FSEvents watcher (see `state::startup_sync`).
            ensure_started(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::fda_status,
            commands::open_fda_settings,
            commands::list_conversations,
            commands::get_thread,
            commands::search,
            commands::get_message_context,
            commands::reindex,
            commands::index_status,
            commands::resolve_contacts,
            commands::contacts_permission_status,
            commands::open_contacts_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Better iMessage");
}
