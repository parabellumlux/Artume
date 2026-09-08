//! Project-level file and symbol indexing for the audio IDE.
//!
//! Provides directory scanning, fuzzy file lookup, symbol search,
//! tree rendering, and file-system watching for a project workspace.

use anyhow::{Context, Result};
use log::{info, warn};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for a project scan.
#[derive(Debug, Clone)]
pub struct ProjectConfig {
    /// Root directory of the project.
    pub root_dir: PathBuf,
    /// Directory names to skip during scanning.
    pub excluded_dirs: Vec<String>,
    /// File extensions (without leading dot) that are considered source files.
    pub supported_extensions: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from("."),
            excluded_dirs: vec![
                "target".into(),
                ".git".into(),
                "node_modules".into(),
                ".venv".into(),
                "__pycache__".into(),
                ".build".into(),
                "dist".into(),
                ".next".into(),
            ],
            supported_extensions: vec![
                "rs".into(),
                "py".into(),
                "js".into(),
                "ts".into(),
                "tsx".into(),
                "go".into(),
                "json".into(),
                "toml".into(),
                "yaml".into(),
                "yml".into(),
                "md".into(),
                "html".into(),
                "css".into(),
                "scss".into(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Metadata about a single file in the project.
#[derive(Debug, Clone)]
pub struct ProjectFile {
    /// Absolute or relative path to the file.
    pub path: PathBuf,
    /// Detected language (derived from extension).
    pub language: String,
    /// Number of lines in the file.
    pub line_count: usize,
    /// Last modification time, if available.
    pub last_modified: Option<SystemTime>,
}

/// A symbol (function, class, etc.) found in a project file.
#[derive(Debug, Clone)]
pub struct ProjectSymbol {
    /// Name of the symbol.
    pub name: String,
    /// Kind of symbol (e.g. "function", "class", "method").
    pub kind: String,
    /// Path of the file that contains this symbol.
    pub file_path: PathBuf,
    /// 0-indexed start line.
    pub start_line: usize,
    /// 0-indexed end line.
    pub end_line: usize,
}

/// The main project index, holding scanned files and symbols.
pub struct ProjectIndex {
    /// Root directory of the project.
    pub root: PathBuf,
    /// All indexed files.
    pub files: Vec<ProjectFile>,
    /// All indexed symbols.
    pub symbols: Vec<ProjectSymbol>,
    /// Optional file-system watcher (active after `watch_project` is called).
    pub watcher: Option<RecommendedWatcher>,
    /// Receiver for watcher events.
    watcher_rx: Option<Mutex<mpsc::Receiver<Result<Event, notify::Error>>>>,
}

impl ProjectIndex {
    /// Create a new, empty project index rooted at `root`.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: Vec::new(),
            symbols: Vec::new(),
            watcher: None,
            watcher_rx: None,
        }
    }

    /// Create a new project index and immediately scan the directory.
    pub fn scan(config: &ProjectConfig) -> Result<Self> {
        let mut index = Self::new(config.root_dir.clone());
        index.scan_project(config)?;
        Ok(index)
    }

    // -----------------------------------------------------------------------
    // Scanning
    // -----------------------------------------------------------------------

    /// Walk the project directory tree, collect source files, and (optionally)
    /// extract symbols.  Skips directories listed in `config.excluded_dirs`.
    pub fn scan_project(&mut self, config: &ProjectConfig) -> Result<()> {
        let root = &config.root_dir;
        if !root.is_dir() {
            anyhow::bail!("Project root is not a directory: {}", root.display());
        }

        let mut files: Vec<ProjectFile> = Vec::new();
        let mut symbols: Vec<ProjectSymbol> = Vec::new();

        self.walk_dir(root, root, config, &mut files, &mut symbols)?;

        info!(
            "Scanned project {}: {} files, {} symbols",
            root.display(),
            files.len(),
            symbols.len()
        );

        self.files = files;
        self.symbols = symbols;
        Ok(())
    }

    /// Recursive directory walker.
    fn walk_dir(
        &self,
        base: &Path,
        dir: &Path,
        config: &ProjectConfig,
        files: &mut Vec<ProjectFile>,
        _symbols: &mut Vec<ProjectSymbol>,
    ) -> Result<()> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Cannot read directory {}: {}", dir.display(), e);
                return Ok(());
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Error reading entry in {}: {}", dir.display(), e);
                    continue;
                }
            };

            let path = entry.path();

            // Skip excluded directories
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if config.excluded_dirs.iter().any(|d| d == name) {
                        continue;
                    }
                }
                self.walk_dir(base, &path, config, files, _symbols)?;
                continue;
            }

            // Only process regular files
            if !path.is_file() {
                continue;
            }

            // Check extension
            let ext = match path.extension().and_then(|e| e.to_str()) {
                Some(e) => e,
                None => continue,
            };

            if !config.supported_extensions.iter().any(|s| s == ext) {
                continue;
            }

            // Derive language from extension
            let language = language_from_extension(ext);

            // Line count
            let line_count = count_lines(&path).unwrap_or(0);

            // Last modified
            let last_modified = path.metadata().ok().and_then(|m| m.modified().ok());

            let relative = path.strip_prefix(base).unwrap_or(&path).to_path_buf();

            files.push(ProjectFile {
                path: relative,
                language,
                line_count,
                last_modified,
            });
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Fuzzy file search
    // -----------------------------------------------------------------------

    /// Find files whose path contains `name` (case-insensitive).
    ///
    /// Results are sorted by relevance:
    /// 1. Exact match (file name equals `name`)
    /// 2. Prefix match (file name starts with `name`)
    /// 3. Substring match (file name contains `name`)
    /// 4. Path substring match (any part of the path contains `name`)
    pub fn fuzzy_find_file(&self, name: &str) -> Vec<&ProjectFile> {
        if name.is_empty() {
            return Vec::new();
        }

        let lower = name.to_lowercase();
        let mut scored: Vec<(usize, &ProjectFile)> = Vec::new();

        for file in &self.files {
            let path_str = file.path.to_string_lossy().to_lowercase();
            let file_name = file
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default();

            // Score: lower is better
            let score = if file_name == lower {
                0 // exact match
            } else if file_name.starts_with(&lower) {
                1 // prefix match
            } else if file_name.contains(&lower) {
                2 // substring in file name
            } else if path_str.contains(&lower) {
                3 // substring anywhere in path
            } else {
                continue; // no match
            };

            scored.push((score, file));
        }

        // Stable sort by score (relevance)
        scored.sort_by_key(|(score, _)| *score);

        scored.into_iter().map(|(_, f)| f).collect()
    }

    // -----------------------------------------------------------------------
    // Symbol search
    // -----------------------------------------------------------------------

    /// Find symbols whose name contains `name` (case-insensitive).
    pub fn find_symbol(&self, name: &str) -> Vec<&ProjectSymbol> {
        if name.is_empty() {
            return Vec::new();
        }

        let lower = name.to_lowercase();
        self.symbols
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&lower))
            .collect()
    }

    // -----------------------------------------------------------------------
    // File tree rendering
    // -----------------------------------------------------------------------

    /// Build a tree-shaped text representation of the project suitable for
    /// audio rendering (e.g. spoken navigation or sonification).
    ///
    /// Format:
    /// ```text
    /// project_name/
    /// ├── src/
    /// │   ├── main.rs
    /// │   └── lib.rs
    /// └── Cargo.toml
    /// ```
    pub fn get_file_tree(&self) -> String {
        if self.files.is_empty() {
            return String::new();
        }

        // Build a tree from the relative paths
        let root_name = self
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        let mut tree_lines: Vec<String> = Vec::new();
        tree_lines.push(format!("{}/", root_name));

        // Collect all directory prefixes
        let mut dirs: Vec<PathBuf> = Vec::new();
        for file in &self.files {
            if let Some(parent) = file.path.parent() {
                if !parent.as_os_str().is_empty() && !dirs.contains(&parent.to_path_buf()) {
                    dirs.push(parent.to_path_buf());
                }
            }
        }
        dirs.sort();

        // Collect all file paths
        let mut file_paths: Vec<&PathBuf> = self.files.iter().map(|f| &f.path).collect();
        file_paths.sort();

        // Build a set of all paths (dirs + files) for quick lookup
        let all_paths: std::collections::HashSet<&PathBuf> = file_paths.iter().copied().collect();

        // Render the tree
        render_tree_nodes(
            &PathBuf::from(""),
            &dirs,
            &file_paths,
            &all_paths,
            &mut tree_lines,
        );

        tree_lines.join("\n")
    }

    // -----------------------------------------------------------------------
    // File-system watching
    // -----------------------------------------------------------------------

    /// Start watching the project root for file changes.
    ///
    /// When a file is created or deleted the project is automatically
    /// re-scanned.  The watcher runs on a background thread; call
    /// `poll_watcher()` periodically (or from an async task) to process
    /// events and trigger re-scans.
    pub fn watch_project(&mut self, _config: &ProjectConfig) -> Result<()> {
        let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();

        let mut watcher = RecommendedWatcher::new(tx, Config::default())
            .context("Failed to create file system watcher")?;

        watcher
            .watch(&self.root, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch directory: {}", self.root.display()))?;

        info!("Watching project directory: {}", self.root.display());

        self.watcher = Some(watcher);
        self.watcher_rx = Some(Mutex::new(rx));

        Ok(())
    }

    /// Poll the watcher for pending events and re-scan if files changed.
    ///
    /// Returns `true` if a re-scan was triggered.
    pub fn poll_watcher(&mut self, config: &ProjectConfig) -> Result<bool> {
        let mut needs_rescan = false;

        // Drain all available events (non-blocking). Scope the locked receiver
        // so the guard is dropped before the re-scan below.
        {
            let rx = match &self.watcher_rx {
                Some(rx) => rx.lock(),
                None => return Ok(false),
            };

            while let Ok(event) = rx.try_recv() {
                match event {
                    Ok(event) => {
                        let kind = &event.kind;
                        match kind {
                            EventKind::Create(_) | EventKind::Remove(_) => {
                                info!(
                                    "File system change detected ({:?}): {:?}",
                                    kind, event.paths
                                );
                                needs_rescan = true;
                            }
                            _ => {
                                // Ignore modify and other events for now
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Watcher error: {}", e);
                    }
                }
            }
        }

        if needs_rescan {
            info!("Re-scanning project after file system change");
            self.scan_project(config)?;
        }

        Ok(needs_rescan)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Derive a human-readable language name from a file extension.
fn language_from_extension(ext: &str) -> String {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        "html" => "html",
        "css" => "css",
        "scss" => "scss",
        _ => "unknown",
    }
    .to_string()
}

/// Count the number of lines in a file.
fn count_lines(path: &Path) -> Option<usize> {
    let content = std::fs::read_to_string(path).ok()?;
    Some(content.lines().count())
}

/// Recursively render tree nodes into `lines`.
fn render_tree_nodes(
    prefix: &Path,
    dirs: &[PathBuf],
    files: &[&PathBuf],
    _all_paths: &std::collections::HashSet<&PathBuf>,
    lines: &mut Vec<String>,
) {
    // Collect immediate children of this prefix
    let mut child_dirs: Vec<&PathBuf> = Vec::new();
    let mut child_files: Vec<&PathBuf> = Vec::new();

    for d in dirs {
        if d.parent() == Some(prefix) {
            child_dirs.push(d);
        }
    }

    for f in files {
        if f.parent() == Some(prefix) {
            child_files.push(f);
        }
    }

    child_dirs.sort();
    child_files.sort();

    let total_children = child_dirs.len() + child_files.len();

    for (i, dir) in child_dirs.iter().enumerate() {
        let is_last = i + 1 == total_children;
        let connector = if is_last { "└── " } else { "├── " };
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        lines.push(format!(
            "{}{}{}/",
            "    ".repeat(prefix.components().count()),
            connector,
            dir_name
        ));

        // Recurse into this directory
        let child_prefix = (*dir).clone();
        render_tree_nodes(&child_prefix, dirs, files, _all_paths, lines);
    }

    for (i, file) in child_files.iter().enumerate() {
        let dir_offset = child_dirs.len();
        let is_last = i + dir_offset + 1 == total_children;
        let connector = if is_last { "└── " } else { "├── " };
        let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        lines.push(format!(
            "{}{}{}",
            "    ".repeat(prefix.components().count()),
            connector,
            file_name
        ));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn create_temp_project() -> (tempfile::TempDir, ProjectConfig) {
        let dir = tempfile::tempdir().unwrap();

        // Create some source files
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();

        let mut main_rs = fs::File::create(src.join("main.rs")).unwrap();
        writeln!(main_rs, "fn main() {{}}").unwrap();

        let mut lib_rs = fs::File::create(src.join("lib.rs")).unwrap();
        writeln!(lib_rs, "pub fn hello() {{}}").unwrap();

        let mut parser_rs = fs::File::create(src.join("parser.rs")).unwrap();
        writeln!(parser_rs, "pub fn parse() {{}}").unwrap();

        // Create a file in root
        let mut cargo = fs::File::create(dir.path().join("Cargo.toml")).unwrap();
        writeln!(cargo, "[package]\nname = \"test\"").unwrap();

        // Create an excluded dir that should be skipped
        let target = dir.path().join("target");
        fs::create_dir_all(&target).unwrap();
        let mut _build = fs::File::create(target.join("build.rs")).unwrap();

        let config = ProjectConfig {
            root_dir: dir.path().to_path_buf(),
            excluded_dirs: vec!["target".into()],
            supported_extensions: vec!["rs".into(), "toml".into()],
        };

        (dir, config)
    }

    #[test]
    fn test_scan_project() {
        let (_dir, config) = create_temp_project();
        let index = ProjectIndex::scan(&config).unwrap();

        assert_eq!(
            index.files.len(),
            4,
            "Should find 4 files (3 .rs + 1 .toml)"
        );
        assert!(
            index.files.iter().any(|f| f.path.ends_with("main.rs")),
            "Should contain main.rs"
        );
        assert!(
            index.files.iter().any(|f| f.path.ends_with("Cargo.toml")),
            "Should contain Cargo.toml"
        );
        // target/ should be excluded
        assert!(
            !index
                .files
                .iter()
                .any(|f| f.path.to_string_lossy().contains("target")),
            "Should not contain files from excluded dirs"
        );
    }

    #[test]
    fn test_fuzzy_find_file() {
        let (_dir, config) = create_temp_project();
        let index = ProjectIndex::scan(&config).unwrap();

        // Exact match
        let results = index.fuzzy_find_file("parser.rs");
        assert!(!results.is_empty(), "Should find parser.rs");
        assert!(results[0].path.ends_with("parser.rs"));

        // Prefix match
        let results = index.fuzzy_find_file("parse");
        assert!(!results.is_empty(), "Should find files matching 'parse'");
        assert!(results[0].path.ends_with("parser.rs"));

        // Substring match
        let results = index.fuzzy_find_file("main");
        assert!(!results.is_empty(), "Should find files matching 'main'");
        assert!(results[0].path.ends_with("main.rs"));

        // No match
        let results = index.fuzzy_find_file("nonexistent");
        assert!(results.is_empty(), "Should return empty for no match");
    }

    #[test]
    fn test_get_file_tree() {
        let (_dir, config) = create_temp_project();
        let index = ProjectIndex::scan(&config).unwrap();

        let tree = index.get_file_tree();
        assert!(!tree.is_empty(), "Tree should not be empty");
        assert!(tree.contains("src/"), "Tree should contain src/");
        assert!(tree.contains("main.rs"), "Tree should contain main.rs");
        assert!(tree.contains("parser.rs"), "Tree should contain parser.rs");
        assert!(
            tree.contains("Cargo.toml"),
            "Tree should contain Cargo.toml"
        );
    }

    #[test]
    fn test_find_symbol_empty() {
        let (_dir, config) = create_temp_project();
        let index = ProjectIndex::scan(&config).unwrap();

        // No symbols were extracted (we don't parse in scan_project)
        let results = index.find_symbol("main");
        assert!(
            results.is_empty(),
            "Should be empty since no symbols extracted"
        );
    }

    #[test]
    fn test_empty_index() {
        let index = ProjectIndex::new(PathBuf::from("/nonexistent"));
        assert!(index.files.is_empty());
        assert!(index.symbols.is_empty());
        assert!(index.fuzzy_find_file("test").is_empty());
        assert!(index.find_symbol("test").is_empty());
        assert!(index.get_file_tree().is_empty());
    }
}
