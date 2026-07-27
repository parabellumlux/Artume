use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::sync::Mutex;

pub struct SqliteIndex {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct DbFileRecord {
    pub path: String,
    pub filename: String,
    pub size_bytes: i64,
    pub modified_time: i64,
    pub classified_type: String,
    pub sparse_hash: Option<String>,
    pub full_hash: Option<String>,
    pub is_duplicate: bool,
    pub canonical_path: Option<String>,
    pub spoken_summary: String,
    pub temporal_context: String,
    pub location_context: String,
}

impl SqliteIndex {
    /// Open or create the SQLite database file.
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        
        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON;", [])?;

        let s = Self {
            conn: Mutex::new(conn),
        };
        s.initialize_schema()?;
        Ok(s)
    }

    fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Core files metadata table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                filename TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                modified_time INTEGER NOT NULL,
                classified_type TEXT NOT NULL,
                sparse_hash TEXT,
                full_hash TEXT,
                is_duplicate INTEGER DEFAULT 0,
                canonical_path TEXT,
                spoken_summary TEXT NOT NULL,
                temporal_context TEXT NOT NULL,
                location_context TEXT NOT NULL
            );",
            [],
        )?;

        // FTS5 Lexical Search table
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
                path UNINDEXED,
                filename,
                classified_type,
                spoken_summary,
                temporal_context,
                content = 'files',
                content_rowid = 'id'
            );",
            [],
        )?;

        // FTS5 Triggers to automatically sync files -> files_fts
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS files_ai AFTER INSERT ON files BEGIN
                INSERT INTO files_fts(rowid, path, filename, classified_type, spoken_summary, temporal_context)
                VALUES (new.id, new.path, new.filename, new.classified_type, new.spoken_summary, new.temporal_context);
            END;",
            [],
        )?;

        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS files_ad AFTER DELETE ON files BEGIN
                INSERT INTO files_fts(files_fts, rowid, path, filename, classified_type, spoken_summary, temporal_context)
                VALUES('delete', old.id, old.path, old.filename, old.classified_type, old.spoken_summary, old.temporal_context);
            END;",
            [],
        )?;

        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS files_au AFTER UPDATE ON files BEGIN
                INSERT INTO files_fts(files_fts, rowid, path, filename, classified_type, spoken_summary, temporal_context)
                VALUES('delete', old.id, old.path, old.filename, old.classified_type, old.spoken_summary, old.temporal_context);
                INSERT INTO files_fts(rowid, path, filename, classified_type, spoken_summary, temporal_context)
                VALUES (new.id, new.path, new.filename, new.classified_type, new.spoken_summary, new.temporal_context);
            END;",
            [],
        )?;

        // Indexing fields for deduplication performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_size ON files(size_bytes);",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_hashes ON files(sparse_hash, full_hash);",
            [],
        )?;

        Ok(())
    }

    /// Insert or update a file record.
    pub fn upsert_file(&self, record: &DbFileRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO files (
                path, filename, size_bytes, modified_time, classified_type, 
                sparse_hash, full_hash, is_duplicate, canonical_path, 
                spoken_summary, temporal_context, location_context
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(path) DO UPDATE SET
                size_bytes = excluded.size_bytes,
                modified_time = excluded.modified_time,
                classified_type = excluded.classified_type,
                sparse_hash = excluded.sparse_hash,
                full_hash = excluded.full_hash,
                is_duplicate = excluded.is_duplicate,
                canonical_path = excluded.canonical_path,
                spoken_summary = excluded.spoken_summary,
                temporal_context = excluded.temporal_context,
                location_context = excluded.location_context;",
            params![
                record.path,
                record.filename,
                record.size_bytes,
                record.modified_time,
                record.classified_type,
                record.sparse_hash,
                record.full_hash,
                record.is_duplicate as i32,
                record.canonical_path,
                record.spoken_summary,
                record.temporal_context,
                record.location_context,
            ],
        )?;
        Ok(())
    }

    /// Retrieve all duplicate records.
    pub fn get_duplicate_groups(&self) -> Result<Vec<(String, i64, String, Vec<String>)>> {
        let conn = self.conn.lock().unwrap();
        // Query duplicate files grouped by their full_hash and size
        let mut stmt = conn.prepare(
            "SELECT canonical_path, size_bytes, full_hash, GROUP_CONCAT(path)
             FROM files
             WHERE is_duplicate = 1 AND full_hash IS NOT NULL AND canonical_path IS NOT NULL
             GROUP BY canonical_path, size_bytes, full_hash;",
        )?;

        let rows = stmt.query_map([], |row| {
            let canonical: String = row.get(0)?;
            let size: i64 = row.get(1)?;
            let hash: String = row.get(2)?;
            let dup_paths_str: String = row.get(3)?;
            let dups = dup_paths_str.split(',').map(|s| s.to_string()).collect();
            Ok((canonical, size, hash, dups))
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    /// Query for files with the exact same size (stage 1 of deduplication).
    pub fn find_files_by_size(&self, size: i64, exclude_path: &str) -> Result<Vec<DbFileRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT path, filename, size_bytes, modified_time, classified_type,
                    sparse_hash, full_hash, is_duplicate, canonical_path,
                    spoken_summary, temporal_context, location_context
             FROM files
             WHERE size_bytes = ?1 AND path != ?2;",
        )?;

        let rows = stmt.query_map(params![size, exclude_path], |row| self.row_to_record(row))?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    /// Query using FTS5 (lexical search).
    pub fn search_lexical(&self, query_text: &str) -> Result<Vec<(DbFileRecord, f32)>> {
        let conn = self.conn.lock().unwrap();
        // Clean query text for FTS5 syntax safety
        let safe_query = query_text.replace('"', "").replace('\'', "");
        if safe_query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            "SELECT f.path, f.filename, f.size_bytes, f.modified_time, f.classified_type,
                    f.sparse_hash, f.full_hash, f.is_duplicate, f.canonical_path,
                    f.spoken_summary, f.temporal_context, f.location_context, bm25(files_fts)
             FROM files f
             JOIN files_fts fts ON f.id = fts.rowid
             WHERE files_fts MATCH ?1
             ORDER BY bm25(files_fts) ASC
             LIMIT 25;",
        )?;

        let rows = stmt.query_map(params![format!("{}*", safe_query)], |row| {
            let record = self.row_to_record(row)?;
            let bm25_score: f64 = row.get(12)?;
            // Negate bm25 score because smaller is better, convert to positive score
            let score = (-bm25_score) as f32;
            Ok((record, score))
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    /// Fetch a file record by its path.
    pub fn get_file(&self, path: &str) -> Result<Option<DbFileRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT path, filename, size_bytes, modified_time, classified_type,
                    sparse_hash, full_hash, is_duplicate, canonical_path,
                    spoken_summary, temporal_context, location_context
             FROM files
             WHERE path = ?1;",
        )?;

        let mut rows = stmt.query_map(params![path], |row| self.row_to_record(row))?;
        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    fn row_to_record(&self, row: &rusqlite::Row) -> Result<DbFileRecord> {
        let is_dup_int: i32 = row.get(7)?;
        Ok(DbFileRecord {
            path: row.get(0)?,
            filename: row.get(1)?,
            size_bytes: row.get(2)?,
            modified_time: row.get(3)?,
            classified_type: row.get(4)?,
            sparse_hash: row.get(5)?,
            full_hash: row.get(6)?,
            is_duplicate: is_dup_int != 0,
            canonical_path: row.get(8)?,
            spoken_summary: row.get(9)?,
            temporal_context: row.get(10)?,
            location_context: row.get(11)?,
        })
    }
}
