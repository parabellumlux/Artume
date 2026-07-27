pub mod magic;
pub mod onnx;

use std::path::Path;
pub use magic::classify_magic;
pub use onnx::OnnxClassifier;

/// Top-level classification interface.
/// Determines a file category and content tags using Magic Bytes, falling back to ONNX if available.
pub struct FileClassifier {
    onnx: OnnxClassifier,
}

impl FileClassifier {
    pub fn new<P: AsRef<Path>>(model_dir: P) -> Self {
        Self {
            onnx: OnnxClassifier::new(model_dir),
        }
    }

    /// Classification summary. Returns (category, details).
    /// e.g. ("image/png", "visual embedding calculated"), or ("text/plain", "classified: technology")
    pub fn classify(&self, path: &Path) -> (String, String) {
        let magic_res = classify_magic(path).unwrap_or("unknown");
        
        if magic_res.starts_with("text/") {
            if self.onnx.has_fasttext() {
                if let Ok(text) = std::fs::read_to_string(path) {
                    if let Some(topic) = self.onnx.classify_text(&text) {
                        return (magic_res.to_string(), format!("topic: {}", topic));
                    }
                }
            }
            return (magic_res.to_string(), "raw text".to_string());
        }

        if magic_res.starts_with("image/") {
            if self.onnx.has_mobileclip() {
                if let Some(features) = self.onnx.classify_image(path) {
                    return (magic_res.to_string(), format!("image features (dim: {})", features.len()));
                }
            }
            return (magic_res.to_string(), "raw image".to_string());
        }

        (magic_res.to_string(), "binary".to_string())
    }

    /// Generate embeddings for textual content (or file names).
    pub fn get_text_embedding(&self, text: &str) -> Option<Vec<f32>> {
        self.onnx.generate_embedding(text)
    }
}
