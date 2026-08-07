//! Artume Conversation Loop
//!
//! The main conversational loop that ties all subsystems together:
//!
//! 1. Listen (STT) → 2. Classify (Router) → 3. Dispatch → 4. Respond (TTS)
//!
//! This is the top-level orchestrator that makes Artume feel like
//! a conversation rather than a menu system.

use crate::file_search::FileSearchClient;
use crate::ollama::{OllamaClient, OllamaModel, ToolCallResult, ToolDef, ToolFunction};
use crate::profile::UserProfile;
use crate::router::{Intent, IntentRouter, RouterConfig};
#[cfg(feature = "stt")]
use crate::stt::{SttConfig, SttEngine};
#[cfg(feature = "tts")]
use crate::tts::{TtsConfig, TtsEngine};
use aether_attention::{
    CognitiveLoadEvaluator, DeliveryDecision, PendingNotificationQueue, SystemEvent,
    EventCategory, EventSeverity,
};
use aether_audio::{ContextStack, SpatialMixer, SpatialPosition, VirtualSource};
use aether_audio::wake_word::{WakeWordDetector, WakeWordConfig, WakeWordModel, WakeWordEvent};
use aether_browser::{BrowserEngine, ReadabilityExtractor, ConversationalFormatter};
use aether_buffer::{ContextResolver, TranscriptRingBuffer};
use log::{debug, info, warn};
use std::path::PathBuf;
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
    /// Path to the aetherfs-core daemon socket.
    pub file_search_socket: String,
    /// Path to the soul.md file.
    pub soul_path: String,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        // Resolve soul.md path relative to the crate source.
        let soul_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("soul.md")
            .to_string_lossy()
            .to_string();

        Self {
            router: RouterConfig::default(),
            #[cfg(feature = "stt")]
            stt: SttConfig::default(),
            #[cfg(feature = "tts")]
            tts: TtsConfig::default(),
            system_prompt: String::new(), // built at runtime from soul + profile
            voice_enabled: false,
            max_history_turns: 5,
            file_search_socket: "/tmp/aetherfs.sock".to_string(),
            soul_path,
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
    /// Browser engine for web fetching (None if it failed to initialise).
    browser: Option<BrowserEngine>,
    /// Transcript ring buffer for entity lookup.
    transcript_buffer: TranscriptRingBuffer,
    /// File search client for aetherfs-core daemon.
    file_search: FileSearchClient,
    /// Spatial audio mixer for binaural TTS output.
    spatial_mixer: SpatialMixer,
    /// Audio context stack for interruption handling.
    #[allow(dead_code)]
    context_stack: ContextStack,
    /// Cognitive load evaluator for notification management.
    attention_evaluator: CognitiveLoadEvaluator,
    /// Pending notification queue for suppressed events.
    notification_queue: PendingNotificationQueue,
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
    /// The user's profile (loaded from disk, or None on first run).
    profile: Option<UserProfile>,
    /// The soul identity text (loaded from soul.md).
    #[allow(dead_code)]
    soul_text: String,
    /// Whether this is the first run (no profile exists yet).
    is_first_run: bool,
    /// Wake word detector (background thread).
    wake_word: Option<WakeWordDetector>,
}

impl ConversationLoop {
    /// Create a new conversation loop.
    pub fn new(config: ConversationConfig) -> Self {
        let router = IntentRouter::new(config.router.clone());
        let ollama = OllamaClient::new(None);
        let browser = match BrowserEngine::new() {
            Ok(b) => Some(b),
            Err(e) => {
                warn!("Failed to create browser engine: {e} — web fetch will be unavailable");
                None
            }
        };
        let file_search = FileSearchClient::new(Some(config.file_search_socket.clone()));

        // Load the soul identity from soul.md.
        let soul_text = std::fs::read_to_string(&config.soul_path)
            .unwrap_or_else(|e| {
                warn!("Failed to load soul.md from '{}': {e}", config.soul_path);
                String::new()
            });

        // Load or initialise the user profile.
        let (profile, is_first_run) = if UserProfile::is_first_run() {
            info!("First run detected — no user profile found");
            (None, true)
        } else {
            let p = UserProfile::load().unwrap_or_else(|| {
                warn!("Failed to load user profile, using defaults");
                UserProfile::default()
            });
            (Some(p), false)
        };

        // Build the system prompt from soul + profile and store it.
        let system_prompt = Self::build_system_prompt(&soul_text, profile.as_ref());
        let mut config = config;
        config.system_prompt = system_prompt;

        // Clone config for TTS/STT before moving it into Self.
        let tts_config = config.tts.clone();
        #[cfg(feature = "stt")]
        let stt_config = config.stt.clone();

        // Wake word detection starts as None — call start_wake_word() to enable.
        let wake_word = None;

        // Initialise spatial audio with default sources.
        let mut spatial_mixer = SpatialMixer::new();
        spatial_mixer.add_source(VirtualSource::new(
            "Primary Voice",
            SpatialPosition::CENTRE,
        ));
        spatial_mixer.add_source(VirtualSource::new(
            "System Alert",
            SpatialPosition::SOFT_RIGHT_45,
        ));
        spatial_mixer.add_source(VirtualSource::new(
            "Background Context",
            SpatialPosition::SOFT_LEFT_45,
        ));

        Self {
            config,
            router,
            ollama,
            browser,
            transcript_buffer: TranscriptRingBuffer::new(),
            file_search,
            spatial_mixer,
            context_stack: ContextStack::new(8),
            attention_evaluator: CognitiveLoadEvaluator::new(),
            notification_queue: PendingNotificationQueue::new(50),
            #[cfg(feature = "stt")]
            stt: SttEngine::new(stt_config),
            #[cfg(feature = "tts")]
            tts: TtsEngine::new(tts_config),
            history: Vec::new(),
            total_turns: 0,
            profile,
            soul_text,
            is_first_run,
            wake_word,
        }
    }

    /// Start the wake word detector on a background thread.
    pub fn start_wake_word(&mut self) {
        if self.wake_word.is_none() {
            self.wake_word = Self::start_wake_word_inner();
        }
    }

    /// Internal: start the wake word detector.
    fn start_wake_word_inner() -> Option<WakeWordDetector> {
        // Uses the built-in "Hey Jarvis" OpenWakeWord model as a placeholder
        // until a custom "Hey Artume" model is trained.
        // To train: collect ~50 samples of "Hey Artume", use OpenWakeWord
        // training tools, then use:
        //   WakeWordModel::Custom { path: "models/hey_artume.onnx", trigger_word: "Hey Artume" }
        let config = WakeWordConfig {
            model_source: WakeWordModel::BuiltInHeyJarvis,
            threshold: 0.3,
            device_name: None,
        };

        match WakeWordDetector::start(config) {
            Ok(detector) => {
                info!("WakeWordDetector: listening for wake word (placeholder: 'Hey Jarvis' model)");
                Some(detector)
            }
            Err(e) => {
                warn!("WakeWordDetector: failed to start: {e}");
                None
            }
        }
    }

    /// Check if the wake word has been detected.
    pub fn check_wake_word(&mut self) -> Option<String> {
        if let Some(ref detector) = self.wake_word {
            match detector.try_recv() {
                Some(WakeWordEvent::Detected { word, .. }) => {
                    info!("WakeWordDetector: wake word detected: '{}'", word);
                    Some(word)
                }
                Some(WakeWordEvent::Error(e)) => {
                    warn!("WakeWordDetector: error: {e}");
                    None
                }
                None => None,
            }
        } else {
            None
        }
    }

    /// Check if wake word detection is active.
    pub fn wake_word_active(&self) -> bool {
        self.wake_word.is_some()
    }

    /// Build the system prompt from the soul identity and user profile.
    fn build_system_prompt(soul_text: &str, profile: Option<&UserProfile>) -> String {
        let mut prompt = String::new();

        // Soul identity (always present).
        if !soul_text.is_empty() {
            prompt.push_str(soul_text);
        } else {
            prompt.push_str("You are Artume, a voice-native operating system assistant. \
                            Keep responses concise and conversational since they will be spoken aloud. \
                            Use natural language. Be helpful, clear, and direct.");
        }

        // User profile (if available).
        if let Some(profile) = profile {
            prompt.push_str("\n\n---\n\n");
            prompt.push_str(&profile.to_system_prompt());
        }

        prompt
    }

    /// Check if this is the first run (no user profile exists).
    pub fn is_first_run(&self) -> bool {
        self.is_first_run
    }

    /// Get a reference to the user profile, if loaded.
    pub fn profile(&self) -> Option<&UserProfile> {
        self.profile.as_ref()
    }

    /// Get a mutable reference to the user profile, if loaded.
    pub fn profile_mut(&mut self) -> Option<&mut UserProfile> {
        self.profile.as_mut()
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

    /// Check if the file search daemon is reachable.
    pub async fn check_file_search_health(&mut self) -> bool {
        self.file_search.health().await
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
    /// 4. Optionally synthesize speech through spatial audio
    /// 5. Evaluate cognitive load for notification management
    pub async fn process_turn(&mut self, user_text: &str) -> anyhow::Result<Turn> {
        self.process_turn_with_callback(user_text, None).await
    }

    /// Process a turn, optionally streaming text tokens to `on_token` as they
    /// arrive (for TTS). Returns the full turn.
    pub async fn process_turn_with_callback(
        &mut self,
        user_text: &str,
        mut on_token: Option<&mut dyn FnMut(&str)>,
    ) -> anyhow::Result<Turn> {
        let start = Instant::now();
        let timestamp = chrono::Utc::now();

        // Push user text into transcript buffer for entity lookup.
        self.transcript_buffer.push(user_text, aether_buffer::TranscriptSource::User);

        // Step 1: Classify intent.
        let intent = self.router.classify(user_text).await;

        // Step 2: Dispatch based on intent.
        let response = match intent {
            Intent::Conversation => self.handle_conversation(user_text, on_token).await,
            Intent::EntityLookup => self.handle_entity_lookup(user_text),
            Intent::WebFetch => self.handle_web_fetch(user_text).await,
            Intent::FileSearch => self.handle_file_search(user_text).await,
            Intent::ExecuteAction => self.handle_execute_action(user_text),
            Intent::SystemCommand => self.handle_system_command(user_text),
            Intent::SwitchMode => self.handle_switch_mode(user_text),
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

        // Step 3: Evaluate cognitive load for notification management.
        let event = SystemEvent::new(
            EventCategory::MessageNotification,
            EventSeverity::Normal,
            format!("User said: {}", user_text.chars().take(40).collect::<String>()),
        );
        let decision = self.attention_evaluator.evaluate(&event);
        match decision {
            DeliveryDecision::Queue => {
                self.notification_queue.push(event);
            }
            DeliveryDecision::Drop => {
                debug!("Attention: dropped trivial event during high focus");
            }
            _ => {}
        }

        // Tick the notification queue for idle transitions.
        if let Some(summary) = self.notification_queue.tick(&self.attention_evaluator) {
            info!("Attention: idle transition — batch summary: {summary}");
        }

        self.history.push(turn.clone());
        self.total_turns += 1;

        // Index this conversation turn for future RAG (non-blocking, best-effort)
        if self.file_search.health().await {
            let session_id = format!("session_{}", self.total_turns);
            let _ = self
                .file_search
                .index_conversation_turn(
                    &session_id,
                    &turn.user_text,
                    &turn.response,
                    turn.intent.label(),
                )
                .await;
        }

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

    /// Build the message list for the reasoning model: system prompt, RAG
    /// context, sliding history window, and the current user input.
    ///
    /// Returns owned `(role, content)` pairs so the caller can borrow them
    /// freely without lifetime conflicts.
    async fn build_messages(&mut self, user_text: &str) -> Vec<(String, String)> {
        let mut messages: Vec<(String, String)> = Vec::new();

        // System prompt.
        messages.push(("system".to_string(), self.config.system_prompt.clone()));

        // RAG: query aetherfs for relevant file context based on user input.
        if self.file_search.health().await {
            match self.file_search.search(user_text, 3).await {
                Ok(results) if !results.is_empty() => {
                    let mut rag_context = String::from("\n\nRelevant files from your system:\n");
                    for (i, r) in results.iter().enumerate() {
                        rag_context.push_str(&format!(
                            "{}. {} — {}. {}. {}\n",
                            i + 1,
                            r.filename,
                            r.spoken_summary,
                            r.temporal_context,
                            r.location_context,
                        ));
                    }
                    rag_context.push_str("\nUse this context if relevant to the user's question.\n");
                    messages.push(("system".to_string(), rag_context));
                }
                _ => {}
            }
        }

        // Recent history (sliding window).
        let start_idx = self
            .history
            .len()
            .saturating_sub(self.config.max_history_turns);
        for turn in &self.history[start_idx..] {
            messages.push(("user".to_string(), turn.user_text.clone()));
            messages.push(("assistant".to_string(), turn.response.clone()));
        }

        // Current user input.
        messages.push(("user".to_string(), user_text.to_string()));

        messages
    }

    /// Borrow the owned messages as `&[(&str, &str)]` for the Ollama client.
    fn as_ref_messages(messages: &[(String, String)]) -> Vec<(&str, &str)> {
        messages
            .iter()
            .map(|(role, content)| (role.as_str(), content.as_str()))
            .collect()
    }

    /// Single-pass conversational handler.
    ///
    /// One call to the 8B with tool definitions. The model either:
    ///   (a) emits a tool call (act) — we execute it and return a short
    ///       confirmation, or
    ///   (b) emits a plain conversational response (converse).
    ///
    /// If the tool call is malformed or the model errors, we fall back to the
    /// deterministic label router, then to a template. This never returns
    /// empty speech.
    ///
    /// When `on_token` is provided, text tokens are streamed to it as they
    /// arrive (for TTS), so speech can start before the response completes.
    async fn handle_conversation(
        &mut self,
        user_text: &str,
        mut on_token: Option<&mut dyn FnMut(&str)>,
    ) -> String {
        let owned = self.build_messages(user_text).await;
        let messages = Self::as_ref_messages(&owned);

        // Single-pass tool-calling on the 8B (streaming when a callback is given).
        let result = if let Some(cb) = on_token.as_deref_mut() {
            self.ollama
                .chat_with_tools_stream(
                    &OllamaModel::REASONING,
                    &messages,
                    &self.tool_definitions(),
                    0.7,
                    512,
                    cb,
                )
                .await
        } else {
            self.ollama
                .chat_with_tools(
                    &OllamaModel::REASONING,
                    &messages,
                    &self.tool_definitions(),
                    0.7,
                    512,
                )
                .await
        };

        match result {
            Ok(ToolCallResult::Response(text)) if !text.trim().is_empty() => text,
            Ok(ToolCallResult::ToolCall { name, arguments }) => {
                // Guard against malformed tool calls (e.g. the model emitting
                // `{"name": "<nil>"}` or an unknown tool). Fall back to the
                // label router rather than returning a generic message.
                if self.is_known_tool(&name) {
                    info!("ToolCall: {name} {arguments}");
                    self.execute_tool_call(&name, &arguments).await
                } else {
                    warn!("ToolCall: malformed/unknown tool '{name}' — falling back to label router");
                    self.fallback_router(user_text).await
                }
            }
            Ok(ToolCallResult::Response(_)) => {
                // Empty response — fall back to the label router.
                self.fallback_router(user_text).await
            }
            Err(e) => {
                warn!("Ollama[1080] single-pass failed: {e} — falling back to label router");
                self.fallback_router(user_text).await
            }
        }
    }

    /// Whether a tool name is one we actually implement.
    fn is_known_tool(&self, name: &str) -> bool {
        matches!(
            name,
            "set_volume" | "switch_mode" | "search_files" | "get_time"
        )
    }

    /// Fallback path: classify via the label router, then dispatch. Used when
    /// single-pass tool-calling fails or returns empty.
    async fn fallback_router(&mut self, user_text: &str) -> String {
        let intent = self.router.classify(user_text).await;
        match intent {
            Intent::Conversation => {
                // Try a plain (non-tool) chat call; template on failure.
                let owned = self.build_messages(user_text).await;
                let messages = Self::as_ref_messages(&owned);
                match self
                    .ollama
                    .chat_with_messages(&OllamaModel::REASONING, &messages, 0.7, 512)
                    .await
                {
                    Ok(response) if !response.trim().is_empty() => response,
                    _ => self.template_conversation(user_text),
                }
            }
            Intent::EntityLookup => self.handle_entity_lookup(user_text),
            Intent::WebFetch => self.handle_web_fetch(user_text).await,
            Intent::FileSearch => self.handle_file_search(user_text).await,
            Intent::ExecuteAction => self.handle_execute_action(user_text),
            Intent::SystemCommand => self.handle_system_command(user_text),
            Intent::SwitchMode => self.handle_switch_mode(user_text),
            Intent::Unknown => self.handle_unknown(user_text),
        }
    }

    /// The tool definitions exposed to the 8B for single-pass act-or-converse.
    fn tool_definitions(&self) -> Vec<ToolDef> {
        use serde_json::json;
        vec![
            ToolDef {
                r#type: "function",
                function: ToolFunction {
                    name: "set_volume",
                    description: "Set the system volume to a level between 0 and 100.",
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "level": {"type": "integer", "minimum": 0, "maximum": 100}
                        },
                        "required": ["level"]
                    }),
                },
            },
            ToolDef {
                r#type: "function",
                function: ToolFunction {
                    name: "switch_mode",
                    description: "Switch the active mode to browser, email, ide, files, docs, settings, ebook, or desktop.",
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "mode": {"type": "string", "enum": ["browser", "email", "ide", "files", "docs", "settings", "ebook", "desktop"]}
                        },
                        "required": ["mode"]
                    }),
                },
            },
            ToolDef {
                r#type: "function",
                function: ToolFunction {
                    name: "search_files",
                    description: "Search the local file index for files matching a query.",
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"}
                        },
                        "required": ["query"]
                    }),
                },
            },
            ToolDef {
                r#type: "function",
                function: ToolFunction {
                    name: "get_time",
                    description: "Get the current local time.",
                    parameters: json!({"type": "object", "properties": {}}),
                },
            },
        ]
    }

    /// Execute a tool call emitted by the model and return a spoken response.
    async fn execute_tool_call(&mut self, name: &str, arguments: &serde_json::Value) -> String {
        match name {
            "set_volume" => {
                let level = arguments.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
                format!("Setting volume to {} percent.", level)
            }
            "switch_mode" => {
                let mode = arguments
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("desktop");
                format!("Switching to {} mode.", mode)
            }
            "search_files" => {
                let query = arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                self.handle_file_search(query).await
            }
            "get_time" => {
                let now = chrono::Local::now();
                format!("The current time is {}.", now.format("%I:%M %p"))
            }
            other => {
                warn!("Unknown tool call: {other}");
                "I'm not sure how to do that yet.".to_string()
            }
        }
    }

    /// Stream a conversational response, invoking `on_token` for each text
    /// chunk as it arrives. Returns the full response.
    ///
    /// This is the perceived-latency win: the caller feeds tokens to TTS as
    /// they stream in instead of waiting for the whole response.
    pub async fn stream_conversation(
        &mut self,
        user_text: &str,
        mut on_token: impl FnMut(&str),
    ) -> String {
        let owned = self.build_messages(user_text).await;
        let messages = Self::as_ref_messages(&owned);
        match self
            .ollama
            .chat_stream(&OllamaModel::REASONING, &messages, 0.7, 512, &mut on_token)
            .await
        {
            Ok(response) if !response.trim().is_empty() => response,
            _ => {
                let fallback = self.template_conversation(user_text);
                on_token(&fallback);
                fallback
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
        // If the user asked to search (no URL), do a web search instead.
        let lower = user_text.to_lowercase();
        let is_search = lower.contains("search")
            || lower.contains("look up")
            || lower.contains("find")
            || lower.contains("news about")
            || lower.contains("what's new")
            || lower.contains("what is new");

        if is_search && self.extract_url(user_text).is_none() {
            let query = self.extract_search_query(user_text);
            info!("WebFetch: searching for '{query}'");
            let Some(browser) = self.browser.as_ref() else {
                warn!("WebFetch: browser engine unavailable");
                return "Web search is unavailable right now.".to_string();
            };
            match browser.search(&query, 5).await {
                Ok(results) if !results.is_empty() => {
                    let mut out = format!("Here are the top results for \"{query}\": ");
                    for (i, r) in results.iter().enumerate() {
                        out.push_str(&format!("{}. {}. ", i + 1, r.title));
                    }
                    out.push_str("Say 'open result 1' or give me a URL to read one.");
                    out
                }
                Ok(_) => format!("I couldn't find any results for \"{query}\"."),
                Err(e) => {
                    warn!("WebFetch: search failed: {e}");
                    format!("I couldn't search for \"{query}\": {e}")
                }
            }
        } else {
            self.fetch_and_summarize(user_text).await
        }
    }

    /// Fetch a URL and summarize its content.
    async fn fetch_and_summarize(&mut self, user_text: &str) -> String {
        let url = self.extract_url(user_text);

        match url {
            Some(u) => {
                info!("WebFetch: fetching {u}");
                let Some(browser) = self.browser.as_ref() else {
                    warn!("WebFetch: browser engine unavailable");
                    return "Web browsing is unavailable right now.".to_string();
                };
                match browser.fetch(&u).await {
                    Ok(result) => {
                        let content = ReadabilityExtractor::extract(&result.html);
                        let formatted = ConversationalFormatter::format(&content);

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

    async fn handle_file_search(&mut self, user_text: &str) -> String {
        // Connect to the daemon if not already connected.
        if self.file_search.health().await {
            match self.file_search.search(user_text, 5).await {
                Ok(results) => {
                    if results.is_empty() {
                        format!("I couldn't find any files matching \"{}\".", user_text)
                    } else {
                        let top = &results[0];
                        let mut response = format!(
                            "I found {} result{}. The top match is \"{}\" — {}. ",
                            results.len(),
                            if results.len() == 1 { "" } else { "s" },
                            top.filename,
                            top.spoken_summary,
                        );
                        if results.len() > 1 {
                            let others: Vec<&str> = results[1..]
                                .iter()
                                .map(|r| r.filename.as_str())
                                .collect();
                            response.push_str(&format!("Also found: {}.", others.join(", ")));
                        }
                        response
                    }
                }
                Err(e) => {
                    warn!("FileSearch: query failed: {e}");
                    "I had trouble searching your files. The file index daemon may not be running.".to_string()
                }
            }
        } else {
            "File search is not available. Start the aetherfs-core daemon first with: cargo run --bin aetherfs-core".to_string()
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
            "System command: \"{}\". Use 'volume up', 'volume down', 'set volume to 50', 'status', 'timer 10 minutes', 'bluetooth', 'wifi', or 'switch audio to headphones'.",
            user_text.chars().take(60).collect::<String>()
        )
    }

    fn handle_switch_mode(&mut self, user_text: &str) -> String {
        let lower = user_text.to_lowercase();
        let modes = [
            ("browser", "browser"),
            ("email", "email"),
            ("ide", "IDE"),
            ("files", "files"),
            ("docs", "documents"),
            ("settings", "settings"),
            ("ebook", "ebook reader"),
            ("desktop", "desktop"),
        ];
        for (keyword, label) in modes {
            if lower.contains(keyword) {
                return format!("Switching to {} mode.", label);
            }
        }
        "I can switch to browser, email, IDE, files, documents, settings, or ebook mode.".to_string()
    }

    fn handle_unknown(&mut self, _user_text: &str) -> String {
        "I'm not sure what you need. You can ask me to read a web page, \
         look up information from our conversation, search your files, or just chat."
            .to_string()
    }

    // --- Helpers ---

    fn template_conversation(&self, user_text: &str) -> String {
        let lower = user_text.to_lowercase();
        if lower.contains("hello") || lower.contains("hi ") || lower.contains("hey") {
            "Hello! I'm Artume. How can I help you today?".to_string()
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
            "I'm Artume, your voice-controlled operating system assistant.".to_string()
        } else {
            format!(
                "I heard you say: \"{}\". I'm still learning, but I'm getting better every day!",
                user_text.chars().take(80).collect::<String>()
            )
        }
    }

    fn extract_url(&self, text: &str) -> Option<String> {
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

    /// Extract a search query from natural language, stripping leading
    /// command words like "search for", "look up", "find", "news about".
    fn extract_search_query(&self, text: &str) -> String {
        let lower = text.to_lowercase();
        let prefixes = [
            "search for",
            "search the web for",
            "search",
            "look up",
            "look for",
            "find me",
            "find",
            "news about",
            "what's new in",
            "what is new in",
            "what's new",
            "what is new",
        ];
        for prefix in prefixes {
            if let Some(pos) = lower.find(prefix) {
                let after = text[pos + prefix.len()..].trim();
                if !after.is_empty() {
                    return after.trim_end_matches(&['.', '!', '?'][..]).to_string();
                }
            }
        }
        text.trim().to_string()
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

    /// Synthesize speech from text using the TTS engine.
    /// Returns 22050 Hz f32 PCM samples.
    #[cfg(feature = "tts")]
    pub fn synthesize_speech(&mut self, text: &str) -> anyhow::Result<Vec<f32>> {
        if self.tts.is_loaded() {
            self.tts.synthesize(text).map_err(|e| anyhow::anyhow!("TTS failed: {}", e))
        } else {
            Err(anyhow::anyhow!("TTS engine not loaded"))
        }
    }

    /// Transcribe audio samples using the STT engine.
    /// Takes 16kHz f32 PCM samples, returns transcribed text.
    #[cfg(feature = "stt")]
    pub fn transcribe_audio(&mut self, samples: &[f32]) -> anyhow::Result<String> {
        if self.stt.is_loaded() {
            self.stt.transcribe(samples).map_err(|e| anyhow::anyhow!("STT failed: {}", e))
        } else {
            Err(anyhow::anyhow!("STT engine not loaded"))
        }
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
    fn test_extract_search_query() {
        let loop_ = ConversationLoop::new(ConversationConfig::default());
        assert_eq!(
            loop_.extract_search_query("search for new AI related news"),
            "new AI related news"
        );
        assert_eq!(
            loop_.extract_search_query("look up the weather in London"),
            "the weather in London"
        );
        assert_eq!(
            loop_.extract_search_query("find me the best coffee shops"),
            "the best coffee shops"
        );
        assert_eq!(
            loop_.extract_search_query("news about the stock market"),
            "the stock market"
        );
        // No command word — returns the whole text.
        assert_eq!(loop_.extract_search_query("hello there"), "hello there");
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
