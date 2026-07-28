//! AetherOS Conversation Loop
//!
//! The main conversational loop that ties all subsystems together:
//!
//! 1. Listen (STT) → 2. Classify (Router) → 3. Dispatch → 4. Respond (TTS)
//!
//! This is the top-level orchestrator that makes AetherOS feel like
//! a conversation rather than a menu system.

use crate::ollama::{OllamaClient, OllamaModel};
use crate::router::{Intent, IntentRouter, RouterConfig};
#[cfg(feature = "stt")]
use crate::stt::{SttConfig, SttEngine};
#[cfg(feature = "tts")]
use crate::tts::{TtsConfig, TtsEngine};
use aether_browser::{BrowserEngine, ReadabilityExtractor, ConversationalFormatter};
use aether_buffer::{ContextResolver, TranscriptRingBuffer};
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
    /// Maximum conversation history turns to include in context.
    pub max_history_turns: usize,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
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
            max_history_turns: 5,
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
    /// Browser engine for web fetching.
    browser: BrowserEngine,
    /// Transcript ring buffer for entity lookup.
    transcript_buffer: TranscriptRingBuffer,
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
        let browser = BrowserEngine::new().unwrap_or_else(|e| {
            warn!("Failed to create browser engine: {e} — web fetch will be unavailable");
            // Create a minimal placeholder — BrowserEngine::new() only fails on
            // client builder error, which is rare. We still need the field.
            BrowserEngine::new().expect("BrowserEngine creation failed")
        });

        Self {
            config,
            router,
            ollama,
            browser,
            transcript_buffer: TranscriptRingBuffer::new(),
            #[cfg(feature = "stt")]
            stt: SttEngine::new(config.stt.clone()),
            #[cfg(feature = "tts")]
            tts: TtsEngine::new(config.tts.clone()),
            history: Vec::new(),
            total_turns: 0,
        }
    }

    /// Check if Ollama is reachable.
    pub async fn check_ollama_health(&self) -> bool {
        match self.ollama.health().await {
            Ok(true) => {
                info!("Ollama health check: OK");
                true
            }
            Ok(false) => {
                warn!("Ollama health check: server responded but unexpected status");
                false
            }
            Err(e) => {
                warn!("Ollama health check: FAILED — {e}");
                false
            }
        }
    }

    /// Load all models.
    pub fn load_all(&mut self) -> anyhow::Result<()> {
        info!("ConversationLoop: loading all models...");

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

        // Push user text into transcript buffer for entity lookup.
        self.transcript_buffer.push(user_text, aether_buffer::TranscriptSource::User);

        // Step 1: Classify intent.
        let intent = self.router.classify(user_text).await;

        // Step 2: Dispatch based on intent.
        let response = match intent {
            Intent::Conversation => self.handle_conversation(user_text).await,
            Intent::EntityLookup => self.handle_entity_lookup(user_text),
            Intent::WebFetch => self.handle_web_fetch(user_text).await,
            Intent::ExecuteAction => self.handle_execute_action(user_text),
            Intent::SystemCommand => self.handle_system_command(user_text),
            Intent::Unknown => self.handle_unknown(user_text),
        };

        // Push system response into transcript buffer.
        self.transcript_buffer.push(&response, aether_buffer::TranscriptSource::System);

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
        // Build message history for context.
        let mut messages: Vec<(&str, &str)> = Vec::new();

        // System prompt.
        messages.push(("system", &self.config.system_prompt));

        // Recent history (sliding window).
        let start_idx = self
            .history
            .len()
            .saturating_sub(self.config.max_history_turns);
        for turn in &self.history[start_idx..] {
            messages.push(("user", &turn.user_text));
            messages.push(("assistant", &turn.response));
        }

        // Current user input.
        messages.push(("user", user_text));

        // Try Ollama on GTX 1080 first (Tier 1).
        match self
            .ollama
            .chat_with_messages(&OllamaModel::REASONING, &messages, 0.7, 512)
            .await
        {
            Ok(response) => response,
            Err(e) => {
                warn!("Ollama[1080] failed: {e} — using template fallback");
                self.template_conversation(user_text)
            }
        }
    }

    fn handle_entity_lookup(&mut self, user_text: &str) -> String {
        let resolver = ContextResolver::new(&self.transcript_buffer);
        match resolver.resolve_reference(user_text) {
            Some(entity) => {
                info!(
                    "EntityLookup: resolved '{}' → {}: {}",
                    user_text, entity.entity_type, entity.value
                );
                format!("I found {}: {}.", entity.entity_type, entity.value)
            }
            None => {
                // Fallback: search the buffer for any recent entities.
                let all = self.transcript_buffer.all_entities();
                if all.is_empty() {
                    "I don't see any recent information I can look up. Try saying something like 'Copy that tracking number' after I've mentioned one."
                        .to_string()
                } else {
                    let types: std::collections::HashSet<&str> =
                        all.iter().map(|e| e.entity_type.as_str()).collect();
                    let mut type_list: Vec<&str> = types.into_iter().collect();
                    type_list.sort();
                    format!(
                        "I found {} saved entr{}. Available types: {}. Try being more specific.",
                        all.len(),
                        if all.len() == 1 { "y" } else { "ies" },
                        type_list.join(", ")
                    )
                }
            }
        }
    }

    async fn handle_web_fetch(&mut self, user_text: &str) -> String {
        // Extract a URL from the text.
        let url = self.extract_url(user_text);

        match url {
            Some(u) => {
                info!("WebFetch: fetching {u}");
                match self.browser.fetch(&u).await {
                    Ok(result) => {
                        // Extract readable content.
                        let content = ReadabilityExtractor::extract(&result.html);
                        let formatted = ConversationalFormatter::format(&content);

                        // If we have Ollama, summarize the content.
                        if formatted.len() > 200 {
                            let summary_prompt = format!(
                                "Summarize this web page content in 2-3 concise sentences:\n\nTitle: {}\n\n{}",
                                content.title, formatted
                            );
                            match self
                                .ollama
                                .chat(&OllamaModel::REASONING, &summary_prompt, None, 0.3, 256)
                                .await
                            {
                                Ok(summary) => {
                                    format!(
                                        "Here's what I found on \"{}\": {}",
                                        content.title, summary
                                    )
                                }
                                Err(_) => {
                                    // Fallback: return the formatted content directly.
                                    let preview = if formatted.len() > 500 {
                                        format!("{}... (content continues)", &formatted[..500])
                                    } else {
                                        formatted.clone()
                                    };
                                    format!("Here's what I found on \"{}\": {}", content.title, preview)
                                }
                            }
                        } else {
                            format!("Here's what I found on \"{}\": {}", content.title, formatted)
                        }
                    }
                    Err(e) => {
                        warn!("WebFetch: failed to fetch {u}: {e}");
                        format!("I couldn't fetch that page: {e}")
                    }
                }
            }
            None => {
                "I can read web pages for you. Just give me a URL or say 'read me the article'."
                    .to_string()
            }
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
        loop_.load_all().ok();
        let turn = loop_.process_turn("Hello, how are you?").await.unwrap();
        assert!(!turn.response.is_empty());
    }

    #[tokio::test]
    async fn test_process_entity_lookup_turn() {
        let mut loop_ = ConversationLoop::new(ConversationConfig::default());
        loop_.load_all().ok();
        let turn = loop_.process_turn("Copy that tracking number").await.unwrap();
        assert!(!turn.response.is_empty());
    }

    #[tokio::test]
    async fn test_process_web_fetch_turn() {
        let mut loop_ = ConversationLoop::new(ConversationConfig::default());
        loop_.load_all().ok();
        let turn = loop_.process_turn("Read me https://example.com").await.unwrap();
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
