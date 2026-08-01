//! AetherOS Microphone Capture
//!
//! Captures audio from the default microphone after wake word detection.
//! Records until silence is detected (simple energy-based VAD) or a timeout.
//! Returns 16kHz f32 PCM samples suitable for Whisper STT.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{info, warn};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Result of a mic capture session.
pub struct CaptureResult {
    /// PCM f32 samples (mono, 16kHz).
    pub samples: Vec<f32>,
    /// Duration of the captured audio in seconds.
    pub duration_secs: f32,
}

/// Capture audio from the default microphone.
///
/// Records until silence is detected (energy below threshold for 500ms)
/// or the max duration is reached. Returns 16kHz mono f32 samples.
pub fn capture_until_silence(
    max_duration_secs: f32,
    silence_threshold: f32,
    silence_timeout_ms: u64,
) -> Result<CaptureResult, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No input device available".to_string())?;

    let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
    info!("MicCapture: using input device: {}", device_name);

    let config = device
        .default_input_config()
        .map_err(|e| format!("Failed to get input config: {}", e))?;

    let sample_rate: u32 = config.sample_rate();
    let channels = config.channels() as usize;
    info!(
        "MicCapture: device config: {} Hz, {} channels",
        sample_rate, channels
    );

    // Target 16kHz for Whisper
    let target_rate: u32 = 16000;
    let max_samples = (target_rate as f32 * max_duration_secs) as usize;

    let recorded = Arc::new(Mutex::new(Vec::<f32>::new()));
    let recorded_clone = recorded.clone();
    let done = Arc::new(Mutex::new(false));
    let done_clone = done.clone();

    let err_fn = move |err| {
        warn!("MicCapture: stream error: {}", err);
    };

    let stream_config: cpal::StreamConfig = config.into();
    let stream_channels = stream_config.channels as usize;

    let stream = device
        .build_input_stream(
            &stream_config,
            move |data: &[f32], _| {
                let mut rec = recorded_clone.lock().unwrap();
                // Downmix to mono if needed
                if stream_channels > 1 {
                    for frame in data.chunks(stream_channels) {
                        let mono: f32 = frame.iter().sum::<f32>() / stream_channels as f32;
                        rec.push(mono);
                    }
                } else {
                    rec.extend_from_slice(data);
                }
                // Check if we've hit max duration
                if rec.len() >= max_samples {
                    *done_clone.lock().unwrap() = true;
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("Failed to build input stream: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Failed to start input stream: {}", e))?;

    // Wait for silence or timeout
    let mut silence_frames = 0u64;
    let silence_samples = (target_rate as f64 * silence_timeout_ms as f64 / 1000.0) as usize;
    let mut last_sample_count = 0usize;

    loop {
        std::thread::sleep(Duration::from_millis(50));

        let rec = recorded.lock().unwrap();
        let len = rec.len();

        // Check for silence in the latest chunk
        if len > last_sample_count {
            let chunk = &rec[last_sample_count..len];
            let energy: f32 = chunk.iter().map(|&s| s * s).sum::<f32>() / chunk.len() as f32;
            if energy < silence_threshold {
                silence_frames += 1;
            } else {
                silence_frames = 0;
            }
        }
        last_sample_count = len;

        // Stop if silence is long enough
        if silence_frames * 50 >= silence_timeout_ms && len > target_rate as usize / 4 {
            // At least 0.25s of audio before stopping on silence
            break;
        }

        // Stop if max duration reached
        if *done.lock().unwrap() {
            break;
        }
    }

    // Drop stream to stop capture
    drop(stream);

    let samples = recorded.lock().unwrap().clone();

    // Resample to 16kHz if needed
    let samples = if sample_rate != target_rate {
        resample_to_16khz(&samples, sample_rate, target_rate)
    } else {
        samples
    };

    let duration_secs = samples.len() as f32 / target_rate as f32;
    info!(
        "MicCapture: captured {:.1}s of audio ({} samples at {} Hz)",
        duration_secs,
        samples.len(),
        target_rate
    );

    Ok(CaptureResult {
        samples,
        duration_secs,
    })
}

/// Simple linear resample to 16kHz.
fn resample_to_16khz(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let src_idx = (i as f64 * ratio) as usize;
        let idx = src_idx.min(input.len() - 1);
        output.push(input[idx]);
    }
    output
}
