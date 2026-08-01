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
    /// Search the file index ("Find my tax documents").
    FileSearch,
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
            Self::FileSearch => "file_search",
            Self::ExecuteAction => "execute_action",
            Self::SystemCommand => "system_command",
            Self::Unknown => "unknown",
        }
    }

    /// Parse a string from the model into an Intent.
    pub fn from_label(s: &str) -> Self {
        let lower = s.trim().to_lowercase();
        match lower.as_str() {
            "conversation" | "chat" | "talk" | "greeting" | "hello" | "general" => Self::Conversation,
            "entity_lookup" | "entitylookup" | "lookup" | "entity" => Self::EntityLookup,
            "web_fetch" | "webfetch" | "fetch" | "web" | "browse" | "read" => Self::WebFetch,
            "file_search" | "filesearch" | "search" | "file" | "find" => Self::FileSearch,
            "execute_action" | "executeaction" | "action" | "execute" => Self::ExecuteAction,
            "system_command" | "systemcommand" | "command" | "system" => Self::SystemCommand,
            _ => {
                // Also check if the label is embedded in a longer response
                if lower.contains("conversation") || lower.contains("chat") || lower.contains("talk") {
                    Self::Conversation
                } else if lower.contains("entity") || lower.contains("lookup") {
                    Self::EntityLookup
                } else if lower.contains("web") || lower.contains("fetch") || lower.contains("browse") {
                    Self::WebFetch
                } else if lower.contains("file") || lower.contains("search") {
                    Self::FileSearch
                } else if lower.contains("action") || lower.contains("execute") {
                    Self::ExecuteAction
                } else if lower.contains("command") || lower.contains("system") {
                    Self::SystemCommand
                } else {
                    Self::Unknown
                }
            }
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
        // Fast path: keyword-based pre-classification for common patterns
        let lower = utterance.to_lowercase();
        let trimmed = lower.trim();
        if trimmed.is_empty() {
            return Intent::Unknown;
        }

        // Common greetings and conversation starters
        if trimmed.len() < 20
            && (trimmed.starts_with("hello")
                || trimmed.starts_with("hi ")
                || trimmed.starts_with("hey")
                || trimmed.starts_with("how are")
                || trimmed.starts_with("what's up")
                || trimmed.starts_with("good morning")
                || trimmed.starts_with("good evening")
                || trimmed.starts_with("good afternoon")
                || trimmed == "yo"
                || trimmed == "sup"
                || trimmed == "hey")
        {
            return Intent::Conversation;
        }

        // LLM-based classification for everything else
        match self.client.classify_intent(utterance).await {
            Ok(label) => {
                let trimmed_label = label.trim();
                info!("IntentRouter: raw model output for '{}' → '{}'", utterance, trimmed_label);
                if trimmed_label.is_empty() {
                    // Model returned nothing — fall back to conversation
                    info!("IntentRouter: empty response from model, defaulting to Conversation");
                    return Intent::Conversation;
                }
                let intent = Intent::from_label(trimmed_label);
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
        assert_eq!(Intent::from_label("chat"), Intent::Conversation);
        assert_eq!(Intent::from_label("greeting"), Intent::Conversation);
        assert_eq!(Intent::from_label("entity_lookup"), Intent::EntityLookup);
        assert_eq!(Intent::from_label("EntityLookup"), Intent::EntityLookup);
        assert_eq!(Intent::from_label("web_fetch"), Intent::WebFetch);
        assert_eq!(Intent::from_label("fetch"), Intent::WebFetch);
        assert_eq!(Intent::from_label("file_search"), Intent::FileSearch);
        assert_eq!(Intent::from_label("search"), Intent::FileSearch);
        assert_eq!(Intent::from_label("execute_action"), Intent::ExecuteAction);
        assert_eq!(Intent::from_label("system_command"), Intent::SystemCommand);
        assert_eq!(Intent::from_label("garbage"), Intent::Unknown);
        // Fuzzy matching
        assert_eq!(Intent::from_label("this is a conversation"), Intent::Conversation);
        assert_eq!(Intent::from_label("web browsing"), Intent::WebFetch);
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
