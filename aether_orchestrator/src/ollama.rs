//! AetherOS Ollama Backend
//!
//! Wraps the Ollama HTTP API to run models on specific GPUs.
//! Routes requests to the correct GPU based on model tier:
//!
//! - **Tier 1** (GTX 1080, GPU 0): Llama 3.1 8B — main reasoning/conversation
//! - **Tier 2** (GTX 1650S, GPU 1): Nemotron-3 Nano 4B — router/tool-caller
//! - **Tier 3** (CPU): nomic-embed-text — embeddings

use anyhow::{Context, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Ollama API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
    options: Option<Options>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct Options {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: MessageContent,
    done: bool,
    #[serde(default)]
    eval_count: Option<i32>,
    #[serde(default)]
    eval_duration: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    content: String,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Model configuration
// ---------------------------------------------------------------------------

/// A model available via Ollama, mapped to a specific GPU.
#[derive(Debug, Clone, Copy)]
pub struct OllamaModel {
    /// Ollama model name (e.g. "llama3.1:8b").
    pub name: &'static str,
    /// GPU to run on: "0" (1080), "1" (1650S), or "" (CPU).
    pub gpu: &'static str,
    /// Human-readable label.
    pub label: &'static str,
}

impl OllamaModel {
    pub const REASONING: Self = Self {
        name: "llama3.1:8b",
        gpu: "0",
        label: "reasoning",
    };

    pub const ROUTER: Self = Self {
        name: "nemotron-3-nano:4b",
        gpu: "1",
        label: "router",
    };

    pub const EMBED: Self = Self {
        name: "nomic-embed-text",
        gpu: "",
        label: "embed",
    };
}

// ---------------------------------------------------------------------------
// Ollama client
// ---------------------------------------------------------------------------

/// HTTP client for the Ollama API.
pub struct OllamaClient {
    /// Base URL of the Ollama server.
    base_url: String,
    /// Shared HTTP client.
    client: reqwest::Client,
}

impl OllamaClient {
    /// Create a new Ollama client.
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".to_string()),
            client: reqwest::Client::new(),
        }
    }

    /// Check if Ollama is running and a model is available.
    pub async fn health(&self) -> Result<bool> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .with_context(|| "Failed to connect to Ollama — is it running?")?;
        Ok(resp.status().is_success())
    }

    /// Generate a chat completion with full message history.
    ///
    /// `messages` is a list of `(role, content)` pairs where role is
    /// "system", "user", or "assistant". This enables multi-turn
    /// conversation with context.
    pub async fn chat_with_messages(
        &self,
        model: &OllamaModel,
        messages: &[(&str, &str)],
        temperature: f32,
        max_tokens: i32,
    ) -> Result<String> {
        let start = Instant::now();

        let msgs: Vec<Message> = messages
            .iter()
            .map(|(role, content)| Message {
                role: role.to_string(),
                content: content.to_string(),
            })
            .collect();

        let request = ChatRequest {
            model: model.name.to_string(),
            messages: msgs,
            stream: false,
            options: Some(Options {
                num_predict: Some(max_tokens),
                temperature: Some(temperature),
                top_p: Some(0.9),
            }),
        };

        debug!(
            "Ollama[{}] on GPU {}: sending chat with {} messages ({} tokens max, temp={})",
            model.label,
            model.gpu,
            messages.len(),
            max_tokens,
            temperature
        );

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Ollama chat request failed for {}", model.name))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama returned HTTP {}: {}", status, body);
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .with_context(|| "Failed to parse Ollama response")?;

        let elapsed = start.elapsed();
        let tokens = chat_resp.eval_count.unwrap_or(0);
        let tok_s = if elapsed.as_secs_f32() > 0.0 {
            tokens as f32 / elapsed.as_secs_f32()
        } else {
            0.0
        };

        info!(
            "Ollama[{}] on GPU {}: {} tokens in {:.0}ms ({:.1} tok/s)",
            model.label,
            model.gpu,
            tokens,
            elapsed.as_millis(),
            tok_s
        );

        Ok(chat_resp.message.content)
    }

    /// Generate a chat completion on the specified model/GPU.
    ///
    /// `model` defines which Ollama model and which GPU to use.
    /// The GPU is selected by setting `CUDA_VISIBLE_DEVICES` via the
    /// Ollama server's environment (configured in the Ollama systemd
    /// service or launch script).
    pub async fn chat(
        &self,
        model: &OllamaModel,
        prompt: &str,
        system_prompt: Option<&str>,
        temperature: f32,
        max_tokens: i32,
    ) -> Result<String> {
        let start = Instant::now();

        let mut messages = Vec::new();

        if let Some(sys) = system_prompt {
            messages.push(Message {
                role: "system".to_string(),
                content: sys.to_string(),
            });
        }

        messages.push(Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        let request = ChatRequest {
            model: model.name.to_string(),
            messages,
            stream: false,
            options: Some(Options {
                num_predict: Some(max_tokens),
                temperature: Some(temperature),
                top_p: Some(0.9),
            }),
        };

        debug!(
            "Ollama[{}] on GPU {}: sending request ({} tokens max, temp={})",
            model.label, model.gpu, max_tokens, temperature
        );

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Ollama chat request failed for {}", model.name))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama returned HTTP {}: {}", status, body);
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .with_context(|| "Failed to parse Ollama response")?;

        let elapsed = start.elapsed();
        let tokens = chat_resp.eval_count.unwrap_or(0);
        let tok_s = if elapsed.as_secs_f32() > 0.0 {
            tokens as f32 / elapsed.as_secs_f32()
        } else {
            0.0
        };

        info!(
            "Ollama[{}] on GPU {}: {} tokens in {:.0}ms ({:.1} tok/s)",
            model.label,
            model.gpu,
            tokens,
            elapsed.as_millis(),
            tok_s
        );

        Ok(chat_resp.message.content)
    }

    /// Generate an embedding vector on CPU.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let start = Instant::now();

        #[derive(Serialize)]
        struct EmbedRequest<'a> {
            model: &'a str,
            input: &'a str,
        }

        let request = EmbedRequest {
            model: OllamaModel::EMBED.name,
            input: text,
        };

        let resp = self
            .client
            .post(format!("{}/api/embed", self.base_url))
            .json(&request)
            .send()
            .await
            .with_context(|| "Ollama embed request failed")?;

        let embed_resp: EmbedResponse = resp
            .json()
            .await
            .with_context(|| "Failed to parse Ollama embed response")?;

        let elapsed = start.elapsed();
        debug!(
            "Ollama[embed] on CPU: {} dims in {:.0}ms",
            embed_resp.embedding.len(),
            elapsed.as_millis()
        );

        Ok(embed_resp.embedding)
    }

    /// Quick classification using the router model on the 1650S.
    pub async fn classify_intent(&self, utterance: &str) -> Result<String> {
        let prompt = format!(
            "Classify this user utterance into one of these intents: \
             conversation, entity_lookup, web_fetch, execute_action, system_command.\n\n\
             Utterance: {}\n\nIntent:",
            utterance
        );

        self.chat(
            &OllamaModel::ROUTER,
            &prompt,
            None,
            0.0, // greedy for classification
            32,
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_model_constants() {
        assert_eq!(OllamaModel::REASONING.name, "llama3.1:8b");
        assert_eq!(OllamaModel::REASONING.gpu, "0");
        assert_eq!(OllamaModel::ROUTER.name, "nemotron-3-nano:4b");
        assert_eq!(OllamaModel::ROUTER.gpu, "1");
        assert_eq!(OllamaModel::EMBED.name, "nomic-embed-text");
        assert_eq!(OllamaModel::EMBED.gpu, "");
    }

    #[test]
    fn test_ollama_client_creation() {
        let client = OllamaClient::new(None);
        assert_eq!(client.base_url, "http://localhost:11434");
    }
}
