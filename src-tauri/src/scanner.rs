use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

use crate::db::Db;
use crate::error::AppResult;
use crate::hashing::{fingerprint_file, full_hash_file};
use crate::models::{
  InternalScanState, ScanCompletedEvent, ScanErrorEvent, ScanProgressEvent, ScanStartedEvent,
};

const VIDEO_EXTENSIONS: [&str; 9] = ["mp4", "mkv", "avi", "mov", "flv", "wmv", "webm", "m4v", "ts"];

pub fn run_scan(
  app: AppHandle,
  db: Db,
  scan_state: Arc<Mutex<InternalScanState>>,
  root_path: String,
) -> AppResult<()> {
  let canonical_root = normalize_root_path(&root_path)?;
  let scan_id = db.start_scan(&canonical_root)?;
  let mut walk_errors = 0_u64;

  let mut candidates: Vec<PathBuf> = Vec::new();
  for entry in WalkDir::new(&canonical_root).into_iter() {
    match entry {
      Ok(file_entry) => {
        if file_entry.file_type().is_file() && is_video_file(file_entry.path()) {
          candidates.push(file_entry.path().to_path_buf());
        }
      }
      Err(err) => {
        walk_errors += 1;
        let event = ScanErrorEvent {
          scan_id,
          path: None,
          message: err.to_string(),
        };
        let _ = app.emit("scan:error", event);
      }
    }
  }

  {
    let mut state = scan_state.lock().expect("scan state poisoned");
    state.total_candidates = candidates.len() as u64;
  }

  let started_event = ScanStartedEvent {
    root_path: canonical_root.clone(),
    scan_id,
    total_candidates: candidates.len() as u64,
  };
  let _ = app.emit("scan:started", started_event);

  let mut seen_paths = HashSet::new();
  let mut hashed_files = 0_u64;
  let mut scanned_files = 0_u64;
  let mut error_count = walk_errors;

  for file_path in &candidates {
    let normalized_path = file_path.to_string_lossy().to_string();
    seen_paths.insert(normalized_path.clone());

    match process_video_file(&db, scan_id, file_path, &normalized_path) {
      Ok(was_hashed) => {
        if was_hashed {
          hashed_files += 1;
        }
      }
      Err(err) => {
        error_count += 1;
        let event = ScanErrorEvent {
          scan_id,
          path: Some(normalized_path.clone()),
          message: err.to_string(),
        };
        let _ = app.emit("scan:error", event);
        let mut state = scan_state.lock().expect("scan state poisoned");
        state.last_error = Some(err.to_string());
      }
    }

    scanned_files += 1;
    {
      let mut state = scan_state.lock().expect("scan state poisoned");
      state.scanned_files = scanned_files;
      state.hashed_files = hashed_files;
      state.error_count = error_count;
    }

    let progress_event = ScanProgressEvent {
      scan_id,
      path: normalized_path,
      scanned_files,
      total_candidates: candidates.len() as u64,
      hashed_files,
      error_count,
    };
    let _ = app.emit("scan:progress", progress_event);
  }

  let mut removed_paths = 0_u64;
  for existing in db.list_paths_under_root(&canonical_root)? {
    if !seen_paths.contains(&existing) {
      db.delete_path(&existing)?;
      removed_paths += 1;
    }
  }

  db.cleanup_orphan_contents()?;
  db.finish_scan(scan_id, "completed", scanned_files, hashed_files, error_count)?;

  let finished_at = now_iso8601();
  {
    let mut state = scan_state.lock().expect("scan state poisoned");
    state.is_running = false;
    state.finished_at = Some(finished_at.clone());
    state.error_count = error_count;
    state.scanned_files = scanned_files;
    state.hashed_files = hashed_files;
  }

  let completed_event = ScanCompletedEvent {
    scan_id,
    root_path: canonical_root,
    scanned_files,
    hashed_files,
    error_count,
    removed_paths,
    finished_at,
  };
  let _ = app.emit("scan:completed", completed_event);

  Ok(())
}

fn process_video_file(db: &Db, scan_id: i64, path: &Path, path_string: &str) -> AppResult<bool> {
  let metadata = fs::metadata(path)?;
  let file_size = metadata.len();
  let mtime = metadata
    .modified()
    .ok()
    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
    .map(|duration| duration.as_secs() as i64)
    .unwrap_or(0);

  if let Some(existing) = db.get_path_metadata(path_string)? {
    if existing.size == file_size && existing.mtime == mtime {
      db.touch_path(path_string, scan_id)?;
      return Ok(false);
    }
  }

  let fingerprint_hash = fingerprint_file(path, file_size)?;
  let full_hash = full_hash_file(path)?;
  db.upsert_hashed_path(
    path_string,
    scan_id,
    mtime,
    file_size,
    &fingerprint_hash,
    &full_hash,
  )?;

  Ok(true)
}

fn normalize_root_path(root: &str) -> AppResult<String> {
  let canonical = fs::canonicalize(root)?;
  Ok(canonical.to_string_lossy().to_string())
}

fn is_video_file(path: &Path) -> bool {
  path
    .extension()
    .and_then(|ext| ext.to_str())
    .map(|ext| {
      let lower = ext.to_ascii_lowercase();
      VIDEO_EXTENSIONS.iter().any(|candidate| candidate == &lower)
    })
    .unwrap_or(false)
}

fn now_iso8601() -> String {
  chrono::Utc::now().to_rfc3339()
}
