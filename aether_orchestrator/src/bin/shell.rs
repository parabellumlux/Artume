//! AetherOS Conversational Shell
//!
//! Full-duplex conversational shell that:
//! - Polls for wake word detection (background thread)
//! - Accepts text input (non-blocking stdin polling)
//! - Captures mic audio on wake word → STT → process → TTS playback
//! - Plays TTS responses through the default audio output device

use aether_orchestrator::{ConversationConfig, ConversationLoop};
use aether_audio::output::AudioOutput;
use aether_audio::capture::capture_until_silence;
use cpal::traits::{DeviceTrait, HostTrait};
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    // Parse CLI args
    let args: Vec<String> = std::env::args().collect();
    let mut device_name: Option<String> = None;
    let mut sink_name: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--device" | "-d" => {
                i += 1;
                device_name = Some(args.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("error: --device requires a value (device name substring)");
                    std::process::exit(1);
                }));
            }
            "--sink" | "-s" => {
                i += 1;
                sink_name = Some(args.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("error: --sink requires a value (PulseAudio sink name)");
                    std::process::exit(1);
                }));
            }
            "--list-devices" | "-l" => {
                let host = cpal::default_host();
                println!("Available audio output devices:");
                for dev in host.output_devices().unwrap_or_else(|e| {
                    eprintln!("error: failed to enumerate devices: {e}");
                    std::process::exit(1);
                }) {
                    println!("  {}", dev.name().unwrap_or_else(|_| "unknown".into()));
                }
                return Ok(());
            }
            "--voice" => {
                // Accepted for compatibility with start.sh, but voice is always on
            }
            other => {
                eprintln!("error: unknown flag '{other}'");
                eprintln!("usage: aether-shell [--device <name>] [--sink <name>] [--list-devices] [--voice]");
                std::process::exit(1);
            }
        }
        i += 1;
    }

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
    println!("Subsystems:");
    println!("  Web Fetch:    aether_browser (HTTP + Readability)");
    println!("  Entity Lookup: aether_buffer (NER + ring buffer)");
    println!("  File Search:  aetherfs-core (gRPC + SQLite + Qdrant)");
    println!("  Spatial Audio: aether_audio (binaural HRTF mixer)");
    println!("  Attention:    aether_attention (cognitive load governor)");
    println!("  Wake Word:    OpenWakeWord (background mic listener)");
    println!("  Audio Output: cpal (default playback device)");
    println!();

    let config = ConversationConfig {
        voice_enabled: true,
        ..ConversationConfig::default()
    };

    let mut loop_ = ConversationLoop::new(config);
    loop_.load_all()?;

    // Health checks.
    print!("  Checking Ollama... ");
    io::stdout().flush()?;
    if loop_.check_ollama_health().await {
        println!("✓ connected");
    } else {
        println!("✗ not reachable — will use template fallbacks");
        println!("  Start Ollama with: ollama serve");
    }

    print!("  Checking file search daemon... ");
    io::stdout().flush()?;
    if loop_.check_file_search_health().await {
        println!("✓ connected");
    } else {
        println!("✗ not reachable — file search unavailable");
        println!("  Start with: cargo run --bin aetherfs-core");
    }

    // Start audio output.
    print!("  Starting audio output... ");
    io::stdout().flush()?;
    let audio_out = if let Some(ref sink) = sink_name {
        match AudioOutput::with_sink(sink) {
            Ok(out) => {
                println!("✓ ready (sink: {sink})");
                Some(out)
            }
            Err(e) => {
                println!("✗ {e}");
                None
            }
        }
    } else {
        match AudioOutput::with_device(device_name.as_deref()) {
            Ok(out) => {
                println!("✓ ready");
                Some(out)
            }
            Err(e) => {
                println!("✗ {e}");
                None
            }
        }
    };

    // Start TTS service in background thread.
    let tts = audio_out.as_ref().and_then(|out| {
        print!("  Starting TTS service... ");
        let svc = aether_orchestrator::tts_service::TtsService::start(out);
        if svc.is_some() {
            println!("✓ Piper ready in background");
        } else {
            println!("✗ failed to load Piper");
        }
        svc
    });

    // Start wake word detection.
    print!("  Starting wake word detection... ");
    io::stdout().flush()?;
    loop_.start_wake_word();
    if loop_.wake_word_active() {
        println!("✓ listening (say 'Hey Jarvis' to activate)");
    } else {
        println!("✗ not available — no mic or model issue");
    }
    println!();

    // Spawn a thread to read stdin lines and send them through a channel.
    let (input_tx, input_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => {
                    let trimmed = l.trim().to_string();
                    if !trimmed.is_empty() {
                        if input_tx.send(trimmed).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    println!("Ready. Say the wake word or type a message.");
    println!("Type 'quit' or 'exit' to stop.");
    println!();

    loop {
        // Poll for wake word events.
        if let Some(word) = loop_.check_wake_word() {
            println!("\nAether > Wake word '{}' detected! Listening...", word);
            if let Some(ref out) = audio_out {
                let beep = generate_beep(440.0, 0.15, 24000);
                out.play(beep, 24000);
            }

            // Capture mic audio until silence
            match capture_until_silence(10.0, 0.005, 800) {
                Ok(capture) => {
                    if capture.duration_secs < 0.3 {
                        println!("  (too short, ignoring)");
                        continue;
                    }
                    println!("  Captured {:.1}s of audio, transcribing...", capture.duration_secs);

                    // Transcribe with STT
                    match loop_.transcribe_audio(&capture.samples) {
                        Ok(text) => {
                            let text = text.trim().to_string();
                            if text.is_empty() {
                                println!("  (no speech detected)");
                                continue;
                            }
                            println!("  You said: {}", text);

                            // Process the turn
                            match loop_.process_turn(&text).await {
                                Ok(turn) => {
                                    let response = turn.response;
                                    println!(
                                        "Aether > [{}] {}",
                                        turn.intent.label(),
                                        response
                                    );
                                    println!("       ({:.0} ms)", turn.turn_ms as f64);

                                    // Play TTS response
                                    #[cfg(feature = "tts")]
                                    if loop_.voice_enabled() {
                                        if let Some(ref out) = audio_out {
                                            if let Ok(samples) = loop_.synthesize_speech(&response) {
                                                eprintln!("       (TTS: {} samples)", samples.len());
                                                out.play(samples, 22050);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("  Transcription failed: {e}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  Mic capture failed: {e}");
                }
            }
        }

        // Poll for text input (non-blocking).
        if let Ok(line) = input_rx.try_recv() {
            if matches!(line.as_str(), "quit" | "exit" | "q") {
                println!("Goodbye!");
                break;
            }

            match loop_.process_turn(&line).await {
                Ok(turn) => {
                    let response = turn.response;
                    println!(
                        "Aether > [{}] {}",
                        turn.intent.label(),
                        response
                    );
                    println!("       ({:.0} ms)", turn.turn_ms as f64);

                    // Play TTS response through audio output (non-blocking).
                    if let Some(ref tts) = tts {
                        tts.speak(&response);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                }
            }
            println!();
        }

        // Sleep a bit to avoid busy-waiting.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

/// Generate a simple sine wave beep for audio feedback.
fn generate_beep(freq: f32, duration_secs: f32, sample_rate: u32) -> Vec<f32> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * std::f32::consts::PI * freq * t).sin();
        // Apply envelope (fade in/out)
        let envelope = if i < 100 {
            i as f32 / 100.0
        } else if i > num_samples - 100 {
            (num_samples - i) as f32 / 100.0
        } else {
            1.0
        };
        samples.push(sample * envelope * 0.5);
    }
    samples
}
