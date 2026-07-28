//! AetherOS Conversational Orchestrator
//!
//! The central nervous system of AetherOS — routes natural-language input
//! through the dual-GPU AI stack:
//!
//! ```text
//! User Input → [Ollama API] → Intent Classification (Nemotron-3 Nano on 1650S)
//!   ├── Conversation → [Llama 3.1 8B on GTX 1080] → response (with history)
//!   ├── EntityLookup → [aether_buffer NER + ring buffer] → entity value
//!   ├── WebFetch     → [aether_browser HTTP + Readability] → [Llama 3.1 summary] → response
//!   ├── FileSearch   → [aetherfs-core gRPC daemon] → file results
//!   └── ExecuteAction → [system call] → confirm
//! ```

pub mod ollama;
pub mod router;
#[cfg(feature = "stt")]
pub mod stt;
#[cfg(feature = "tts")]
pub mod tts;
pub mod conversation;
pub mod file_search;

pub use ollama::{OllamaClient, OllamaModel};
pub use router::{Intent, IntentRouter, RouterConfig};
#[cfg(feature = "stt")]
pub use stt::{SttEngine, SttConfig};
#[cfg(feature = "tts")]
pub use tts::{TtsEngine, TtsConfig};
pub use conversation::{ConversationLoop, ConversationConfig, Turn};
pub use file_search::FileSearchClient;
