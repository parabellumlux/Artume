//! AetherOS Wake Word Detection
//!
//! Runs OpenWakeWord ONNX models on CPU via tract-onnx to detect a wake word
//! from the microphone input. Runs on a background thread and signals the
//! orchestrator when the wake word is heard.
//!
//! ## Architecture
//! ```text
//! Mic (cpal) → 16kHz resample → AudioFeatures (MFCC) → ONNX model → detection callback
//! ```
//!
//! Uses `oww-rs` under the hood which bundles pre-trained OpenWakeWord models
//! (Alexa, Hey Mycroft, Hey Jarvis) and supports loading custom `.onnx` files.

use log::{debug, info, warn};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// ---------------------------------------------------------------------------
// Wake word detection events
// ---------------------------------------------------------------------------

/// Events emitted by the wake word detector.
#[derive(Debug, Clone)]
pub enum WakeWordEvent {
    /// Wake word detected with the given probability.
    Detected { probability: f32, word: String },
    /// An error occurred in the detection loop.
    Error(String),
}

// ---------------------------------------------------------------------------
// Wake word detector
// ---------------------------------------------------------------------------

/// Configuration for the wake word detector.
#[derive(Debug, Clone)]
pub struct WakeWordConfig {
    /// Which wake word model to use.
    pub model_source: WakeWordModel,
    /// Detection threshold (0.0–1.0). Lower = more sensitive but more false positives.
    pub threshold: f32,
    /// Which microphone device to use (None = default).
    pub device_name: Option<String>,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            model_source: WakeWordModel::BuiltInAlexa,
            threshold: 0.3,
            device_name: None,
        }
    }
}

/// Source of the wake word ONNX model.
#[derive(Debug, Clone)]
pub enum WakeWordModel {
    /// Use the built-in "Alexa" model (embedded in oww-rs).
    BuiltInAlexa,
    /// Use the built-in "Hey Mycroft" model.
    BuiltInHeyMycroft,
    /// Use the built-in "Hey Jarvis" model.
    BuiltInHeyJarvis,
    /// Load a custom OpenWakeWord ONNX model from a file path.
    Custom { path: String, trigger_word: String },
}

impl WakeWordModel {
    fn trigger_word(&self) -> &str {
        match self {
            WakeWordModel::BuiltInAlexa => "Alexa",
            WakeWordModel::BuiltInHeyMycroft => "Hey Mycroft",
            WakeWordModel::BuiltInHeyJarvis => "Hey Jarvis",
            WakeWordModel::Custom { trigger_word, .. } => trigger_word,
        }
    }
}

// ---------------------------------------------------------------------------
// Wake word detector
// ---------------------------------------------------------------------------

/// The wake word detector. Runs in a background thread.
pub struct WakeWordDetector {
    /// Channel receiver for wake word events.
    rx: mpsc::Receiver<WakeWordEvent>,
    /// Handle to stop the detector thread.
    stop_tx: Option<mpsc::Sender<()>>,
    /// The trigger word being listened for.
    trigger_word: String,
}

impl WakeWordDetector {
    /// Create and start a new wake word detector.
    ///
    /// Spawns a background thread that captures microphone audio and runs
    /// the wake word model. Events are received via the returned receiver.
    pub fn start(config: WakeWordConfig) -> Result<Self, String> {
        let trigger_word = config.model_source.trigger_word().to_string();
        let trigger_word_for_struct = trigger_word.clone();
        let trigger_word_for_thread = trigger_word.clone();
        let (event_tx, event_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();

        let threshold = config.threshold;
        let model_source = config.model_source.clone();

        thread::spawn(move || {
            info!("WakeWordDetector: starting with model '{}'", trigger_word_for_thread);

            // Build the OWW model
            let mut model = match build_model(&model_source, threshold) {
                Ok(m) => m,
                Err(e) => {
                    let msg = format!("Failed to load wake word model: {}", e);
                    warn!("WakeWordDetector: {}", msg);
                    let _ = event_tx.send(WakeWordEvent::Error(msg));
                    return;
                }
            };

            // Open the microphone
            let host = match cpal::default_host() {
                h => h,
            };

            let device = match config.device_name.as_ref() {
                Some(name) => {
                    host.devices()
                        .ok()
                        .and_then(|devs| {
                            devs.filter(|d| {
                                d.name().map(|n| n.contains(name)).unwrap_or(false)
                            })
                            .next()
                        })
                        .unwrap_or_else(|| {
                            warn!("WakeWordDetector: device '{}' not found, using default", name);
                            host.default_input_device().expect("No input device available")
                        })
                }
                None => host.default_input_device().expect("No input device available"),
            };

            let device_desc = device.name().unwrap_or_else(|_| "unknown".to_string());
            info!("WakeWordDetector: using mic device: {}", device_desc);

            // Find best config
            let (config, sample_format) = match find_best_config(&device, false) {
                Ok(c) => c,
                Err(e) => {
                    let msg = format!("No suitable audio config found: {}", e);
                    let _ = event_tx.send(WakeWordEvent::Error(msg.clone()));
                    warn!("WakeWordDetector: {}", msg);
                    return;
                }
            };

            let original_sample_rate = config.sample_rate as f32;
            let channels = config.channels as usize;

            // Build resampler (model expects 16kHz mono)
            let mut resampler = match make_resampler(original_sample_rate as _, 1280, channels) {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("Failed to create resampler: {}", e);
                    let _ = event_tx.send(WakeWordEvent::Error(msg));
                    return;
                }
            };

            let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(vec![]));
            let buffer_clone = buffer.clone();
            let event_tx_clone = event_tx.clone();
            let trigger_word_clone = trigger_word_for_thread.clone();

            // Build the input stream
            let err_fn = move |err| {
                warn!("WakeWordDetector: stream error: {}", err);
            };

            let stream = match build_input_stream(
                &device,
                &config,
                sample_format,
                move |data| {
                    let chunks = resample_into_chunks(
                        data,
                        &buffer_clone,
                        channels,
                        &mut resampler,
                    );
                    for chunk in chunks {
                        if let Some(channel_data) = chunk.data_f32.get(0) {
                            let d = model.detection(channel_data.to_vec());
                            if d.detected {
                                info!(
                                    "WakeWordDetector: detected '{}' (prob: {:.2})",
                                    trigger_word_clone, d.probability
                                );
                                let _ = event_tx_clone.send(WakeWordEvent::Detected {
                                    probability: d.probability,
                                    word: trigger_word_clone.clone(),
                                });
                            }
                        }
                    }
                },
                err_fn,
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!("Failed to create audio stream: {}", e);
                    let _ = event_tx.send(WakeWordEvent::Error(msg));
                    return;
                }
            };

            if let Err(e) = stream.play() {
                let msg = format!("Failed to start audio stream: {}", e);
                let _ = event_tx.send(WakeWordEvent::Error(msg));
                return;
            }

            info!("WakeWordDetector: listening for '{}'", trigger_word_for_thread);

            // Block until stop signal
            let _ = stop_rx.recv();

            info!("WakeWordDetector: stopped");
        });

        Ok(Self {
            rx: event_rx,
            stop_tx: Some(stop_tx),
            trigger_word: trigger_word_for_struct,
        })
    }

    /// Try to receive a wake word event (non-blocking).
    pub fn try_recv(&self) -> Option<WakeWordEvent> {
        self.rx.try_recv().ok()
    }

    /// Block until a wake word event is received.
    pub fn recv(&self) -> Result<WakeWordEvent, mpsc::RecvError> {
        self.rx.recv()
    }

    /// Stop the wake word detector thread.
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }

    /// The trigger word being listened for.
    pub fn trigger_word(&self) -> &str {
        &self.trigger_word
    }
}

impl Drop for WakeWordDetector {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Helpers — thin wrappers around oww_rs / audio_tools internals
// ---------------------------------------------------------------------------

use oww_rs::config::SpeechUnlockType;
use oww_rs::oww::OwwModel;
use oww_rs::oww::OWW_MODEL_CHUNK_SIZE;
use audio_tools::mic_config::find_best_config;
use audio_tools::process_audio::resample_into_chunks;
use audio_tools::resampler::make_resampler;
use oww_rs::mic_cpal::build_input_stream;

fn build_model(source: &WakeWordModel, threshold: f32) -> Result<OwwModel, String> {
    match source {
        WakeWordModel::BuiltInAlexa => {
            OwwModel::new(SpeechUnlockType::OpenWakeWordAlexa, threshold)
        }
        WakeWordModel::BuiltInHeyMycroft => {
            OwwModel::new(SpeechUnlockType::OpenWakeWordHeyMycroft, threshold)
        }
        WakeWordModel::BuiltInHeyJarvis => {
            OwwModel::new(SpeechUnlockType::OpenWakeWordHeyJarvis, threshold)
        }
        WakeWordModel::Custom { path, trigger_word } => {
            OwwModel::from_file(path, trigger_word.clone(), threshold)
                .map_err(|e| format!("IO error loading model: {}", e))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wake_word_config_defaults() {
        let config = WakeWordConfig::default();
        assert_eq!(config.threshold, 0.3);
        assert!(matches!(config.model_source, WakeWordModel::BuiltInAlexa));
    }

    #[test]
    fn test_trigger_word_builtin() {
        assert_eq!(WakeWordModel::BuiltInAlexa.trigger_word(), "Alexa");
        assert_eq!(WakeWordModel::BuiltInHeyMycroft.trigger_word(), "Hey Mycroft");
        assert_eq!(WakeWordModel::BuiltInHeyJarvis.trigger_word(), "Hey Jarvis");
    }

    #[test]
    fn test_trigger_word_custom() {
        let model = WakeWordModel::Custom {
            path: "/tmp/model.onnx".to_string(),
            trigger_word: "Hey Artume".to_string(),
        };
        assert_eq!(model.trigger_word(), "Hey Artume");
    }
}
