//! AetherOS Headless Semantic Web Browser
//!
//! Converts complex websites into clean, conversational audio streams.
//!
//! ## Architecture
//! ```text
//! [Raw Web Page URL]
//!         |
//!         v (Headless Browser Render)
//! [DOM Tree Extraction]
//!         |
//!         v (Readability / Noise Filter)
//! [Clean Semantic Text]
//!         |
//!         v (Conversational Formatter)
//! [Voice-Ready Stream]
//! ```

pub mod engine;
pub mod extractor;

pub use engine::{BrowserEngine, BrowserError, FetchResult};
pub use extractor::{
    ActionableElement, ContentLink, ConversationalFormatter, ExtractedContent,
    ExtractionMetadata, ReadabilityExtractor, TableDescription,
};
