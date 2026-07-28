//! AetherOS Conversational Shell
//!
//! A text-based interactive shell that demonstrates the full conversational
//! pipeline: classify intent → dispatch → respond.
//!
//! Run with: cargo run -p aether-orchestrator --bin aether-shell
//!
//! For voice mode (requires model files):
//!   cargo run -p aether-orchestrator --bin aether-shell -- --voice

use aether_orchestrator::{ConversationConfig, ConversationLoop};
use std::io::{self, BufRead, Write};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    println!();
    println!("╔══════════════════════════════════════════════╗");
    println!("║        AetherOS Conversational Shell         ║");
    println!("║   Dual-GPU AI Pipeline (1080 + 1650S)       ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();
    println!("GPU Pipeline:");
    println!("  Router:       Nemotron-3 Nano on GTX 1650S (GPU 1)");
    println!("  Conversation: Llama 3.1 8B on GTX 1080 (GPU 0)");
    println!("  Embeddings:   nomic-embed-text on CPU");
    println!();
    println!("Type 'quit' or 'exit' to stop.");
    println!();

    let config = ConversationConfig {
        voice_enabled: std::env::args().any(|a| a == "--voice"),
        ..ConversationConfig::default()
    };

    let mut loop_ = ConversationLoop::new(config);
    loop_.load_all()?;

    if loop_.voice_enabled() {
        println!("Voice mode enabled (requires model files).");
    }
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("You > ");
        stdout.flush()?;

        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let line = line.trim().to_string();

        if line.is_empty() {
            continue;
        }

        if matches!(line.as_str(), "quit" | "exit" | "q") {
            println!("Goodbye!");
            break;
        }

        match loop_.process_turn(&line).await {
            Ok(turn) => {
                println!(
                    "Aether > [{}] {}",
                    turn.intent.label(),
                    turn.response
                );
                println!("       ({:.0} ms)", turn.turn_ms as f64);
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
        println!();
    }

    Ok(())
}
