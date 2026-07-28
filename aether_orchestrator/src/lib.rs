//! AetherOS Conversational Orchestrator
//!
//! The central nervous system of AetherOS — takes the audio menu system
//! and makes it conversational by routing natural-language input through
//! a local CPU model ensemble:
//!
//! ```text
//! User Speech → [Whisper tiny] → Text
//!   → [Qwen2.5-1.5B Router] → Intent Classification
//!      ├── Conversation → [SmolLM3 3B] → response → [Kokoro TTS]
//!      ├── EntityLookup → [NER model] → buffer lookup → response
//!      ├── WebFetch     → [Browser engine] → [Gemma 3 2B summary] → TTS
//!      └── ExecuteAction → [Intent model] → system call → confirm
//! ```

pub mod models;
pub mod router;
#[cfg(feature = "stt")]
pub mod stt;
#[cfg(feature = "tts")]
pub mod tts;
pub mod conversation;

pub use models::{ModelEngine, ModelKind, InferenceParams, GenerationResult};
pub use router::{Intent, IntentRouter, RouterConfig};
#[cfg(feature = "stt")]
pub use stt::{SttEngine, SttConfig};
#[cfg(feature = "tts")]
pub use tts::{TtsEngine, TtsConfig};
pub use conversation::{ConversationLoop, ConversationConfig, Turn};
