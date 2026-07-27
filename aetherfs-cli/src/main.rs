use clap::{Parser, Subcommand};
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;

use aetherfs_proto::aetherfs::aether_engine_client::AetherEngineClient;
use aetherfs_proto::aetherfs::{
    VoiceSearchRequest, IndexRequest, DuplicateRequest,
};

#[derive(Parser)]
#[command(name = "aetherfs")]
#[command(about = "CLI client for the AetherFS conversational background file engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Live voice search using semantic and lexical queries
    Search {
        /// The query text to search for
        query: String,
        
        /// Optional path constraint to scope the search
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Request the background daemon to index a directory
    Index {
        /// The absolute path to scan and index
        path: String,
        
        /// Whether to index directories recursively (default: true)
        #[arg(short, long, default_value_t = true)]
        recursive: bool,
    },
    /// Retrieve lists of duplicate files across drives
    Dups,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let socket_path = "/tmp/aetherfs.sock";

    // Create the custom connection channel to gRPC daemon
    let channel = connect_ipc(socket_path).await?;
    let mut client = AetherEngineClient::new(channel);

    match args.command {
        Commands::Search { query, scope } => {
            println!("AetherFS CLI: Initiating search for: \"{}\"", query);
            
            // Build the request stream
            let req = VoiceSearchRequest {
                session_id: uuid::Uuid::new_v4().to_string(),
                input: Some(aetherfs_proto::aetherfs::voice_search_request::Input::TextQuery(query)),
                path_scope: scope.unwrap_or_default(),
            };

            // Stream request
            let request_stream = tokio_stream::iter(vec![req]);
            
            let response = client.live_voice_search(request_stream).await?;
            let mut response_stream = response.into_inner();

            while let Some(res) = response_stream.message().await? {
                println!("\n--- [Stream Response Update] ---");
                println!("Status: {:?}", res.status());
                if !res.spoken_summary.is_empty() {
                    println!("Spoken Summary (Voice Output Target):");
                    println!("  > \"{}\"", res.spoken_summary);
                }

                if !res.results.is_empty() {
                    println!("\nMatching Files Found:");
                    for (i, matched) in res.results.iter().enumerate() {
                        println!("  {}. {} (Score: {:.2})", i + 1, matched.filename, matched.score);
                        println!("     Path: {}", matched.path);
                        println!("     Type: {}", matched.classified_type);
                        if let Some(anchor) = &matched.conversational_anchor {
                            println!("     Spoken Anchor: {}", anchor.spoken_summary);
                            println!("     Temporal Context: {}", anchor.temporal_context);
                            println!("     Location Context: {}", anchor.location_context);
                        }
                        if matched.has_duplicates {
                            println!("     [WARNING: This file has duplicate copies on disk]");
                        }
                        println!();
                    }
                }
            }
        }
        Commands::Index { path, recursive } => {
            println!("AetherFS CLI: Requesting index of directory: {}", path);
            let req = IndexRequest {
                directory_path: path,
                recursive,
            };

            let response = client.index_directory(req).await?;
            let res_body = response.into_inner();

            if res_body.success {
                println!("Success: {}", res_body.message);
            } else {
                println!("Failed: {}", res_body.message);
            }
        }
        Commands::Dups => {
            println!("AetherFS CLI: Retrieving duplicate files registry...");
            let req = DuplicateRequest {
                path_scope: "".to_string(),
            };

            let response = client.get_duplicates(req).await?;
            let res_body = response.into_inner();

            if res_body.groups.is_empty() {
                println!("No duplicate files found.");
            } else {
                println!("Duplicate Groups Found:");
                for (i, group) in res_body.groups.iter().enumerate() {
                    println!("\nGroup {}: Size {} bytes", i + 1, group.file_size_bytes);
                    println!("  Canonical Path: {}", group.canonical_path);
                    println!("  BLAKE3 Hash:   {}", group.blake3_hash);
                    println!("  Duplicate Copies:");
                    for dup_path in &group.duplicate_paths {
                        println!("    - {}", dup_path);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn connect_ipc(socket_path: &str) -> Result<Channel, tonic::transport::Error> {
    #[cfg(unix)]
    {
        let path = socket_path.to_string();
        Endpoint::try_from("http://[::]:50051")?
            .connect_timeout(Duration::from_secs(5))
            .connect_with_connector(service_fn(move |_| {
                let p = path.clone();
                async move { tokio::net::UnixStream::connect(p).await }
            }))
            .await
    }

    #[cfg(not(unix))]
    {
        // Connect via TCP fallback
        let addr = "http://127.0.0.1:50051";
        Endpoint::try_from(addr)?
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await
    }
}
