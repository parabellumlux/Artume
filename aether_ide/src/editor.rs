//! Voice-driven editing commands for the audio-first IDE.
//!
//! Provides high-level editing operations designed to be triggered by
//! voice commands. Each method operates on a `TextBuffer` and delegates
//! undo/redo to the buffer's built-in edit tracking.

use crate::buffer::TextBuffer;

/// A handle for LSP-aware editing operations.
///
/// When connected to a language server, this provides intelligent
/// editing features. Currently a minimal stub — the full LSP client
/// lives in the `lsp` module and is wired through the IPC server.
#[derive(Debug, Clone)]
pub struct LspClient {
    /// Language server process ID or connection identifier.
    pub id: String,
}

impl LspClient {
    /// Create a new LSP client handle.
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

/// Voice-driven editor that wraps a `TextBuffer` with high-level commands.
///
/// Each method is designed to be a single voice command target:
/// "insert line before", "delete lines 5 through 8", "wrap in try-catch", etc.
/// All mutations go through the buffer's undo system.
#[derive(Debug, Clone)]
pub struct VoiceEditor {
    /// The underlying text buffer.
    pub buffer: TextBuffer,
    /// Optional LSP client for language-aware operations.
    pub lsp: Option<LspClient>,
}

/// Snapshot of line information for a buffer, owned so it doesn't borrow.
struct LineSnapshot {
    /// Byte offset of each line (0-indexed).
    starts: Vec<usize>,
    /// Text of each line (owned).
    lines: Vec<String>,
    /// Total content length.
    content_len: usize,
}

impl LineSnapshot {
    fn from(content: &str) -> Self {
        let mut starts = Vec::new();
        let mut lines = Vec::new();
        let mut pos = 0;
        for line in content.lines() {
            starts.push(pos);
            lines.push(line.to_string());
            pos += line.len() + 1; // +1 for newline
        }
        if content.is_empty() {
            starts.push(0);
            lines.push(String::new());
        }
        Self {
            starts,
            lines,
            content_len: content.len(),
        }
    }

    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn line_start(&self, idx: usize) -> usize {
        self.starts[idx]
    }

    fn line_end(&self, idx: usize) -> usize {
        self.starts[idx] + self.lines[idx].len()
    }

    fn line_text(&self, idx: usize) -> &str {
        &self.lines[idx]
    }
}

impl VoiceEditor {
    /// Create a new voice editor wrapping the given buffer.
    pub fn new(buffer: TextBuffer) -> Self {
        Self { buffer, lsp: None }
    }

    /// Create a new voice editor with an LSP client.
    pub fn with_lsp(buffer: TextBuffer, lsp: LspClient) -> Self {
        Self {
            buffer,
            lsp: Some(lsp),
        }
    }

    // ── Line-level operations ──────────────────────────────────────────

    /// Insert a new line of text before the given line number (0-indexed).
    ///
    /// If `line` is 0, inserts at the very beginning.
    /// If `line` is past the end, appends at the end.
    pub fn insert_line_before(&mut self, line: usize, text: &str) {
        let snap = LineSnapshot::from(self.buffer.content());
        let pos = if line < snap.line_count() {
            snap.line_start(line)
        } else {
            snap.content_len
        };
        let insertion = format!("{text}\n");
        self.buffer.insert(pos, &insertion);
    }

    /// Insert a new line of text after the given line number (0-indexed).
    ///
    /// If `line` is past the last line, appends at the end.
    pub fn insert_line_after(&mut self, line: usize, text: &str) {
        let snap = LineSnapshot::from(self.buffer.content());
        if snap.line_count() == 0 || snap.content_len == 0 {
            self.buffer.insert(0, text);
            return;
        }
        let line = line.min(snap.line_count().saturating_sub(1));
        let is_last = line == snap.line_count() - 1;
        let pos = if is_last {
            // Last line — insert after the line text (no trailing newline)
            snap.line_end(line)
        } else {
            // Not last — insert after the trailing newline
            snap.line_end(line) + 1
        };
        let insertion = if is_last {
            format!("\n{text}")
        } else {
            format!("{text}\n")
        };
        self.buffer.insert(pos, &insertion);
    }

    /// Delete a single line (0-indexed).
    ///
    /// Does nothing if the line index is out of bounds.
    pub fn delete_line(&mut self, line: usize) {
        let snap = LineSnapshot::from(self.buffer.content());
        if line >= snap.line_count() {
            return;
        }
        let start = snap.line_start(line);
        let end = snap.line_end(line);
        if line == snap.line_count() - 1 && line > 0 {
            // Last line — also remove the preceding newline
            self.buffer.delete_range(start.saturating_sub(1), end);
        } else {
            let end = if end < snap.content_len { end + 1 } else { end };
            self.buffer.delete_range(start, end);
        }
    }

    /// Delete a range of lines (0-indexed, inclusive).
    ///
    /// Does nothing if `start > end` or `start` is out of bounds.
    pub fn delete_lines(&mut self, start: usize, end: usize) {
        if start > end {
            return;
        }
        let snap = LineSnapshot::from(self.buffer.content());
        if start >= snap.line_count() {
            return;
        }
        let end = end.min(snap.line_count().saturating_sub(1));
        let range_start = snap.line_start(start);
        let range_end = snap.line_end(end);
        let (del_start, del_end) = if end == snap.line_count() - 1 && start > 0 {
            (range_start.saturating_sub(1), range_end)
        } else {
            (
                range_start,
                if range_end < snap.content_len {
                    range_end + 1
                } else {
                    range_end
                },
            )
        };
        self.buffer.delete_range(del_start, del_end);
    }

    /// Duplicate a line by copying it below the original.
    ///
    /// Does nothing if the line index is out of bounds.
    pub fn duplicate_line(&mut self, line: usize) {
        let snap = LineSnapshot::from(self.buffer.content());
        if line >= snap.line_count() {
            return;
        }
        let text = snap.line_text(line).to_string();
        drop(snap);
        self.insert_line_after(line, &text);
    }

    // ── Text transformation operations ─────────────────────────────────

    /// Find and replace text in the buffer.
    ///
    /// Returns a string describing the result (e.g. "3 replacements made").
    pub fn change_text(&mut self, old: &str, new: &str) -> String {
        let count = self.buffer.find_and_replace(old, new);
        if count == 0 {
            format!("No occurrences of \"{old}\" found")
        } else if count == 1 {
            "1 replacement made".to_string()
        } else {
            format!("{count} replacements made")
        }
    }

    /// Wrap the selected line range in a try-catch block.
    ///
    /// For the selected lines, this inserts a `try {` before the first line
    /// and a `} catch { }` after the last line. The indentation of the
    /// inserted lines matches the indentation of the first selected line.
    pub fn wrap_in_try_catch(&mut self, start: usize, end: usize) {
        let snap = LineSnapshot::from(self.buffer.content());
        if start >= snap.line_count() || start > end {
            return;
        }
        let end = end.min(snap.line_count().saturating_sub(1));
        let first_line = snap.line_text(start);
        let indent: String = first_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();

        let after_last_pos = snap.line_end(end);
        let before_first_pos = snap.line_start(start);

        // Drop snap before mutating
        drop(snap);

        // Insert closing block first (right-to-left so positions stay valid)
        let catch_block = format!("\n{indent}}} catch {{\n{indent}}}");
        self.buffer.insert(after_last_pos, &catch_block);

        // Insert opening block
        let try_block = format!("{indent}try {{\n");
        self.buffer.insert(before_first_pos, &try_block);
    }

    /// Extract the selected line range into a new function and replace with a call.
    ///
    /// The extracted lines become the body of a new function named `name`,
    /// placed after the current scope. The original lines are replaced with
    /// a call to the new function.
    pub fn extract_function(&mut self, start: usize, end: usize, name: &str) {
        let snap = LineSnapshot::from(self.buffer.content());
        if start >= snap.line_count() || start > end {
            return;
        }
        let end = end.min(snap.line_count().saturating_sub(1));
        let first_line = snap.line_text(start);
        let indent: String = first_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();

        // Build extracted body with consistent indentation
        let extracted_text: String = snap.lines[start..=end]
            .iter()
            .map(|l| {
                let stripped = l.trim_start().to_string();
                format!("{indent}{stripped}")
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Build the new function definition
        let func_def = format!("\n\n{indent}fn {name}() {{\n{extracted_text}\n{indent}}}\n");

        // Calculate byte positions
        let range_start = snap.line_start(start);
        let range_end = snap.line_end(end);

        // Drop snap before mutating
        drop(snap);

        // Replace the selected lines with a function call
        let call_text = format!("{indent}{name}();");
        self.buffer.replace(range_start, range_end, &call_text);

        // Insert the function definition after the call
        let insert_pos = range_start + call_text.len();
        self.buffer.insert(insert_pos, &func_def);
    }

    /// Toggle comment on a range of lines.
    ///
    /// If all lines in the range are commented, uncomments them.
    /// Otherwise, comments them. Uses `//` as the comment marker.
    pub fn comment_lines(&mut self, start: usize, end: usize) {
        let snap = LineSnapshot::from(self.buffer.content());
        if start >= snap.line_count() || start > end {
            return;
        }
        let end = end.min(snap.line_count().saturating_sub(1));
        let target_lines: Vec<String> = snap.lines[start..=end].to_vec();

        // Check if all non-blank lines are already commented
        let all_commented = target_lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .all(|l| l.trim_start().starts_with("//"));

        // Collect all edits to apply (positions relative to original content)
        struct LineEdit {
            pos: usize,
            old_len: usize,
            new_text: String,
        }

        let mut edits: Vec<LineEdit> = Vec::new();

        if all_commented {
            // Uncomment: remove leading "//" (with optional space)
            for (i, line) in target_lines.iter().enumerate() {
                let line_idx = start + i;
                let line_start = snap.line_start(line_idx);
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix("//") {
                    let leading_spaces = line.len() - trimmed.len();
                    let content_after = rest.strip_prefix(' ').unwrap_or(rest);
                    let new_line = format!("{}{content_after}", &line[..leading_spaces]);
                    edits.push(LineEdit {
                        pos: line_start,
                        old_len: line.len(),
                        new_text: new_line,
                    });
                }
            }
        } else {
            // Comment: add "// " before each non-blank line
            for (i, line) in target_lines.iter().enumerate() {
                let line_idx = start + i;
                let line_start = snap.line_start(line_idx);
                if !line.trim().is_empty() {
                    let leading = line.len() - line.trim_start().len();
                    let new_line =
                        format!("{}// {}", &line[..leading], line[leading..].trim_start());
                    edits.push(LineEdit {
                        pos: line_start,
                        old_len: line.len(),
                        new_text: new_line,
                    });
                }
            }
        }

        // Drop snap before mutating
        drop(snap);

        // Apply edits right-to-left so positions stay valid
        for edit in edits.into_iter().rev() {
            self.buffer
                .replace(edit.pos, edit.pos + edit.old_len, &edit.new_text);
        }
    }

    /// Increase indentation of a range of lines by 4 spaces.
    pub fn indent_lines(&mut self, start: usize, end: usize) {
        let snap = LineSnapshot::from(self.buffer.content());
        if start >= snap.line_count() || start > end {
            return;
        }
        let end = end.min(snap.line_count().saturating_sub(1));

        // Collect insert positions (right-to-left)
        let mut positions: Vec<usize> = Vec::new();
        for i in start..=end {
            let line = snap.line_text(i);
            if line.trim().is_empty() {
                continue;
            }
            positions.push(snap.line_start(i));
        }

        drop(snap);

        // Apply right-to-left
        for pos in positions.into_iter().rev() {
            self.buffer.insert(pos, "    ");
        }
    }

    /// Decrease indentation of a range of lines by up to 4 spaces.
    pub fn outdent_lines(&mut self, start: usize, end: usize) {
        let snap = LineSnapshot::from(self.buffer.content());
        if start >= snap.line_count() || start > end {
            return;
        }
        let end = end.min(snap.line_count().saturating_sub(1));

        // Collect delete ranges (right-to-left)
        struct DeleteRange {
            start: usize,
            end: usize,
        }
        let mut ranges: Vec<DeleteRange> = Vec::new();
        for i in start..=end {
            let line = snap.line_text(i);
            let leading = line.len() - line.trim_start().len();
            if leading == 0 {
                continue;
            }
            let remove = leading.min(4);
            ranges.push(DeleteRange {
                start: snap.line_start(i),
                end: snap.line_start(i) + remove,
            });
        }

        drop(snap);

        // Apply right-to-left
        for r in ranges.into_iter().rev() {
            self.buffer.delete_range(r.start, r.end);
        }
    }

    // ── Undo / Redo ────────────────────────────────────────────────────

    /// Undo the last edit, delegating to the buffer.
    pub fn undo(&mut self) {
        self.buffer.undo();
    }

    /// Redo the last undone edit, delegating to the buffer.
    pub fn redo(&mut self) {
        self.buffer.redo();
    }

    // ── Summary ────────────────────────────────────────────────────────

    /// Get a human-readable summary of the buffer state.
    ///
    /// Example: `"Buffer has 142 lines, 3 unsaved changes"`
    pub fn get_summary(&self) -> String {
        let lines = self.buffer.line_count();
        let changes = self.buffer.undo_count();
        let dirty = if self.buffer.is_dirty() {
            format!(", {changes} unsaved changes")
        } else {
            ", saved".to_string()
        };
        format!("Buffer has {lines} lines{dirty}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_editor(content: &str) -> VoiceEditor {
        VoiceEditor::new(TextBuffer::new(content.to_string()))
    }

    #[test]
    fn test_insert_line_before_first() {
        let mut ed = make_editor("line1\nline2");
        ed.insert_line_before(0, "header");
        assert_eq!(ed.buffer.content(), "header\nline1\nline2");
    }

    #[test]
    fn test_insert_line_before_middle() {
        let mut ed = make_editor("line1\nline2\nline3");
        ed.insert_line_before(1, "inserted");
        assert_eq!(ed.buffer.content(), "line1\ninserted\nline2\nline3");
    }

    #[test]
    fn test_insert_line_after() {
        let mut ed = make_editor("line1\nline2");
        ed.insert_line_after(0, "after1");
        assert_eq!(ed.buffer.content(), "line1\nafter1\nline2");
    }

    #[test]
    fn test_insert_line_after_end() {
        let mut ed = make_editor("line1");
        ed.insert_line_after(0, "after");
        assert_eq!(ed.buffer.content(), "line1\nafter");
    }

    #[test]
    fn test_delete_line() {
        let mut ed = make_editor("a\nb\nc");
        ed.delete_line(1);
        assert_eq!(ed.buffer.content(), "a\nc");
    }

    #[test]
    fn test_delete_line_first() {
        let mut ed = make_editor("a\nb\nc");
        ed.delete_line(0);
        assert_eq!(ed.buffer.content(), "b\nc");
    }

    #[test]
    fn test_delete_line_last() {
        let mut ed = make_editor("a\nb\nc");
        ed.delete_line(2);
        assert_eq!(ed.buffer.content(), "a\nb");
    }

    #[test]
    fn test_delete_lines_range() {
        let mut ed = make_editor("a\nb\nc\nd\ne");
        ed.delete_lines(1, 3);
        assert_eq!(ed.buffer.content(), "a\ne");
    }

    #[test]
    fn test_duplicate_line() {
        let mut ed = make_editor("a\nb\nc");
        ed.duplicate_line(1);
        assert_eq!(ed.buffer.content(), "a\nb\nb\nc");
    }

    #[test]
    fn test_change_text() {
        let mut ed = make_editor("foo bar foo");
        let result = ed.change_text("foo", "baz");
        assert_eq!(result, "2 replacements made");
        assert_eq!(ed.buffer.content(), "baz bar baz");
    }

    #[test]
    fn test_change_text_no_match() {
        let mut ed = make_editor("hello world");
        let result = ed.change_text("zzz", "aaa");
        assert_eq!(result, "No occurrences of \"zzz\" found");
    }

    #[test]
    fn test_wrap_in_try_catch() {
        let mut ed = make_editor("    do_something();\n    do_other();");
        ed.wrap_in_try_catch(0, 1);
        let content = ed.buffer.content();
        assert!(content.contains("try {"));
        assert!(content.contains("} catch {"));
    }

    #[test]
    fn test_comment_lines() {
        let mut ed = make_editor("a\nb\nc");
        ed.comment_lines(0, 2);
        assert_eq!(ed.buffer.content(), "// a\n// b\n// c");
    }

    #[test]
    fn test_uncomment_lines() {
        let mut ed = make_editor("// a\n// b\n// c");
        ed.comment_lines(0, 2);
        assert_eq!(ed.buffer.content(), "a\nb\nc");
    }

    #[test]
    fn test_indent_lines() {
        let mut ed = make_editor("a\nb\nc");
        ed.indent_lines(0, 2);
        assert_eq!(ed.buffer.content(), "    a\n    b\n    c");
    }

    #[test]
    fn test_outdent_lines() {
        let mut ed = make_editor("    a\n    b\n    c");
        ed.outdent_lines(0, 2);
        assert_eq!(ed.buffer.content(), "a\nb\nc");
    }

    #[test]
    fn test_undo_redo() {
        let mut ed = make_editor("hello");
        ed.buffer.insert(5, " world");
        assert_eq!(ed.buffer.content(), "hello world");
        ed.undo();
        assert_eq!(ed.buffer.content(), "hello");
        ed.redo();
        assert_eq!(ed.buffer.content(), "hello world");
    }

    #[test]
    fn test_get_summary() {
        let mut ed = make_editor("line1\nline2");
        ed.buffer.insert(0, "x"); // make it dirty
        let summary = ed.get_summary();
        assert!(summary.contains("2 lines"));
        assert!(summary.contains("unsaved"));
    }

    #[test]
    fn test_extract_function() {
        let mut ed = make_editor("fn main() {\n    println!(\"hi\");\n    println!(\"there\");\n}");
        ed.extract_function(1, 2, "greet");
        let content = ed.buffer.content();
        assert!(content.contains("greet();"));
        assert!(content.contains("fn greet()"));
    }
}
