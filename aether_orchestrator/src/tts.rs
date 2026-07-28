//! AetherOS Text-to-Speech Engine
//!
//! Wraps `any-tts` to synthesise speech from text using Kokoro-82M
//! on CPU via Candle.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::Path;
use std::time::Instant;

// ---------------------------------------------------------------------------
// TTS configuration
// ---------------------------------------------------------------------------

/// Configuration for the TTS engine.
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// Path to the Kokoro model directory (containing config.json, model.safetensors).
    pub model_path: String,
    /// Voice preset name (e.g., "af_heart", "am_adam").
    pub voice: String,
    /// Language code.
    pub language: String,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            model_path: "models/Kokoro-82M".to_string(),
            voice: "af_heart".to_string(),
            language: "en".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// TTS engine
// ---------------------------------------------------------------------------

/// Text-to-speech engine using Kokoro-82M on CPU.
///
/// Wraps `any-tts` for local, private speech synthesis.
pub struct TtsEngine {
    config: TtsConfig,
    /// The loaded Kokoro model.
    model: Option<Box<dyn any_tts::TtsModel>>,
    /// Total utterances synthesised.
    total_synthesized: u64,
    /// Total audio seconds generated.
    total_audio_seconds: f64,
}

impl TtsEngine {
    /// Create a new TTS engine.
    pub fn new(config: TtsConfig) -> Self {
        Self {
            config,
            model: None,
            total_synthesized: 0,
            total_audio_seconds: 0.0,
        }
    }

    /// Load the Kokoro model via `any-tts`.
    pub fn load(&mut self) -> Result<()> {
        let path = Path::new(&self.config.model_path);
        if !path.join("config.json").exists() {
            anyhow::bail!(
                "Kokoro model not found at {} — download from Hugging Face (hexgrad/Kokoro-82M)",
                self.config.model_path
            );
        }

        info!("TtsEngine: loading Kokoro model from {}", self.config.model_path);

        let model = any_tts::load_model(
            any_tts::TtsConfig::new(any_tts::ModelType::Kokoro)
                .with_model_path(&self.config.model_path),
        )
        .with_context(|| format!("Failed to load Kokoro model from {}", self.config.model_path))?;

        self.model = Some(model);
        info!("TtsEngine: Kokoro model loaded successfully");
        Ok(())
    }

    /// Synthesize text to speech audio (f32 PCM, 24 kHz, mono).
    ///
    /// Returns raw audio samples.
    pub fn synthesize(&mut self, text: &str) -> Result<Vec<f32>> {
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("TTS engine not loaded — call load() first"))?;

        let start = Instant::now();

        debug!(
            "TtsEngine: synthesizing {} chars (voice: {}, lang: {})",
            text.len(),
            self.config.voice,
            self.config.language
        );

        let audio = model
            .synthesize(
                &any_tts::SynthesisRequest::new(text)
                    .with_voice(&self.config.voice)
                    .with_language(&self.config.language),
            )
            .with_context(|| "TTS synthesis failed")?;

        let samples = audio.samples.clone();
        let sample_rate = audio.sample_rate as f32;
        let duration_secs = samples.len() as f32 / sample_rate;

        let elapsed = start.elapsed();
        let realtime_factor = if elapsed.as_secs_f32() > 0.0 {
            duration_secs / elapsed.as_secs_f32()
        } else {
            0.0
        };

        self.total_synthesized += 1;
        self.total_audio_seconds += duration_secs as f64;

        info!(
            "TtsEngine: synthesized {:.1}s of audio in {:.0}ms ({:.1}x realtime)",
            duration_secs,
            elapsed.as_millis(),
            realtime_factor
        );

        Ok(samples)
    }

    /// Check if the engine is loaded.
    pub fn is_loaded(&self) -> bool {
        self.model.is_some()
    }

    /// Get statistics.
    pub fn stats(&self) -> TtsStats {
        TtsStats {
            loaded: self.model.is_some(),
            total_synthesized: self.total_synthesized,
            total_audio_seconds: self.total_audio_seconds,
        }
    }
}

/// Statistics about the TTS engine.
#[derive(Debug, Clone)]
pub struct TtsStats {
    pub loaded: bool,
    pub total_synthesized: u64,
    pub total_audio_seconds: f64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_engine_creation() {
        let engine = TtsEngine::new(TtsConfig::default());
        assert!(!engine.is_loaded());
    }

    #[test]
    fn test_tts_config_defaults() {
        let config = TtsConfig::default();
        assert_eq!(config.voice, "af_heart");
        assert_eq!(config.language, "en");
    }
}
