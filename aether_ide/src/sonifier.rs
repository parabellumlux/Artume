//! Code sonification — maps code structure to audio parameters.
//!
//! Converts a `CodeTree` into audio cues:
//! - Nesting depth → pitch (deeper = higher pitch drone)
//! - Scope boundaries → earcon triggers
//! - Code density → tempo/tick rate
//! - Line types → timbre/texture

use crate::parser::{CodeTree, LineInfo, ScopeBoundary, ScopeKind, SymbolKind};

/// Audio parameters for a single line of code.
#[derive(Debug, Clone)]
pub struct LineAudio {
    /// 0-indexed line number.
    pub line: usize,
    /// Pitch in Hz for the background drone at this depth.
    pub drone_pitch: f32,
    /// Volume of the drone (0.0 - 1.0).
    pub drone_volume: f32,
    /// Earcon to play at this line, if any.
    pub earcon: Option<Earcon>,
    /// Tempo multiplier (1.0 = normal, >1 = faster reading).
    pub tempo: f32,
    /// Whether this line should be spoken.
    pub speak: bool,
    /// Text to speak (if speak is true).
    pub speak_text: Option<String>,
}

/// Types of earcons (audio icons).
#[derive(Debug, Clone, PartialEq)]
pub enum Earcon {
    /// Rising arpeggio — entering a scope.
    ScopeEnter(ScopeKind),
    /// Falling arpeggio — exiting a scope.
    ScopeExit(ScopeKind),
    /// Soft chime — comment block.
    Comment,
    /// Warning buzz — long line.
    LongLine,
    /// Error tone — LSP diagnostic.
    Error,
    /// Warning tone — LSP warning.
    Warning,
    /// Click — blank line / visual gap.
    BlankLine,
    /// Pulse — loop iteration.
    LoopIteration,
    /// Double-beep — conditional branch.
    Conditional,
}

/// Pitch mapping: nesting depth → Hz
const DEPTH_PITCH: &[f32] = &[
    65.41,   // C2 — depth 0 (top-level)
    130.81,  // C3 — depth 1 (class/module)
    261.63,  // C4 — depth 2 (function/method) — default speaking pitch
    523.25,  // C5 — depth 3 (if/for/while)
    1046.50, // C6 — depth 4+
];

/// Maximum depth before we clamp.
const MAX_DEPTH: usize = 4;

/// Convert nesting depth to a drone pitch.
pub fn depth_to_pitch(depth: usize) -> f32 {
    let idx = depth.min(MAX_DEPTH);
    DEPTH_PITCH[idx]
}

/// Convert nesting depth to drone volume (deeper = quieter drone, more speech focus).
pub fn depth_to_volume(depth: usize) -> f32 {
    match depth {
        0 => 0.15,
        1 => 0.12,
        2 => 0.08,
        3 => 0.05,
        _ => 0.03,
    }
}

/// Get the earcon for a scope boundary.
pub fn boundary_earcon(boundary: &ScopeBoundary, is_start: bool) -> Option<Earcon> {
    if is_start {
        match boundary.kind {
            ScopeKind::Class | ScopeKind::Module => Some(Earcon::ScopeEnter(boundary.kind.clone())),
            ScopeKind::Function
            | ScopeKind::Method
            | ScopeKind::AsyncFunction
            | ScopeKind::AsyncMethod => Some(Earcon::ScopeEnter(boundary.kind.clone())),
            ScopeKind::If | ScopeKind::Else | ScopeKind::Match => Some(Earcon::Conditional),
            ScopeKind::For | ScopeKind::While => Some(Earcon::LoopIteration),
            ScopeKind::Try => Some(Earcon::ScopeEnter(boundary.kind.clone())),
            ScopeKind::Catch => Some(Earcon::ScopeEnter(boundary.kind.clone())),
            ScopeKind::With | ScopeKind::Lambda => None,
        }
    } else {
        match boundary.kind {
            ScopeKind::Class | ScopeKind::Module => Some(Earcon::ScopeExit(boundary.kind.clone())),
            ScopeKind::Function
            | ScopeKind::Method
            | ScopeKind::AsyncFunction
            | ScopeKind::AsyncMethod => Some(Earcon::ScopeExit(boundary.kind.clone())),
            _ => None,
        }
    }
}

/// Compute code density tempo multiplier for a line.
/// Dense lines (many tokens) → slower reading. Sparse lines → faster.
pub fn line_tempo(line: &LineInfo) -> f32 {
    let tokens = line.token_count;
    if tokens == 0 {
        2.0 // blank lines fly past
    } else if tokens <= 5 {
        1.5 // sparse
    } else if tokens <= 20 {
        1.0 // normal
    } else if tokens <= 40 {
        0.8 // dense
    } else {
        0.6 // very dense
    }
}

/// Generate audio parameters for every line in a code tree.
pub fn sonify_tree(tree: &CodeTree) -> Vec<LineAudio> {
    let mut audio_lines = Vec::with_capacity(tree.lines.len());

    for line_info in &tree.lines {
        let drone_pitch = depth_to_pitch(line_info.depth);
        let drone_volume = depth_to_volume(line_info.depth);
        let tempo = line_tempo(line_info);

        // Determine earcon
        let earcon = if line_info.is_boundary_start {
            line_info
                .boundary
                .as_ref()
                .and_then(|b| boundary_earcon(b, true))
        } else if line_info.is_boundary_end {
            line_info
                .boundary
                .as_ref()
                .and_then(|b| boundary_earcon(b, false))
        } else if line_info.is_comment {
            Some(Earcon::Comment)
        } else if line_info.is_blank {
            Some(Earcon::BlankLine)
        } else {
            None
        };

        // Determine if this line should be spoken
        let speak = !line_info.is_blank && !line_info.is_comment;

        audio_lines.push(LineAudio {
            line: line_info.line,
            drone_pitch,
            drone_volume,
            earcon,
            tempo,
            speak,
            speak_text: None, // filled in by the reader with actual source text
        });
    }

    audio_lines
}

/// Generate a text summary of the code structure for the "skim" reading mode.
pub fn structure_summary(tree: &CodeTree) -> String {
    let mut parts = Vec::new();

    parts.push(format!(
        "File {}: {} lines, {}.",
        tree.path.split('/').next_back().unwrap_or(&tree.path),
        tree.total_lines,
        tree.language
    ));

    // Group symbols by depth
    let top_level: Vec<_> = tree.symbols.iter().filter(|s| s.depth == 0).collect();
    let second_level: Vec<_> = tree.symbols.iter().filter(|s| s.depth == 1).collect();

    if !top_level.is_empty() {
        let desc: Vec<String> = top_level
            .iter()
            .map(|s| {
                let kind = match s.kind {
                    SymbolKind::Class => "class",
                    SymbolKind::Function => "function",
                    SymbolKind::Method => "method",
                    SymbolKind::AsyncFunction => "async function",
                };
                format!("{} {} (line {})", kind, s.name, s.start_line + 1)
            })
            .collect();
        parts.push(format!("Top level: {}.", desc.join(", ")));
    }

    if !second_level.is_empty() {
        let desc: Vec<String> = second_level
            .iter()
            .map(|s| {
                let kind = match s.kind {
                    SymbolKind::Class => "class",
                    SymbolKind::Function => "function",
                    SymbolKind::Method => "method",
                    SymbolKind::AsyncFunction => "async function",
                };
                format!("{} {} (line {})", kind, s.name, s.start_line + 1)
            })
            .collect();
        parts.push(format!("Nested: {}.", desc.join(", ")));
    }

    parts.join(" ")
}

/// Generate a tree-shaped audio description of the structure.
pub fn tree_summary(tree: &CodeTree) -> String {
    let mut parts = Vec::new();
    let indent = "  ";

    for symbol in &tree.symbols {
        let prefix = if symbol.depth == 0 { "" } else { indent };
        let kind = match symbol.kind {
            SymbolKind::Class => "class",
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::AsyncFunction => "async function",
        };
        let lines = symbol.end_line - symbol.start_line + 1;
        parts.push(format!(
            "{}{} {} ({} lines)",
            prefix, kind, symbol.name, lines
        ));
    }

    if parts.is_empty() {
        format!(
            "File has no top-level symbols. {} lines total.",
            tree.total_lines
        )
    } else {
        format!("Structure: {}", parts.join(". "))
    }
}
