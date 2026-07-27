use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use blake3::Hasher;

/// Deduplication engine using a 3-stage matching pipeline:
/// 1. Exact Size
/// 2. BLAKE3 Sparse 24KB Hash (First 8KB, Middle 8KB, Last 8KB)
/// 3. BLAKE3 Full Hash
pub struct DedupPipeline;

impl DedupPipeline {
    /// Calculate the sparse BLAKE3 hash for a file.
    /// Sparse hash comprises:
    /// - First 8KB
    /// - Middle 8KB
    /// - Last 8KB
    /// If the file size is less than 24KB, we hash the entire file.
    pub fn calculate_sparse_hash(path: &Path, file_size: u64) -> std::io::Result<blake3::Hash> {
        let mut file = File::open(path)?;
        let mut hasher = Hasher::new();

        if file_size <= 24_576 {
            // File is small, read and hash the whole thing
            let mut buf = vec![0u8; file_size as usize];
            file.read_exact(&mut buf)?;
            hasher.update(&buf);
        } else {
            let chunk_size = 8192;
            let mut buf = vec![0u8; chunk_size];

            // 1. Read first 8KB
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(&mut buf)?;
            hasher.update(&buf);

            // 2. Read middle 8KB
            let middle_offset = (file_size / 2).saturating_sub(chunk_size as u64 / 2);
            file.seek(SeekFrom::Start(middle_offset))?;
            file.read_exact(&mut buf)?;
            hasher.update(&buf);

            // 3. Read last 8KB
            let last_offset = file_size.saturating_sub(chunk_size as u64);
            file.seek(SeekFrom::Start(last_offset))?;
            file.read_exact(&mut buf)?;
            hasher.update(&buf);
        }

        Ok(hasher.finalize())
    }

    /// Calculate the full BLAKE3 hash of a file.
    pub fn calculate_full_hash(path: &Path) -> std::io::Result<blake3::Hash> {
        let mut file = File::open(path)?;
        let mut hasher = Hasher::new();
        let mut buf = vec![0u8; 65536]; // 64KB read buffer

        loop {
            let bytes_read = file.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buf[..bytes_read]);
        }

        Ok(hasher.finalize())
    }
}
