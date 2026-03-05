use std::path::PathBuf;

use rusqlite::{params, params_from_iter, Connection};

use crate::error::{AppError, AppResult};
use crate::models::{PathMetadata, RootRecord, TreeVideoRow};

#[derive(Debug, Clone)]
pub struct Db {
  db_path: PathBuf,
}

impl Db {
  pub fn new(db_path: PathBuf) -> Self {
    Self { db_path }
  }

  pub fn init_schema(&self) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute_batch(
      r#"
      DROP TABLE IF EXISTS paths;
      DROP TABLE IF EXISTS contents;
      DROP TABLE IF EXISTS scans;

      CREATE TABLE IF NOT EXISTS scan_roots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        canonical_path TEXT NOT NULL UNIQUE,
        status TEXT NOT NULL DEFAULT 'idle',
        last_scanned_at TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
      );

      CREATE TABLE IF NOT EXISTS files (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        full_hash TEXT NOT NULL UNIQUE,
        fingerprint_hash TEXT NOT NULL,
        file_size INTEGER NOT NULL,
        backup_count INTEGER NOT NULL DEFAULT 0,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
      );

      CREATE TABLE IF NOT EXISTS file_locations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        file_id INTEGER NOT NULL,
        root_id INTEGER NOT NULL,
        relative_path TEXT NOT NULL,
        dir_path TEXT NOT NULL,
        file_name TEXT NOT NULL,
        mtime INTEGER NOT NULL,
        size INTEGER NOT NULL,
        present INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(root_id, relative_path),
        FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE,
        FOREIGN KEY (root_id) REFERENCES scan_roots(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS root_file_stats (
        root_id INTEGER PRIMARY KEY,
        total_videos INTEGER NOT NULL DEFAULT 0,
        backed_up_videos INTEGER NOT NULL DEFAULT 0,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY (root_id) REFERENCES scan_roots(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_scan_roots_path ON scan_roots(canonical_path);
      CREATE INDEX IF NOT EXISTS idx_file_locations_root_dir ON file_locations(root_id, dir_path);
      CREATE INDEX IF NOT EXISTS idx_file_locations_file_id ON file_locations(file_id);
      CREATE INDEX IF NOT EXISTS idx_file_locations_root_rel ON file_locations(root_id, relative_path);
      CREATE INDEX IF NOT EXISTS idx_files_backup_count ON files(backup_count);
      "#,
    )?;

    Ok(())
  }

  pub fn upsert_scan_root(&self, canonical_path: &str) -> AppResult<i64> {
    let conn = self.connection()?;
    conn.execute(
      r#"
      INSERT INTO scan_roots(canonical_path, status, last_scanned_at, updated_at)
      VALUES (?1, 'running', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
      ON CONFLICT(canonical_path) DO UPDATE SET
        status = 'running',
        last_scanned_at = CURRENT_TIMESTAMP,
        updated_at = CURRENT_TIMESTAMP
      "#,
      params![canonical_path],
    )?;

    let root_id: i64 = conn.query_row(
      "SELECT id FROM scan_roots WHERE canonical_path = ?1",
      params![canonical_path],
      |row| row.get(0),
    )?;

    Ok(root_id)
  }

  pub fn set_root_status(&self, root_id: i64, status: &str) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      r#"
      UPDATE scan_roots
      SET status = ?2,
          updated_at = CURRENT_TIMESTAMP,
          last_scanned_at = CASE WHEN ?2 = 'ready' THEN CURRENT_TIMESTAMP ELSE last_scanned_at END
      WHERE id = ?1
      "#,
      params![root_id, status],
    )?;
    Ok(())
  }

  pub fn mark_root_locations_missing(&self, root_id: i64) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      "UPDATE file_locations SET present = 0 WHERE root_id = ?1",
      params![root_id],
    )?;
    Ok(())
  }

  pub fn get_path_metadata(&self, root_id: i64, relative_path: &str) -> AppResult<Option<PathMetadata>> {
    let conn = self.connection()?;
    let mut stmt = conn.prepare(
      "SELECT mtime, size FROM file_locations WHERE root_id = ?1 AND relative_path = ?2",
    )?;
    let mut rows = stmt.query(params![root_id, relative_path])?;

    if let Some(row) = rows.next()? {
      let mtime: i64 = row.get(0)?;
      let size: i64 = row.get(1)?;
      return Ok(Some(PathMetadata {
        mtime,
        size: size.max(0) as u64,
      }));
    }

    Ok(None)
  }

  pub fn touch_path(&self, root_id: i64, relative_path: &str) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      r#"
      UPDATE file_locations
      SET present = 1,
          updated_at = CURRENT_TIMESTAMP
      WHERE root_id = ?1 AND relative_path = ?2
      "#,
      params![root_id, relative_path],
    )?;
    Ok(())
  }

  pub fn upsert_hashed_path(
    &self,
    root_id: i64,
    relative_path: &str,
    dir_path: &str,
    file_name: &str,
    mtime: i64,
    size: u64,
    fingerprint_hash: &str,
    full_hash: &str,
  ) -> AppResult<()> {
    let mut conn = self.connection()?;
    let tx = conn.transaction()?;

    tx.execute(
      r#"
      INSERT INTO files(full_hash, fingerprint_hash, file_size, backup_count, updated_at)
      VALUES (?1, ?2, ?3, 0, CURRENT_TIMESTAMP)
      ON CONFLICT(full_hash) DO UPDATE SET
        fingerprint_hash = excluded.fingerprint_hash,
        file_size = excluded.file_size,
        updated_at = CURRENT_TIMESTAMP
      "#,
      params![full_hash, fingerprint_hash, size as i64],
    )?;

    let file_id: i64 = tx.query_row(
      "SELECT id FROM files WHERE full_hash = ?1",
      params![full_hash],
      |row| row.get(0),
    )?;

    tx.execute(
      r#"
      INSERT INTO file_locations(root_id, file_id, relative_path, dir_path, file_name, mtime, size, present, updated_at)
      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, CURRENT_TIMESTAMP)
      ON CONFLICT(root_id, relative_path) DO UPDATE SET
        file_id = excluded.file_id,
        dir_path = excluded.dir_path,
        file_name = excluded.file_name,
        mtime = excluded.mtime,
        size = excluded.size,
        present = 1,
        updated_at = CURRENT_TIMESTAMP
      "#,
      params![
        root_id,
        file_id,
        relative_path,
        dir_path,
        file_name,
        mtime,
        size as i64,
      ],
    )?;

    tx.commit()?;
    Ok(())
  }

  pub fn delete_missing_locations(&self, root_id: i64) -> AppResult<u64> {
    let conn = self.connection()?;
    let removed = conn.execute(
      "DELETE FROM file_locations WHERE root_id = ?1 AND present = 0",
      params![root_id],
    )?;
    Ok(removed as u64)
  }

  pub fn cleanup_orphan_files(&self) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      r#"
      DELETE FROM files
      WHERE id NOT IN (SELECT DISTINCT file_id FROM file_locations)
      "#,
      [],
    )?;
    Ok(())
  }

  pub fn recompute_backup_counts(&self) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      r#"
      UPDATE files
      SET backup_count = (
            SELECT COUNT(*)
            FROM file_locations fl
            WHERE fl.file_id = files.id
          ),
          updated_at = CURRENT_TIMESTAMP
      "#,
      [],
    )?;
    Ok(())
  }

  pub fn recompute_root_stats(&self, root_id: i64) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      r#"
      INSERT INTO root_file_stats(root_id, total_videos, backed_up_videos, updated_at)
      SELECT
        ?1,
        COUNT(fl.id),
        COALESCE(SUM(CASE WHEN f.backup_count > 1 THEN 1 ELSE 0 END), 0),
        CURRENT_TIMESTAMP
      FROM file_locations fl
      LEFT JOIN files f ON f.id = fl.file_id
      WHERE fl.root_id = ?1
      ON CONFLICT(root_id) DO UPDATE SET
        total_videos = excluded.total_videos,
        backed_up_videos = excluded.backed_up_videos,
        updated_at = CURRENT_TIMESTAMP
      "#,
      params![root_id],
    )?;
    Ok(())
  }

  pub fn list_scan_roots(&self) -> AppResult<Vec<RootRecord>> {
    let conn = self.connection()?;
    let mut stmt = conn.prepare(
      r#"
      SELECT
        sr.id,
        sr.canonical_path,
        sr.status,
        sr.last_scanned_at,
        COALESCE(rfs.total_videos, 0),
        COALESCE(rfs.backed_up_videos, 0)
      FROM scan_roots sr
      LEFT JOIN root_file_stats rfs ON rfs.root_id = sr.id
      ORDER BY sr.canonical_path ASC
      "#,
    )?;

    let rows = stmt.query_map([], |row| {
      Ok(RootRecord {
        id: row.get(0)?,
        canonical_path: row.get(1)?,
        status: row.get(2)?,
        last_scanned_at: row.get(3)?,
        total_videos: row.get::<usize, i64>(4)?.max(0) as u64,
        backed_up_videos: row.get::<usize, i64>(5)?.max(0) as u64,
      })
    })?;

    let mut roots = Vec::new();
    for row in rows {
      roots.push(row?);
    }

    Ok(roots)
  }

  pub fn list_scan_roots_by_ids(&self, root_ids: &[i64]) -> AppResult<Vec<RootRecord>> {
    if root_ids.is_empty() {
      return Ok(Vec::new());
    }

    let conn = self.connection()?;
    let placeholders = in_clause_placeholders(root_ids.len());
    let sql = format!(
      r#"
      SELECT
        sr.id,
        sr.canonical_path,
        sr.status,
        sr.last_scanned_at,
        COALESCE(rfs.total_videos, 0),
        COALESCE(rfs.backed_up_videos, 0)
      FROM scan_roots sr
      LEFT JOIN root_file_stats rfs ON rfs.root_id = sr.id
      WHERE sr.id IN ({placeholders})
      ORDER BY sr.canonical_path ASC
      "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(root_ids.iter()), |row| {
      Ok(RootRecord {
        id: row.get(0)?,
        canonical_path: row.get(1)?,
        status: row.get(2)?,
        last_scanned_at: row.get(3)?,
        total_videos: row.get::<usize, i64>(4)?.max(0) as u64,
        backed_up_videos: row.get::<usize, i64>(5)?.max(0) as u64,
      })
    })?;

    let mut roots = Vec::new();
    for row in rows {
      roots.push(row?);
    }

    Ok(roots)
  }

  pub fn query_tree_rows(&self, root_ids: &[i64]) -> AppResult<Vec<TreeVideoRow>> {
    if root_ids.is_empty() {
      return Ok(Vec::new());
    }

    let conn = self.connection()?;
    let placeholders = in_clause_placeholders(root_ids.len());
    let sql = format!(
      r#"
      SELECT
        sr.id,
        sr.canonical_path,
        fl.dir_path,
        fl.relative_path,
        fl.file_name,
        f.backup_count
      FROM file_locations fl
      INNER JOIN files f ON f.id = fl.file_id
      INNER JOIN scan_roots sr ON sr.id = fl.root_id
      WHERE fl.root_id IN ({placeholders})
      ORDER BY sr.canonical_path ASC, fl.dir_path ASC, fl.file_name ASC
      "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(root_ids.iter()), |row| {
      Ok(TreeVideoRow {
        root_id: row.get(0)?,
        root_path: row.get(1)?,
        dir_path: row.get(2)?,
        relative_path: row.get(3)?,
        file_name: row.get(4)?,
        backup_count: row.get::<usize, i64>(5)?.max(0) as u64,
      })
    })?;

    let mut items = Vec::new();
    for row in rows {
      items.push(row?);
    }

    Ok(items)
  }

  fn connection(&self) -> AppResult<Connection> {
    let conn = Connection::open(&self.db_path)
      .map_err(|err| AppError::DbInit(format!("open database failed: {err}")))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
      .map_err(|err| AppError::DbInit(format!("configure database failed: {err}")))?;
    Ok(conn)
  }
}

fn in_clause_placeholders(size: usize) -> String {
  vec!["?"; size].join(",")
}
