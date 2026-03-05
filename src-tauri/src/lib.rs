mod commands;
mod db;
mod error;
mod hashing;
mod models;
mod scanner;

use std::fs;
use std::sync::{Arc, Mutex};

use commands::{get_backup_tree, get_scan_status, list_scan_roots, start_scan, AppState};
use db::Db;
use models::InternalScanState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|err| format!("failed to resolve app data directory: {err}"))?;
            fs::create_dir_all(&app_data_dir)?;

            let db = Db::new(app_data_dir.join("video_index.db"));
            db.init_schema()?;

            app.manage(AppState {
                db,
                scan_state: Arc::new(Mutex::new(InternalScanState::default())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_scan,
            list_scan_roots,
            get_backup_tree,
            get_scan_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
