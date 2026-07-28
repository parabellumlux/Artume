//! AetherOS Speech-to-Text Engine
//!
//! Wraps `whisper-rs` to transcribe audio from the microphone or audio
//! files using Whisper tiny.en on CPU.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::Path;
use std::time::Instant;

// ---------------------------------------------------------------------------
// STT configuration
// ---------------------------------------------------------------------------

/// Configuration for the STT engine.
#[derive(Debug, Clone)]
pub struct SttConfig {
    /// Path to the Whisper GGML model file (e.g., ggml-tiny.en.bin).
    pub model_path: String,
    /// Language code (e.g., "en" for English).
    pub language: String,
    /// Number of threads to use for inference.
    pub n_threads: i32,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            model_path: "models/ggml-tiny.en.bin".to_string(),
            language: "en".to_string(),
            n_threads: 4,
        }
    }
}

// ---------------------------------------------------------------------------
// STT engine
// ---------------------------------------------------------------------------

/// Speech-to-text engine using Whisper tiny.en on CPU.
///
/// Wraps `whisper-rs` for local, private transcription.
pub struct SttEngine {
    config: SttConfig,
    /// The loaded Whisper context.
    ctx: Option<whisper_rs::WhisperContext>,
    /// Total transcriptions processed.
    total_transcriptions: u64,
}

impl SttEngine {
    /// Create a new STT engine.
    pub fn new(config: SttConfig) -> Self {
        Self {
            config,
            ctx: None,
            total_transcriptions: 0,
        }
    }

    /// Load the Whisper model via `whisper-rs`.
    pub fn load(&mut self) -> Result<()> {
        let path = Path::new(&self.config.model_path);
        if !path.exists() {
            anyhow::bail!(
                "Whisper model not found: {} — download ggml-tiny.en.bin from Hugging Face",
                self.config.model_path
            );
        }

        info!("SttEngine: loading Whisper model from {}", self.config.model_path);

        let ctx = whisper_rs::WhisperContext::new_with_params(
            &self.config.model_path,
            whisper_rs::WhisperContextParameters::default(),
        )
        .with_context(|| format!("Failed to load Whisper model from {}", self.config.model_path))?;

        self.ctx = Some(ctx);
        info!("SttEngine: Whisper model loaded successfully");
        Ok(())
    }

    /// Transcribe audio samples (f32 PCM, 16 kHz, mono) to text.
    ///
    /// `samples` should be raw 16 kHz mono f32 PCM audio data.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        let ctx = self
            .ctx
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("STT engine not loaded — call load() first"))?;

        let start = Instant::now();
        let duration_secs = samples.len() as f32 / 16000.0;

        debug!(
            "SttEngine: transcribing {:.1}s of audio ({} samples)",
            duration_secs,
            samples.len()
        );

        // Build the inference parameters.
        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });

        params.set_n_threads(self.config.n_threads);
        params.set_language(Some(&self.config.language));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);

        // Create a state and run the full transcription.
        let mut state = ctx
            .create_state()
            .with_context(|| "Failed to create Whisper state")?;

        state
            .full(params, samples)
            .with_context(|| "Whisper transcription failed")?;

        // Collect the transcribed segments into a single string.
        let n_segments = state.full_n_segments();

        let mut text = String::new();
        for i in 0..n_segments {
            let segment = state
                .get_segment(i)
                .with_context(|| format!("Failed to get segment {i}"))?;
            let seg_text = segment
                .to_str_lossy()
                .with_context(|| format!("Failed to get segment text {i}"))?;
            text.push_str(&seg_text);
            text.push(' ');
        }

        let elapsed = start.elapsed();
        let realtime_factor = if elapsed.as_secs_f32() > 0.0 {
            duration_secs / elapsed.as_secs_f32()
        } else {
            0.0
        };

        self.total_transcriptions += 1;

        info!(
            "SttEngine: transcribed {:.1}s of audio in {:.0}ms ({:.1}x realtime): \"{}\"",
            duration_secs,
            elapsed.as_millis(),
            realtime_factor,
            text.trim().chars().take(80).collect::<String>()
        );

        Ok(text.trim().to_string())
    }

    /// Check if the engine is loaded.
    pub fn is_loaded(&self) -> bool {
        self.ctx.is_some()
    }

    /// Get statistics.
    pub fn stats(&self) -> SttStats {
        SttStats {
            loaded: self.ctx.is_some(),
            total_transcriptions: self.total_transcriptions,
        }
    }
}

/// Statistics about the STT engine.
#[derive(Debug, Clone)]
pub struct SttStats {
    pub loaded: bool,
    pub total_transcriptions: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stt_engine_creation() {
        let engine = SttEngine::new(SttConfig::default());
        assert!(!engine.is_loaded());
    }

    #[test]
    fn test_stt_config_defaults() {
        let config = SttConfig::default();
        assert_eq!(config.n_threads, 4);
        assert_eq!(config.language, "en");
    }
}
