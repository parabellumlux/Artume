//! Tree-sitter based code parser and structure extractor.
//!
//! Parses source files into a `CodeTree` that captures:
//! - Functions, classes, methods with line ranges
//! - Nesting depth at each line
//! - Conditionals, loops, try-catch blocks
//! - Comments and blank lines
//! - Symbol definitions for LSP integration

use anyhow::{Context, Result};
use log::info;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// A single scope boundary in the code.
#[derive(Debug, Clone)]
pub struct ScopeBoundary {
    /// Type of boundary.
    pub kind: ScopeKind,
    /// Name (function name, class name, etc.)
    pub name: Option<String>,
    /// 0-indexed start line.
    pub start_line: usize,
    /// 0-indexed end line.
    pub end_line: usize,
    /// Nesting depth (0 = top-level).
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScopeKind {
    Module,
    Class,
    Function,
    Method,
    AsyncFunction,
    AsyncMethod,
    If,
    Else,
    For,
    While,
    Try,
    Catch,
    With,
    Match,
    Lambda,
}

/// A single line's structural information.
#[derive(Debug, Clone)]
pub struct LineInfo {
    /// 0-indexed line number.
    pub line: usize,
    /// Nesting depth at this line.
    pub depth: usize,
    /// Whether this line is a scope boundary start.
    pub is_boundary_start: bool,
    /// Whether this line is a scope boundary end.
    pub is_boundary_end: bool,
    /// The boundary at this line, if any.
    pub boundary: Option<ScopeBoundary>,
    /// Whether this line is a comment.
    pub is_comment: bool,
    /// Whether this line is blank.
    pub is_blank: bool,
    /// Number of tokens on this line (approximate density).
    pub token_count: usize,
}

/// The complete structural tree of a source file.
#[derive(Debug, Clone)]
pub struct CodeTree {
    /// File path.
    pub path: String,
    /// Language detected.
    pub language: String,
    /// Total lines.
    pub total_lines: usize,
    /// All scope boundaries, ordered by start line.
    pub boundaries: Vec<ScopeBoundary>,
    /// Per-line information.
    pub lines: Vec<LineInfo>,
    /// Top-level symbols (functions, classes) for quick navigation.
    pub symbols: Vec<Symbol>,
}

/// A navigable symbol in the code.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Class,
    Method,
    AsyncFunction,
}

/// Get a tree-sitter Language for a file extension.
fn language_for_extension(ext: &str) -> Option<tree_sitter::Language> {
    match ext {
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "js" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "ts" | "tsx" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "json" => Some(tree_sitter_json::LANGUAGE.into()),
        _ => None,
    }
}

/// Parse a source file and extract its code tree.
pub fn parse_file(path: &Path) -> Result<CodeTree> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let grammar = language_for_extension(ext)
        .ok_or_else(|| anyhow::anyhow!("Unsupported language: .{}", ext))?;

    let language_name = match ext {
        "py" => "python",
        "rs" => "rust",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        _ => "unknown",
    };

    let mut parser = Parser::new();
    parser
        .set_language(&grammar)
        .map_err(|e| anyhow::anyhow!("Failed to set language: {:?}", e))?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse file"))?;

    let root = tree.root_node();
    let total_lines = source.lines().count();

    let mut extractor = StructureExtractor::new(&source, language_name);
    extractor.walk(root, 0);

    let boundaries = extractor.boundaries;
    let symbols = extractor.symbols;
    let lines = build_line_info(total_lines, &boundaries, &source);

    info!(
        "Parsed {} ({}): {} lines, {} symbols, {} boundaries",
        path.display(),
        language_name,
        total_lines,
        symbols.len(),
        boundaries.len()
    );

    Ok(CodeTree {
        path: path.to_string_lossy().to_string(),
        language: language_name.to_string(),
        total_lines,
        boundaries,
        lines,
        symbols,
    })
}

/// Extracts structure by walking the tree-sitter CST.
struct StructureExtractor<'a> {
    source: &'a str,
    #[allow(dead_code)]
    language: &'a str,
    boundaries: Vec<ScopeBoundary>,
    symbols: Vec<Symbol>,
    depth: usize,
}

impl<'a> StructureExtractor<'a> {
    fn new(source: &'a str, language: &'a str) -> Self {
        Self {
            source,
            language,
            boundaries: Vec::new(),
            symbols: Vec::new(),
            depth: 0,
        }
    }

    fn walk(&mut self, node: Node, depth: usize) {
        let kind = node.kind();
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;

        // Detect scope boundaries based on node kind
        let scope_kind = match kind {
            "class_definition" | "class_declaration" => Some(ScopeKind::Class),
            "function_definition" | "function_declaration" => Some(ScopeKind::Function),
            "method_definition" => Some(ScopeKind::Method),
            "async_function_definition" => Some(ScopeKind::AsyncFunction),
            "if_statement" | "if_expression" => Some(ScopeKind::If),
            "else_clause" => Some(ScopeKind::Else),
            "for_statement" | "for_in_statement" => Some(ScopeKind::For),
            "while_statement" => Some(ScopeKind::While),
            "try_statement" | "try_expression" => Some(ScopeKind::Try),
            "catch_clause" | "except_clause" => Some(ScopeKind::Catch),
            "with_statement" => Some(ScopeKind::With),
            "match_statement" | "match_expression" => Some(ScopeKind::Match),
            _ => None,
        };

        if let Some(ref kind) = scope_kind {
            let name = self.extract_name(node);
            let boundary = ScopeBoundary {
                kind: kind.clone(),
                name: name.clone(),
                start_line,
                end_line,
                depth,
            };
            self.boundaries.push(boundary);

            // Track top-level symbols for navigation
            if depth == 0 || depth == 1 {
                let symbol_kind = match kind {
                    ScopeKind::Class => SymbolKind::Class,
                    ScopeKind::Function => SymbolKind::Function,
                    ScopeKind::Method => SymbolKind::Method,
                    ScopeKind::AsyncFunction => SymbolKind::AsyncFunction,
                    _ => {
                        // Recurse into children at increased depth
                        self.depth = depth + 1;
                        for i in 0..node.child_count() {
                            if let Some(child) = node.child(i as u32) {
                                self.walk(child, depth + 1);
                            }
                        }
                        self.depth = depth;
                        return;
                    }
                };
                if let Some(ref n) = name {
                    self.symbols.push(Symbol {
                        name: n.clone(),
                        kind: symbol_kind,
                        start_line,
                        end_line,
                        depth,
                    });
                }
            }

            // Recurse into children at increased depth
            self.depth = depth + 1;
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i as u32) {
                    self.walk(child, depth + 1);
                }
            }
            self.depth = depth;
        } else {
            // Not a scope boundary — recurse at same depth
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i as u32) {
                    self.walk(child, depth);
                }
            }
        }
    }

    /// Extract the name from a definition node.
    fn extract_name(&self, node: Node) -> Option<String> {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                let kind = child.kind();
                if matches!(kind, "identifier" | "name" | "property_identifier") {
                    if let Ok(text) = child.utf8_text(self.source.as_bytes()) {
                        return Some(text.to_string());
                    }
                }
            }
        }
        None
    }
}

/// Build per-line information from the extracted boundaries.
fn build_line_info(
    total_lines: usize,
    boundaries: &[ScopeBoundary],
    source: &str,
) -> Vec<LineInfo> {
    let lines: Vec<&str> = source.lines().collect();
    let mut info = Vec::with_capacity(total_lines);

    // Build a map of start/end lines to boundaries
    let mut start_map: HashMap<usize, Vec<&ScopeBoundary>> = HashMap::new();
    let mut end_map: HashMap<usize, Vec<&ScopeBoundary>> = HashMap::new();
    for b in boundaries {
        start_map.entry(b.start_line).or_default().push(b);
        end_map.entry(b.end_line).or_default().push(b);
    }

    // Track current depth as we walk through lines
    let mut current_depth = 0usize;
    let mut depth_stack: Vec<usize> = Vec::new();

    for line_num in 0..total_lines {
        // Check for scope starts on this line
        let mut boundary_start: Option<ScopeBoundary> = None;
        if let Some(starts) = start_map.get(&line_num) {
            if let Some(b) = starts.first() {
                boundary_start = Some((*b).clone());
                depth_stack.push(current_depth);
                current_depth = b.depth;
            }
        }

        // Check for scope ends on this line
        let mut boundary_end: Option<ScopeBoundary> = None;
        if let Some(ends) = end_map.get(&line_num) {
            if let Some(b) = ends.first() {
                boundary_end = Some((*b).clone());
                if let Some(prev_depth) = depth_stack.pop() {
                    current_depth = prev_depth;
                }
            }
        }

        let line_text = lines.get(line_num).unwrap_or(&"");
        let trimmed = line_text.trim();
        let is_comment =
            trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with("/*");
        let is_blank = trimmed.is_empty();
        let token_count = line_text.split_whitespace().count();

        info.push(LineInfo {
            line: line_num,
            depth: current_depth,
            is_boundary_start: boundary_start.is_some(),
            is_boundary_end: boundary_end.is_some(),
            boundary: boundary_start.or(boundary_end),
            is_comment,
            is_blank,
            token_count,
        });
    }

    info
}
