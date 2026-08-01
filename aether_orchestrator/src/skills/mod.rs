//! AetherOS Skill System
//!
//! A skill is a self-contained capability that Artume can discover, load,
//! and dispatch to. Skills declare what intents they handle and provide
//! an execute function. This replaces hardcoded intent handlers with a
//! pluggable system.
//!
//! Skills are loaded from `~/.config/artume/skills/` (XDG config dir) and
//! from built-in defaults compiled into the binary.

use crate::file_search::FileSearchClient;
use crate::ollama::OllamaClient;
use aether_browser::{BrowserEngine, ConversationalFormatter, ReadabilityExtractor};
use aether_buffer::{ContextResolver, TranscriptRingBuffer};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Skill trait
// ---------------------------------------------------------------------------

/// A single capability that Artume can execute.
pub trait Skill: Send + Sync {
    /// Unique identifier (e.g. "web_fetch", "file_search").
    fn name(&self) -> &'static str;

    /// Human-readable description for discovery.
    fn description(&self) -> &'static str;

    /// The intents this skill handles.
    fn intents(&self) -> &[crate::router::Intent];

    /// Execute the skill with the given user text and context.
    fn execute(
        &self,
        user_text: &str,
        ctx: Arc<SkillContext>,
    ) -> Pin<Box<dyn std::future::Future<Output = String> + Send>>;
}

// ---------------------------------------------------------------------------
// Skill context (shared state passed to execute)
// ---------------------------------------------------------------------------

/// Context passed to every skill execution.
pub struct SkillContext {
    pub ollama: OllamaClient,
    pub browser: BrowserEngine,
    pub transcript_buffer: TranscriptRingBuffer,
    pub file_search: Mutex<FileSearchClient>,
}

// ---------------------------------------------------------------------------
// Skill manifest (for file-based skills)
// ---------------------------------------------------------------------------

/// A skill loaded from a TOML manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub skill: SkillMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub version: String,
    /// Intents this skill handles (matches Intent::from_label).
    pub intents: Vec<String>,
    /// Optional: path to an executable script (relative to skill dir).
    pub script: Option<String>,
    /// Optional: inline prompt template for LLM-based skills.
    pub prompt_template: Option<String>,
}

// ---------------------------------------------------------------------------
// Skill registry
// ---------------------------------------------------------------------------

/// Registry of all available skills.
pub struct SkillRegistry {
    /// Built-in skills (compiled into the binary).
    builtin: Vec<Box<dyn Skill>>,
    /// File-based skills loaded from disk.
    file_skills: Vec<FileSkill>,
    /// Index: intent → list of skill names that handle it.
    intent_index: HashMap<crate::router::Intent, Vec<String>>,
}

/// A skill loaded from a manifest file.
struct FileSkill {
    manifest: SkillManifest,
    /// Resolved directory path for script execution.
    dir: PathBuf,
}

impl SkillRegistry {
    /// Create a new registry with built-in skills.
    pub fn new() -> Self {
        let mut registry = Self {
            builtin: Vec::new(),
            file_skills: Vec::new(),
            intent_index: HashMap::new(),
        };
        registry.register_builtin(WebFetchSkill);
        registry.register_builtin(FileSearchSkill);
        registry.register_builtin(EntityLookupSkill);
        registry.register_builtin(SystemCommandSkill);
        registry
    }

    /// Register a built-in skill.
    pub fn register_builtin(&mut self, skill: impl Skill + 'static) {
        let name = skill.name().to_string();
        for intent in skill.intents() {
            self.intent_index
                .entry(*intent)
                .or_default()
                .push(name.clone());
        }
        self.builtin.push(Box::new(skill));
    }

    /// Load file-based skills from the skills directory.
    pub fn load_file_skills(&mut self) {
        let skills_dir = self.skills_dir();
        if !skills_dir.exists() {
            info!(
                "SkillRegistry: no skills directory at {}",
                skills_dir.display()
            );
            return;
        }

        let entries = match std::fs::read_dir(&skills_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("SkillRegistry: failed to read skills dir: {e}");
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("skill.toml");
                if manifest_path.exists() {
                    match self.load_skill(&path, &manifest_path) {
                        Ok(skill) => {
                            let name = skill.manifest.skill.name.clone();
                            for intent_label in &skill.manifest.skill.intents {
                                let intent = crate::router::Intent::from_label(intent_label);
                                self.intent_index
                                    .entry(intent)
                                    .or_default()
                                    .push(name.clone());
                            }
                            info!(
                                "SkillRegistry: loaded skill '{}' from {}",
                                name,
                                path.display()
                            );
                            self.file_skills.push(skill);
                        }
                        Err(e) => {
                            warn!(
                                "SkillRegistry: failed to load skill from {}: {e}",
                                path.display()
                            );
                        }
                    }
                }
            }
        }
    }

    /// Load a single skill from a directory with a manifest.
    fn load_skill(&self, dir: &PathBuf, manifest_path: &PathBuf) -> anyhow::Result<FileSkill> {
        let content = std::fs::read_to_string(manifest_path)?;
        let manifest: SkillManifest = toml::from_str(&content)?;
        Ok(FileSkill {
            manifest,
            dir: dir.clone(),
        })
    }

    /// Get skills directory path (XDG config dir).
    fn skills_dir(&self) -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| {
            let home = dirs::home_dir().expect("HOME must be set");
            home.join(".config")
        });
        base.join("artume").join("skills")
    }

    /// Find skills that handle a given intent.
    pub fn skills_for_intent(&self, intent: &crate::router::Intent) -> Vec<&str> {
        self.intent_index
            .get(intent)
            .map(|names| names.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Execute a built-in skill by name.
    pub async fn execute_builtin(
        &self,
        name: &str,
        user_text: &str,
        ctx: Arc<SkillContext>,
    ) -> Option<String> {
        for skill in &self.builtin {
            if skill.name() == name {
                return Some(skill.execute(user_text, ctx).await);
            }
        }
        None
    }

    /// Execute a file-based skill by name.
    pub async fn execute_file_skill(
        &self,
        name: &str,
        user_text: &str,
    ) -> Option<String> {
        for skill in &self.file_skills {
            if skill.manifest.skill.name == name {
                return self.run_file_skill(skill, user_text).await;
            }
        }
        None
    }

    /// Run a file-based skill (script or LLM prompt).
    async fn run_file_skill(&self, skill: &FileSkill, user_text: &str) -> Option<String> {
        let meta = &skill.manifest.skill;

        // If a script is specified, run it
        if let Some(script_rel) = &meta.script {
            let script_path = skill.dir.join(script_rel);
            if script_path.exists() {
                match std::process::Command::new(&script_path)
                    .arg(user_text)
                    .output()
                {
                    Ok(output) => {
                        if output.status.success() {
                            return Some(
                                String::from_utf8_lossy(&output.stdout).trim().to_string(),
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Skill '{}': script execution failed: {e}", meta.name);
                    }
                }
            }
        }

        // If a prompt template is specified, use LLM
        if let Some(template) = &meta.prompt_template {
            let _prompt = template.replace("{input}", user_text);
            return Some(format!(
                "[Skill '{}' would process: {}]",
                meta.name, user_text
            ));
        }

        None
    }

    /// List all registered skills (built-in + file).
    pub fn list_skills(&self) -> Vec<SkillInfo> {
        let mut skills = Vec::new();
        for skill in &self.builtin {
            skills.push(SkillInfo {
                name: skill.name().to_string(),
                description: skill.description().to_string(),
                source: "builtin".to_string(),
                intents: skill.intents().iter().map(|i| i.label().to_string()).collect(),
            });
        }
        for skill in &self.file_skills {
            skills.push(SkillInfo {
                name: skill.manifest.skill.name.clone(),
                description: skill.manifest.skill.description.clone(),
                source: "file".to_string(),
                intents: skill.manifest.skill.intents.clone(),
            });
        }
        skills
    }
}

/// Public info about a skill.
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub source: String,
    pub intents: Vec<String>,
}

// ---------------------------------------------------------------------------
// Built-in skills
// ---------------------------------------------------------------------------

struct WebFetchSkill;
impl Skill for WebFetchSkill {
    fn name(&self) -> &'static str {
        "web_fetch"
    }
    fn description(&self) -> &'static str {
        "Fetch and summarize web pages from URLs"
    }
    fn intents(&self) -> &[crate::router::Intent] {
        &[crate::router::Intent::WebFetch]
    }
    fn execute(
        &self,
        user_text: &str,
        ctx: Arc<SkillContext>,
    ) -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
        let text = user_text.to_string();
        Box::pin(async move {
            let url = extract_url(&text);
            match url {
                Some(u) => {
                    info!("WebFetch: fetching {u}");
                    match ctx.browser.fetch(&u).await {
                        Ok(result) => {
                            let content = ReadabilityExtractor::extract(&result.html);
                            let formatted = ConversationalFormatter::format(&content);
                            if formatted.len() > 200 {
                                let summary_prompt = format!(
                                    "Summarize this web page content in 2-3 concise sentences:\n\nTitle: {}\n\n{}",
                                    content.title, formatted
                                );
                                match ctx
                                    .ollama
                                    .chat(
                                        &crate::ollama::OllamaModel::REASONING,
                                        &summary_prompt,
                                        None,
                                        0.3,
                                        256,
                                    )
                                    .await
                                {
                                    Ok(summary) => {
                                        format!("Here's what I found on \"{}\": {}", content.title, summary)
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
                        Err(e) => format!("I couldn't fetch that page: {e}"),
                    }
                }
                None => {
                    "I can read web pages for you. Just give me a URL or say 'read me the article'."
                        .to_string()
                }
            }
        })
    }
}

struct FileSearchSkill;
impl Skill for FileSearchSkill {
    fn name(&self) -> &'static str {
        "file_search"
    }
    fn description(&self) -> &'static str {
        "Search indexed files on the local filesystem"
    }
    fn intents(&self) -> &[crate::router::Intent] {
        &[crate::router::Intent::FileSearch]
    }
    fn execute(
        &self,
        user_text: &str,
        ctx: Arc<SkillContext>,
    ) -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
        let text = user_text.to_string();
        Box::pin(async move {
            if ctx.file_search.lock().await.health().await {
                match ctx.file_search.lock().await.search(&text, 5).await {
                    Ok(results) => {
                        if results.is_empty() {
                            format!("I couldn't find any files matching \"{}\".", text)
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
                                let others: Vec<&str> =
                                    results[1..].iter().map(|r| r.filename.as_str()).collect();
                                response.push_str(&format!("Also found: {}.", others.join(", ")));
                            }
                            response
                        }
                    }
                    Err(e) => {
                        warn!("FileSearch: query failed: {e}");
                        "I had trouble searching your files. The file index daemon may not be running."
                            .to_string()
                    }
                }
            } else {
                "File search is not available. Start the aetherfs-core daemon first.".to_string()
            }
        })
    }
}

struct EntityLookupSkill;
impl Skill for EntityLookupSkill {
    fn name(&self) -> &'static str {
        "entity_lookup"
    }
    fn description(&self) -> &'static str {
        "Look up entities (tracking numbers, codes) from recent conversation context"
    }
    fn intents(&self) -> &[crate::router::Intent] {
        &[crate::router::Intent::EntityLookup]
    }
    fn execute(
        &self,
        user_text: &str,
        ctx: Arc<SkillContext>,
    ) -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
        let text = user_text.to_string();
        Box::pin(async move {
            let resolver = ContextResolver::new(&ctx.transcript_buffer);
            match resolver.resolve_reference(&text) {
                Some(entity) => {
                    info!(
                        "EntityLookup: resolved '{}' → {}: {}",
                        text, entity.entity_type, entity.value
                    );
                    format!("I found {}: {}.", entity.entity_type, entity.value)
                }
                None => {
                    let all = ctx.transcript_buffer.all_entities();
                    if all.is_empty() {
                        "I don't see any recent information I can look up. Try saying something like 'Copy that tracking number' after I've mentioned one.".to_string()
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
        })
    }
}

struct SystemCommandSkill;
impl Skill for SystemCommandSkill {
    fn name(&self) -> &'static str {
        "system_command"
    }
    fn description(&self) -> &'static str {
        "Execute system commands (volume, settings, help)"
    }
    fn intents(&self) -> &[crate::router::Intent] {
        &[crate::router::Intent::SystemCommand]
    }
    fn execute(
        &self,
        user_text: &str,
        _ctx: Arc<SkillContext>,
    ) -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
        let text = user_text.to_string();
        Box::pin(async move {
            format!(
                "System command received: \"{}\". Volume and settings control coming soon.",
                text.chars().take(60).collect::<String>()
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a URL from user text.
fn extract_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") {
            return Some(
                word.trim_end_matches(&['.', ',', ';', '!', '?'][..])
                    .to_string(),
            );
        }
        if word.starts_with("www.") {
            return Some(format!(
                "https://{}",
                word.trim_end_matches(&['.', ',', ';', '!', '?'][..])
            ));
        }
    }
    None
}
