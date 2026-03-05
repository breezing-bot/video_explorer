use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HashWithPaths {
  pub full_hash: String,
  pub fingerprint_hash: String,
  pub file_size: u64,
  pub paths: Vec<String>,
  pub occurrence_count: usize,
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
