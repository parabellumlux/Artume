//! AetherOS Text-to-Speech Engine
//!
//! Uses Kokoro-82M via a Python subprocess bridge for high-quality speech
//! synthesis on CPU. The Python bridge script handles model loading and
//! inference using the `kokoro` PyPI package with PyTorch on CPU.

use anyhow::{Context, Result};
use log::{debug, info};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

// ---------------------------------------------------------------------------
// TTS configuration
// ---------------------------------------------------------------------------

/// Configuration for the TTS engine.
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// Path to the Kokoro Python bridge script.
    pub script_path: String,
    /// Python interpreter to use (default: project venv python3).
    pub python_bin: String,
    /// Voice preset name (e.g., "af_heart", "am_adam").
    pub voice: String,
}

impl Default for TtsConfig {
    fn default() -> Self {
        // Resolve script path relative to the crate source
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("scripts")
            .join("kokoro_tts.py");

        // Resolve venv python relative to the crate source
        let venv_python = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".venv")
            .join("bin")
            .join("python3");

        Self {
            script_path: script.to_string_lossy().to_string(),
            python_bin: venv_python.to_string_lossy().to_string(),
            voice: "af_heart".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// TTS engine
// ---------------------------------------------------------------------------

/// Text-to-speech engine using Kokoro-82M via Python subprocess.
pub struct TtsEngine {
    config: TtsConfig,
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
            total_synthesized: 0,
            total_audio_seconds: 0.0,
        }
    }

    /// Verify the Python bridge script exists.
    pub fn load(&mut self) -> Result<()> {
        let script = PathBuf::from(&self.config.script_path);
        if !script.exists() {
            anyhow::bail!(
                "Kokoro bridge script not found at {}. Expected at scripts/kokoro_tts.py",
                script.display()
            );
        }

        // Quick check: python3 is available
        let check = Command::new("which")
            .arg(&self.config.python_bin)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("Failed to check for python3")?;
        if !check.success() {
            anyhow::bail!("python3 not found in PATH");
        }

        info!(
            "TtsEngine: Kokoro bridge ready (voice: {})",
            self.config.voice
        );

        Ok(())
    }

    /// Synthesize text to speech audio (f32 PCM, 24 kHz, mono).
    ///
    /// Calls the Python Kokoro bridge script, which outputs a WAV on stdout.
    /// We parse the WAV header and extract the raw PCM samples.
    pub fn synthesize(&mut self, text: &str) -> Result<Vec<f32>> {
        let start = Instant::now();

        debug!(
            "TtsEngine: synthesizing {} chars with Kokoro (voice: {})",
            text.len(),
            self.config.voice
        );

        // Spawn the Python bridge, pipe text in, get WAV out
        let mut child = Command::new(&self.config.python_bin)
            .args([&self.config.script_path, "--voice", &self.config.voice])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn Kokoro bridge process")?;

        // Write text to stdin and close it
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .context("Failed to write text to Kokoro bridge stdin")?;
        }

        let output = child
            .wait_with_output()
            .context("Kokoro bridge process failed")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "Kokoro bridge exited with code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr.lines().next().unwrap_or("unknown error")
            );
        }

        let wav_bytes = output.stdout;
        if wav_bytes.len() < 44 {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "Kokoro bridge produced no audio output (stderr: {})",
                stderr.lines().next().unwrap_or("empty")
            );
        }

        // Debug: check first bytes
        if &wav_bytes[..4] != b"RIFF" {
            let preview = String::from_utf8_lossy(&wav_bytes[..wav_bytes.len().min(200)]);
            anyhow::bail!(
                "Kokoro bridge output is not a WAV file (starts with {:02x?}): {}",
                &wav_bytes[..4],
                preview
            );
        }

        // Parse WAV using hound (already a dependency)
        let reader = hound::WavReader::new(std::io::Cursor::new(&wav_bytes))
            .context("Failed to parse WAV output from Kokoro bridge")?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        let channels = spec.channels as usize;

        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => match spec.bits_per_sample {
                16 => reader
                    .into_samples::<i16>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / 32768.0)
                    .collect(),
                8 => reader
                    .into_samples::<i8>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / 128.0)
                    .collect(),
                bps => anyhow::bail!("Unsupported WAV bit depth: {bps}"),
            },
            hound::SampleFormat::Float => reader
                .into_samples::<f32>()
                .filter_map(|s| s.ok())
                .collect(),
        };

        // If multi-channel, take only the first channel
        let samples: Vec<f32> = if channels > 1 {
            samples.iter().step_by(channels).copied().collect()
        } else {
            samples
        };

        let duration_secs = samples.len() as f32 / sample_rate as f32;

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

    /// Check if the engine is loaded (always true for subprocess-based engine).
    pub fn is_loaded(&self) -> bool {
        true
    }

    /// Get statistics.
    pub fn stats(&self) -> TtsStats {
        TtsStats {
            loaded: true,
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
        assert!(engine.is_loaded());
    }

    #[test]
    fn test_tts_config_defaults() {
        let config = TtsConfig::default();
        assert!(config.script_path.contains("kokoro_tts.py"));
        assert_eq!(config.voice, "af_heart");
    }
}
