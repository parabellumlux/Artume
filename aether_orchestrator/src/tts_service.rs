//! AetherOS TTS Service
//!
//! Runs Piper TTS synthesis in a dedicated background thread so it never
//! blocks the main async loop. Accepts text via a channel, synthesizes,
//! and sends PCM samples to the audio output.

use crate::tts::{TtsConfig, TtsEngine};
use aether_audio::output::{AudioOutput, PlaybackRequest};
use log::{error, info};
use std::sync::mpsc;
use std::thread;

/// A TTS service that runs synthesis in a background thread.
pub struct TtsService {
    /// Send text to synthesize.
    tx: mpsc::Sender<String>,
}

impl TtsService {
    /// Start the TTS service. Loads Piper once in a background thread.
    /// Returns None if the model fails to load.
    pub fn start(audio_out: &AudioOutput) -> Option<Self> {
        let (tx, rx) = mpsc::channel::<String>();
        let out_tx = audio_out.sender();

        let _handle = thread::spawn(move || {
            info!("TtsService: loading Piper in background thread...");
            let mut engine = TtsEngine::new(TtsConfig::default());
            if let Err(e) = engine.load() {
                error!("TtsService: failed to load Piper: {e}");
                return;
            }
            info!("TtsService: Piper ready for synthesis");

            while let Ok(text) = rx.recv() {
                match engine.synthesize(&text) {
                    Ok(samples) => {
                        info!("TtsService: synthesized {} samples for '{}'", samples.len(), &text[..text.len().min(40)]);
                        let _ = out_tx.send(PlaybackRequest {
                            samples,
                            sample_rate: 22050,
                        });
                    }
                    Err(e) => {
                        error!("TtsService: synthesis failed: {e}");
                    }
                }
            }
        });

        // Give the thread a moment to start
        thread::sleep(std::time::Duration::from_millis(100));

        Some(Self { tx })
    }

    /// Send text to be synthesized and played. Non-blocking.
    pub fn speak(&self, text: &str) {
        let _ = self.tx.send(text.to_string());
    }
}
