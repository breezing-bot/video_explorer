use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::AppResult;

const SAMPLE_SIZE: usize = 64 * 1024;

pub fn fingerprint_file(path: &Path, file_size: u64) -> AppResult<String> {
  let mut file = File::open(path)?;
  let mut hasher = blake3::Hasher::new();

  let mut start_buf = vec![0_u8; SAMPLE_SIZE.min(file_size as usize)];
  if !start_buf.is_empty() {
    file.read_exact(&mut start_buf)?;
    hasher.update(&start_buf);
  }

  if file_size > SAMPLE_SIZE as u64 {
    let middle_start = file_size / 2;
    file.seek(SeekFrom::Start(middle_start.saturating_sub((SAMPLE_SIZE / 2) as u64)))?;
    let mut middle_buf = vec![0_u8; SAMPLE_SIZE.min(file_size as usize)];
    if !middle_buf.is_empty() {
      let bytes_read = file.read(&mut middle_buf)?;
      hasher.update(&middle_buf[..bytes_read]);
    }
  }

  if file_size > (SAMPLE_SIZE as u64) * 2 {
    file.seek(SeekFrom::End(-(SAMPLE_SIZE as i64)))?;
    let mut end_buf = vec![0_u8; SAMPLE_SIZE];
    let bytes_read = file.read(&mut end_buf)?;
    hasher.update(&end_buf[..bytes_read]);
  }

  hasher.update(&file_size.to_le_bytes());
  Ok(hasher.finalize().to_hex().to_string())
}

pub fn full_hash_file(path: &Path) -> AppResult<String> {
  let mut file = File::open(path)?;
  let mut hasher = blake3::Hasher::new();
  let mut buf = vec![0_u8; 128 * 1024];

  loop {
    let bytes_read = file.read(&mut buf)?;
    if bytes_read == 0 {
      break;
    }
    hasher.update(&buf[..bytes_read]);
  }

  Ok(hasher.finalize().to_hex().to_string())
}
