//! Text buffer with undo/redo support for the audio-first IDE.
//!
//! Provides a rope-like text buffer that tracks edits for undo/redo,
//! supports file I/O, and exposes line-oriented access for voice-driven
//! editing commands.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// A single edit operation, stored for undo/redo.
#[derive(Debug, Clone)]
pub struct Edit {
    /// The text that was removed (empty for insertions).
    pub old_text: String,
    /// The text that was added (empty for deletions).
    pub new_text: String,
    /// The byte offset where the edit starts.
    pub start: usize,
    /// The byte offset where the edit ends (before the edit was applied).
    pub end: usize,
}

/// A text buffer supporting undo/redo, file I/O, and line-based access.
///
/// Internally stores content as a flat `String` and records every
/// mutation as an `Edit` on the undo stack.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    /// The full text content.
    content: String,
    /// Optional file path this buffer was loaded from / saved to.
    path: Option<String>,
    /// Stack of undone edits (most recent first).
    undo_stack: Vec<Edit>,
    /// Stack of redone edits (most recent first).
    redo_stack: Vec<Edit>,
    /// Whether the buffer has unsaved changes.
    dirty: bool,
}

impl TextBuffer {
    /// Create a new buffer from a string, with no associated file path.
    pub fn new(content: String) -> Self {
        Self {
            content,
            path: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            dirty: false,
        }
    }

    /// Load a file from disk into a new buffer.
    ///
    /// The buffer's `path` is set to the canonicalised file path.
    pub fn load(path: &str) -> Result<Self> {
        let resolved = Path::new(path);
        let content =
            fs::read_to_string(resolved).with_context(|| format!("Failed to read file: {path}"))?;
        let canonical = resolved.canonicalize().ok();
        let path_str = canonical
            .as_deref()
            .unwrap_or(resolved)
            .to_string_lossy()
            .to_string();

        Ok(Self {
            content,
            path: Some(path_str),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            dirty: false,
        })
    }

    /// Save the buffer contents to its associated file path.
    ///
    /// Returns an error if no path has been set.
    pub fn save(&self) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No file path set for this buffer"))?;
        fs::write(path, &self.content).with_context(|| format!("Failed to write file: {path}"))?;
        Ok(())
    }

    /// Get a specific line (0-indexed).
    ///
    /// Returns `None` if the line index is out of bounds.
    pub fn get_line(&self, line: usize) -> Option<&str> {
        self.content.lines().nth(line)
    }

    /// Get a range of lines as a single string (0-indexed, inclusive of both ends).
    ///
    /// Lines are joined with newlines. If `end` is beyond the last line,
    /// returns all available lines from `start` onward.
    pub fn get_lines(&self, start: usize, end: usize) -> String {
        let lines: Vec<&str> = self.content.lines().collect();
        let start = start.min(lines.len().saturating_sub(1));
        let end = end.min(lines.len().saturating_sub(1));
        if start > end {
            return String::new();
        }
        lines[start..=end].join("\n")
    }

    /// Insert text at a byte position.
    ///
    /// Records the operation on the undo stack and clears the redo stack.
    pub fn insert(&mut self, pos: usize, text: &str) {
        let pos = pos.min(self.content.len());
        let edit = Edit {
            old_text: String::new(),
            new_text: text.to_string(),
            start: pos,
            end: pos,
        };
        self.content.insert_str(pos, text);
        self.undo_stack.push(edit);
        self.redo_stack.clear();
        self.dirty = true;
    }

    /// Delete a range of bytes.
    ///
    /// Records the operation on the undo stack and clears the redo stack.
    /// Clamps `start` and `end` to valid bounds.
    pub fn delete_range(&mut self, start: usize, end: usize) {
        let len = self.content.len();
        let start = start.min(len);
        let end = end.min(len);
        if start >= end {
            return;
        }
        let removed = self.content[start..end].to_string();
        let edit = Edit {
            old_text: removed,
            new_text: String::new(),
            start,
            end,
        };
        self.content.drain(start..end);
        self.undo_stack.push(edit);
        self.redo_stack.clear();
        self.dirty = true;
    }

    /// Replace a range of bytes with new text.
    ///
    /// Records the operation on the undo stack and clears the redo stack.
    /// Clamps `start` and `end` to valid bounds.
    pub fn replace(&mut self, start: usize, end: usize, text: &str) {
        let len = self.content.len();
        let start = start.min(len);
        let end = end.min(len);
        if start > end {
            return;
        }
        let removed = self.content[start..end].to_string();
        let edit = Edit {
            old_text: removed,
            new_text: text.to_string(),
            start,
            end,
        };
        self.content.drain(start..end);
        self.content.insert_str(start, text);
        self.undo_stack.push(edit);
        self.redo_stack.clear();
        self.dirty = true;
    }

    /// Find all non-overlapping occurrences of `old` and replace them with `new`.
    ///
    /// Returns the number of replacements made. Each replacement is recorded
    /// as a single composite edit on the undo stack.
    pub fn find_and_replace(&mut self, old: &str, new: &str) -> usize {
        if old.is_empty() {
            return 0;
        }

        let mut count = 0;
        let mut pos = 0;
        let mut replacements = Vec::new();

        // Collect all replacement positions first (to avoid borrow issues)
        let _bytes = self.content.as_bytes();
        while pos < self.content.len() {
            if let Some(found) = self.content[pos..].find(old) {
                let abs_pos = pos + found;
                replacements.push((abs_pos, abs_pos + old.len()));
                pos = abs_pos + old.len();
                count += 1;
            } else {
                break;
            }
        }

        if count == 0 {
            return 0;
        }

        // Apply replacements right-to-left so positions stay valid
        let old_text = if count == 1 {
            // Single replacement — record the exact old text
            let (s, e) = replacements[0];
            self.content[s..e].to_string()
        } else {
            // Composite: record the full old range
            let first_start = replacements[0].0;
            let last_end = match replacements.last() {
                Some(&(_, e)) => e,
                None => return 0,
            };
            self.content[first_start..last_end].to_string()
        };

        let start = replacements[0].0;
        let end = match replacements.last() {
            Some(&(_, e)) => e,
            None => return 0,
        };

        for &(s, e) in replacements.iter().rev() {
            self.content.drain(s..e);
            self.content.insert_str(s, new);
        }

        let edit = Edit {
            old_text,
            new_text: new.repeat(count),
            start,
            end,
        };
        self.undo_stack.push(edit);
        self.redo_stack.clear();
        self.dirty = true;

        count
    }

    /// Undo the last edit operation.
    ///
    /// Does nothing if the undo stack is empty.
    pub fn undo(&mut self) {
        let edit = match self.undo_stack.pop() {
            Some(e) => e,
            None => return,
        };

        // Reverse the edit
        if edit.new_text.is_empty() {
            // Was a deletion — re-insert the old text
            self.content.insert_str(edit.start, &edit.old_text);
        } else if edit.old_text.is_empty() {
            // Was an insertion — remove the new text
            let end = edit.start + edit.new_text.len();
            self.content.drain(edit.start..end);
        } else {
            // Was a replacement — put back the old text
            let end = edit.start + edit.new_text.len();
            self.content.drain(edit.start..end);
            self.content.insert_str(edit.start, &edit.old_text);
        }

        self.redo_stack.push(edit);
        self.dirty = true;
    }

    /// Redo the last undone edit operation.
    ///
    /// Does nothing if the redo stack is empty.
    pub fn redo(&mut self) {
        let edit = match self.redo_stack.pop() {
            Some(e) => e,
            None => return,
        };

        // Re-apply the edit
        if edit.old_text.is_empty() {
            // Was an insertion — re-insert
            self.content.insert_str(edit.start, &edit.new_text);
        } else if edit.new_text.is_empty() {
            // Was a deletion — re-delete
            self.content.drain(edit.start..edit.end);
        } else {
            // Was a replacement — re-replace
            let end = edit.start + edit.old_text.len();
            self.content.drain(edit.start..end);
            self.content.insert_str(edit.start, &edit.new_text);
        }

        self.undo_stack.push(edit);
        self.dirty = true;
    }

    /// Return the number of lines in the buffer.
    ///
    /// An empty buffer has 1 line. Lines are separated by `\n`.
    pub fn line_count(&self) -> usize {
        if self.content.is_empty() {
            1
        } else {
            self.content.lines().count()
        }
    }

    /// Return the total number of characters (bytes) in the buffer.
    pub fn total_chars(&self) -> usize {
        self.content.len()
    }

    /// Return a reference to the full content string.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Return the associated file path, if any.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Set the file path for this buffer.
    pub fn set_path(&mut self, path: String) {
        self.path = Some(path);
    }

    /// Whether the buffer has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the buffer as clean (e.g. after saving).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Return the number of entries on the undo stack.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Return the number of entries on the redo stack.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let buf = TextBuffer::new("hello\nworld".to_string());
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.total_chars(), 11);
        assert_eq!(buf.get_line(0), Some("hello"));
        assert_eq!(buf.get_line(1), Some("world"));
        assert_eq!(buf.get_line(2), None);
    }

    #[test]
    fn test_empty_buffer_line_count() {
        let buf = TextBuffer::new(String::new());
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.total_chars(), 0);
    }

    #[test]
    fn test_insert() {
        let mut buf = TextBuffer::new("helo".to_string());
        buf.insert(3, "l");
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_delete_range() {
        let mut buf = TextBuffer::new("hello world".to_string());
        buf.delete_range(5, 6);
        assert_eq!(buf.content(), "helloworld");
    }

    #[test]
    fn test_replace() {
        let mut buf = TextBuffer::new("hello world".to_string());
        buf.replace(6, 11, "there");
        assert_eq!(buf.content(), "hello there");
    }

    #[test]
    fn test_find_and_replace() {
        let mut buf = TextBuffer::new("foo bar foo baz".to_string());
        let count = buf.find_and_replace("foo", "qux");
        assert_eq!(count, 2);
        assert_eq!(buf.content(), "qux bar qux baz");
    }

    #[test]
    fn test_undo_insert() {
        let mut buf = TextBuffer::new("helo".to_string());
        buf.insert(3, "l");
        assert_eq!(buf.content(), "hello");
        buf.undo();
        assert_eq!(buf.content(), "helo");
    }

    #[test]
    fn test_undo_redo() {
        let mut buf = TextBuffer::new("abc".to_string());
        buf.insert(3, "def");
        assert_eq!(buf.content(), "abcdef");
        buf.undo();
        assert_eq!(buf.content(), "abc");
        buf.redo();
        assert_eq!(buf.content(), "abcdef");
    }

    #[test]
    fn test_undo_delete() {
        let mut buf = TextBuffer::new("hello world".to_string());
        buf.delete_range(5, 11);
        assert_eq!(buf.content(), "hello");
        buf.undo();
        assert_eq!(buf.content(), "hello world");
    }

    #[test]
    fn test_get_lines() {
        let buf = TextBuffer::new("a\nb\nc\nd".to_string());
        assert_eq!(buf.get_lines(0, 1), "a\nb");
        assert_eq!(buf.get_lines(1, 2), "b\nc");
        assert_eq!(buf.get_lines(2, 5), "c\nd");
    }

    #[test]
    fn test_redo_stack_cleared_on_new_edit() {
        let mut buf = TextBuffer::new("abc".to_string());
        buf.insert(1, "X");
        buf.undo();
        assert_eq!(buf.redo_count(), 1);
        buf.insert(1, "Y");
        assert_eq!(buf.redo_count(), 0);
    }

    #[test]
    fn test_find_and_replace_no_match() {
        let mut buf = TextBuffer::new("hello world".to_string());
        let count = buf.find_and_replace("zzz", "aaa");
        assert_eq!(count, 0);
        assert_eq!(buf.content(), "hello world");
    }

    #[test]
    fn test_find_and_replace_empty_old() {
        let mut buf = TextBuffer::new("hello".to_string());
        let count = buf.find_and_replace("", "x");
        assert_eq!(count, 0);
    }
}
