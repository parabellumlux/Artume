use std::sync::Arc;
use std::path::Path;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use aetherfs_proto::aetherfs::aether_engine_server::AetherEngine;
use aetherfs_proto::aetherfs::{
    VoiceSearchRequest, VoiceSearchResponse, IndexRequest, IndexResponse,
    DuplicateRequest, DuplicateResponse, DuplicateGroup, FileMatch, ConversationalAnchor,
};
use crate::index::IndexManager;

pub struct AetherEngineService {
    index_manager: Arc<IndexManager>,
}

impl AetherEngineService {
    pub fn new(index_manager: Arc<IndexManager>) -> Self {
        Self { index_manager }
    }
}

#[tonic::async_trait]
impl AetherEngine for AetherEngineService {
    type LiveVoiceSearchStream = ReceiverStream<Result<VoiceSearchResponse, Status>>;

    async fn live_voice_search(
        &self,
        request: Request<Streaming<VoiceSearchRequest>>,
    ) -> Result<Response<Self::LiveVoiceSearchStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(10);
        let index_manager = self.index_manager.clone();

        tokio::spawn(async move {
            while let Ok(Some(req)) = stream.message().await {
                let session_id = req.session_id.clone();
                
                // Extract query text
                let query_text = match &req.input {
                    Some(aetherfs_proto::aetherfs::voice_search_request::Input::TextQuery(txt)) => txt.clone(),
                    Some(aetherfs_proto::aetherfs::voice_search_request::Input::AudioChunk(_)) => {
                        // Mock speech-to-text transcription for voice channel.
                        // In production, we'd pipe audio into a Whisper/ONNX Speech engine.
                        "transcribed search query".to_string()
                    }
                    None => "".to_string(),
                };

                if query_text.is_empty() {
                    continue;
                }

                // Send processing status
                let _ = tx.send(Ok(VoiceSearchResponse {
                    session_id: session_id.clone(),
                    status: 0, // PROCESSING
                    partial_transcription: query_text.clone(),
                    spoken_summary: "".to_string(),
                    results: vec![],
                    error_message: "".to_string(),
                })).await;

                // Execute Hybrid Search (Lexical FTS5 + Semantic Vector)
                let start_time = std::time::Instant::now();
                let matches = index_manager.search_hybrid(&query_text, 10).await;
                let search_duration = start_time.elapsed();
                println!("AetherFS Search: Responded in {:?}", search_duration);

                // Build matching response list
                let results: Vec<FileMatch> = matches
                    .into_iter()
                    .map(|m| FileMatch {
                        path: m.path,
                        filename: m.filename,
                        size_bytes: m.size_bytes,
                        classified_type: m.classified_type,
                        score: m.score,
                        conversational_anchor: Some(ConversationalAnchor {
                            spoken_summary: m.spoken_summary,
                            temporal_context: m.temporal_context,
                            location_context: m.location_context,
                        }),
                        has_duplicates: m.has_duplicates,
                    })
                    .collect();

                // Build synthesis spoken summary
                let spoken_summary = if results.is_empty() {
                    format!("I couldn't find any files matching {}.", query_text)
                } else {
                    format!("I found {} matches. The top match is {}.", results.len(), results[0].filename)
                };

                // Send finished results
                let response = VoiceSearchResponse {
                    session_id: session_id.clone(),
                    status: 2, // COMPLETED
                    partial_transcription: query_text.clone(),
                    spoken_summary,
                    results,
                    error_message: "".to_string(),
                };

                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn index_directory(
        &self,
        request: Request<IndexRequest>,
    ) -> Result<Response<IndexResponse>, Status> {
        let req = request.into_inner();
        let path = std::path::PathBuf::from(&req.directory_path);
        
        if !path.exists() {
            return Err(Status::invalid_argument("Directory path does not exist"));
        }

        // Trigger indexing task in the background.
        // It runs asynchronously under lower scheduling priority.
        let manager = self.index_manager.clone();
        tokio::spawn(async move {
            // Traverse directory recursively and register files.
            // Let's implement directory scanner logic.
            println!("AetherFS Background: Starting directory scan of {}", path.display());
            let mut scanned = 0;
            
            // Re-use core scanner setup (this would trigger daemon's scanner logic)
            // For now, print status
            let filter = crate::filter::PathFilter::default();
            let mut governor = crate::governor::CpuGovernor::new(0.35);

            fn scan_dir(
                dir: &std::path::Path,
                mgr: &IndexManager,
                flt: &crate::filter::PathFilter,
                gov: &mut crate::governor::CpuGovernor,
                counter: &mut i64,
            ) {
                if flt.should_exclude(dir) {
                    return;
                }

                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if flt.should_exclude(&path) {
                            continue;
                        }

                        if path.is_dir() {
                            scan_dir(&path, mgr, flt, gov, counter);
                        } else if path.is_file() {
                            if let Ok(metadata) = entry.metadata() {
                                let size = metadata.len() as i64;
                                let modified = metadata
                                    .modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| d.as_secs() as i64)
                                    .unwrap_or(0);

                                // Background tasks must run on low thread priority & throttled
                                gov.start_work();

                                // Hashing/Deduplication check.
                                // First search SQLite for file size matches (Stage 1)
                                let mut is_duplicate = false;
                                let mut canonical_path: Option<String> = None;
                                let mut sparse_hash: Option<String> = None;
                                let mut full_hash: Option<String> = None;

                                if let Ok(candidates) = mgr.sqlite().find_files_by_size(size, &path.to_string_lossy()) {
                                    if !candidates.is_empty() {
                                        // Compute sparse hash (Stage 2)
                                        if let Ok(sh) = crate::dedup::DedupPipeline::calculate_sparse_hash(&path, size as u64) {
                                            let sh_str = sh.to_string();
                                            sparse_hash = Some(sh_str.clone());

                                            for cand in candidates {
                                                if cand.sparse_hash.as_ref() == Some(&sh_str) {
                                                    // Compute full hash (Stage 3)
                                                    if let (Ok(fh), Ok(cand_fh_bytes)) = (
                                                        crate::dedup::DedupPipeline::calculate_full_hash(&path),
                                                        crate::dedup::DedupPipeline::calculate_full_hash(Path::new(&cand.path)),
                                                    ) {
                                                        let fh_str = fh.to_string();
                                                        let cand_fh_str = cand_fh_bytes.to_string();
                                                        full_hash = Some(fh_str.clone());

                                                        if fh_str == cand_fh_str {
                                                            is_duplicate = true;
                                                            canonical_path = Some(cand.path.clone());
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Index file in the database
                                let _ = tokio::task::block_in_place(|| {
                                    futures::executor::block_on(mgr.index_file(
                                        &path,
                                        size,
                                        modified,
                                        sparse_hash,
                                        full_hash,
                                        is_duplicate,
                                        canonical_path,
                                    ))
                                });

                                *counter += 1;

                                // Sleep thread if necessary to keep core use below 35%
                                futures::executor::block_on(gov.end_work_and_throttle());
                            }
                        }
                    }
                }
            }

            scan_dir(&path, &manager, &filter, &mut governor, &mut scanned);
            println!("AetherFS Background: Completed directory scan. Indexed {} files.", scanned);
        });

        Ok(Response::new(IndexResponse {
            success: true,
            files_indexed: 0, // Scan occurs in background
            message: "Indexing initiated in background".to_string(),
        }))
    }

    async fn get_duplicates(
        &self,
        _request: Request<DuplicateRequest>,
    ) -> Result<Response<DuplicateResponse>, Status> {
        let db_groups = self
            .index_manager
            .sqlite()
            .get_duplicate_groups()
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let groups = db_groups
            .into_iter()
            .map(|(canonical, size, hash, duplicates)| DuplicateGroup {
                canonical_path: canonical,
                file_size_bytes: size,
                blake3_hash: hash,
                duplicate_paths: duplicates,
            })
            .collect();

        Ok(Response::new(DuplicateResponse { groups }))
    }
}
