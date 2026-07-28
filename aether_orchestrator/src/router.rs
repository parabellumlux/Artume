//! AetherOS Intent Router
//!
//! Classifies user utterances into intents using the Nemotron-3 Nano
//! model on the GTX 1650S via Ollama. This replaces the menu hierarchy
//! with a single natural-language entry point.

use crate::ollama::OllamaClient;
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

    /// Parse a string from the model into an Intent.
    pub fn from_label(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "conversation" => Self::Conversation,
            "entity_lookup" | "entitylookup" => Self::EntityLookup,
            "web_fetch" | "webfetch" => Self::WebFetch,
            "execute_action" | "executeaction" => Self::ExecuteAction,
            "system_command" | "systemcommand" => Self::SystemCommand,
            _ => Self::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Router configuration
// ---------------------------------------------------------------------------

/// Configuration for the intent router.
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Fallback intent when classification fails.
    pub fallback_intent: Intent,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            fallback_intent: Intent::Conversation,
        }
    }
}

// ---------------------------------------------------------------------------
// Intent router
// ---------------------------------------------------------------------------

/// Routes user utterances to the correct sub-system.
///
/// Uses the Nemotron-3 Nano model on the GTX 1650S via Ollama to
/// classify intent, then dispatches to the appropriate handler.
pub struct IntentRouter {
    /// Ollama client for model inference.
    client: OllamaClient,
    /// Configuration.
    config: RouterConfig,
}

impl IntentRouter {
    /// Create a new intent router.
    pub fn new(config: RouterConfig) -> Self {
        Self {
            client: OllamaClient::new(None),
            config,
        }
    }

    /// Create with a custom Ollama client.
    pub fn with_client(client: OllamaClient, config: RouterConfig) -> Self {
        Self { client, config }
    }

    /// Classify a user utterance into an intent.
    ///
    /// Uses the Nemotron-3 Nano model on the 1650S with greedy decoding
    /// for deterministic classification.
    pub async fn classify(&self, utterance: &str) -> Intent {
        match self.client.classify_intent(utterance).await {
            Ok(label) => {
                let intent = Intent::from_label(&label);
                debug!("IntentRouter: '{}' → {:?}", utterance, intent);
                intent
            }
            Err(e) => {
                warn!("IntentRouter: classification failed: {e}");
                self.config.fallback_intent
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_from_label() {
        assert_eq!(Intent::from_label("conversation"), Intent::Conversation);
        assert_eq!(Intent::from_label("entity_lookup"), Intent::EntityLookup);
        assert_eq!(Intent::from_label("EntityLookup"), Intent::EntityLookup);
        assert_eq!(Intent::from_label("web_fetch"), Intent::WebFetch);
        assert_eq!(Intent::from_label("execute_action"), Intent::ExecuteAction);
        assert_eq!(Intent::from_label("system_command"), Intent::SystemCommand);
        assert_eq!(Intent::from_label("garbage"), Intent::Unknown);
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
