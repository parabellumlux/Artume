//! AetherFS Content Extraction & Chunking
//!
//! Extracts readable text from files (PDF, DOCX, code, plain text, etc.),
//! splits into semantic chunks, and generates embeddings for each chunk.
//! This enables RAG — the conversational layer can search file *content*,
//! not just filenames.

use std::path::Path;

/// A single chunk of extracted content with metadata.
#[derive(Debug, Clone)]
pub struct ContentChunk {
    /// The file this chunk came from.
    pub source_path: String,
    /// Chunk index within the file (0-based).
    pub chunk_index: usize,
    /// The extracted text content.
    pub text: String,
    /// Character offset in the original document.
    pub char_start: usize,
    /// Character length of this chunk.
    pub char_len: usize,
}

/// Extract text content from a file based on its type.
/// Returns None for binary/unreadable files.
pub fn extract_text(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        // Plain text formats
        "txt" | "md" | "markdown" | "rst" | "log" | "cfg" | "ini" | "conf" | "toml"
        | "yaml" | "yml" | "json" | "xml" | "csv" | "tsv" => {
            std::fs::read_to_string(path).ok()
        }

        // Code files
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "go" | "java" | "c" | "cpp" | "h"
        | "hpp" | "rb" | "php" | "swift" | "kt" | "scala" | "clj" | "cljs" | "elm"
        | "hs" | "lua" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "sql"
        | "r" | "m" | "mm" | "pl" | "pm" | "t" | "dart" | "zig" | "nim" | "ex"
        | "exs" | "cr" | "svelte" | "vue" | "css" | "scss" | "less" | "html" | "htm"
        | "dockerfile" | "makefile" | "cmake" | "gradle" | "proto" | "graphql" | "gql" => {
            std::fs::read_to_string(path).ok()
        }

        // PDF — requires pdf-extract or similar
        "pdf" => extract_pdf_text(path),

        // Office documents
        "docx" => extract_docx_text(path),
        "doc" => {
            // .doc is harder — skip for now
            None
        }
        "xlsx" | "xls" => {
            // Spreadsheets — skip for now
            None
        }
        "pptx" | "ppt" => {
            // Presentations — skip for now
            None
        }

        // Everything else is binary
        _ => None,
    }
}

/// Extract text from a PDF file using pdf-extract.
fn extract_pdf_text(path: &Path) -> Option<String> {
    // Try pdf-extract first, fall back to basic extraction
    #[cfg(feature = "pdf-extract")]
    {
        match pdf_extract::extract_text(path) {
            Ok(text) if !text.trim().is_empty() => return Some(text),
            _ => {}
        }
    }

    // Fallback: try to read raw text from PDF (works for simple PDFs)
    std::fs::read_to_string(path)
        .ok()
        .map(|s| {
            s.lines()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty()
                        && !t.starts_with("%")
                        && !t.starts_with("<<")
                        && !t.starts_with(">>")
                        && !t.starts_with("/")
                        && !t.starts_with("endobj")
                        && !t.starts_with("obj")
                        && !t.starts_with("stream")
                        && !t.starts_with("endstream")
                        && !t.starts_with("xref")
                        && !t.starts_with("trailer")
                        && !t.starts_with("startxref")
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|s| s.len() > 50) // Only keep if we got meaningful content
}

/// Extract text from a .docx file.
fn extract_docx_text(path: &Path) -> Option<String> {
    // .docx is a ZIP of XML. Try to extract text from word/document.xml
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    // Try the main document body
    if let Ok(mut doc) = archive.by_name("word/document.xml") {
        let mut content = String::new();
        doc.read_to_string(&mut content).ok()?;

        // Strip XML tags to get text content
        let text = strip_xml_tags(&content);
        if !text.trim().is_empty() {
            return Some(text);
        }
    }

    // Fallback: try header/footer
    for name in &["word/header1.xml", "word/footer1.xml"] {
        if let Ok(mut part) = archive.by_name(name) {
            let mut content = String::new();
            part.read_to_string(&mut content).ok()?;
            let text = strip_xml_tags(&content);
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }

    None
}

/// Crude XML tag stripper for DOCX extraction.
fn strip_xml_tags(xml: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_entity = false;
    let mut entity_buf = String::new();

    for c in xml.chars() {
        match c {
            '<' => {
                in_tag = true;
                // Add space between blocks
                if !result.ends_with(' ') && !result.is_empty() {
                    result.push(' ');
                }
            }
            '>' => {
                in_tag = false;
            }
            '&' if !in_tag => {
                in_entity = true;
                entity_buf.clear();
            }
            ';' if in_entity => {
                in_entity = false;
                match entity_buf.as_str() {
                    "lt" => result.push('<'),
                    "gt" => result.push('>'),
                    "amp" => result.push('&'),
                    "quot" => result.push('"'),
                    "apos" => result.push('\''),
                    _ => {} // skip unknown entities
                }
            }
            _ if !in_tag && !in_entity => {
                result.push(c);
            }
            _ if in_entity => {
                entity_buf.push(c);
            }
            _ => {}
        }
    }

    // Collapse whitespace
    let mut cleaned = String::with_capacity(result.len());
    let mut prev_space = false;
    for c in result.chars() {
        if c.is_whitespace() {
            if !prev_space {
                cleaned.push(' ');
                prev_space = true;
            }
        } else {
            cleaned.push(c);
            prev_space = false;
        }
    }

    cleaned.trim().to_string()
}

/// Split extracted text into chunks of approximately `target_chars` characters,
/// breaking at sentence boundaries when possible.
pub fn chunk_text(text: &str, target_chars: usize) -> Vec<(usize, String)> {
    if text.len() <= target_chars {
        return vec![(0, text.to_string())];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let mut chunk_idx = 0;

    while start < text.len() {
        let end = if start + target_chars >= text.len() {
            text.len()
        } else {
            // Try to break at a sentence boundary near the target
            let search_start = (start + target_chars).saturating_sub(target_chars / 4);
            let search_end = (start + target_chars + target_chars / 4).min(text.len());

            // Look for sentence-ending punctuation followed by space or newline
            let slice = &text[search_start..search_end];
            let mut break_pos = None;

            for (i, pat) in [". ", "! ", "? ", ".\n", "!\n", "?\n", "\n\n"].iter().enumerate() {
                if let Some(pos) = slice.rfind(pat) {
                    let candidate = search_start + pos + pat.len();
                    // Prefer later breaks (closer to target)
                    if candidate > break_pos.unwrap_or(0) {
                        break_pos = Some(candidate);
                    }
                }
            }

            // If no sentence break found, try last space
            if break_pos.is_none() {
                if let Some(pos) = slice.rfind(' ') {
                    break_pos = Some(search_start + pos + 1);
                }
            }

            // If still nothing, hard break at target
            break_pos.unwrap_or(start + target_chars)
        };

        let chunk_text = text[start..end].trim().to_string();
        if !chunk_text.is_empty() {
            chunks.push((chunk_idx, chunk_text));
            chunk_idx += 1;
        }

        start = end;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_plain_text() {
        let dir = std::env::temp_dir().join("aetherfs_test_extract");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        std::fs::write(&path, "Hello, this is a test file.").unwrap();

        let result = extract_text(&path);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Hello"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_chunk_text_small() {
        let text = "Short text.";
        let chunks = chunk_text(text, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].1, "Short text.");
    }

    #[test]
    fn test_chunk_text_large() {
        let text = "First sentence. Second sentence. Third sentence. Fourth sentence. Fifth sentence. ";
        let chunks = chunk_text(text, 30);
        assert!(chunks.len() >= 2, "Should produce multiple chunks, got {}", chunks.len());
        // Each chunk should be non-empty
        for (_, chunk) in &chunks {
            assert!(!chunk.is_empty());
        }
    }

    #[test]
    fn test_chunk_text_sentence_boundary() {
        let text = "This is a long paragraph that should break at a sentence boundary. Here is the second sentence. And here is a third one for good measure.";
        let chunks = chunk_text(text, 60);
        assert!(chunks.len() >= 2);
        // First chunk should end with a complete sentence
        assert!(chunks[0].1.ends_with('.') || chunks[0].1.ends_with('!') || chunks[0].1.ends_with('?'));
    }

    #[test]
    fn test_strip_xml_tags() {
        let xml = r#"<w:p><w:r><w:t>Hello World</w:t></w:r></w:p>"#;
        let text = strip_xml_tags(xml);
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn test_extract_code_file() {
        let dir = std::env::temp_dir().join("aetherfs_test_code");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.rs");
        std::fs::write(&path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let result = extract_text(&path);
        assert!(result.is_some());
        assert!(result.unwrap().contains("fn main"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
