//! Artume Spatial Audio Engine
//!
//! This crate provides the core audio subsystem for Artume:
//!
//! - **Spatial Mixer** — PipeWire-integrated binaural spatial audio mixer
//!   using HRTF-based ITD/IID processing for 3D headphone rendering.
//! - **Context Stack** — Thread-safe LIFO stack for managing audio stream
//!   interruptions with gain ducking and smooth cross-fade restoration.

pub mod capture;
pub mod context_stack;
pub mod output;
pub mod spatial_mixer;
pub mod wake_word;

pub use context_stack::{AudioContext, ContextStack};
pub use spatial_mixer::{AudioError, BinauralKernel, SpatialMixer, SpatialPosition, VirtualSource};
