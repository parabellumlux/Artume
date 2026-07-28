//! AetherOS Conversational Orchestrator
//!
//! The central nervous system of AetherOS — routes natural-language input
//! through the dual-GPU AI stack:
//!
//! ```text
//! User Input → [Ollama API] → Intent Classification (Nemotron-3 Nano on 1650S)
//!   ├── Conversation → [Llama 3.1 8B on GTX 1080] → response
//!   ├── EntityLookup → [NER via Ollama] → buffer lookup → response
//!   ├── WebFetch     → [Browser engine] → [Llama 3.1 summary] → response
//!   └── ExecuteAction → [system call] → confirm
//! ```

pub mod models;
pub mod ollama;
pub mod router;
#[cfg(feature = "stt")]
pub mod stt;
#[cfg(feature = "tts")]
pub mod tts;
pub mod conversation;

pub use ollama::{OllamaClient, OllamaModel};
pub use router::{Intent, IntentRouter, RouterConfig};
#[cfg(feature = "stt")]
pub use stt::{SttEngine, SttConfig};
#[cfg(feature = "tts")]
pub use tts::{TtsEngine, TtsConfig};
pub use conversation::{ConversationLoop, ConversationConfig, Turn};
