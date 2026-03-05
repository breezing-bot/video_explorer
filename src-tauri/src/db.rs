use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::models::{HashWithPaths, PathMetadata};

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
      CREATE TABLE IF NOT EXISTS contents (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        full_hash TEXT NOT NULL UNIQUE,
        fingerprint_hash TEXT NOT NULL,
        file_size INTEGER NOT NULL,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
      );

      CREATE TABLE IF NOT EXISTS scans (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        root_path TEXT NOT NULL,
        started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        finished_at TEXT,
        status TEXT NOT NULL,
        scanned_files INTEGER NOT NULL DEFAULT 0,
        hashed_files INTEGER NOT NULL DEFAULT 0,
        error_count INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS paths (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        path TEXT NOT NULL UNIQUE,
        content_id INTEGER NOT NULL,
        mtime INTEGER NOT NULL,
        size INTEGER NOT NULL,
        last_seen_scan_id INTEGER NOT NULL,
        status TEXT NOT NULL DEFAULT 'ok',
        FOREIGN KEY (content_id) REFERENCES contents(id) ON DELETE CASCADE,
        FOREIGN KEY (last_seen_scan_id) REFERENCES scans(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_contents_fingerprint ON contents(fingerprint_hash);
      CREATE INDEX IF NOT EXISTS idx_paths_content_id ON paths(content_id);
      CREATE INDEX IF NOT EXISTS idx_paths_last_seen_scan ON paths(last_seen_scan_id);
      "#,
    )?;

    Ok(())
  }

  pub fn start_scan(&self, root_path: &str) -> AppResult<i64> {
    let conn = self.connection()?;
    conn.execute(
      "INSERT INTO scans(root_path, status) VALUES (?1, 'running')",
      params![root_path],
    )?;
    Ok(conn.last_insert_rowid())
  }

  pub fn finish_scan(
    &self,
    scan_id: i64,
    status: &str,
    scanned_files: u64,
    hashed_files: u64,
    error_count: u64,
  ) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      r#"
      UPDATE scans
      SET status = ?2,
          finished_at = CURRENT_TIMESTAMP,
          scanned_files = ?3,
          hashed_files = ?4,
          error_count = ?5
      WHERE id = ?1
      "#,
      params![
        scan_id,
        status,
        scanned_files as i64,
        hashed_files as i64,
        error_count as i64
      ],
    )?;
    Ok(())
  }

  pub fn get_path_metadata(&self, path: &str) -> AppResult<Option<PathMetadata>> {
    let conn = self.connection()?;
    let mut stmt = conn.prepare("SELECT mtime, size FROM paths WHERE path = ?1")?;
    let mut rows = stmt.query(params![path])?;

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

  pub fn touch_path(&self, path: &str, scan_id: i64) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      "UPDATE paths SET last_seen_scan_id = ?2, status = 'ok' WHERE path = ?1",
      params![path, scan_id],
    )?;
    Ok(())
  }

  pub fn upsert_hashed_path(
    &self,
    path: &str,
    scan_id: i64,
    mtime: i64,
    size: u64,
    fingerprint_hash: &str,
    full_hash: &str,
  ) -> AppResult<()> {
    let mut conn = self.connection()?;
    let tx = conn.transaction()?;

    tx.execute(
      r#"
      INSERT INTO contents(full_hash, fingerprint_hash, file_size)
      VALUES (?1, ?2, ?3)
      ON CONFLICT(full_hash) DO UPDATE SET
        fingerprint_hash = excluded.fingerprint_hash,
        file_size = excluded.file_size,
        updated_at = CURRENT_TIMESTAMP
      "#,
      params![full_hash, fingerprint_hash, size as i64],
    )?;

    let content_id: i64 = tx.query_row(
      "SELECT id FROM contents WHERE full_hash = ?1",
      params![full_hash],
      |row| row.get(0),
    )?;

    tx.execute(
      r#"
      INSERT INTO paths(path, content_id, mtime, size, last_seen_scan_id, status)
      VALUES (?1, ?2, ?3, ?4, ?5, 'ok')
      ON CONFLICT(path) DO UPDATE SET
        content_id = excluded.content_id,
        mtime = excluded.mtime,
        size = excluded.size,
        last_seen_scan_id = excluded.last_seen_scan_id,
        status = 'ok'
      "#,
      params![path, content_id, mtime, size as i64, scan_id],
    )?;

    tx.commit()?;
    Ok(())
  }

  pub fn list_paths_under_root(&self, root_path: &str) -> AppResult<Vec<String>> {
    let conn = self.connection()?;
    let root_with_sep = if root_path.ends_with('\\') || root_path.ends_with('/') {
      root_path.to_string()
    } else {
      format!("{root_path}\\")
    };

    let mut stmt = conn.prepare(
      r#"
      SELECT path
      FROM paths
      WHERE path = ?1 OR path LIKE ?2
      "#,
    )?;

    let rows = stmt.query_map(params![root_path, format!("{root_with_sep}%")], |row| {
      row.get::<usize, String>(0)
    })?;

    let mut paths = Vec::new();
    for item in rows {
      paths.push(item?);
    }

    Ok(paths)
  }

  pub fn delete_path(&self, path: &str) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute("DELETE FROM paths WHERE path = ?1", params![path])?;
    Ok(())
  }

  pub fn cleanup_orphan_contents(&self) -> AppResult<()> {
    let conn = self.connection()?;
    conn.execute(
      r#"
      DELETE FROM contents
      WHERE id NOT IN (SELECT DISTINCT content_id FROM paths)
      "#,
      [],
    )?;
    Ok(())
  }

  pub fn query_hashes_with_paths(&self, duplicates_only: bool) -> AppResult<Vec<HashWithPaths>> {
    let conn = self.connection()?;
    let mut stmt = conn.prepare(
      r#"
      SELECT c.full_hash, c.fingerprint_hash, c.file_size, p.path
      FROM contents c
      INNER JOIN paths p ON p.content_id = c.id
      ORDER BY c.full_hash ASC, p.path ASC
      "#,
    )?;

    let rows = stmt.query_map([], |row| {
      Ok((
        row.get::<usize, String>(0)?,
        row.get::<usize, String>(1)?,
        row.get::<usize, i64>(2)?,
        row.get::<usize, String>(3)?,
      ))
    })?;

    let mut grouped: BTreeMap<String, HashWithPaths> = BTreeMap::new();

    for row in rows {
      let (full_hash, fingerprint_hash, file_size, path) = row?;
      let entry = grouped
        .entry(full_hash.clone())
        .or_insert_with(|| HashWithPaths {
          full_hash,
          fingerprint_hash,
          file_size: file_size.max(0) as u64,
          paths: Vec::new(),
          occurrence_count: 0,
        });
      entry.paths.push(path);
      entry.occurrence_count += 1;
    }

    let entries = grouped
      .into_values()
      .filter(|entry| !duplicates_only || entry.occurrence_count > 1)
      .collect();

    Ok(entries)
  }

  fn connection(&self) -> AppResult<Connection> {
    let conn = Connection::open(&self.db_path)
      .map_err(|err| AppError::DbInit(format!("open database failed: {err}")))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
      .map_err(|err| AppError::DbInit(format!("configure database failed: {err}")))?;
    Ok(conn)
  }
}
