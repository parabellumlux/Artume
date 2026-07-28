//! AetherOS Spatial Audio Engine
//!
//! This crate provides the core audio subsystem for AetherOS:
//!
//! - **Spatial Mixer** — PipeWire-integrated binaural spatial audio mixer
//!   using HRTF-based ITD/IID processing for 3D headphone rendering.
//! - **Context Stack** — Thread-safe LIFO stack for managing audio stream
//!   interruptions with gain ducking and smooth cross-fade restoration.

pub mod spatial_mixer;
pub mod context_stack;

pub use spatial_mixer::{
    AudioError, BinauralKernel, SpatialMixer, SpatialPosition, VirtualSource,
};
pub use context_stack::{AudioContext, ContextStack};
