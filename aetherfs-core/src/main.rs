mod governor;
mod filter;
mod classifier;
mod dedup;
mod extract;
mod index;
mod ipc;

use std::sync::Arc;
use std::path::{Path, PathBuf};
use notify::{Watcher, RecursiveMode, EventKind};
use tokio::sync::mpsc;
use index::IndexManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Set background scheduling priority on launch
    governor::set_background_priority();

    println!("==================================================");
    println!("     AETHERFS CORE BACKGROUND DAEMON STARTED       ");
    println!("==================================================");

    // Setup working directories and db files
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let aether_dir = home.join(".aetherfs");
    std::fs::create_dir_all(&aether_dir)?;

    let db_path = aether_dir.join("aetherfs_index.db");
    let model_dir = aether_dir.join("models");
    let qdrant_url = "http://localhost:6334";
    let collection_name = "aetherfs_files";
    
    // Fallback socket path for non-root accessibility
    let socket_path = "/tmp/aetherfs.sock";

    println!("AetherFS Config: DB Path: {}", db_path.display());
    println!("AetherFS Config: Model Dir: {}", model_dir.display());
    println!("AetherFS Config: Socket Path: {}", socket_path);

    // Initialize indexing and classification managers
    let index_manager = Arc::new(IndexManager::new(
        db_path,
        qdrant_url,
        collection_name,
        model_dir,
    )?);

    // Init collections (e.g. Qdrant Edge)
    let _ = index_manager.init().await;

    // Create a watcher channel to queue background indexing tasks safely
    let (watcher_tx, mut watcher_rx) = mpsc::channel(100);

    // Start FS Watcher on user directories
    let watch_targets = vec![
        home.join("Documents"),
        home.join("Desktop"),
        home.join("Downloads"),
    ];

    for target in &watch_targets {
        let _ = std::fs::create_dir_all(target);
        println!("AetherFS Watcher: Monitoring directory: {}", target.display());
    }

    let watcher_tx_clone = watcher_tx.clone();

    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => {
                    for path in event.paths {
                        let _ = watcher_tx_clone.blocking_send(WatcherEvent::Update(path));
                    }
                }
                EventKind::Remove(_) => {
                    for path in event.paths {
                        let _ = watcher_tx_clone.blocking_send(WatcherEvent::Remove(path));
                    }
                }
                _ => {}
            }
        }
    })?;

    for target in &watch_targets {
        watcher.watch(target, RecursiveMode::Recursive)?;
    }

    // Spawn watcher event processing loop with CpuGovernor throttling
    let index_mgr_proc = index_manager.clone();
    tokio::spawn(async move {
        let mut governor = governor::CpuGovernor::new(0.35);
        let filter = filter::PathFilter::new();
        // Debounce: skip re-processing the same path within this window
        let debounce_dur = std::time::Duration::from_secs(2);
        let mut last_indexed: std::collections::HashMap<std::path::PathBuf, std::time::Instant> = std::collections::HashMap::new();

        while let Some(evt) = watcher_rx.recv().await {
            match evt {
                WatcherEvent::Update(path) => {
                    if filter.should_exclude(&path) {
                        continue;
                    }

                    // Debounce: skip if we indexed this path recently
                    let now = std::time::Instant::now();
                    if let Some(last) = last_indexed.get(&path) {
                        if now.duration_since(*last) < debounce_dur {
                            continue;
                        }
                    }
                    last_indexed.insert(path.clone(), now);

                    if path.is_file() {
                        if let Ok(metadata) = std::fs::metadata(&path) {
                            let size = metadata.len() as i64;
                            let modified = metadata
                                .modified()
                                .ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0);

                            governor.start_work();
                            
                            // Check for duplicates
                            let mut is_duplicate = false;
                            let mut canonical_path: Option<String> = None;
                            let mut sparse_hash: Option<String> = None;
                            let mut full_hash: Option<String> = None;

                            if let Ok(candidates) = index_mgr_proc.sqlite().find_files_by_size(size, &path.to_string_lossy()) {
                                if !candidates.is_empty() {
                                    if let Ok(sh) = dedup::DedupPipeline::calculate_sparse_hash(&path, size as u64) {
                                        let sh_str = sh.to_string();
                                        sparse_hash = Some(sh_str.clone());

                                        for cand in candidates {
                                            if cand.sparse_hash.as_ref() == Some(&sh_str) {
                                                if let (Ok(fh), Ok(cand_fh_bytes)) = (
                                                    dedup::DedupPipeline::calculate_full_hash(&path),
                                                    dedup::DedupPipeline::calculate_full_hash(Path::new(&cand.path)),
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

                            // Perform indexing and vectorization
                            let path_display = path.display().to_string();
                            if let Err(e) = index_mgr_proc.index_file(
                                &path,
                                size,
                                modified,
                                sparse_hash,
                                full_hash,
                                is_duplicate,
                                canonical_path,
                            ).await {
                                eprintln!("AetherFS Indexer: Failed to index file {}: {}", path_display, e);
                            } else {
                                println!("AetherFS Indexer: Automatically indexed {}", path_display);
                            }

                            governor.end_work_and_throttle().await;
                        }
                    }
                }
                WatcherEvent::Remove(path) => {
                    // Handle file deletion from index/databases if necessary
                    println!("AetherFS Watcher: Detected file deletion: {}", path.display());
                    // In a production setup, we would execute SQL delete and Qdrant point delete here.
                }
            }
        }
    });

    // Start gRPC streaming service over Unix Domain Sockets
    ipc::start_ipc_server(index_manager, socket_path).await?;

    Ok(())
}

enum WatcherEvent {
    Update(PathBuf),
    Remove(PathBuf),
}

// Module helper dependencies
// (dirs crate used instead of custom module)
