//! AetherOS Audio-First IDE Daemon
//!
//! Runs as a background daemon, providing:
//! - Tree-sitter based code parsing
//! - Code sonification (structure → audio parameters)
//! - JSON-RPC IPC for Python voice UI
//!
//! Usage: aether-ide-daemon [--socket /tmp/aether-ide.sock]

use aether_ide::ipc::IdeServerState;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mut socket_path = "/tmp/aether-ide.sock".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" | "-s" => {
                i += 1;
                socket_path = args.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("error: --socket requires a value");
                    std::process::exit(1);
                });
            }
            "--help" | "-h" => {
                println!("AetherOS Audio-First IDE Daemon");
                println!();
                println!("Usage: aether-ide-daemon [--socket <path>]");
                println!();
                println!("Options:");
                println!("  --socket, -s <path>  Unix socket path (default: /tmp/aether-ide.sock)");
                println!("  --help, -h           Show this help");
                return Ok(());
            }
            other => {
                eprintln!("error: unknown flag '{other}'");
                eprintln!("usage: aether-ide-daemon [--socket <path>]");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    log::info!("AetherOS IDE Daemon starting...");
    log::info!("Socket: {}", socket_path);

    let state = Arc::new(IdeServerState::new());
    aether_ide::ipc::start_ipc_server(state, &socket_path).await
}
