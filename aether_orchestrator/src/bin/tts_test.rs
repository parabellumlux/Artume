//! Quick TTS test: synthesize text and play it using Kokoro via Python bridge.
//! Usage:
//!   cargo run --bin tts-test
//!   cargo run --bin tts-test -- --device default
//!   cargo run --bin tts-test -- --sink bluez_output.54_15_89_14_C7_DE.1

use aether_audio::output::AudioOutput;
use aether_orchestrator::tts::{TtsConfig, TtsEngine};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mut device: Option<&str> = None;
    let mut sink: Option<&str> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--device" | "-d" => {
                i += 1;
                device = args.get(i).map(|s| s.as_str());
            }
            "--sink" | "-s" => {
                i += 1;
                sink = args.get(i).map(|s| s.as_str());
            }
            _ => {
                eprintln!("usage: tts-test [--device <name>] [--sink <name>]");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    println!("Loading Kokoro...");
    let mut engine = TtsEngine::new(TtsConfig::default());
    engine.load()?;
    println!("Kokoro loaded.");

    println!("Starting audio output...");
    let audio = if let Some(s) = sink {
        AudioOutput::with_sink(s)
            .map_err(|e| anyhow::anyhow!("Audio output failed: {e}"))?
    } else {
        AudioOutput::with_device(device)
            .map_err(|e| anyhow::anyhow!("Audio output failed: {e}"))?
    };
    println!("Audio output ready.");

    let text = "Hello, this is a test of the Kokoro voice through the LG RP4 speaker.";
    println!("Synthesizing: \"{text}\"");
    let samples = engine.synthesize(text)?;
    let duration_secs = samples.len() as f32 / 24000.0;
    println!("Synthesized {} samples ({:.1}s), playing...", samples.len(), duration_secs);

    audio.play(samples, 24000);
    std::thread::sleep(Duration::from_secs_f32(duration_secs + 1.0));

    println!("Done.");
    Ok(())
}
