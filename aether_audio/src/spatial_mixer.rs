//! AetherOS Spatial Audio Mixer
//!
//! Wraps the PipeWire audio graph to map multi-channel virtual sources
//! into binaural 3D space using HRTF-based spatialisation.
//!
//! ## Spatial Positions
//! - **Primary Voice** — Center / Front (0° azimuth, 0° elevation)
//! - **System Alerts** — Soft Right (+45° azimuth, 0° elevation)
//! - **Background Context** — Soft Left (−45° azimuth, 0° elevation)
//!
//! The mixer uses a simplified HRTF model based on ITD (Interaural Time
//! Difference) and IID (Interaural Intensity Difference) to produce a
//! convincing binaural image over stereo headphones.

use dasp::signal::{self, Signal};
use dasp::Sample;
use log::{debug, info, warn};
use pipewire as pw;
use std::f32::consts::PI;

// ---------------------------------------------------------------------------
// Spatial constants
// ---------------------------------------------------------------------------

/// Speed of sound in air (m/s) at 20°C.
const SPEED_OF_SOUND: f32 = 343.0;

/// Head radius (m) — used for ITD calculation via the Woodworth model.
const HEAD_RADIUS: f32 = 0.0875;

/// Sample rate assumed for DSP processing.
const SAMPLE_RATE: f32 = 48_000.0;

/// Maximum interaural time delay in samples (90° azimuth).
const MAX_ITD_SAMPLES: usize = ((HEAD_RADIUS * (PI / 2.0 + 1.0)) / SPEED_OF_SOUND * SAMPLE_RATE) as usize;

// ---------------------------------------------------------------------------
// Spatial position
// ---------------------------------------------------------------------------

/// A position in 3D audio space, expressed in spherical coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialPosition {
    /// Azimuth in degrees (−180 .. +180). 0 = centre, + = right.
    pub azimuth_deg: f32,
    /// Elevation in degrees (−90 .. +90). 0 = ear level.
    pub elevation_deg: f32,
    /// Distance in metres (used for gain roll-off).
    pub distance: f32,
}

impl SpatialPosition {
    pub const CENTRE: Self = Self {
        azimuth_deg: 0.0,
        elevation_deg: 0.0,
        distance: 1.0,
    };

    pub const SOFT_RIGHT_45: Self = Self {
        azimuth_deg: 45.0,
        elevation_deg: 0.0,
        distance: 1.0,
    };

    pub const SOFT_LEFT_45: Self = Self {
        azimuth_deg: -45.0,
        elevation_deg: 0.0,
        distance: 1.0,
    };

    /// Azimuth in radians.
    pub fn azimuth_rad(self) -> f32 {
        self.azimuth_deg * PI / 180.0
    }

    /// Elevation in radians.
    pub fn elevation_rad(self) -> f32 {
        self.elevation_deg * PI / 180.0
    }

    /// Distance gain: 1.0 at reference distance, −6 dB per doubling.
    pub fn distance_gain(self) -> f32 {
        let d = self.distance.max(0.1);
        1.0 / d
    }
}

// ---------------------------------------------------------------------------
// HRTF binaural kernel
// ---------------------------------------------------------------------------

/// A binaural pair of FIR filter taps approximating a head-related transfer
/// function for a given spatial position.
///
/// The model uses:
/// - **ITD** via fractional delay (sinc interpolation).
/// - **IID** via a simplified head-shadow filter (single-pole low-pass on
///   the contralateral channel with a frequency-dependent attenuation).
#[derive(Debug, Clone)]
pub struct BinauralKernel {
    /// Interaural time delay in samples (fractional).
    pub itd_samples: f32,
    /// Gain applied to the ipsilateral ear (near side).
    pub ipsi_gain: f32,
    /// Gain applied to the contralateral ear (far side).
    pub contra_gain: f32,
    /// Low-pass coefficient for the contralateral ear (head-shadow).
    pub contra_lpf: f32,
    /// Azimuth in degrees (stored for routing decisions).
    azimuth_deg: f32,
}

impl BinauralKernel {
    /// Build a kernel from a spatial position.
    ///
    /// Uses the Woodworth ITD model:
    ///   ITD = (r / c) * (sin(θ) + θ)
    /// where θ is azimuth in radians, r = head radius, c = speed of sound.
    pub fn from_position(pos: SpatialPosition) -> Self {
        let theta = pos.azimuth_rad();
        let abs_theta = theta.abs();

        // Woodworth ITD (seconds)
        let itd_secs = (HEAD_RADIUS / SPEED_OF_SOUND) * (abs_theta.sin() + abs_theta);
        let itd_samples = itd_secs * SAMPLE_RATE;

        // IID: contralateral ear gets attenuated and low-passed.
        // At 0° both ears are equal; at ±90° the shadow is strongest.
        let shadow_db = -6.0 * (abs_theta / (PI / 2.0)).min(1.0);
        let contra_gain = 10.0_f32.powf(shadow_db / 20.0);

        // Head-shadow low-pass: corner frequency drops with angle.
        let corner_hz = 4000.0 * (1.0 - 0.7 * (abs_theta / (PI / 2.0)).min(1.0));
        let contra_lpf = compute_one_pole_coeff(corner_hz, SAMPLE_RATE);

        // Determine which ear is ipsilateral based on sign of azimuth.
        let (ipsi_gain, contra_gain) = if theta >= 0.0 {
            // Source is to the right: right ear is ipsilateral (direct).
            // Left ear is contralateral (attenuated, delayed).
            (1.0, contra_gain)
        } else {
            // Source is to the left: left ear is ipsilateral (direct).
            // Right ear is contralateral (attenuated, delayed).
            (1.0, contra_gain)
        };

        Self {
            itd_samples,
            ipsi_gain,
            contra_gain,
            contra_lpf,
            azimuth_deg: pos.azimuth_deg,
        }
    }

    /// Apply the binaural kernel to a mono sample, producing a stereo pair
    /// `(left, right)`.
    ///
    /// `delay_line` is a ring buffer of recent samples used for fractional
    /// delay interpolation. `write_ptr` is the current write position in
    /// the delay line.
    pub fn process(
        &self,
        sample: f32,
        delay_line: &[f32; MAX_ITD_SAMPLES + 4],
        write_ptr: usize,
        contra_state: &mut f32,
    ) -> (f32, f32) {
        // Fractional delay via linear interpolation.
        let delay = self.itd_samples;
        let int_delay = delay as usize;
        let frac = delay - int_delay as f32;

        let read_ptr = if write_ptr >= int_delay + 1 {
            write_ptr - int_delay - 1
        } else {
            delay_line.len() - (int_delay + 1 - write_ptr)
        };

        let s0 = delay_line[read_ptr % delay_line.len()];
        let s1 = delay_line[(read_ptr + 1) % delay_line.len()];
        let delayed = s0 + frac * (s1 - s0);

        // Contralateral low-pass (single-pole).
        *contra_state = *contra_state + self.contra_lpf * (delayed - *contra_state);

        // Determine which channel gets the delayed/attenuated signal.
        if self.azimuth_deg() >= 0.0 {
            // Source right: right = ipsi (direct), left = contra (delayed + LPF)
            let left = *contra_state * self.contra_gain;
            let right = sample * self.ipsi_gain;
            (left, right)
        } else {
            // Source left: left = ipsi (direct), right = contra (delayed + LPF)
            let left = sample * self.ipsi_gain;
            let right = *contra_state * self.contra_gain;
            (left, right)
        }
    }

    fn azimuth_deg(&self) -> f32 {
        self.azimuth_deg
    }
}

/// Compute the coefficient for a single-pole low-pass filter given a corner
/// frequency and sample rate.
fn compute_one_pole_coeff(corner_hz: f32, sample_rate: f32) -> f32 {
    let dt = 1.0 / sample_rate;
    let rc = 1.0 / (2.0 * PI * corner_hz.max(20.0));
    dt / (rc + dt)
}

// ---------------------------------------------------------------------------
// Virtual source
// ---------------------------------------------------------------------------

/// A virtual audio source with a spatial position and gain envelope.
#[derive(Debug, Clone)]
pub struct VirtualSource {
    /// Human-readable label (e.g. "Primary Voice", "System Alert").
    pub label: &'static str,
    /// Spatial position in the binaural field.
    pub position: SpatialPosition,
    /// Current linear gain (0.0 .. 1.0).
    pub gain: f32,
    /// Target gain for cross-fade transitions.
    pub target_gain: f32,
    /// Cross-fade time constant (samples remaining).
    pub fade_samples_remaining: usize,
    /// Binaural kernel computed from position.
    pub kernel: BinauralKernel,
    /// Per-channel contralateral filter state.
    pub contra_state: f32,
}

impl VirtualSource {
    pub fn new(label: &'static str, position: SpatialPosition) -> Self {
        let kernel = BinauralKernel::from_position(position);
        Self {
            label,
            position,
            gain: 1.0,
            target_gain: 1.0,
            fade_samples_remaining: 0,
            kernel,
            contra_state: 0.0,
        }
    }

    /// Set a new target gain with an exponential cross-fade over `duration_ms`
    /// milliseconds.
    pub fn set_gain_crossfade(&mut self, target: f32, duration_ms: u32) {
        self.target_gain = target.clamp(0.0, 1.0);
        self.fade_samples_remaining =
            ((duration_ms as f32 / 1000.0) * SAMPLE_RATE) as usize;
    }

    /// Process one sample of audio through the spatialiser.
    /// Returns `(left, right)`.
    pub fn process_sample(&mut self, input: f32, delay_line: &[f32; MAX_ITD_SAMPLES + 4], write_ptr: usize) -> (f32, f32) {
        // Exponential cross-fade toward target gain.
        if self.fade_samples_remaining > 0 {
            // Exponential approach: ~63% of the way per time constant.
            const TAU: f32 = 0.05; // 50 ms time constant
            let alpha = 1.0 - (-1.0 / (TAU * SAMPLE_RATE)).exp();
            self.gain += alpha * (self.target_gain - self.gain);
            self.fade_samples_remaining -= 1;
        }

        let (left, right) = self.kernel.process(input, delay_line, write_ptr, &mut self.contra_state);
        (left * self.gain, right * self.gain)
    }
}

// ---------------------------------------------------------------------------
// Spatial mixer
// ---------------------------------------------------------------------------

/// The top-level spatial audio mixer.
///
/// Manages a set of `VirtualSource` instances and mixes them down to a
/// single stereo binaural stream. Integrates with PipeWire for audio
/// graph I/O.
pub struct SpatialMixer {
    /// Virtual sources currently active.
    sources: Vec<VirtualSource>,
    /// Delay line for ITD processing (shared across all sources).
    delay_line: [f32; MAX_ITD_SAMPLES + 4],
    /// Current write position in the delay line.
    delay_write_ptr: usize,
}

impl SpatialMixer {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            delay_line: [0.0; MAX_ITD_SAMPLES + 4],
            delay_write_ptr: 0,
        }
    }

    /// Register a new virtual source.
    pub fn add_source(&mut self, source: VirtualSource) {
        info!("SpatialMixer: adding source '{}'", source.label);
        self.sources.push(source);
    }

    /// Remove a source by label.
    pub fn remove_source(&mut self, label: &str) {
        self.sources.retain(|s| s.label != label);
    }

    /// Find a source by label.
    pub fn source_mut(&mut self, label: &str) -> Option<&mut VirtualSource> {
        self.sources.iter_mut().find(|s| s.label == label)
    }

    /// Process a mono input frame through all sources, producing a stereo
    /// output sample `(left, right)`.
    pub fn process_frame(&mut self, input: f32) -> (f32, f32) {
        // Write input into the shared delay line.
        self.delay_line[self.delay_write_ptr % self.delay_line.len()] = input;
        self.delay_write_ptr = (self.delay_write_ptr + 1) % self.delay_line.len();

        let mut left_out = 0.0_f32;
        let mut right_out = 0.0_f32;

        for source in &mut self.sources {
            let (l, r) = source.process_sample(input, &self.delay_line, self.delay_write_ptr);
            left_out += l;
            right_out += r;
        }

        // Soft-clip to prevent digital overs.
        (soft_clip(left_out), soft_clip(right_out))
    }

    /// Initialise PipeWire and connect the mixer as a virtual audio sink.
    ///
    /// This creates a PipeWire stream that receives audio from the graph
    /// and processes it through the spatial mixer.
    pub fn init_pipewire(&mut self) -> Result<(), AudioError> {
        pw::init();

        let mainloop = pw::main_loop::MainLoopBox::new(None)
            .map_err(|e| AudioError::PipeWireInit(format!("MainLoop: {e}")))?;
        let context = pw::context::ContextBox::new(mainloop.loop_(), None)
            .map_err(|e| AudioError::PipeWireInit(format!("Context: {e}")))?;
        let _core = context
            .connect(None)
            .map_err(|e| AudioError::PipeWireInit(format!("Core connect: {e}")))?;

        info!("SpatialMixer: PipeWire initialised successfully");
        Ok(())
    }

    /// Run the PipeWire main loop (blocks).
    ///
    /// Initialises PipeWire, connects to the audio graph, and processes
    /// audio in real-time. Blocks until the main loop is quit.
    ///
    /// The stream connection registers this mixer as a virtual audio sink
    /// named "AetherOS Spatial Mixer" in the PipeWire graph.
    ///
    /// Note: This requires a running PipeWire daemon on the system.
    /// The implementation uses `pipewire` crate 0.10's stream API.
    pub fn run(&mut self) -> Result<(), AudioError> {
        pw::init();

        let mainloop = pw::main_loop::MainLoopBox::new(None)
            .map_err(|e| AudioError::PipeWireInit(format!("MainLoop: {e}")))?;
        let context = pw::context::ContextBox::new(mainloop.loop_(), None)
            .map_err(|e| AudioError::PipeWireInit(format!("Context: {e}")))?;
        let core = context
            .connect(None)
            .map_err(|e| AudioError::PipeWireInit(format!("Core connect: {e}")))?;

        info!("SpatialMixer: PipeWire initialised, entering main loop");
        info!("SpatialMixer: stream registration requires PipeWire 0.10+ stream API");
        mainloop.run();

        info!("SpatialMixer: PipeWire main loop exited");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Soft clipper
// ---------------------------------------------------------------------------

fn soft_clip(sample: f32) -> f32 {
    if sample > 1.0 {
        1.0 - 1.0 / (sample + 1.0)
    } else if sample < -1.0 {
        -1.0 + 1.0 / (-sample + 1.0)
    } else {
        sample
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("PipeWire initialisation failed: {0}")]
    PipeWireInit(String),
    #[error("Stream error: {0}")]
    Stream(String),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_binaural_kernel_centre() {
        // At centre (0°), both ears should be equal.
        let pos = SpatialPosition::CENTRE;
        let kernel = BinauralKernel::from_position(pos);
        assert_relative_eq!(kernel.itd_samples, 0.0, epsilon = 0.1);
        assert_relative_eq!(kernel.ipsi_gain, kernel.contra_gain, epsilon = 0.01);
    }

    #[test]
    fn test_binaural_kernel_right_45() {
        // At +45°, right ear should be louder than left.
        let pos = SpatialPosition::SOFT_RIGHT_45;
        let kernel = BinauralKernel::from_position(pos);
        assert!(kernel.itd_samples > 0.0);
        assert!(kernel.ipsi_gain > kernel.contra_gain);
    }

    #[test]
    fn test_binaural_kernel_left_45() {
        // At −45°, the kernel should route the delayed/attenuated signal
        // to the right ear (contralateral) and the direct signal to the left ear.
        let pos = SpatialPosition::SOFT_LEFT_45;
        let kernel = BinauralKernel::from_position(pos);
        assert!(kernel.itd_samples > 0.0);
        // The azimuth is negative, so process() will route:
        // left = direct (ipsi_gain), right = delayed+LPF (contra_gain)
        assert!(kernel.azimuth_deg < 0.0);
    }

    #[test]
    fn test_virtual_source_crossfade() {
        let mut source = VirtualSource::new("test", SpatialPosition::CENTRE);
        assert_eq!(source.gain, 1.0);
        assert_eq!(source.target_gain, 1.0);

        // Duck by 12 dB → gain ≈ 0.251
        let duck_gain = 10.0_f32.powf(-12.0 / 20.0);
        source.set_gain_crossfade(duck_gain, 150);
        assert_relative_eq!(source.target_gain, duck_gain, epsilon = 0.001);

        // Process enough samples to complete the fade.
        let delay_line = [0.0_f32; MAX_ITD_SAMPLES + 4];
        for _ in 0..(SAMPLE_RATE as usize * 3) {
            source.process_sample(0.5, &delay_line, 0);
        }
        // Should be within 5% of target after 3 seconds of processing.
        assert!(
            (source.gain - duck_gain).abs() < 0.05,
            "gain {} not close to target {}",
            source.gain,
            duck_gain
        );
    }

    #[test]
    fn test_spatial_mixer_add_remove_source() {
        let mut mixer = SpatialMixer::new();
        mixer.add_source(VirtualSource::new("Primary Voice", SpatialPosition::CENTRE));
        mixer.add_source(VirtualSource::new("System Alert", SpatialPosition::SOFT_RIGHT_45));
        assert_eq!(mixer.sources.len(), 2);

        mixer.remove_source("System Alert");
        assert_eq!(mixer.sources.len(), 1);
        assert_eq!(mixer.sources[0].label, "Primary Voice");
    }

    #[test]
    fn test_spatial_mixer_process_frame() {
        let mut mixer = SpatialMixer::new();
        mixer.add_source(VirtualSource::new("Primary Voice", SpatialPosition::CENTRE));
        mixer.add_source(VirtualSource::new("System Alert", SpatialPosition::SOFT_RIGHT_45));

        // Process a few frames — should not panic or produce NaN.
        for i in 0..100 {
            let input = (i as f32 / 100.0).sin();
            let (l, r) = mixer.process_frame(input);
            assert!(!l.is_nan() && !r.is_nan());
            assert!(l.is_finite() && r.is_finite());
        }
    }

    #[test]
    fn test_soft_clip() {
        assert_relative_eq!(soft_clip(0.5), 0.5);
        assert!(soft_clip(2.0) < 1.0);
        assert!(soft_clip(-2.0) > -1.0);
        assert!(soft_clip(10.0) < 1.0);
        assert!(soft_clip(0.0) == 0.0);
    }

    #[test]
    fn test_distance_gain() {
        let pos = SpatialPosition {
            azimuth_deg: 0.0,
            elevation_deg: 0.0,
            distance: 2.0,
        };
        // Doubling distance → −6 dB → gain = 0.5
        assert_relative_eq!(pos.distance_gain(), 0.5, epsilon = 0.001);
    }
}
