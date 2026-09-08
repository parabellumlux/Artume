//! Artume TTS Service
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
#[derive(Clone)]
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
                        info!(
                            "TtsService: synthesized {} samples for '{}'",
                            samples.len(),
                            &text[..text.len().min(40)]
                        );
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

/// A streaming TTS sink that buffers incoming text and synthesizes on
/// sentence boundaries, so speech starts before the full response is done.
///
/// Feed it token chunks as they stream from the LLM; it flushes a sentence
/// to the underlying `TtsService` as soon as a sentence terminator is seen.
pub struct StreamingTts {
    /// Underlying TTS service.
    tts: TtsService,
    /// Buffer of text not yet flushed to a sentence.
    buffer: String,
}

impl StreamingTts {
    /// Create a streaming sink over an existing TTS service.
    pub fn new(tts: TtsService) -> Self {
        Self {
            tts,
            buffer: String::new(),
        }
    }

    /// Feed a token chunk. Flushes complete sentences to TTS.
    pub fn feed(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
        // Flush on sentence boundaries (period, question mark, exclamation,
        // newline). Keep a trailing partial sentence buffered.
        while let Some(pos) = self.buffer.find(['.', '!', '?', '\n']) {
            let end = pos + 1;
            let sentence = self.buffer[..end].trim().to_string();
            self.buffer.drain(..end);
            if !sentence.is_empty() {
                self.tts.speak(&sentence);
            }
        }
    }

    /// Flush any remaining buffered text to TTS. Call at end of stream.
    pub fn flush(&mut self) {
        let remaining = self.buffer.trim().to_string();
        self.buffer.clear();
        if !remaining.is_empty() {
            self.tts.speak(&remaining);
        }
    }
}
