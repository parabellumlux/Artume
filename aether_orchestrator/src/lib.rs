//! Artume Conversational Orchestrator
//!
//! The central nervous system of Artume — routes natural-language input
//! through the dual-GPU AI stack:
//!
//! ```text
//! User Input → [Ollama API] → Intent Classification (Llama 3.1 8B on 1080)
//!   ├── Conversation → [Llama 3.1 8B on GTX 1080] → response (with history)
//!   ├── EntityLookup → [aether_buffer NER + ring buffer] → entity value
//!   ├── WebFetch     → [aether_browser HTTP + Readability] → [Llama 3.1 summary] → response
//!   ├── FileSearch   → [aetherfs-core gRPC daemon] → file results
//!   └── ExecuteAction → [system call] → confirm
//! ```

pub mod conversation;
pub mod file_search;
pub mod ollama;
pub mod profile;
pub mod router;
pub mod skills;
#[cfg(feature = "stt")]
pub mod stt;
pub mod system_commands;
#[cfg(feature = "tts")]
pub mod tts;
#[cfg(feature = "tts")]
pub mod tts_service;
#[cfg(feature = "tts")]
pub use tts_service::{StreamingTts, TtsService};

pub use conversation::{ConversationConfig, ConversationLoop, Turn};
pub use file_search::FileSearchClient;
pub use ollama::{OllamaClient, OllamaModel};
pub use router::{Intent, IntentRouter, RouterConfig};
#[cfg(feature = "stt")]
pub use stt::{SttConfig, SttEngine};
#[cfg(feature = "tts")]
pub use tts::{TtsConfig, TtsEngine};
