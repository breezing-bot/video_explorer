use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, State};

use crate::db::Db;
use crate::error::AppError;
use crate::models::{
  BackupTreeNodeDto, BackupTreeQueryDto, InternalScanState, ScanErrorEvent, ScanStatusDto,
  ScanRootOptionDto, TreeVideoRow,
};
use crate::scanner::run_scan;

pub struct AppState {
  pub db: Db,
  pub scan_state: Arc<Mutex<InternalScanState>>,
}

#[derive(Debug, Clone)]
struct MutableTreeNode {
  key: String,
  name: String,
  node_type: String,
  full_path: String,
  backup_count: u64,
  video_count: u64,
  backed_up_video_count: u64,
  children: BTreeMap<String, MutableTreeNode>,
}

impl MutableTreeNode {
  fn new_dir(key: String, name: String, full_path: String) -> Self {
    Self {
      key,
      name,
      node_type: "directory".to_string(),
      full_path,
      backup_count: 0,
      video_count: 0,
      backed_up_video_count: 0,
      children: BTreeMap::new(),
    }
  }

  fn new_video(
    key: String,
    name: String,
    full_path: String,
    backup_count: u64,
    is_backed_up: bool,
  ) -> Self {
    Self {
      key,
      name,
      node_type: "video".to_string(),
      full_path,
      backup_count,
      video_count: 1,
      backed_up_video_count: if is_backed_up { 1 } else { 0 },
      children: BTreeMap::new(),
    }
  }

  fn into_dto(mut self) -> BackupTreeNodeDto {
    let mut child_dtos = Vec::with_capacity(self.children.len());

    if self.node_type == "directory" {
      let mut total_videos = 0_u64;
      let mut total_backed_up = 0_u64;

      for (_, child) in std::mem::take(&mut self.children) {
        let child_dto = child.into_dto();
        total_videos += child_dto.video_count;
        total_backed_up += child_dto.backed_up_video_count;
        child_dtos.push(child_dto);
      }

      self.video_count = total_videos;
      self.backed_up_video_count = total_backed_up;
    }

    child_dtos.sort_by(|left, right| {
      left
        .node_type
        .cmp(&right.node_type)
        .then(left.name.cmp(&right.name))
    });

    let backup_ratio = if self.video_count == 0 {
      0.0
    } else {
      self.backed_up_video_count as f64 / self.video_count as f64
    };

    BackupTreeNodeDto {
      key: self.key,
      name: self.name,
      node_type: self.node_type,
      full_path: self.full_path,
      backup_count: self.backup_count,
      video_count: self.video_count,
      backed_up_video_count: self.backed_up_video_count,
      backup_ratio,
      children: child_dtos,
    }
  }
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
    let state_for_scan = scan_state.clone();

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
pub fn list_scan_roots(state: State<'_, AppState>) -> Result<Vec<ScanRootOptionDto>, String> {
  let roots = state
    .db
    .list_scan_roots()
    .map_err(|err| err.to_string())?
    .into_iter()
    .map(|record| record.to_dto())
    .collect();

  Ok(roots)
}

#[tauri::command]
pub fn get_backup_tree(
  query: BackupTreeQueryDto,
  state: State<'_, AppState>,
) -> Result<Vec<BackupTreeNodeDto>, String> {
  let mut root_ids: Vec<i64> = query
    .root_ids
    .into_iter()
    .filter(|id| *id > 0)
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect();

  if root_ids.is_empty() {
    return Ok(Vec::new());
  }

  root_ids.sort_unstable();

  let roots = state
    .db
    .list_scan_roots_by_ids(&root_ids)
    .map_err(|err| err.to_string())?;
  let rows = state
    .db
    .query_tree_rows(&root_ids)
    .map_err(|err| err.to_string())?;

  Ok(build_tree(roots, rows))
}

#[tauri::command]
pub fn get_scan_status(state: State<'_, AppState>) -> Result<ScanStatusDto, String> {
  let status = state
    .scan_state
    .lock()
    .map_err(|_| "scan state lock failed".to_string())?;
  Ok(status.to_dto())
}

fn build_tree(
  roots: Vec<crate::models::RootRecord>,
  rows: Vec<TreeVideoRow>,
) -> Vec<BackupTreeNodeDto> {
  let mut root_nodes: BTreeMap<i64, MutableTreeNode> = BTreeMap::new();

  for root in roots {
    root_nodes.insert(
      root.id,
      MutableTreeNode::new_dir(
        format!("root:{}", root.id),
        root.canonical_path.clone(),
        root.canonical_path,
      ),
    );
  }

  for row in rows {
    let Some(root_node) = root_nodes.get_mut(&row.root_id) else {
      continue;
    };

    let mut current = root_node;

    if !row.dir_path.is_empty() {
      let mut built = String::new();
      for segment in row.dir_path.split('/') {
        if segment.is_empty() {
          continue;
        }

        if !built.is_empty() {
          built.push('/');
        }
        built.push_str(segment);

        let key = format!("dir:{}:{}", row.root_id, built);
        current = current
          .children
          .entry(key.clone())
          .or_insert_with(|| {
            MutableTreeNode::new_dir(
              key,
              segment.to_string(),
              format!("{}/{}", row.root_path, built),
            )
          });
      }
    }

    let video_key = format!("video:{}:{}", row.root_id, row.relative_path);
    let is_backed_up = row.backup_count > 1;

    current.children.insert(
      video_key.clone(),
      MutableTreeNode::new_video(
        video_key,
        row.file_name,
        format!("{}/{}", row.root_path, row.relative_path),
        row.backup_count,
        is_backed_up,
      ),
    );
  }

  root_nodes.into_values().map(|node| node.into_dto()).collect()
}
