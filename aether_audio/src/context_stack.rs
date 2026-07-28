//! AetherOS Audio Context Stack
//!
//! A thread-safe LIFO stack managing active voice and audio stream contexts.
//!
//! ## Interruption Model
//! When an interruption occurs (e.g., user speaks over a long readout):
//! 1. The current audio context is **PUSHED** onto the stack.
//! 2. Volume is instantly ducked by 12 dB (cross-fade).
//! 3. The new task takes 0° spatial alignment (centre).
//!
//! Upon completion of the interrupting task:
//! 1. The previous context is **POPPED** from the stack.
//! 2. Gain is restored with a smooth 150 ms exponential cross-fade.
//! 3. The spatial position is restored to the previous context's position.

use crate::spatial_mixer::{SpatialMixer, SpatialPosition};
use log::{debug, info, warn};
use parking_lot::RwLock;

// ---------------------------------------------------------------------------
// Audio context
// ---------------------------------------------------------------------------

/// Represents the full state of an audio stream at a point in time.
#[derive(Debug, Clone)]
pub struct AudioContext {
    /// Human-readable label for this context (e.g. "Long Readout", "User Dictation").
    pub label: String,
    /// The spatial position of the source in this context.
    pub position: SpatialPosition,
    /// The linear gain of the source before interruption.
    pub gain: f32,
    /// Timestamp (monotonic nanoseconds) when this context was created.
    pub created_at_ns: u64,
}

impl AudioContext {
    pub fn new(label: &str, position: SpatialPosition, gain: f32) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let created_at_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self {
            label: label.to_string(),
            position,
            gain,
            created_at_ns,
        }
    }
}

// ---------------------------------------------------------------------------
// Context stack
// ---------------------------------------------------------------------------

/// A thread-safe LIFO stack managing audio stream contexts.
///
/// The stack is protected by a `parking_lot::RwLock` for concurrent access
/// from the audio processing thread and control threads.
pub struct ContextStack {
    /// The LIFO stack of saved audio contexts.
    stack: RwLock<Vec<AudioContext>>,
    /// Maximum depth to prevent runaway stacking.
    max_depth: usize,
}

impl ContextStack {
    /// Create a new empty context stack.
    pub fn new(max_depth: usize) -> Self {
        Self {
            stack: RwLock::new(Vec::with_capacity(max_depth.min(32))),
            max_depth,
        }
    }

    /// Push the current audio state onto the stack and apply ducking.
    ///
    /// This captures the current gain and spatial position of the source
    /// identified by `source_label`, then ducks the volume by `duck_db`
    /// decibels and repositions the source to `new_position`.
    ///
    /// Returns the saved context on success, or `None` if the source was
    /// not found or the stack is full.
    pub fn push_interruption(
        &self,
        mixer: &mut SpatialMixer,
        source_label: &str,
        new_label: &str,
        new_position: SpatialPosition,
        duck_db: f32,
        fade_ms: u32,
    ) -> Option<AudioContext> {
        let source = mixer.source_mut(source_label)?;

        // Capture the current state.
        let saved = AudioContext::new(
            &format!("{} [interrupted]", source_label),
            source.position,
            source.gain,
        );

        // Check stack depth.
        {
            let mut stack = self.stack.write();
            if stack.len() >= self.max_depth {
                warn!(
                    "ContextStack: max depth ({}) reached, dropping oldest context",
                    self.max_depth
                );
                // Drop the oldest (bottom of stack) to make room.
                stack.remove(0);
            }
            stack.push(saved.clone());
        }

        // Apply ducking: reduce gain by duck_db dB.
        let duck_gain = 10.0_f32.powf(duck_db / 20.0);
        source.set_gain_crossfade(duck_gain, fade_ms);
        source.position = new_position;
        source.kernel = crate::spatial_mixer::BinauralKernel::from_position(new_position);

        info!(
            "ContextStack: PUSH '{}' → '{}' (duck {:.1} dB, fade {} ms)",
            source_label, new_label, duck_db, fade_ms
        );

        Some(saved)
    }

    /// Pop the most recent context from the stack and restore its state.
    ///
    /// Restores the gain and spatial position of the source with a smooth
    /// exponential cross-fade over `fade_ms` milliseconds.
    ///
    /// Returns the restored context, or `None` if the stack is empty or
    /// the source is not found.
    pub fn pop_restore(
        &self,
        mixer: &mut SpatialMixer,
        source_label: &str,
        fade_ms: u32,
    ) -> Option<AudioContext> {
        let previous = {
            let mut stack = self.stack.write();
            stack.pop()
        };

        match previous {
            Some(ctx) => {
                let source = mixer.source_mut(source_label)?;
                source.set_gain_crossfade(ctx.gain, fade_ms);
                source.position = ctx.position;
                source.kernel = crate::spatial_mixer::BinauralKernel::from_position(ctx.position);

                info!(
                    "ContextStack: POP → restore '{}' (gain {:.3}, fade {} ms)",
                    ctx.label, ctx.gain, fade_ms
                );
                Some(ctx)
            }
            None => {
                debug!("ContextStack: POP called but stack is empty");
                None
            }
        }
    }

    /// Peek at the top of the stack without modifying it.
    pub fn peek(&self) -> Option<AudioContext> {
        let stack = self.stack.read();
        stack.last().cloned()
    }

    /// Current depth of the stack.
    pub fn depth(&self) -> usize {
        let stack = self.stack.read();
        stack.len()
    }

    /// Clear all saved contexts (e.g., on system reset).
    pub fn clear(&self) {
        let mut stack = self.stack.write();
        stack.clear();
        debug!("ContextStack: cleared all contexts");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial_mixer::VirtualSource;
    use crate::spatial_mixer::SpatialPosition;
    use approx::assert_relative_eq;

    /// Simulate an audio interruption and stack recovery.
    ///
    /// Scenario:
    /// 1. A "Long Readout" source is playing at centre position, gain 1.0.
    /// 2. User interrupts → context is PUSHED, gain ducked by 12 dB.
    /// 3. New "User Dictation" takes centre position.
    /// 4. Dictation completes → context is POPPED, gain restored with 150 ms fade.
    #[test]
    fn test_interruption_push_pop() {
        let mut mixer = SpatialMixer::new();
        mixer.add_source(VirtualSource::new(
            "Primary Voice",
            SpatialPosition::CENTRE,
        ));

        let stack = ContextStack::new(8);

        // --- Phase 1: Initial state ---
        let source = mixer.source_mut("Primary Voice").unwrap();
        source.gain = 1.0;
        source.target_gain = 1.0;

        // --- Phase 2: Interruption ---
        let saved = stack
            .push_interruption(
                &mut mixer,
                "Primary Voice",
                "User Dictation",
                SpatialPosition::CENTRE,
                -12.0, // duck by 12 dB
                10,    // fast duck
            )
            .expect("push should succeed");

        assert_eq!(saved.gain, 1.0);
        assert_eq!(saved.position, SpatialPosition::CENTRE);

        let source = mixer.source_mut("Primary Voice").unwrap();
        // Target gain should be 12 dB down: 10^(-12/20) ≈ 0.251
        let expected_duck_gain = 10.0_f32.powf(-12.0 / 20.0);
        assert_relative_eq!(source.target_gain, expected_duck_gain, epsilon = 0.001);

        // --- Phase 3: Stack state ---
        assert_eq!(stack.depth(), 1);

        // --- Phase 4: Restore ---
        let restored = stack
            .pop_restore(&mut mixer, "Primary Voice", 150)
            .expect("pop should succeed");

        assert_eq!(restored.gain, 1.0);
        assert_eq!(restored.position, SpatialPosition::CENTRE);

        let source = mixer.source_mut("Primary Voice").unwrap();
        assert_relative_eq!(source.target_gain, 1.0, epsilon = 0.001);

        assert_eq!(stack.depth(), 0);
    }

    /// Test that the stack correctly handles multiple nested interruptions.
    #[test]
    fn test_nested_interruptions() {
        let mut mixer = SpatialMixer::new();
        mixer.add_source(VirtualSource::new(
            "Primary Voice",
            SpatialPosition::CENTRE,
        ));

        let stack = ContextStack::new(8);

        // Push 3 interruptions.
        for i in 0..3 {
            stack.push_interruption(
                &mut mixer,
                "Primary Voice",
                &format!("Interruption {}", i),
                SpatialPosition::CENTRE,
                -12.0,
                10,
            );
        }

        assert_eq!(stack.depth(), 3);

        // Pop all 3 in reverse order.
        for i in (0..3).rev() {
            let ctx = stack.pop_restore(&mut mixer, "Primary Voice", 150);
            assert!(ctx.is_some(), "pop {} should succeed", i);
        }

        assert_eq!(stack.depth(), 0);

        // Popping an empty stack should return None.
        assert!(stack.pop_restore(&mut mixer, "Primary Voice", 150).is_none());
    }

    /// Test that the stack enforces its maximum depth.
    #[test]
    fn test_max_depth() {
        let mut mixer = SpatialMixer::new();
        mixer.add_source(VirtualSource::new(
            "Primary Voice",
            SpatialPosition::CENTRE,
        ));

        let stack = ContextStack::new(3);

        // Push 5 interruptions — only 3 should be retained.
        for i in 0..5 {
            stack.push_interruption(
                &mut mixer,
                "Primary Voice",
                &format!("Interruption {}", i),
                SpatialPosition::CENTRE,
                -12.0,
                10,
            );
        }

        assert_eq!(stack.depth(), 3);
    }

    /// Test that peek returns the top without modifying the stack.
    #[test]
    fn test_peek() {
        let mut mixer = SpatialMixer::new();
        mixer.add_source(VirtualSource::new(
            "Primary Voice",
            SpatialPosition::CENTRE,
        ));

        let stack = ContextStack::new(8);

        assert!(stack.peek().is_none());

        stack.push_interruption(
            &mut mixer,
            "Primary Voice",
            "Alert",
            SpatialPosition::SOFT_RIGHT_45,
            -12.0,
            10,
        );

        let top = stack.peek().expect("peek should return a context");
        assert_eq!(top.label, "Primary Voice [interrupted]");

        // Depth should still be 1 after peek.
        assert_eq!(stack.depth(), 1);
    }

    /// Test clear.
    #[test]
    fn test_clear() {
        let mut mixer = SpatialMixer::new();
        mixer.add_source(VirtualSource::new(
            "Primary Voice",
            SpatialPosition::CENTRE,
        ));

        let stack = ContextStack::new(8);

        stack.push_interruption(
            &mut mixer,
            "Primary Voice",
            "Alert",
            SpatialPosition::SOFT_RIGHT_45,
            -12.0,
            10,
        );
        assert_eq!(stack.depth(), 1);

        stack.clear();
        assert_eq!(stack.depth(), 0);
    }
}
