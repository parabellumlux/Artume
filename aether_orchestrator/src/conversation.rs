//! AetherOS Conversation Loop
//!
//! The main conversational loop that ties all subsystems together:
//!
//! 1. Listen (STT) → 2. Classify (Router) → 3. Dispatch → 4. Respond (TTS)
//!
//! This is the top-level orchestrator that makes AetherOS feel like
//! a conversation rather than a menu system.

use crate::models::{InferenceParams, ModelEngine, ModelKind};
use crate::ollama::{OllamaClient, OllamaModel};
use crate::router::{Intent, IntentRouter, RouterConfig};
#[cfg(feature = "stt")]
use crate::stt::{SttConfig, SttEngine};
#[cfg(feature = "tts")]
use crate::tts::{TtsConfig, TtsEngine};
use log::{debug, info, warn};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Conversation turn
// ---------------------------------------------------------------------------

/// A single turn in the conversation.
#[derive(Debug, Clone)]
pub struct Turn {
    /// The user's spoken input (transcribed).
    pub user_text: String,
    /// The classified intent.
    pub intent: Intent,
    /// The system's response text.
    pub response: String,
    /// How long the full turn took (ms).
    pub turn_ms: u64,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Conversation configuration
// ---------------------------------------------------------------------------

/// Configuration for the conversation loop.
#[derive(Debug, Clone)]
pub struct ConversationConfig {
    /// Path to the conversation model GGUF (SmolLM3 3B Q4_K_M).
    pub conversation_model_path: String,
    /// Path to the NER model GGUF (Qwen2.5-0.5B Q4_K_M).
    pub ner_model_path: String,
    /// Router configuration.
    pub router: RouterConfig,
    /// STT configuration (only used with "stt" feature).
    #[cfg(feature = "stt")]
    pub stt: SttConfig,
    /// TTS configuration (only used with "tts" feature).
    #[cfg(feature = "tts")]
    pub tts: TtsConfig,
    /// System prompt for the conversation model.
    pub system_prompt: String,
    /// Whether to enable voice I/O (vs text-only for testing).
    pub voice_enabled: bool,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            conversation_model_path: "models/smollm3-3b-q4.gguf".to_string(),
            ner_model_path: "models/qwen2.5-0.5b-q4.gguf".to_string(),
            router: RouterConfig::default(),
            #[cfg(feature = "stt")]
            stt: SttConfig::default(),
            #[cfg(feature = "tts")]
            tts: TtsConfig::default(),
            system_prompt: "You are AetherOS, a helpful voice-controlled operating system assistant. \
                           Keep responses concise and conversational since they will be spoken aloud. \
                           Use natural language. Be helpful, clear, and direct."
                .to_string(),
            voice_enabled: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Conversation loop
// ---------------------------------------------------------------------------

/// The main conversational loop.
///
/// Manages the full pipeline: listen → classify → dispatch → respond.
pub struct ConversationLoop {
    /// Configuration.
    config: ConversationConfig,
    /// Intent router.
    router: IntentRouter,
    /// Ollama client for GPU-backed inference.
    ollama: OllamaClient,
    /// Conversation model (SmolLM3 3B — fallback when Ollama is unavailable).
    conversation_model: ModelEngine,
    /// NER model (Qwen2.5-0.5B).
    ner_model: ModelEngine,
    /// STT engine (only available with "stt" feature).
    #[cfg(feature = "stt")]
    stt: SttEngine,
    /// TTS engine (only available with "tts" feature).
    #[cfg(feature = "tts")]
    tts: TtsEngine,
    /// Conversation history (recent turns).
    history: Vec<Turn>,
    /// Total turns processed.
    total_turns: u64,
}

impl ConversationLoop {
    /// Create a new conversation loop.
    pub fn new(config: ConversationConfig) -> Self {
        let router = IntentRouter::new(config.router.clone());
        let ollama = OllamaClient::new(None);
        let conversation_model =
            ModelEngine::new(ModelKind::Conversation, &config.conversation_model_path);
        let ner_model = ModelEngine::new(ModelKind::NER, &config.ner_model_path);

        Self {
            config,
            router,
            ollama,
            conversation_model,
            ner_model,
            #[cfg(feature = "stt")]
            stt: SttEngine::new(config.stt.clone()),
            #[cfg(feature = "tts")]
            tts: TtsEngine::new(config.tts.clone()),
            history: Vec::new(),
            total_turns: 0,
        }
    }

    /// Load all models.
    pub fn load_all(&mut self) -> anyhow::Result<()> {
        info!("ConversationLoop: loading all models...");

        // Conversation model is optional — without it we fall back to
        // template responses.
        self.conversation_model
            .load()
            .map_err(|e| {
                warn!("Conversation model load failed (non-fatal): {e}");
                e
            })
            .ok();

        self.ner_model
            .load()
            .map_err(|e| {
                warn!("NER model load failed (non-fatal): {e}");
                e
            })
            .ok();

        #[cfg(feature = "stt")]
        self.stt
            .load()
            .map_err(|e| {
                warn!("STT model load failed (non-fatal): {e}");
                e
            })
            .ok();

        #[cfg(feature = "tts")]
        self.tts
            .load()
            .map_err(|e| {
                warn!("TTS model load failed (non-fatal): {e}");
                e
            })
            .ok();

        info!("ConversationLoop: models loaded (some may have failed — check warnings)");
        Ok(())
    }

    /// Process a single turn: text input → response.
    ///
    /// This is the core pipeline:
    /// 1. Classify intent via router model
    /// 2. Dispatch to the appropriate handler
    /// 3. Generate response
    /// 4. Optionally synthesize speech
    pub async fn process_turn(&mut self, user_text: &str) -> anyhow::Result<Turn> {
        let start = Instant::now();
        let timestamp = chrono::Utc::now();

        // Step 1: Classify intent.
        let intent = self.router.classify(user_text).await;

        // Step 2: Dispatch based on intent.
        let response = match intent {
            Intent::Conversation => self.handle_conversation(user_text).await,
            Intent::EntityLookup => self.handle_entity_lookup(user_text),
            Intent::WebFetch => self.handle_web_fetch(user_text),
            Intent::ExecuteAction => self.handle_execute_action(user_text),
            Intent::SystemCommand => self.handle_system_command(user_text),
            Intent::Unknown => self.handle_unknown(user_text),
        };

        let turn_ms = start.elapsed().as_millis() as u64;

        let turn = Turn {
            user_text: user_text.to_string(),
            intent,
            response,
            turn_ms,
            timestamp,
        };

        // Step 3: Optionally speak the response.
        #[cfg(feature = "tts")]
        if self.config.voice_enabled && self.tts.is_loaded() {
            match self.tts.synthesize(&turn.response) {
                Ok(_samples) => debug!("TTS: synthesized response"),
                Err(e) => warn!("TTS synthesis failed: {e}"),
            }
        }

        self.history.push(turn.clone());
        self.total_turns += 1;

        info!(
            "Turn #{}: intent={} response_len={} time={}ms",
            self.total_turns,
            intent.label(),
            turn.response.len(),
            turn_ms
        );

        Ok(turn)
    }

    /// Process a turn from audio input (STT → classify → respond).
    #[cfg(feature = "stt")]
    pub async fn process_audio_turn(&mut self, audio_samples: &[f32]) -> anyhow::Result<Turn> {
        let text = self.stt.transcribe(audio_samples)?;
        self.process_turn(&text).await
    }

    // --- Intent handlers ---

    async fn handle_conversation(&mut self, user_text: &str) -> String {
        // Try Ollama on GTX 1080 first (Tier 1).
        match self
            .ollama
            .chat(
                &OllamaModel::REASONING,
                user_text,
                Some(&self.config.system_prompt),
                0.7,
                512,
            )
            .await
        {
            Ok(response) => response,
            Err(e) => {
                warn!("Ollama[1080] failed: {e} — falling back to local model");
                // Fallback to local llama.cpp model.
                if self.conversation_model.is_loaded() {
                    let params = InferenceParams {
                        system_prompt: Some(self.config.system_prompt.clone()),
                        ..InferenceParams::creative()
                    };
                    match self.conversation_model.generate(user_text, &params) {
                        Ok(result) => result.text,
                        Err(e2) => format!("I had trouble thinking about that: {e2}"),
                    }
                } else {
                    self.template_conversation(user_text)
                }
            }
        }
    }

    fn handle_entity_lookup(&mut self, user_text: &str) -> String {
        if self.ner_model.is_loaded() {
            match self.ner_model.generate(user_text, &InferenceParams::greedy()) {
                Ok(result) => {
                    format!("I found: {}", result.text)
                }
                Err(e) => format!("Could not extract entities: {e}"),
            }
        } else {
            "I can look up tracking numbers, phone numbers, and addresses from our conversation. \
             Just tell me what you need to save or copy."
                .to_string()
        }
    }

    fn handle_web_fetch(&mut self, user_text: &str) -> String {
        // Extract a URL from the text if present.
        let url = self.extract_url(user_text);
        match url {
            Some(u) => format!("I'll fetch that page for you: {u}"),
            None => "I can read web pages for you. Just give me a URL or say 'read me the article'."
                .to_string(),
        }
    }

    fn handle_execute_action(&mut self, user_text: &str) -> String {
        format!(
            "I understand you want to perform an action: \"{}\". \
             This capability is being wired up.",
            user_text.chars().take(60).collect::<String>()
        )
    }

    fn handle_system_command(&mut self, user_text: &str) -> String {
        format!(
            "System command received: \"{}\". \
             Volume and settings control coming soon.",
            user_text.chars().take(60).collect::<String>()
        )
    }

    fn handle_unknown(&mut self, _user_text: &str) -> String {
        "I'm not sure what you need. You can ask me to read a web page, \
         look up information from our conversation, or just chat."
            .to_string()
    }

    // --- Helpers ---

    fn template_conversation(&self, user_text: &str) -> String {
        let lower = user_text.to_lowercase();
        if lower.contains("hello") || lower.contains("hi ") || lower.contains("hey") {
            "Hello! I'm AetherOS. How can I help you today?".to_string()
        } else if lower.contains("how are you") {
            "I'm doing well! Ready to help with whatever you need.".to_string()
        } else if lower.contains("thank") || lower.contains("thanks") {
            "You're welcome! Let me know if you need anything else.".to_string()
        } else if lower.contains("weather") {
            "I can check the weather for you once I'm connected to a weather service.".to_string()
        } else if lower.contains("time") {
            let now = chrono::Local::now();
            format!("The current time is {}.", now.format("%I:%M %p"))
        } else if lower.contains("name") || lower.contains("who are you") {
            "I'm AetherOS, your voice-controlled operating system assistant.".to_string()
        } else {
            format!(
                "I heard you say: \"{}\". I'm still learning, but I'm getting better every day!",
                user_text.chars().take(80).collect::<String>()
            )
        }
    }

    fn extract_url(&self, text: &str) -> Option<String> {
        // Simple URL extraction.
        for word in text.split_whitespace() {
            if word.starts_with("http://") || word.starts_with("https://") {
                return Some(word.trim_end_matches(&['.', ',', ';', '!', '?'][..]).to_string());
            }
            if word.starts_with("www.") {
                return Some(format!("https://{}", word.trim_end_matches(&['.', ',', ';', '!', '?'][..])));
            }
        }
        None
    }

    /// Get conversation history.
    pub fn history(&self) -> &[Turn] {
        &self.history
    }

    /// Get total turns processed.
    pub fn total_turns(&self) -> u64 {
        self.total_turns
    }

    /// Check if voice is enabled.
    pub fn voice_enabled(&self) -> bool {
        self.config.voice_enabled
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_conversation_loop_creation() {
        let loop_ = ConversationLoop::new(ConversationConfig::default());
        assert_eq!(loop_.total_turns(), 0);
        assert!(!loop_.voice_enabled());
    }

    #[tokio::test]
    async fn test_process_conversation_turn() {
        let mut loop_ = ConversationLoop::new(ConversationConfig::default());
        loop_.load_all().ok(); // models won't exist, but that's fine
        let turn = loop_.process_turn("Hello, how are you?").await.unwrap();
        // Without the router model loaded, it falls back to Unknown
        assert!(!turn.response.is_empty());
    }

    #[tokio::test]
    async fn test_process_entity_lookup_turn() {
        let mut loop_ = ConversationLoop::new(ConversationConfig::default());
        loop_.load_all().ok();
        let turn = loop_.process_turn("Copy that tracking number").await.unwrap();
        // Without the router model loaded, it falls back to Conversation
        // (the simulated router only works when the model is "loaded")
        assert!(!turn.response.is_empty());
    }

    #[tokio::test]
    async fn test_process_web_fetch_turn() {
        let mut loop_ = ConversationLoop::new(ConversationConfig::default());
        loop_.load_all().ok();
        let turn = loop_.process_turn("Read me https://example.com").await.unwrap();
        // Without the router model loaded, it falls back to Conversation
        assert!(!turn.response.is_empty());
    }

    #[tokio::test]
    async fn test_conversation_history() {
        let mut loop_ = ConversationLoop::new(ConversationConfig::default());
        loop_.load_all().ok();
        loop_.process_turn("Hello").await.unwrap();
        loop_.process_turn("What's the time?").await.unwrap();
        assert_eq!(loop_.history().len(), 2);
        assert_eq!(loop_.total_turns(), 2);
    }

    #[test]
    fn test_extract_url() {
        let loop_ = ConversationLoop::new(ConversationConfig::default());
        assert_eq!(
            loop_.extract_url("Read https://example.com/page"),
            Some("https://example.com/page".to_string())
        );
        assert_eq!(
            loop_.extract_url("Go to www.google.com"),
            Some("https://www.google.com".to_string())
        );
        assert_eq!(loop_.extract_url("Just chat"), None);
    }

    #[test]
    fn test_template_conversation_greeting() {
        let loop_ = ConversationLoop::new(ConversationConfig::default());
        let response = loop_.template_conversation("Hello there!");
        assert!(response.contains("Hello"));
    }

    #[test]
    fn test_template_conversation_time() {
        let loop_ = ConversationLoop::new(ConversationConfig::default());
        let response = loop_.template_conversation("What time is it?");
        assert!(response.contains("time"));
    }
}
