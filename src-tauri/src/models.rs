use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRootOptionDto {
  pub id: i64,
  pub canonical_path: String,
  pub status: String,
  pub last_scanned_at: Option<String>,
  pub total_videos: u64,
  pub backed_up_videos: u64,
  pub backup_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTreeNodeDto {
  pub key: String,
  pub name: String,
  pub node_type: String,
  pub full_path: String,
  pub backup_count: u64,
  pub video_count: u64,
  pub backed_up_video_count: u64,
  pub backup_ratio: f64,
  pub children: Vec<BackupTreeNodeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTreeQueryDto {
  pub root_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatusDto {
  pub is_running: bool,
  pub root_path: Option<String>,
  pub total_candidates: u64,
  pub scanned_files: u64,
  pub hashed_files: u64,
  pub error_count: u64,
  pub last_error: Option<String>,
  pub started_at: Option<String>,
  pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStartedEvent {
  pub root_path: String,
  pub scan_id: i64,
  pub total_candidates: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressEvent {
  pub scan_id: i64,
  pub path: String,
  pub scanned_files: u64,
  pub total_candidates: u64,
  pub hashed_files: u64,
  pub error_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanErrorEvent {
  pub scan_id: i64,
  pub path: Option<String>,
  pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCompletedEvent {
  pub scan_id: i64,
  pub root_path: String,
  pub scanned_files: u64,
  pub hashed_files: u64,
  pub error_count: u64,
  pub removed_paths: u64,
  pub finished_at: String,
}

#[derive(Debug, Clone)]
pub struct InternalScanState {
  pub is_running: bool,
  pub root_path: Option<String>,
  pub total_candidates: u64,
  pub scanned_files: u64,
  pub hashed_files: u64,
  pub error_count: u64,
  pub last_error: Option<String>,
  pub started_at: Option<String>,
  pub finished_at: Option<String>,
}

impl Default for InternalScanState {
  fn default() -> Self {
    Self {
      is_running: false,
      root_path: None,
      total_candidates: 0,
      scanned_files: 0,
      hashed_files: 0,
      error_count: 0,
      last_error: None,
      started_at: None,
      finished_at: None,
    }
  }
}

impl InternalScanState {
  pub fn to_dto(&self) -> ScanStatusDto {
    ScanStatusDto {
      is_running: self.is_running,
      root_path: self.root_path.clone(),
      total_candidates: self.total_candidates,
      scanned_files: self.scanned_files,
      hashed_files: self.hashed_files,
      error_count: self.error_count,
      last_error: self.last_error.clone(),
      started_at: self.started_at.clone(),
      finished_at: self.finished_at.clone(),
    }
  }
}

#[derive(Debug, Clone)]
pub struct PathMetadata {
  pub mtime: i64,
  pub size: u64,
}

#[derive(Debug, Clone)]
pub struct RootRecord {
  pub id: i64,
  pub canonical_path: String,
  pub status: String,
  pub last_scanned_at: Option<String>,
  pub total_videos: u64,
  pub backed_up_videos: u64,
}

impl RootRecord {
  pub fn to_dto(&self) -> ScanRootOptionDto {
    let backup_ratio = if self.total_videos == 0 {
      0.0
    } else {
      self.backed_up_videos as f64 / self.total_videos as f64
    };

    ScanRootOptionDto {
      id: self.id,
      canonical_path: self.canonical_path.clone(),
      status: self.status.clone(),
      last_scanned_at: self.last_scanned_at.clone(),
      total_videos: self.total_videos,
      backed_up_videos: self.backed_up_videos,
      backup_ratio,
    }
  }
}

#[derive(Debug, Clone)]
pub struct TreeVideoRow {
  pub root_id: i64,
  pub root_path: String,
  pub dir_path: String,
  pub relative_path: String,
  pub file_name: String,
  pub backup_count: u64,
}
