use std::path::Path;

/// Engine for matching paths against exclusion rules.
pub struct PathFilter {
    // Exact folder names to ignore anywhere in the path (e.g., "node_modules", ".git")
    ignored_folder_names: Vec<&'static str>,
    // Absolute paths prefix to ignore (e.g., "/proc", "/sys")
    ignored_prefixes: Vec<&'static str>,
    // File extensions to ignore (e.g., "dll", "so", "exe", "pyc")
    ignored_extensions: Vec<&'static str>,
}

impl Default for PathFilter {
    fn default() -> Self {
        Self {
            ignored_folder_names: vec![
                "node_modules",
                "target",
                ".git",
                "venv",
                ".venv",
                ".cargo",
                "build",
                "dist",
                ".idea",
                ".vscode",
                "System Volume Information",
                "$RECYCLE.BIN",
            ],
            ignored_prefixes: vec![
                "/proc",
                "/sys",
                "/dev",
                "/run",
                "/sys",
                "/tmp",
                "/var/tmp",
                "/lost+found",
            ],
            ignored_extensions: vec![
                "dll", "so", "exe", "pyc", "dylib", "o", "a", "lib", 
                "bin", "msi", "dmg", "iso", "class", "pdb", "suo",
            ],
        }
    }
}

impl PathFilter {
    /// Create a default PathFilter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a given file or directory path should be excluded.
    pub fn should_exclude(&self, path: &Path) -> bool {
        // Convert to absolute or clean representation
        let path_str = path.to_string_lossy();

        // 1. Check absolute prefixes
        for prefix in &self.ignored_prefixes {
            if path_str.starts_with(prefix) {
                return true;
            }
        }

        // Windows specific system exclusions
        #[cfg(target_os = "windows")]
        {
            let lower_path = path_str.to_lowercase();
            if lower_path.starts_with("c:\\windows") || lower_path.starts_with("c:\\program files") {
                return true;
            }
        }

        // 2. Check individual components (folder names)
        for component in path.components() {
            if let Some(comp_str) = component.as_os_str().to_str() {
                if self.ignored_folder_names.contains(&comp_str) {
                    return true;
                }
            }
        }

        // 3. Check extensions
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if self.ignored_extensions.contains(&ext_lower.as_str()) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_exclusions() {
        let filter = PathFilter::new();

        assert!(filter.should_exclude(&PathBuf::from("/proc/cpuinfo")));
        assert!(filter.should_exclude(&PathBuf::from("/home/user/project/node_modules/index.js")));
        assert!(filter.should_exclude(&PathBuf::from("/home/user/project/target/debug/app")));
        assert!(filter.should_exclude(&PathBuf::from("/home/user/app.exe")));
        assert!(filter.should_exclude(&PathBuf::from("/home/user/library.so")));

        assert!(!filter.should_exclude(&PathBuf::from("/home/user/documents/report.pdf")));
        assert!(!filter.should_exclude(&PathBuf::from("/home/user/src/main.rs")));
    }
}
