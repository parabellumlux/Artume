pub mod grpc;

use std::sync::Arc;
use std::path::Path;
use tonic::transport::Server;
use aetherfs_proto::aetherfs::aether_engine_server::AetherEngineServer;
use crate::index::IndexManager;
use grpc::AetherEngineService;

/// Starts the gRPC IPC server. 
/// On Unix systems, it binds to a Unix Domain Socket (UDS) path (e.g. `/var/run/aetherfs.sock` or a local fallback).
/// On Windows systems, it binds to a TCP port or localhost loopback for convenience and speed compatibility.
pub async fn start_ipc_server(
    index_manager: Arc<IndexManager>,
    socket_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = AetherEngineService::new(index_manager);
    let server = AetherEngineServer::new(service);

    #[cfg(unix)]
    {
        println!("AetherFS IPC: Binding to Unix Domain Socket at '{}'", socket_path);
        
        // Ensure the directory for the socket path exists
        if let Some(parent) = Path::new(socket_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Clean up socket file if it already exists
        if Path::new(socket_path).exists() {
            let _ = std::fs::remove_file(socket_path);
        }

        let listener = tokio::net::UnixListener::bind(socket_path)?;
        let uds_stream = tokio_stream::wrappers::UnixListenerStream::new(listener);

        Server::builder()
            .add_service(server)
            .serve_with_incoming(uds_stream)
            .await?;
    }

    #[cfg(not(unix))]
    {
        // On Windows or other non-unix systems, bind to local loopback (e.g. 127.0.0.1:50051)
        let addr = "127.0.0.1:50051".parse()?;
        println!("AetherFS IPC: Unix domain sockets not supported. Binding to TCP local loopback at {}", addr);
        Server::builder()
            .add_service(server)
            .serve(addr)
            .await?;
    }

    Ok(())
}
