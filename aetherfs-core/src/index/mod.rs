pub mod sqlite;
pub mod qdrant;

use std::path::Path;
use chrono::{DateTime, Utc, Local, Timelike};
use sqlite::{SqliteIndex, DbFileRecord, DbContentChunk};
use qdrant::QdrantIndex;
use crate::classifier::FileClassifier;
use crate::extract::{extract_text, chunk_text, ContentChunk};

pub struct IndexManager {
    sqlite: SqliteIndex,
    qdrant: QdrantIndex,
    classifier: FileClassifier,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub filename: String,
    pub size_bytes: i64,
    pub classified_type: String,
    pub score: f32,
    pub spoken_summary: String,
    pub temporal_context: String,
    pub location_context: String,
    pub has_duplicates: bool,
}

impl IndexManager {
    /// Initialize Sqlite and Qdrant Indexers.
    pub fn new<P: AsRef<Path>>(
        db_path: P,
        qdrant_url: &str,
        collection_name: &str,
        model_dir: P,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let sqlite = SqliteIndex::new(db_path)?;
        let qdrant = QdrantIndex::new(qdrant_url, collection_name);
        let classifier = FileClassifier::new(model_dir);

        Ok(Self {
            sqlite,
            qdrant,
            classifier,
        })
    }

    /// Asynchronously initialize the search engines (e.g. create Qdrant collection).
    pub async fn init(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = self.qdrant.init_collection().await;
        Ok(())
    }

    /// Access underlying SQLite index.
    pub fn sqlite(&self) -> &SqliteIndex {
        &self.sqlite
    }

    /// Access underlying classifier.
    pub fn classifier(&self) -> &FileClassifier {
        &self.classifier
    }

    /// Indexes a single file: Classifies, extracts content, generates spoken summaries/anchors, computes embeddings, and updates databases.
    pub async fn index_file(
        &self,
        path: &Path,
        size_bytes: i64,
        modified_time: i64,
        sparse_hash: Option<String>,
        full_hash: Option<String>,
        is_duplicate: bool,
        canonical_path: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // 1. Classify the file using Magic + ONNX
        let (classified_type, detail_tag) = self.classifier.classify(path);

        // 2. Generate Conversational Anchors
        let spoken_summary = generate_spoken_summary(&filename, &classified_type, &detail_tag, size_bytes);
        let temporal_context = generate_temporal_context(modified_time);
        let location_context = generate_location_context(path);

        // 3. Extract text content and create chunks
        let content_chunks: Vec<ContentChunk> = if let Some(text) = extract_text(path) {
            let chunks = chunk_text(&text, 1000);
            chunks
                .into_iter()
                .map(|(idx, chunk_text)| ContentChunk {
                    source_path: path.to_string_lossy().to_string(),
                    chunk_index: idx,
                    text: chunk_text,
                    char_start: 0, // approximate — we lose exact offset with chunking
                    char_len: 0,
                })
                .collect()
        } else {
            Vec::new()
        };

        // 4. Save to SQLite
        let record = DbFileRecord {
            path: path.to_string_lossy().to_string(),
            filename,
            size_bytes,
            modified_time,
            classified_type: classified_type.clone(),
            sparse_hash,
            full_hash,
            is_duplicate,
            canonical_path,
            spoken_summary: spoken_summary.clone(),
            temporal_context: temporal_context.clone(),
            location_context: location_context.clone(),
        };
        self.sqlite.upsert_file(&record)?;

        // 5. Save content chunks to SQLite
        for chunk in &content_chunks {
            let db_chunk = DbContentChunk {
                source_path: chunk.source_path.clone(),
                chunk_index: chunk.chunk_index as i64,
                content: chunk.text.clone(),
            };
            let _ = self.sqlite.upsert_chunk(&db_chunk);
        }

        // 6. Generate Semantic Embeddings (if MiniLM loaded) and upload to Qdrant
        let text_to_embed = format!(
            "File named {} of type {}. Spoken summary: {}. Context: {}.",
            record.filename, record.classified_type, record.spoken_summary, record.temporal_context
        );
        if let Some(embedding) = self.classifier.get_text_embedding(&text_to_embed) {
            let _ = self.qdrant
                .upsert_vector(&record.path, embedding, &record.classified_type, &record.spoken_summary)
                .await;
        }

        // 7. Also embed each content chunk for fine-grained RAG
        for chunk in &content_chunks {
            if let Some(embedding) = self.classifier.get_text_embedding(&chunk.text) {
                let chunk_id = format!("{}#chunk{}", chunk.source_path, chunk.chunk_index);
                let _ = self.qdrant
                    .upsert_vector(&chunk_id, embedding, &format!("{}_chunk", record.classified_type), &chunk.text)
                    .await;
            }
        }

        Ok(())
    }

    /// Hybrid search: Combines SQLite FTS5 lexical results and Qdrant semantic vector results.
    pub async fn search_hybrid(&self, query_text: &str, limit: usize) -> Vec<SearchResult> {
        let mut combined_results = std::collections::HashMap::new();

        // 1. Lexical search (SQLite FTS5)
        if let Ok(lexical_matches) = self.sqlite.search_lexical(query_text) {
            for (rec, score) in lexical_matches {
                combined_results.insert(
                    rec.path.clone(),
                    SearchResult {
                        path: rec.path.clone(),
                        filename: rec.filename,
                        size_bytes: rec.size_bytes,
                        classified_type: rec.classified_type,
                        score: score * 0.4, // weight lexical score
                        spoken_summary: rec.spoken_summary,
                        temporal_context: rec.temporal_context,
                        location_context: rec.location_context,
                        has_duplicates: rec.is_duplicate,
                    },
                );
            }
        }

        // 2. Semantic search (Qdrant)
        if let Some(query_embedding) = self.classifier.get_text_embedding(query_text) {
            if let Ok(semantic_matches) = self.qdrant.search_vector(query_embedding, limit as u64).await {
                for (path, score, _) in semantic_matches {
                    if let Ok(Some(rec)) = self.sqlite.get_file(&path) {
                        if let Some(existing) = combined_results.get_mut(&path) {
                            // Boost score if found in both
                            existing.score += score * 0.6;
                        } else {
                            combined_results.insert(
                                path.clone(),
                                SearchResult {
                                    path,
                                    filename: rec.filename,
                                    size_bytes: rec.size_bytes,
                                    classified_type: rec.classified_type,
                                    score: score * 0.6,
                                    spoken_summary: rec.spoken_summary,
                                    temporal_context: rec.temporal_context,
                                    location_context: rec.location_context,
                                    has_duplicates: rec.is_duplicate,
                                },
                            );
                        }
                    }
                }
            }
        }

        // 3. Sort by score descending and return
        let mut results: Vec<SearchResult> = combined_results.into_values().collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }
}

// Spoken summary generator optimized for blind users.
fn generate_spoken_summary(filename: &str, mime: &str, details: &str, size_bytes: i64) -> String {
    let size_desc = if size_bytes >= 1_048_576 {
        format!("{:.1} megabytes", size_bytes as f64 / 1_048_576.0)
    } else if size_bytes >= 1024 {
        format!("{:.1} kilobytes", size_bytes as f64 / 1024.0)
    } else {
        format!("{} bytes", size_bytes)
    };

    let readable_type = match mime {
        "application/pdf" => "PDF document",
        "image/png" | "image/jpeg" | "image/gif" => "image file",
        "text/plain" => "text document",
        "text/json" => "structured JSON data file",
        "audio/mpeg" | "audio/wav" => "audio recording",
        "video/mp4" => "video recording",
        "application/zip" => "zip archive",
        _ => "unclassified file",
    };

    let detail_desc = if details.is_empty() || details == "binary" || details == "raw text" || details == "raw image" {
        "".to_string()
    } else {
        format!(", containing {}", details)
    };

    format!(
        "This is a {} named {}, sized at {}{}.",
        readable_type, filename, size_desc, detail_desc
    )
}

// Convert absolute unix epoch time to relative conversational phrase
fn generate_temporal_context(modified_time_secs: i64) -> String {
    let now = Utc::now().timestamp();
    let diff = now - modified_time_secs;

    if diff < 0 {
        return "Modified in the future".to_string();
    }

    let dt = DateTime::<Utc>::from_timestamp(modified_time_secs, 0)
        .unwrap_or(Utc::now())
        .with_timezone(&Local);

    let time_str = format!("{:02}:{:02}", dt.hour(), dt.minute());

    if diff < 60 {
        "modified just seconds ago".to_string()
    } else if diff < 3600 {
        let mins = diff / 60;
        format!("modified {} minutes ago at {}", mins, time_str)
    } else if diff < 86400 {
        let hours = diff / 3600;
        format!("modified {} hours ago at {}", hours, time_str)
    } else if diff < 172800 {
        format!("modified yesterday at {}", time_str)
    } else if diff < 604800 {
        let days = diff / 86400;
        let day_of_week = dt.format("%A").to_string();
        format!("modified {} days ago on {} at {}", days, day_of_week, time_str)
    } else {
        let date_str = dt.format("%B %d, %Y").to_string();
        format!("modified on {} at {}", date_str, time_str)
    }
}

// Generate relative physical/logical context for files
fn generate_location_context(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    if path_str.contains("/home/") {
        "stored inside your personal user profile".to_string()
    } else if path_str.contains("/media/") || path_str.contains("/mnt/") {
        "stored on a connected external storage drive".to_string()
    } else {
        "stored in the system hierarchy".to_string()
    }
}
