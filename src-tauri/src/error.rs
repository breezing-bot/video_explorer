use std::io;

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
  #[error("io error: {0}")]
  Io(#[from] io::Error),
  #[error("database error: {0}")]
  Sqlite(#[from] rusqlite::Error),
  #[error("database init error: {0}")]
  DbInit(String),
  #[error("invalid path: {0}")]
  InvalidPath(String),
  #[error("scan already running")]
  ScanAlreadyRunning,
  #[error("task join error: {0}")]
  TaskJoin(String),
}
