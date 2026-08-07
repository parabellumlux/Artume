//! Voice-driven cursor movement and spatial audio workspace rendering.
//!
//! Provides:
//! - [`Cursor`] with position tracking and history for "go back" navigation
//! - [`CursorNavigation`] for voice-driven code navigation over a parsed
//!   [`CodeTree`](crate::parser::CodeTree)
//! - [`SpatialAudioWorkspace`] for mapping context lines to 3D audio
//!   positions via the aether_audio [`SpatialMixer`]

use crate::parser::{CodeTree, ScopeBoundary, ScopeKind, SymbolKind};
use aether_audio::{SpatialMixer, SpatialPosition, VirtualSource};

// ---------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------

/// Current cursor position with a history stack for "go back" navigation.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// Current file path.
    pub file: String,
    /// 0-indexed line number.
    pub line: usize,
    /// 0-indexed column number.
    pub column: usize,
    /// History of previous positions — each entry is `(file, line, column)`.
    pub history: Vec<(String, usize, usize)>,
}

impl Cursor {
    /// Create a new cursor positioned at the start of the given file.
    pub fn new(file: String) -> Self {
        Self {
            file,
            line: 0,
            column: 0,
            history: Vec::new(),
        }
    }

    /// Push the current position onto the history stack, then move to a new
    /// position.
    pub fn move_to(&mut self, file: String, line: usize, column: usize) {
        self.history
            .push((self.file.clone(), self.line, self.column));
        self.file = file;
        self.line = line;
        self.column = column;
    }

    /// Pop the last position from the history stack and return to it.
    ///
    /// Returns `true` if a position was restored, `false` if the history
    /// stack was empty.
    pub fn go_back(&mut self) -> bool {
        if let Some((file, line, column)) = self.history.pop() {
            self.file = file;
            self.line = line;
            self.column = column;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// LSP diagnostic (lightweight)
// ---------------------------------------------------------------------------

/// A lightweight LSP diagnostic for problem-based navigation.
#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    /// 0-indexed line number where the diagnostic is located.
    pub line: usize,
    /// Severity level: 1 = error, 2 = warning, 3 = info, 4 = hint.
    pub severity: u8,
    /// Human-readable diagnostic message.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Cursor navigation
// ---------------------------------------------------------------------------

/// Voice-driven cursor navigation over a parsed code tree.
///
/// Wraps a [`Cursor`] together with an optional [`CodeTree`] and a list of
/// LSP diagnostics, providing high-level navigation commands that can be
/// driven by voice input.
#[derive(Debug, Clone)]
pub struct CursorNavigation {
    /// The current cursor position.
    pub cursor: Cursor,
    /// The parsed code tree for the current file, if available.
    pub tree: Option<CodeTree>,
    /// LSP diagnostics for the current file.
    pub diagnostics: Vec<LspDiagnostic>,
}

impl CursorNavigation {
    /// Create a new navigator wrapping the given cursor.
    pub fn new(cursor: Cursor) -> Self {
        Self {
            cursor,
            tree: None,
            diagnostics: Vec::new(),
        }
    }

    // -- Line navigation ---------------------------------------------------

    /// Set the cursor to a specific 0-indexed line, preserving the current
    /// column.
    ///
    /// Returns an error if the line is out of range (when a code tree is
    /// loaded).
    pub fn move_to_line(&mut self, line: usize) -> Result<(), String> {
        if let Some(ref tree) = self.tree {
            if line >= tree.total_lines {
                return Err(format!(
                    "Line {} is out of range (file has {} lines)",
                    line + 1,
                    tree.total_lines
                ));
            }
        }
        self.cursor
            .move_to(self.cursor.file.clone(), line, self.cursor.column);
        Ok(())
    }

    // -- Symbol navigation -------------------------------------------------

    /// Find a function by name in the code tree and jump to its start line.
    ///
    /// Returns the 0-indexed line number on success, or an error if the
    /// function is not found or no tree is loaded.
    pub fn move_to_function(&mut self, name: &str) -> Result<usize, String> {
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| "No code tree loaded".to_string())?;
        let sym = tree
            .symbols
            .iter()
            .find(|s| {
                matches!(s.kind, SymbolKind::Function | SymbolKind::AsyncFunction) && s.name == name
            })
            .ok_or_else(|| format!("Function '{}' not found", name))?;
        self.cursor
            .move_to(self.cursor.file.clone(), sym.start_line, 0);
        Ok(sym.start_line)
    }

    /// Find a class by name in the code tree and jump to its start line.
    ///
    /// Returns the 0-indexed line number on success, or an error if the
    /// class is not found or no tree is loaded.
    pub fn move_to_class(&mut self, name: &str) -> Result<usize, String> {
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| "No code tree loaded".to_string())?;
        let sym = tree
            .symbols
            .iter()
            .find(|s| matches!(s.kind, SymbolKind::Class) && s.name == name)
            .ok_or_else(|| format!("Class '{}' not found", name))?;
        self.cursor
            .move_to(self.cursor.file.clone(), sym.start_line, 0);
        Ok(sym.start_line)
    }

    /// Find any symbol (function, class, method) by name and jump to it.
    ///
    /// Returns the 0-indexed line number on success, or an error if the
    /// symbol is not found or no tree is loaded.
    pub fn move_to_symbol(&mut self, name: &str) -> Result<usize, String> {
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| "No code tree loaded".to_string())?;
        let sym = tree
            .symbols
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| format!("Symbol '{}' not found", name))?;
        self.cursor
            .move_to(self.cursor.file.clone(), sym.start_line, 0);
        Ok(sym.start_line)
    }

    // -- Function boundary navigation --------------------------------------

    /// Jump to the next function/method boundary after the current cursor
    /// line.
    ///
    /// Returns the 0-indexed line number of the next function, or an error
    /// if none is found or no tree is loaded.
    pub fn move_next_function(&mut self) -> Result<usize, String> {
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| "No code tree loaded".to_string())?;
        let current = self.cursor.line;
        let next = tree
            .boundaries
            .iter()
            .filter(|b| {
                matches!(
                    b.kind,
                    ScopeKind::Function | ScopeKind::AsyncFunction | ScopeKind::Method
                )
            })
            .find(|b| b.start_line > current)
            .ok_or_else(|| "No next function found".to_string())?;
        self.cursor
            .move_to(self.cursor.file.clone(), next.start_line, 0);
        Ok(next.start_line)
    }

    /// Jump to the previous function/method boundary before the current
    /// cursor line.
    ///
    /// Returns the 0-indexed line number of the previous function, or an
    /// error if none is found or no tree is loaded.
    pub fn move_prev_function(&mut self) -> Result<usize, String> {
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| "No code tree loaded".to_string())?;
        let current = self.cursor.line;
        let prev = tree
            .boundaries
            .iter()
            .filter(|b| {
                matches!(
                    b.kind,
                    ScopeKind::Function | ScopeKind::AsyncFunction | ScopeKind::Method
                )
            })
            .filter(|b| b.start_line < current)
            .last()
            .ok_or_else(|| "No previous function found".to_string())?;
        self.cursor
            .move_to(self.cursor.file.clone(), prev.start_line, 0);
        Ok(prev.start_line)
    }

    // -- Problem navigation ------------------------------------------------

    /// Jump to the next LSP diagnostic after the current cursor line.
    ///
    /// Returns the 0-indexed line number of the next diagnostic, or an
    /// error if none is found.
    pub fn move_next_problem(&mut self) -> Result<usize, String> {
        let current = self.cursor.line;
        let next = self
            .diagnostics
            .iter()
            .find(|d| d.line > current)
            .ok_or_else(|| "No next problem found".to_string())?;
        self.cursor.move_to(self.cursor.file.clone(), next.line, 0);
        Ok(next.line)
    }

    // -- History navigation ------------------------------------------------

    /// Return to the previous cursor position from the history stack.
    ///
    /// Returns `true` if a position was restored, `false` if the history
    /// stack was empty.
    pub fn go_back(&mut self) -> bool {
        self.cursor.go_back()
    }

    // -- Location description ----------------------------------------------

    /// Return a spoken description of the current cursor location.
    ///
    /// # Examples
    ///
    /// - `"Line 42 of parser.rs, inside function parse_file, 2 levels deep"`
    /// - `"Line 1 of main.rs, top level"`
    pub fn where_am_i(&self) -> String {
        let file_name = self
            .cursor
            .file
            .rsplit('/')
            .next()
            .unwrap_or(&self.cursor.file);
        let line_display = self.cursor.line + 1; // 1-indexed for display

        let Some(ref tree) = self.tree else {
            return format!("Line {} of {}", line_display, file_name);
        };

        // Find all scope boundaries containing the current line, ordered by
        // start_line (innermost = last).
        let containing: Vec<&ScopeBoundary> = tree
            .boundaries
            .iter()
            .filter(|b| b.start_line <= self.cursor.line && b.end_line >= self.cursor.line)
            .collect();

        if containing.is_empty() {
            return format!("Line {} of {}, top level", line_display, file_name);
        }

        let innermost = containing.last().unwrap();
        let depth = innermost.depth;
        let scope_name = innermost.name.as_deref().unwrap_or(match innermost.kind {
            ScopeKind::Module => "module",
            ScopeKind::Class => "class",
            ScopeKind::Function => "function",
            ScopeKind::Method => "method",
            ScopeKind::AsyncFunction => "async function",
            ScopeKind::AsyncMethod => "async method",
            ScopeKind::If => "if block",
            ScopeKind::Else => "else block",
            ScopeKind::For => "for loop",
            ScopeKind::While => "while loop",
            ScopeKind::Try => "try block",
            ScopeKind::Catch => "catch block",
            ScopeKind::With => "with block",
            ScopeKind::Match => "match block",
            ScopeKind::Lambda => "lambda",
        });

        if depth == 0 {
            format!(
                "Line {} of {}, inside {} {}",
                line_display, file_name, scope_name, depth
            )
        } else {
            format!(
                "Line {} of {}, inside {}, {} levels deep",
                line_display, file_name, scope_name, depth
            )
        }
    }

    // -- Context lines for spatial audio -----------------------------------

    /// Return ±N lines around the cursor with their spatial positions for
    /// 3D audio rendering.
    ///
    /// The current line is positioned at [`SpatialPosition::CENTRE`].
    /// Lines above the cursor are positioned at `SOFT_LEFT`-like positions
    /// that fade further left with distance. Lines below the cursor are
    /// positioned at `SOFT_RIGHT`-like positions that fade further right
    /// with distance.
    ///
    /// Each returned tuple is `(line_number, line_text, spatial_position)`.
    /// Returns an empty vec if no code tree is loaded.
    pub fn get_context_lines(&self, n: usize) -> Vec<(usize, String, SpatialPosition)> {
        let Some(ref tree) = self.tree else {
            return Vec::new();
        };

        let current = self.cursor.line;
        let start = current.saturating_sub(n);
        let end = std::cmp::min(current + n + 1, tree.total_lines);

        // Read the source file to get line text.  If the file can't be read
        // we return empty strings for the line content.
        let source = std::fs::read_to_string(&self.cursor.file).unwrap_or_default();
        let lines: Vec<&str> = source.lines().collect();

        let mut result = Vec::with_capacity(end - start);

        for line_num in start..end {
            let line_text = lines
                .get(line_num)
                .map(|s| s.to_string())
                .unwrap_or_default();

            let position = if line_num == current {
                SpatialPosition::CENTRE
            } else if line_num < current {
                // Lines above: fade left with distance.
                let distance = (current - line_num) as f32;
                let azimuth = (-45.0 - 15.0 * (distance - 1.0).max(0.0)).max(-90.0);
                SpatialPosition {
                    azimuth_deg: azimuth,
                    elevation_deg: 0.0,
                    distance: 1.0 + distance * 0.3,
                }
            } else {
                // Lines below: fade right with distance.
                let distance = (line_num - current) as f32;
                let azimuth = (45.0 + 15.0 * (distance - 1.0).max(0.0)).min(90.0);
                SpatialPosition {
                    azimuth_deg: azimuth,
                    elevation_deg: 0.0,
                    distance: 1.0 + distance * 0.3,
                }
            };

            result.push((line_num, line_text, position));
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Spatial audio workspace
// ---------------------------------------------------------------------------

/// Maps context lines to 3D audio positions using the aether_audio
/// [`SpatialMixer`].
///
/// Each context line is registered as a [`VirtualSource`] at a specific
/// [`SpatialPosition`], allowing the user to hear the code structure
/// spatially — the current line at centre, lines above fading left, and
/// lines below fading right.
///
/// # Source labels
///
/// The workspace uses fixed labels (`"cursor_current"`, `"ctx_0"` … `"ctx_9"`)
/// and updates their positions in-place when [`setup_spatial_workspace`] is
/// called, avoiding unnecessary source removal / re-registration.
pub struct SpatialAudioWorkspace {
    /// The underlying spatial mixer.
    pub mixer: SpatialMixer,
    /// Label for the current-line audio source (`"cursor_current"`).
    pub current_line_source: String,
    /// Labels for context-line audio sources (`"ctx_0"`, `"ctx_1"`, …).
    pub context_sources: Vec<String>,
}

impl SpatialAudioWorkspace {
    /// Create a new empty spatial audio workspace.
    pub fn new() -> Self {
        Self {
            mixer: SpatialMixer::new(),
            current_line_source: String::new(),
            context_sources: Vec::new(),
        }
    }

    /// Configure the spatial workspace from a code tree, placing the cursor
    /// line at [`SpatialPosition::CENTRE`] and surrounding lines at
    /// spatially-distributed positions.
    ///
    /// Up to 5 lines above and 5 lines below the cursor are registered.
    /// Lines above the cursor fade left (negative azimuth) with distance;
    /// lines below fade right (positive azimuth) with distance. Distance
    /// gain roll-off is applied via the `distance` field.
    ///
    /// Existing sources are reused when possible — only the position is
    /// updated — avoiding unnecessary churn in the audio graph.
    pub fn setup_spatial_workspace(&mut self, tree: &CodeTree, cursor_line: usize) {
        const MAX_CONTEXT: usize = 5;
        const FIXED_LABELS: [&str; 11] = [
            "cursor_current",
            "ctx_0",
            "ctx_1",
            "ctx_2",
            "ctx_3",
            "ctx_4",
            "ctx_5",
            "ctx_6",
            "ctx_7",
            "ctx_8",
            "ctx_9",
        ];

        // --- Current line (centre) ---

        if self.mixer.source_mut(FIXED_LABELS[0]).is_none() {
            self.mixer
                .add_source(VirtualSource::new(FIXED_LABELS[0], SpatialPosition::CENTRE));
        }
        self.current_line_source = FIXED_LABELS[0].to_string();

        // --- Context lines ---

        let above_count = cursor_line.min(MAX_CONTEXT);
        let below_count = (tree.total_lines.saturating_sub(cursor_line + 1)).min(MAX_CONTEXT);
        let total_needed = above_count + below_count;

        // Remove excess sources from the previous setup.
        while self.context_sources.len() > total_needed {
            if let Some(label) = self.context_sources.pop() {
                self.mixer.remove_source(&label);
            }
        }

        self.context_sources.clear();

        // Lines above cursor (fading left).
        let start = cursor_line.saturating_sub(MAX_CONTEXT);
        for (i, line_num) in (start..cursor_line).enumerate() {
            let label = FIXED_LABELS[1 + i];
            let distance = (cursor_line - line_num) as f32;
            let azimuth = (-45.0 - 15.0 * (distance - 1.0).max(0.0)).max(-90.0);
            let pos = SpatialPosition {
                azimuth_deg: azimuth,
                elevation_deg: 0.0,
                distance: 1.0 + distance * 0.3,
            };

            if self.mixer.source_mut(label).is_some() {
                // Update position in-place.
                if let Some(src) = self.mixer.source_mut(label) {
                    src.position = pos;
                }
            } else {
                self.mixer.add_source(VirtualSource::new(label, pos));
            }
            self.context_sources.push(label.to_string());
        }

        // Lines below cursor (fading right).
        let end = std::cmp::min(cursor_line + MAX_CONTEXT + 1, tree.total_lines);
        for (i, line_num) in ((cursor_line + 1)..end).enumerate() {
            let label = FIXED_LABELS[1 + above_count + i];
            let distance = (line_num - cursor_line) as f32;
            let azimuth = (45.0 + 15.0 * (distance - 1.0).max(0.0)).min(90.0);
            let pos = SpatialPosition {
                azimuth_deg: azimuth,
                elevation_deg: 0.0,
                distance: 1.0 + distance * 0.3,
            };

            if self.mixer.source_mut(label).is_some() {
                if let Some(src) = self.mixer.source_mut(label) {
                    src.position = pos;
                }
            } else {
                self.mixer.add_source(VirtualSource::new(label, pos));
            }
            self.context_sources.push(label.to_string());
        }
    }
}

impl Default for SpatialAudioWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_move_to_and_go_back() {
        let mut cursor = Cursor::new("test.rs".to_string());
        assert_eq!(cursor.line, 0);
        assert!(cursor.history.is_empty());

        cursor.move_to("test.rs".to_string(), 10, 5);
        assert_eq!(cursor.line, 10);
        assert_eq!(cursor.history.len(), 1);

        cursor.move_to("test.rs".to_string(), 20, 0);
        assert_eq!(cursor.line, 20);
        assert_eq!(cursor.history.len(), 2);

        assert!(cursor.go_back());
        assert_eq!(cursor.line, 10);
        assert_eq!(cursor.history.len(), 1);

        assert!(cursor.go_back());
        assert_eq!(cursor.line, 0);
        assert_eq!(cursor.history.len(), 0);

        assert!(!cursor.go_back());
    }

    #[test]
    fn test_cursor_navigation_move_to_line_out_of_range() {
        let cursor = Cursor::new("test.rs".to_string());
        let mut nav = CursorNavigation::new(cursor);

        // Without a tree, any line is accepted.
        assert!(nav.move_to_line(9999).is_ok());

        // With a tree, out-of-range is rejected.
        let tree = CodeTree {
            path: "test.rs".to_string(),
            language: "rust".to_string(),
            total_lines: 100,
            boundaries: Vec::new(),
            lines: Vec::new(),
            symbols: Vec::new(),
        };
        nav.tree = Some(tree);
        assert!(nav.move_to_line(99).is_ok());
        assert!(nav.move_to_line(100).is_err());
    }

    #[test]
    fn test_where_am_i_no_tree() {
        let cursor = Cursor::new("/home/user/project/main.rs".to_string());
        let nav = CursorNavigation::new(cursor);
        let desc = nav.where_am_i();
        assert!(desc.contains("Line 1 of main.rs"));
        assert!(!desc.contains("top level"));
    }

    #[test]
    fn test_where_am_i_top_level() {
        let cursor = Cursor::new("lib.rs".to_string());
        let tree = CodeTree {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            total_lines: 50,
            boundaries: Vec::new(),
            lines: Vec::new(),
            symbols: Vec::new(),
        };
        let mut nav = CursorNavigation::new(cursor);
        nav.tree = Some(tree);
        let desc = nav.where_am_i();
        assert!(desc.contains("top level"));
    }

    #[test]
    fn test_where_am_i_inside_function() {
        let cursor = Cursor::new("lib.rs".to_string());
        let tree = CodeTree {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            total_lines: 50,
            boundaries: vec![ScopeBoundary {
                kind: ScopeKind::Function,
                name: Some("parse_file".to_string()),
                start_line: 5,
                end_line: 30,
                depth: 1,
            }],
            lines: Vec::new(),
            symbols: Vec::new(),
        };
        let mut nav = CursorNavigation::new(cursor);
        nav.cursor.line = 20;
        nav.tree = Some(tree);
        let desc = nav.where_am_i();
        assert!(desc.contains("Line 21 of lib.rs"));
        assert!(desc.contains("parse_file"));
        assert!(desc.contains("1 levels deep"));
    }

    #[test]
    fn test_get_context_lines_no_tree() {
        let cursor = Cursor::new("test.rs".to_string());
        let nav = CursorNavigation::new(cursor);
        assert!(nav.get_context_lines(3).is_empty());
    }

    #[test]
    fn test_get_context_lines_centered() {
        let cursor = Cursor::new("test.rs".to_string());
        let tree = CodeTree {
            path: "test.rs".to_string(),
            language: "rust".to_string(),
            total_lines: 10,
            boundaries: Vec::new(),
            lines: Vec::new(),
            symbols: Vec::new(),
        };
        let mut nav = CursorNavigation::new(cursor);
        nav.cursor.line = 5;
        nav.tree = Some(tree);

        // Write a temp file so get_context_lines can read it.
        std::fs::write(
            "test.rs",
            "line0\nline1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\n",
        )
        .expect("write test file");

        let ctx = nav.get_context_lines(2);
        assert_eq!(ctx.len(), 5); // lines 3,4,5,6,7

        // Line 5 (current) should be CENTRE.
        assert_eq!(ctx[2].0, 5);
        assert_eq!(ctx[2].2, SpatialPosition::CENTRE);

        // Line 4 (above) should have negative azimuth.
        assert_eq!(ctx[1].0, 4);
        assert!(ctx[1].2.azimuth_deg < 0.0);

        // Line 6 (below) should have positive azimuth.
        assert_eq!(ctx[3].0, 6);
        assert!(ctx[3].2.azimuth_deg > 0.0);

        // Cleanup.
        let _ = std::fs::remove_file("test.rs");
    }

    #[test]
    fn test_spatial_audio_workspace_setup() {
        let tree = CodeTree {
            path: "test.rs".to_string(),
            language: "rust".to_string(),
            total_lines: 20,
            boundaries: Vec::new(),
            lines: Vec::new(),
            symbols: Vec::new(),
        };

        let mut workspace = SpatialAudioWorkspace::new();
        workspace.setup_spatial_workspace(&tree, 10);

        // Should have current line + up to 5 above + up to 5 below = 11.
        assert!(!workspace.current_line_source.is_empty());
        assert_eq!(workspace.context_sources.len(), 10);

        // Calling again should reuse sources (no panic).
        workspace.setup_spatial_workspace(&tree, 3);
        assert_eq!(workspace.context_sources.len(), 8); // 3 above + 5 below
    }
}
