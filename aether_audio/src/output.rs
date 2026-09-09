//! Artume Audio Output Sink
//!
//! Plays PCM audio samples to the default audio output device using cpal,
//! or to a named PulseAudio sink via pacat subprocess.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{info, warn};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::Duration;

/// Audio output sink for playing TTS audio.
pub struct AudioOutput {
    /// Channel to send PCM samples for playback.
    tx: mpsc::Sender<PlaybackRequest>,
}

/// A request to play audio.
#[derive(Clone)]
pub struct PlaybackRequest {
    /// PCM f32 samples (mono, or interleaved stereo when `channels` is 2).
    pub samples: Vec<f32>,
    /// Sample rate of the samples.
    pub sample_rate: u32,
    /// Channel count of the samples (1 = mono, 2 = interleaved stereo).
    pub channels: u16,
}

impl AudioOutput {
    /// Create a new audio output sink using the default cpal device.
    pub fn new() -> Result<Self, String> {
        Self::with_device(None)
    }

    /// Create a new audio output sink, optionally selecting a specific cpal device by name.
    /// Pass `None` to use the system default.
    pub fn with_device(device_name: Option<&str>) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<PlaybackRequest>();

        let device_name_owned = device_name.map(|s| s.to_string());
        thread::spawn(move || {
            let host = cpal::default_host();
            let device: cpal::Device = match &device_name_owned {
                Some(name) => {
                    match host
                        .output_devices()
                        .map_err(|e| format!("failed to enumerate devices: {e}"))
                        .and_then(|mut devs| {
                            devs.find(|d| {
                                d.description()
                                    .map(|desc| desc.name().contains(name.as_str()))
                                    .unwrap_or(false)
                            })
                            .ok_or_else(|| format!("no output device matching '{name}' found"))
                        }) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!("AudioOutput: {e}");
                            return;
                        }
                    }
                }
                None => match host.default_output_device() {
                    Some(d) => d,
                    None => {
                        warn!("AudioOutput: no default output device found");
                        return;
                    }
                },
            };

            let device_name = device
                .description()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            info!("AudioOutput: using output device: {device_name}");

            let config = match device.default_output_config() {
                Ok(c) => c,
                Err(e) => {
                    warn!("AudioOutput: failed to get default config: {e}");
                    return;
                }
            };

            let sample_rate = config.sample_rate();
            let device_channels = config.channels() as usize;
            info!("AudioOutput: device config: {sample_rate} Hz, {device_channels} channels");

            while let Ok(req) = rx.recv() {
                let in_channels = req.channels.max(1) as usize;

                // Resample to device rate if needed (per-channel).
                let samples = if req.sample_rate != sample_rate {
                    resample_channels(&req.samples, req.sample_rate, sample_rate, in_channels)
                } else {
                    req.samples
                };

                // Convert channel count to what the device expects.
                // NOTE: for interleaved stereo buffers, resampling above has
                // already re-interleaved them, so slices below are frames.
                let playback_samples: Vec<f32> = match (in_channels, device_channels) {
                    (2, 2) | (1, 1) => samples,
                    (1, 2) => samples.iter().flat_map(|&s| vec![s, s]).collect(),
                    (2, 1) => samples.chunks(2).map(|f| (f[0] + f[1]) * 0.5).collect(),
                    // Edge case: >2 channels flattened to device count.
                    (n, 2) if n > 2 => {
                        let mut out = Vec::with_capacity(samples.len() / n * 2);
                        for frame in samples.chunks(n) {
                            let mut sum = 0.0_f32;
                            for &s in frame.iter().take(n) {
                                sum += s;
                            }
                            let mix = sum / n as f32;
                            out.extend_from_slice(&[mix, mix]);
                        }
                        out
                    }
                    _ => samples,
                };

                let total_frames = playback_samples.len() / device_channels;
                let played = Arc::new(AtomicBool::new(false));
                let played_clone = played.clone();
                let stream_samples = Arc::new(playback_samples);
                let stream_data = stream_samples.clone();

                let err_fn = move |err| {
                    warn!("AudioOutput: stream error: {err}");
                };

                let stream_config: cpal::StreamConfig = config.clone().into();

                let stream = match device.build_output_stream(
                    &stream_config,
                    move |data: &mut [f32], _| {
                        let len = data.len().min(stream_data.len());
                        for (i, sample) in data[..len].iter_mut().enumerate() {
                            *sample = stream_data[i];
                        }
                        for sample in data[len..].iter_mut() {
                            *sample = 0.0;
                        }
                        played_clone.store(true, Ordering::SeqCst);
                    },
                    err_fn,
                    None,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("AudioOutput: failed to create stream: {e}");
                        continue;
                    }
                };

                if let Err(e) = stream.play() {
                    warn!("AudioOutput: failed to play stream: {e}");
                    continue;
                }

                // Wait for the stream to actually produce audio (first callback)
                let deadline = std::time::Instant::now() + Duration::from_millis(500);
                while !played.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(10));
                }

                // Block for the duration of the audio
                let duration_ms = (total_frames as f32 / sample_rate as f32 * 1000.0) as u64;
                thread::sleep(Duration::from_millis(duration_ms + 200)); // +200ms tail to avoid cutoff
                                                                         // Stream is dropped here, ending playback
            }
        });

        Ok(Self { tx })
    }

    /// Create a new audio output sink that pipes PCM to a named PulseAudio sink via pacat.
    /// `sink_name` is the PulseAudio sink name (e.g. "bluez_output.54_15_89_14_C7_DE.1").
    pub fn with_sink(sink_name: &str) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<PlaybackRequest>();

        let sink = sink_name.to_string();
        thread::spawn(move || {
            info!("AudioOutput: using PulseAudio sink: {sink}");

            while let Ok(req) = rx.recv() {
                // Convert f32 samples to s16le bytes (interleaved frames).
                let in_channels = req.channels.max(1) as usize;
                let mut raw: Vec<u8> = Vec::with_capacity(req.samples.len() * 2);
                for &sample in &req.samples {
                    let clamped = sample.clamp(-1.0, 1.0);
                    let int16 = (clamped * 32767.0) as i16;
                    raw.extend_from_slice(&int16.to_ne_bytes());
                }

                // Spawn pacat to play the samples
                let mut child = match Command::new("pacat")
                    .args([
                        "--device",
                        &sink,
                        "--rate",
                        &req.sample_rate.to_string(),
                        "--format=s16le",
                        "--channels",
                        &in_channels.to_string(),
                        "--raw",
                    ])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("AudioOutput: failed to spawn pacat: {e}");
                        continue;
                    }
                };

                if let Some(mut stdin) = child.stdin.take() {
                    if let Err(e) = stdin.write_all(&raw) {
                        warn!("AudioOutput: failed to write to pacat stdin: {e}");
                    }
                }

                // Wait for playback to finish
                let _ = child.wait();
            }
        });

        Ok(Self { tx })
    }

    /// Play mono PCM samples. Non-blocking — returns immediately.
    pub fn play(&self, samples: Vec<f32>, sample_rate: u32) {
        let _ = self.tx.send(PlaybackRequest {
            samples,
            sample_rate,
            channels: 1,
        });
    }

    /// Play interleaved stereo PCM samples. Non-blocking — returns immediately.
    pub fn play_stereo(&self, samples: Vec<f32>, sample_rate: u32) {
        let _ = self.tx.send(PlaybackRequest {
            samples,
            sample_rate,
            channels: 2,
        });
    }

    /// Check if the output is ready.
    pub fn is_ready(&self) -> bool {
        true
    }

    /// Get a clone of the playback channel sender.
    pub fn sender(&self) -> mpsc::Sender<PlaybackRequest> {
        self.tx.clone()
    }
}

/// Simple linear resample (nearest-neighbor for speed).
fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
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

/// Resample an interleaved multi-channel buffer, rate-converting each
/// channel independently and re-interleaving the result.
fn resample_channels(input: &[f32], from_rate: u32, to_rate: u32, channels: usize) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }
    if channels == 1 {
        return resample(input, from_rate, to_rate);
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_frames = (input.len() / channels) as f64 / ratio;
    let out_len = (out_frames as usize) * channels;
    let mut output = vec![0.0_f32; out_len];
    for ch in 0..channels {
        for i in 0..(out_len / channels) {
            let src_frame = (i as f64 * ratio) as usize;
            let src_idx = (src_frame * channels + ch).min(input.len() - channels);
            output[i * channels + ch] = input[src_idx];
        }
    }
    output
}
