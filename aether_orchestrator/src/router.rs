//! AetherOS Intent Router
//!
//! Classifies user utterances into intents and dispatches to the
//! appropriate sub-system. This is the key component that replaces
//! the menu hierarchy with a single natural-language entry point.

use crate::models::{InferenceParams, ModelEngine, ModelKind, GenerationResult};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Intent types
// ---------------------------------------------------------------------------

/// The classified intent of a user utterance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Intent {
    /// General conversation / chit-chat.
    Conversation,
    /// Look up an entity from the context buffer ("Copy that tracking number").
    EntityLookup,
    /// Fetch and summarize a web page.
    WebFetch,
    /// Execute a system action (call, email, open app).
    ExecuteAction,
    /// System command (volume, settings, help).
    SystemCommand,
    /// Unknown / unclear intent.
    Unknown,
}

impl Intent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::EntityLookup => "entity_lookup",
            Self::WebFetch => "web_fetch",
            Self::ExecuteAction => "execute_action",
            Self::SystemCommand => "system_command",
            Self::Unknown => "unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// Router configuration
// ---------------------------------------------------------------------------

/// Configuration for the intent router.
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Path to the router model GGUF file (Qwen2.5-1.5B-Instruct Q4_K_M).
    pub model_path: String,
    /// Confidence threshold for accepting a classification.
    pub confidence_threshold: f32,
    /// Fallback intent when confidence is low.
    pub fallback_intent: Intent,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            model_path: "models/qwen2.5-1.5b-instruct-q4.gguf".to_string(),
            confidence_threshold: 0.6,
            fallback_intent: Intent::Conversation,
        }
    }
}

// ---------------------------------------------------------------------------
// Intent router
// ---------------------------------------------------------------------------

/// Routes user utterances to the correct sub-system.
///
/// Uses the router model (Qwen2.5-1.5B) to classify intent, then
/// dispatches to the appropriate handler.
pub struct IntentRouter {
    /// The router model engine.
    engine: ModelEngine,
    /// Configuration.
    config: RouterConfig,
}

impl IntentRouter {
    /// Create a new intent router.
    pub fn new(config: RouterConfig) -> Self {
        let engine = ModelEngine::new(ModelKind::Router, &config.model_path);
        Self { engine, config }
    }

    /// Load the router model.
    pub fn load(&mut self) -> anyhow::Result<()> {
        self.engine.load()
    }

    /// Classify a user utterance into an intent.
    ///
    /// Uses the router model with greedy decoding for deterministic
    /// classification.
    pub fn classify(&mut self, utterance: &str) -> Intent {
        // Build a classification prompt.
        let prompt = format!(
            "Classify the following user utterance into one of these intents:\n\
             - Conversation: general chat, questions, small talk\n\
             - EntityLookup: references to tracking numbers, phone numbers, addresses, emails\n\
             - WebFetch: requests to read articles, browse websites, fetch URLs\n\
             - ExecuteAction: requests to call, email, send, open apps\n\
             - SystemCommand: volume, settings, help, system control\n\n\
             Utterance: {}\n\nIntent:",
            utterance
        );

        let result = match self.engine.generate(&prompt, &InferenceParams::greedy()) {
            Ok(r) => r,
            Err(e) => {
                warn!("IntentRouter: classification failed: {e}");
                return self.config.fallback_intent;
            }
        };

        self.parse_intent(&result.text)
    }

    /// Parse the model output into an Intent.
    fn parse_intent(&self, text: &str) -> Intent {
        let trimmed = text.trim();
        match trimmed {
            s if s.eq_ignore_ascii_case("Conversation") => Intent::Conversation,
            s if s.eq_ignore_ascii_case("EntityLookup") => Intent::EntityLookup,
            s if s.eq_ignore_ascii_case("WebFetch") => Intent::WebFetch,
            s if s.eq_ignore_ascii_case("ExecuteAction") => Intent::ExecuteAction,
            s if s.eq_ignore_ascii_case("SystemCommand") => Intent::SystemCommand,
            _ => {
                debug!("IntentRouter: unknown intent '{trimmed}', falling back");
                self.config.fallback_intent
            }
        }
    }

    /// Check if the router is loaded.
    pub fn is_loaded(&self) -> bool {
        self.engine.is_loaded()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_router_creation() {
        let router = IntentRouter::new(RouterConfig::default());
        assert!(!router.is_loaded());
    }

    #[test]
    fn test_parse_intent() {
        let router = IntentRouter::new(RouterConfig::default());
        assert_eq!(router.parse_intent("Conversation"), Intent::Conversation);
        assert_eq!(router.parse_intent("EntityLookup"), Intent::EntityLookup);
        assert_eq!(router.parse_intent("WebFetch"), Intent::WebFetch);
        assert_eq!(router.parse_intent("ExecuteAction"), Intent::ExecuteAction);
        assert_eq!(router.parse_intent("SystemCommand"), Intent::SystemCommand);
        assert_eq!(router.parse_intent("garbage"), Intent::Conversation); // fallback
    }

    #[test]
    fn test_intent_labels() {
        assert_eq!(Intent::Conversation.label(), "conversation");
        assert_eq!(Intent::EntityLookup.label(), "entity_lookup");
        assert_eq!(Intent::WebFetch.label(), "web_fetch");
        assert_eq!(Intent::ExecuteAction.label(), "execute_action");
        assert_eq!(Intent::SystemCommand.label(), "system_command");
        assert_eq!(Intent::Unknown.label(), "unknown");
    }
}
