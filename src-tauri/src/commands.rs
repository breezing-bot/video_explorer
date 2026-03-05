use std::path::Path;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, State};

use crate::db::Db;
use crate::error::AppError;
use crate::models::{HashWithPaths, InternalScanState, ScanErrorEvent, ScanStatusDto};
use crate::scanner::run_scan;

pub struct AppState {
  pub db: Db,
  pub scan_state: Arc<Mutex<InternalScanState>>,
}

#[tauri::command]
pub async fn start_scan(
  root_path: String,
  app: AppHandle,
  state: State<'_, AppState>,
) -> Result<(), String> {
  if !Path::new(&root_path).is_dir() {
    return Err(AppError::InvalidPath(root_path).to_string());
  }

  {
    let mut scan_state = state
      .scan_state
      .lock()
      .map_err(|_| AppError::DbInit("scan state lock failed".to_string()).to_string())?;

    if scan_state.is_running {
      return Err(AppError::ScanAlreadyRunning.to_string());
    }

    *scan_state = InternalScanState {
      is_running: true,
      root_path: Some(root_path.clone()),
      total_candidates: 0,
      scanned_files: 0,
      hashed_files: 0,
      error_count: 0,
      last_error: None,
      started_at: Some(chrono::Utc::now().to_rfc3339()),
      finished_at: None,
    };
  }

  let db = state.db.clone();
  let scan_state = Arc::clone(&state.scan_state);
  tauri::async_runtime::spawn(async move {
    let app_for_scan = app.clone();
    let path_for_scan = root_path.clone();
    let db_for_scan = db.clone();
    let state_for_scan = Arc::clone(&scan_state);

    let scan_result = tauri::async_runtime::spawn_blocking(move || {
      run_scan(app_for_scan, db_for_scan, state_for_scan, path_for_scan)
    })
    .await;

    match scan_result {
      Ok(Ok(())) => {}
      Ok(Err(err)) => {
        if let Ok(mut guard) = scan_state.lock() {
          guard.is_running = false;
          guard.last_error = Some(err.to_string());
          guard.finished_at = Some(chrono::Utc::now().to_rfc3339());
        }
        let _ = app.emit(
          "scan:error",
          ScanErrorEvent {
            scan_id: -1,
            path: None,
            message: err.to_string(),
          },
        );
      }
      Err(join_error) => {
        let message = AppError::TaskJoin(join_error.to_string()).to_string();
        if let Ok(mut guard) = scan_state.lock() {
          guard.is_running = false;
          guard.last_error = Some(message.clone());
          guard.finished_at = Some(chrono::Utc::now().to_rfc3339());
        }
        let _ = app.emit(
          "scan:error",
          ScanErrorEvent {
            scan_id: -1,
            path: None,
            message,
          },
        );
      }
    }
  });

  Ok(())
}

#[tauri::command]
pub fn get_hashes_with_paths(state: State<'_, AppState>) -> Result<Vec<HashWithPaths>, String> {
  state
    .db
    .query_hashes_with_paths()
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn get_scan_status(state: State<'_, AppState>) -> Result<ScanStatusDto, String> {
  let status = state
    .scan_state
    .lock()
    .map_err(|_| "scan state lock failed".to_string())?;
  Ok(status.to_dto())
}
