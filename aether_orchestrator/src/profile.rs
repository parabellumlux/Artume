//! Artume User Profile
//!
//! A persistent user profile stored at `~/.config/artume/profile/`.
//! The profile is co-authored by the system and the user over time:
//!
//! - **identity.md** — name, pronouns, relationship to the system (mostly static)
//! - **preferences.md** — voice speed, verbosity, interruptibility (semi-static)
//! - **routines.md** — morning/night patterns, recurring needs (semi-static)
//! - **context.md** — current state, updated in real-time (dynamic)
//! - **history.md** — learned patterns over time (system-written)
//!
//! On first run, the system detects no profile exists and triggers a
//! guided onboarding conversation to populate identity and preferences.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Profile directory layout
// ---------------------------------------------------------------------------

/// Root directory for the user profile.
pub fn profile_dir() -> PathBuf {
    // Use XDG config dir (Linux: ~/.config, macOS: ~/Library/Application Support)
    // Falls back to ~/.config if XDG_CONFIG_HOME is unset
    let base = dirs::config_dir().unwrap_or_else(|| {
        let home = dirs::home_dir().expect("HOME must be set to use Artume");
        home.join(".config")
    });
    base.join("artume").join("profile")
}

/// Path to a profile file.
fn profile_file(name: &str) -> PathBuf {
    profile_dir().join(name)
}

// ---------------------------------------------------------------------------
// Profile data structures
// ---------------------------------------------------------------------------

/// The user's identity — who they are.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIdentity {
    /// How the user prefers to be addressed.
    pub name: String,
    /// Pronouns (optional).
    pub pronouns: Option<String>,
    /// How the user relates to Artume (e.g. "assistant", "guide", "co-pilot").
    pub relationship: String,
}

impl Default for UserIdentity {
    fn default() -> Self {
        Self {
            name: "Friend".to_string(),
            pronouns: None,
            relationship: "co-pilot".to_string(),
        }
    }
}

/// User preferences — how Artume should behave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Speech speed multiplier (0.5 = slow, 1.0 = normal, 1.5 = fast).
    pub speech_speed: f32,
    /// Verbosity level: 1 = terse, 2 = balanced, 3 = detailed.
    pub verbosity: u8,
    /// Whether Artume can interrupt the user's current activity.
    pub interruptible: bool,
    /// Whether to narrate actions (e.g. "Fetching that page…").
    pub narrate_actions: bool,
    /// Whether to offer proactive suggestions (weather, reminders).
    pub proactive: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            speech_speed: 1.0,
            verbosity: 2,
            interruptible: true,
            narrate_actions: true,
            proactive: true,
        }
    }
}

/// Daily routines — recurring patterns Artume can learn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRoutines {
    /// Morning routine description (e.g. "check weather, then news").
    pub morning: Option<String>,
    /// Evening routine description.
    pub evening: Option<String>,
    /// Workday patterns.
    pub workday: Option<String>,
    /// Weekend patterns.
    pub weekend: Option<String>,
}

impl Default for UserRoutines {
    fn default() -> Self {
        Self {
            morning: None,
            evening: None,
            workday: None,
            weekend: None,
        }
    }
}

/// Current context — updated in real-time during a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    /// The user's current focus level (from attention evaluator).
    pub focus_level: String,
    /// Whether the user is in a meeting or focused task.
    pub do_not_disturb: bool,
    /// Last known activity.
    pub last_activity: Option<String>,
    /// Session count (incremented each start).
    pub session_count: u64,
}

impl Default for UserContext {
    fn default() -> Self {
        Self {
            focus_level: "normal".to_string(),
            do_not_disturb: false,
            last_activity: None,
            session_count: 0,
        }
    }
}

/// Learned patterns — system-written observations over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserHistory {
    /// Patterns Artume has noticed (e.g. "user prefers short summaries before 9am").
    pub patterns: Vec<String>,
    /// Topics the user frequently asks about.
    pub frequent_topics: Vec<String>,
    /// Commands the user uses most.
    pub frequent_commands: Vec<String>,
}

impl Default for UserHistory {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            frequent_topics: Vec::new(),
            frequent_commands: Vec::new(),
        }
    }
}

/// The complete user profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub identity: UserIdentity,
    pub preferences: UserPreferences,
    pub routines: UserRoutines,
    pub context: UserContext,
    pub history: UserHistory,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            identity: UserIdentity::default(),
            preferences: UserPreferences::default(),
            routines: UserRoutines::default(),
            context: UserContext::default(),
            history: UserHistory::default(),
        }
    }
}

impl UserProfile {
    /// Load the profile from disk. Returns `None` on first run.
    pub fn load() -> Option<Self> {
        let dir = profile_dir();
        if !dir.exists() {
            return None;
        }

        let identity = load_file::<UserIdentity>("identity.md")?;
        let preferences = load_file::<UserPreferences>("preferences.md")?;
        let routines = load_file::<UserRoutines>("routines.md")?;
        let context = load_file::<UserContext>("context.md")?;
        let history = load_file::<UserHistory>("history.md")?;

        Some(Self {
            identity,
            preferences,
            routines,
            context,
            history,
        })
    }

    /// Save the profile to disk. Creates the directory if needed.
    pub fn save(&self) -> anyhow::Result<()> {
        let dir = profile_dir();
        fs::create_dir_all(&dir)?;

        save_file("identity.md", &self.identity)?;
        save_file("preferences.md", &self.preferences)?;
        save_file("routines.md", &self.routines)?;
        save_file("context.md", &self.context)?;
        save_file("history.md", &self.history)?;

        Ok(())
    }

    /// Check if this is the first run (no profile exists).
    pub fn is_first_run() -> bool {
        !profile_dir().exists()
    }

    /// Update context in-place and persist.
    pub fn update_context(&mut self, f: impl FnOnce(&mut UserContext)) {
        f(&mut self.context);
        let _ = self.save();
    }

    /// Add a learned pattern and persist.
    pub fn learn_pattern(&mut self, pattern: String) {
        if !self.history.patterns.contains(&pattern) {
            self.history.patterns.push(pattern);
            let _ = self.save();
        }
    }

    /// Render the profile as a system prompt fragment.
    pub fn to_system_prompt(&self) -> String {
        let mut parts = Vec::new();

        parts.push(format!("The user's name is {}.", self.identity.name));
        if let Some(ref pronouns) = self.identity.pronouns {
            parts.push(format!("Their pronouns are {}.", pronouns));
        }
        parts.push(format!(
            "Your relationship to them is: {}.",
            self.identity.relationship
        ));

        if self.preferences.verbosity <= 1 {
            parts.push("The user prefers terse, brief responses.".to_string());
        } else if self.preferences.verbosity >= 3 {
            parts.push("The user prefers detailed, thorough responses.".to_string());
        }

        if self.preferences.proactive {
            parts.push("You may offer proactive suggestions when appropriate.".to_string());
        } else {
            parts.push("Do not offer unsolicited suggestions.".to_string());
        }

        if let Some(ref morning) = self.routines.morning {
            parts.push(format!("Morning routine: {}", morning));
        }
        if let Some(ref evening) = self.routines.evening {
            parts.push(format!("Evening routine: {}", evening));
        }

        if !self.history.patterns.is_empty() {
            parts.push(format!(
                "Things you've noticed about them: {}",
                self.history.patterns.join("; ")
            ));
        }

        parts.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_file<T: serde::de::DeserializeOwned>(name: &str) -> Option<T> {
    let path = profile_file(name);
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}

fn save_file<T: serde::Serialize>(name: &str, value: &T) -> anyhow::Result<()> {
    let path = profile_file(name);
    let content = toml::to_string_pretty(value)?;
    fs::write(&path, content)?;
    Ok(())
}
