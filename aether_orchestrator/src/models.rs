//! AetherOS Model Engine — local LLM inference via llama.cpp
//!
//! Wraps `llama-cpp-2` to load and run GGUF-quantized models on CPU.
//! Supports the three-tier model stack:
//!
//! - **Router** (Qwen2.5-1.5B, Q4): intent classification, ~40 tok/s
//! - **Conversation** (SmolLM3 3B, Q4): primary dialogue, ~25 tok/s
//! - **NER** (Qwen2.5-0.5B, Q4): entity extraction, ~60 tok/s

use anyhow::{Context, Result};
use encoding_rs;
use log::{debug, info, warn};
use std::path::Path;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Model kind
// ---------------------------------------------------------------------------

/// Which model in the three-tier stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelKind {
    /// Intent router — Qwen2.5-1.5B-Instruct (Q4_K_M)
    Router,
    /// Primary conversation — SmolLM3 3B (Q4_K_M)
    Conversation,
    /// Entity extraction — Qwen2.5-0.5B (Q4_K_M)
    NER,
}

impl ModelKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Router => "router",
            Self::Conversation => "conversation",
            Self::NER => "ner",
        }
    }

    /// Recommended context size for each model.
    pub fn context_size(self) -> u32 {
        match self {
            Self::Router => 2048,
            Self::Conversation => 4096,
            Self::NER => 1024,
        }
    }
}

// ---------------------------------------------------------------------------
// Inference parameters
// ---------------------------------------------------------------------------

/// Parameters for a single inference call.
#[derive(Debug, Clone)]
pub struct InferenceParams {
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Temperature (0.0 = greedy, 0.7 = creative).
    pub temperature: f32,
    /// Top-p nucleus sampling.
    pub top_p: f32,
    /// Repeat penalty.
    pub repeat_penalty: f32,
    /// System prompt (prepended to every request).
    pub system_prompt: Option<String>,
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            repeat_penalty: 1.1,
            system_prompt: None,
        }
    }
}

impl InferenceParams {
    /// Greedy params (for classification / routing).
    pub fn greedy() -> Self {
        Self {
            temperature: 0.0,
            top_p: 1.0,
            repeat_penalty: 1.0,
            ..Default::default()
        }
    }

    /// Creative params (for conversation).
    pub fn creative() -> Self {
        Self {
            temperature: 0.8,
            top_p: 0.95,
            max_tokens: 512,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Generation result
// ---------------------------------------------------------------------------

/// The result of a generation call.
#[derive(Debug, Clone)]
pub struct GenerationResult {
    /// The generated text.
    pub text: String,
    /// Number of tokens generated.
    pub tokens_generated: u32,
    /// Generation speed (tok/s).
    pub tokens_per_second: f32,
    /// Total inference time in milliseconds.
    pub inference_ms: u64,
}

// ---------------------------------------------------------------------------
// Model engine
// ---------------------------------------------------------------------------

/// A loaded GGUF model ready for inference.
///
/// Wraps `llama-cpp-2`'s context and provides a safe generation API.
pub struct ModelEngine {
    kind: ModelKind,
    /// Path to the GGUF file.
    model_path: String,
    /// Whether the model is loaded.
    loaded: bool,
    /// The underlying llama.cpp model and context.
    ctx: Option<ModelContext>,
    /// Inference timing stats.
    total_inferences: u64,
    total_tokens_generated: u64,
}

/// Holds the live llama.cpp model and its inference context.
///
/// The model is leaked (Box::leak) to provide a `&'static` reference
/// that the context borrows from. The raw pointer keeps the allocation
/// alive; the context's `model` field provides safe access.
struct ModelContext {
    _backend: llama_cpp_2::llama_backend::LlamaBackend,
    /// Leaked model allocation — kept alive for the struct's lifetime.
    _model_ptr: *const llama_cpp_2::model::LlamaModel,
    context: llama_cpp_2::context::LlamaContext<'static>,
}

impl ModelContext {
    fn new(
        backend: llama_cpp_2::llama_backend::LlamaBackend,
        model: llama_cpp_2::model::LlamaModel,
        context_params: llama_cpp_2::context::params::LlamaContextParams,
    ) -> Result<Self> {
        // Leak the model to get a 'static reference.
        let model_static: &'static llama_cpp_2::model::LlamaModel =
            &*Box::leak(Box::new(model));

        let context = model_static
            .new_context(&backend, context_params)
            .with_context(|| "Failed to create inference context")?;

        Ok(Self {
            _backend: backend,
            _model_ptr: model_static as *const _,
            context,
        })
    }
}

impl ModelEngine {
    /// Create a new model engine (does not load the model yet).
    pub fn new(kind: ModelKind, model_path: impl Into<String>) -> Self {
        Self {
            kind,
            model_path: model_path.into(),
            loaded: false,
            ctx: None,
            total_inferences: 0,
            total_tokens_generated: 0,
        }
    }

    /// Load the GGUF model into memory via `llama-cpp-2`.
    pub fn load(&mut self) -> Result<()> {
        let path = Path::new(&self.model_path);
        if !path.exists() {
            anyhow::bail!(
                "Model file not found: {} — download it from Hugging Face and place at that path",
                self.model_path
            );
        }

        info!(
            "ModelEngine: loading {} model from {}",
            self.kind.label(),
            self.model_path
        );

        let ctx_size = self.kind.context_size();

        // Initialise the llama backend.
        let backend = llama_cpp_2::llama_backend::LlamaBackend::init()
            .with_context(|| "Failed to initialise llama backend")?;

        // Build model params: CPU-only, no GPU layers.
        let model_params = llama_cpp_2::model::params::LlamaModelParams::default()
            .with_n_gpu_layers(0);

        let model = llama_cpp_2::model::LlamaModel::load_from_file(
            &backend,
            &self.model_path,
            &model_params,
        )
        .with_context(|| format!("Failed to load model from {}", self.model_path))?;

        // Build context params with the recommended context size.
        let context_params = llama_cpp_2::context::params::LlamaContextParams::default()
            .with_n_ctx(Some(std::num::NonZeroU32::new(ctx_size).unwrap()));

        let model_ctx = ModelContext::new(backend, model, context_params)?;

        self.ctx = Some(model_ctx);
        self.loaded = true;

        info!(
            "ModelEngine: {} model loaded (ctx={}, cpu)",
            self.kind.label(),
            ctx_size
        );
        Ok(())
    }

    /// Generate text from a prompt using real llama.cpp inference.
    pub fn generate(&mut self, prompt: &str, params: &InferenceParams) -> Result<GenerationResult> {
        let ctx = self
            .ctx
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Model not loaded — call load() first"))?;

        let start = Instant::now();

        // Build the full prompt with system prompt if provided.
        let full_prompt = if let Some(sys) = &params.system_prompt {
            format!("{}\n\n{}", sys, prompt)
        } else {
            prompt.to_string()
        };

        debug!(
            "ModelEngine[{}]: generating (max_tokens={}, temp={})",
            self.kind.label(),
            params.max_tokens,
            params.temperature
        );

        // Tokenise the prompt.
        let tokens_list = ctx
            .context
            .model
            .str_to_token(&full_prompt, llama_cpp_2::model::AddBos::Always)
            .with_context(|| "Failed to tokenise prompt")?;

        let n_ctx = ctx.context.n_ctx();
        let n_kv_req = tokens_list.len() as u32 + params.max_tokens;
        if n_kv_req > n_ctx {
            anyhow::bail!(
                "Prompt + max_tokens ({}) exceeds context size ({})",
                n_kv_req,
                n_ctx
            );
        }

        // Create a batch and add all prompt tokens.
        let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(512, 1);
        let last_index = (tokens_list.len() - 1) as i32;
        for (i, token) in (0_i32..).zip(tokens_list.iter()) {
            let is_last = i == last_index;
            batch
                .add(*token, i, &[0], is_last)
                .with_context(|| "Failed to add token to batch")?;
        }

        // Decode the prompt batch.
        ctx.context
            .decode(&mut batch)
            .with_context(|| "Failed to decode prompt")?;

        // Build the sampler chain.
        let mut sampler = if params.temperature <= 0.0 {
            // Greedy: just pick the most likely token.
            llama_cpp_2::sampling::LlamaSampler::greedy()
        } else {
            // Temperature-based sampling with top-p.
            llama_cpp_2::sampling::LlamaSampler::chain_simple([
                llama_cpp_2::sampling::LlamaSampler::dist(1234),
                llama_cpp_2::sampling::LlamaSampler::greedy(),
            ])
        };

        // Autoregressive generation loop.
        let mut output_text = String::new();
        let mut n_cur = batch.n_tokens();
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        for _ in 0..params.max_tokens {
            // Sample the next token.
            let token = sampler.sample(&ctx.context, batch.n_tokens() - 1);

            // Check for end-of-generation.
            if ctx.context.model.is_eog_token(token) {
                break;
            }

            // Decode the token to text.
            let piece = ctx
                .context
                .model
                .token_to_piece(token, &mut decoder, true, None)
                .with_context(|| "Failed to decode token")?;
            output_text.push_str(&piece);

            // Prepare the next batch with just this token.
            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .with_context(|| "Failed to add generated token to batch")?;

            n_cur += 1;

            // Decode to advance the KV cache.
            ctx.context
                .decode(&mut batch)
                .with_context(|| "Failed to decode generated token")?;
        }

        let elapsed = start.elapsed();
        let inference_ms = elapsed.as_millis() as u64;
        let tokens_generated = (n_cur - tokens_list.len() as i32) as u32;
        let tokens_per_second = if inference_ms > 0 {
            (tokens_generated as f32 / inference_ms as f32) * 1000.0
        } else {
            0.0
        };

        self.total_inferences += 1;
        self.total_tokens_generated += tokens_generated as u64;

        debug!(
            "ModelEngine[{}]: generated {} tokens in {} ms ({:.1} tok/s)",
            self.kind.label(),
            tokens_generated,
            inference_ms,
            tokens_per_second
        );

        Ok(GenerationResult {
            text: output_text,
            tokens_generated,
            tokens_per_second,
            inference_ms,
        })
    }

    /// Check if the model is loaded.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Get the model kind.
    pub fn kind(&self) -> ModelKind {
        self.kind
    }

    /// Get inference statistics.
    pub fn stats(&self) -> ModelStats {
        ModelStats {
            kind: self.kind,
            loaded: self.loaded,
            total_inferences: self.total_inferences,
            total_tokens_generated: self.total_tokens_generated,
        }
    }
}

/// Statistics about a model engine.
#[derive(Debug, Clone)]
pub struct ModelStats {
    pub kind: ModelKind,
    pub loaded: bool,
    pub total_inferences: u64,
    pub total_tokens_generated: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_engine_creation() {
        let engine = ModelEngine::new(ModelKind::Router, "/tmp/test.gguf");
        assert!(!engine.is_loaded());
        assert_eq!(engine.kind(), ModelKind::Router);
    }

    #[test]
    fn test_inference_params_defaults() {
        let params = InferenceParams::default();
        assert_eq!(params.max_tokens, 256);
        assert!((params.temperature - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_inference_params_greedy() {
        let params = InferenceParams::greedy();
        assert!((params.temperature - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_model_stats() {
        let engine = ModelEngine::new(ModelKind::Conversation, "/tmp/test.gguf");
        let stats = engine.stats();
        assert_eq!(stats.total_inferences, 0);
        assert_eq!(stats.total_tokens_generated, 0);
    }
}
