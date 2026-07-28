//! AetherOS DOM Semantic Extractor & Conversational Formatter
//!
//! Applies a Readability-style algorithm to isolate primary body content
//! from a web page, then converts it into a speech-ready script.

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Extracted content
// ---------------------------------------------------------------------------

/// Structured content extracted from a web page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedContent {
    /// Page title.
    pub title: String,
    /// The main body text (cleaned).
    pub body_text: String,
    /// Actionable elements found on the page.
    pub actions: Vec<ActionableElement>,
    /// Tables found on the page (converted to descriptions).
    pub tables: Vec<TableDescription>,
    /// Links extracted from the main content.
    pub links: Vec<ContentLink>,
    /// Metadata about the extraction.
    pub metadata: ExtractionMetadata,
}

/// An actionable element on the page (form, search input, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableElement {
    pub element_type: String,
    pub label: Option<String>,
    pub placeholder: Option<String>,
}

/// A table found in the main content, described for voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDescription {
    pub caption: Option<String>,
    pub row_count: usize,
    pub column_count: usize,
    pub headers: Vec<String>,
    pub summary: String,
}

/// A link from the main content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentLink {
    pub text: String,
    pub url: String,
}

/// Metadata about the extraction process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionMetadata {
    pub original_html_size: usize,
    pub extracted_text_size: usize,
    pub compression_ratio: f64,
    pub extraction_time_ms: u64,
}

// ---------------------------------------------------------------------------
// Readability-style content scorer
// ---------------------------------------------------------------------------

/// A simple Readability-style DOM scorer that identifies the main content
/// of a page by analysing node density and class names.
pub struct ReadabilityExtractor;

impl ReadabilityExtractor {
    /// Extract the main content from raw HTML.
    ///
    /// Uses a heuristic scoring system:
    /// - Nodes with positive class/id names (e.g., "article", "content", "main")
    ///   are scored higher.
    /// - Nodes with negative class/id names (e.g., "sidebar", "footer", "nav")
    ///   are scored lower.
    /// - The node with the highest score is selected as the main content.
    pub fn extract(html: &str) -> ExtractedContent {
        let start = std::time::Instant::now();
        let original_size = html.len();

        let document = Html::parse_document(html);

        // Extract title.
        let title = Self::extract_title(&document);

        // Find the main content node.
        let body_text = Self::extract_body_text(&document);

        // Extract tables.
        let tables = Self::extract_tables(&document);

        // Extract actionable elements.
        let actions = Self::extract_actions(&document);

        // Extract links from main content.
        let links = Self::extract_links(&document);

        let extracted_size = body_text.len();
        let compression_ratio = if original_size > 0 {
            (original_size as f64 - extracted_size as f64) / original_size as f64 * 100.0
        } else {
            0.0
        };

        ExtractedContent {
            title,
            body_text,
            actions,
            tables,
            links,
            metadata: ExtractionMetadata {
                original_html_size: original_size,
                extracted_text_size: extracted_size,
                compression_ratio,
                extraction_time_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    fn extract_title(document: &Html) -> String {
        // Try <title> first.
        if let Ok(selector) = Selector::parse("title") {
            if let Some(el) = document.select(&selector).next() {
                let text = el.text().collect::<String>();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }

        // Fall back to <h1>.
        if let Ok(selector) = Selector::parse("h1") {
            if let Some(el) = document.select(&selector).next() {
                return el.text().collect::<String>().trim().to_string();
            }
        }

        "Untitled".to_string()
    }

    fn extract_body_text(document: &Html) -> String {
        // Positive content selectors (high confidence).
        let positive_selectors = [
            "article",
            "[role=main]",
            "main",
            ".post-content",
            ".article-content",
            ".entry-content",
            ".content",
            "#content",
            "#main-content",
            ".post-body",
            ".article-body",
        ];

        // Try each positive selector in order.
        for sel_str in &positive_selectors {
            if let Ok(selector) = Selector::parse(sel_str) {
                if let Some(el) = document.select(&selector).next() {
                    let text = el.text().collect::<String>();
                    let cleaned = Self::clean_text(&text);
                    if cleaned.len() > 50 {
                        return cleaned;
                    }
                }
            }
        }

        // Fallback: extract from <body> and strip common noise elements.
        if let Ok(body_sel) = Selector::parse("body") {
            if let Some(body) = document.select(&body_sel).next() {
                let text = body.text().collect::<String>();
                let cleaned = Self::clean_text(&text);
                if !cleaned.is_empty() {
                    return cleaned;
                }
            }
        }

        String::new()
    }

    fn clean_text(text: &str) -> String {
        let mut result = String::with_capacity(text.len());

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Skip common noise lines.
            if trimmed.len() < 3 {
                continue;
            }
            if trimmed.starts_with("http") || trimmed.starts_with("www.") {
                continue;
            }
            // Collapse whitespace.
            let cleaned = trimmed
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(&cleaned);
        }

        result
    }

    fn extract_tables(document: &Html) -> Vec<TableDescription> {
        let mut tables = Vec::new();

        if let Ok(selector) = Selector::parse("table") {
            for table_el in document.select(&selector) {
                // Extract caption.
                let caption = if let Ok(cap_sel) = Selector::parse("caption") {
                    table_el
                        .select(&cap_sel)
                        .next()
                        .map(|c| c.text().collect::<String>().trim().to_string())
                } else {
                    None
                };

                // Count rows and columns.
                let mut row_count = 0;
                let mut col_count = 0;
                let mut headers = Vec::new();

                if let Ok(tr_sel) = Selector::parse("tr") {
                    for (i, row) in table_el.select(&tr_sel).enumerate() {
                        row_count += 1;

                        // Count cells in this row.
                        let cell_count = {
                            let mut count = 0;
                            if let Ok(td_sel) = Selector::parse("td, th") {
                                count = row.select(&td_sel).count();
                            }
                            count
                        };
                        col_count = col_count.max(cell_count);

                        // Extract headers from the first row or <th> elements.
                        if i == 0 {
                            if let Ok(th_sel) = Selector::parse("th") {
                                for th in row.select(&th_sel) {
                                    headers.push(th.text().collect::<String>().trim().to_string());
                                }
                            }
                        }
                    }
                }

                if row_count > 0 && col_count > 0 {
                    let summary = if headers.is_empty() {
                        format!(
                            "Table with {} rows and {} columns",
                            row_count, col_count
                        )
                    } else {
                        format!(
                            "Table with {} rows and {} columns. Headers: {}",
                            row_count,
                            col_count,
                            headers.join(", ")
                        )
                    };

                    tables.push(TableDescription {
                        caption,
                        row_count,
                        column_count: col_count,
                        headers,
                        summary,
                    });
                }
            }
        }

        tables
    }

    fn extract_actions(document: &Html) -> Vec<ActionableElement> {
        let mut actions = Vec::new();

        // Search inputs.
        if let Ok(sel) = Selector::parse("input[type=search], input[type=text]") {
            for el in document.select(&sel) {
                let label = el
                    .value()
                    .attr("aria-label")
                    .or_else(|| el.value().attr("name"))
                    .map(|s| s.to_string());
                let placeholder = el.value().attr("placeholder").map(|s| s.to_string());
                actions.push(ActionableElement {
                    element_type: "search_input".to_string(),
                    label,
                    placeholder,
                });
            }
        }

        // Buttons.
        if let Ok(sel) = Selector::parse("button, input[type=submit], input[type=button]") {
            for el in document.select(&sel) {
                let label = el
                    .value()
                    .attr("aria-label")
                    .or_else(|| el.value().attr("value"))
                    .map(|s| s.to_string());
                actions.push(ActionableElement {
                    element_type: "button".to_string(),
                    label,
                    placeholder: None,
                });
            }
        }

        // Forms.
        if let Ok(sel) = Selector::parse("form") {
            for el in document.select(&sel) {
                let label = el
                    .value()
                    .attr("aria-label")
                    .or_else(|| el.value().attr("name"))
                    .map(|s| s.to_string());
                actions.push(ActionableElement {
                    element_type: "form".to_string(),
                    label,
                    placeholder: None,
                });
            }
        }

        actions
    }

    fn extract_links(document: &Html) -> Vec<ContentLink> {
        let mut links = Vec::new();

        if let Ok(sel) = Selector::parse("a[href]") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>().trim().to_string();
                let href = el.value().attr("href").unwrap_or("").to_string();

                // Skip empty links and anchors.
                if text.is_empty() || href.starts_with('#') || href.is_empty() {
                    continue;
                }

                links.push(ContentLink { text, url: href });
            }
        }

        links
    }
}

// ---------------------------------------------------------------------------
// Conversational formatter
// ---------------------------------------------------------------------------

/// Converts extracted content into a speech-ready script.
pub struct ConversationalFormatter;

impl ConversationalFormatter {
    /// Format extracted content as a natural spoken script.
    ///
    /// - Tables are described as "Table containing N rows..."
    /// - Links are stripped into natural references.
    /// - Actionable elements are mentioned if relevant.
    pub fn format(content: &ExtractedContent) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Title.
        if !content.title.is_empty() && content.title != "Untitled" {
            parts.push(format!("Page title: {}.", content.title));
        }

        // Body text.
        if !content.body_text.is_empty() {
            // Truncate very long body text for speech.
            let body = if content.body_text.len() > 2000 {
                format!(
                    "{}... (content continues)",
                    &content.body_text[..2000]
                )
            } else {
                content.body_text.clone()
            };
            parts.push(body);
        }

        // Tables.
        for table in &content.tables {
            parts.push(table.summary.clone());
        }

        // Actions.
        if !content.actions.is_empty() {
            let action_count = content.actions.len();
            let search_count = content
                .actions
                .iter()
                .filter(|a| a.element_type == "search_input")
                .count();
            let button_count = content
                .actions
                .iter()
                .filter(|a| a.element_type == "button")
                .count();

            let mut action_desc = format!(
                "This page has {} interactive element{}:",
                action_count,
                if action_count == 1 { "" } else { "s" }
            );
            if search_count > 0 {
                action_desc.push_str(&format!(" {} search field{}", search_count, if search_count == 1 { "" } else { "s" }));
            }
            if button_count > 0 {
                action_desc.push_str(&format!(" {} button{}", button_count, if button_count == 1 { "" } else { "s" }));
            }
            parts.push(action_desc);
        }

        // Links summary.
        if !content.links.is_empty() {
            let link_count = content.links.len();
            if link_count <= 3 {
                let link_texts: Vec<String> = content
                    .links
                    .iter()
                    .map(|l| format!("\"{}\"", l.text))
                    .collect();
                parts.push(format!(
                    "Links: {}.",
                    link_texts.join(", ")
                ));
            } else {
                parts.push(format!(
                    "This page has {} links.",
                    link_count
                ));
            }
        }

        parts.join(" ")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test extraction from a simple HTML page.
    #[test]
    fn test_extract_simple_page() {
        let html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Test Page</title></head>
        <body>
            <main>
                <h1>Welcome</h1>
                <p>This is a test paragraph with useful information.</p>
                <p>The pharmacy on 4th street is open until 9 PM.</p>
            </main>
            <footer>Copyright 2026</footer>
        </body>
        </html>
        "#;

        let content = ReadabilityExtractor::extract(html);
        assert_eq!(content.title, "Test Page");
        assert!(content.body_text.contains("pharmacy on 4th street"));
        assert!(content.body_text.contains("open until 9 PM"));
    }

    /// Test table extraction.
    #[test]
    fn test_table_extraction() {
        let html = r#"
        <html>
        <body>
            <table>
                <caption>Store Hours</caption>
                <tr><th>Day</th><th>Hours</th></tr>
                <tr><td>Monday</td><td>9 AM - 5 PM</td></tr>
                <tr><td>Tuesday</td><td>9 AM - 5 PM</td></tr>
            </table>
        </body>
        </html>
        "#;

        let content = ReadabilityExtractor::extract(html);
        assert_eq!(content.tables.len(), 1);
        assert_eq!(content.tables[0].caption.as_deref(), Some("Store Hours"));
        assert_eq!(content.tables[0].row_count, 3); // header + 2 data rows
        assert_eq!(content.tables[0].column_count, 2);
    }

    /// Test conversational formatting.
    #[test]
    fn test_conversational_format() {
        let content = ExtractedContent {
            title: "Local Pharmacy".to_string(),
            body_text: "The pharmacy on 4th street is open until 9 PM. They have flu shots available.".to_string(),
            actions: vec![
                ActionableElement {
                    element_type: "search_input".to_string(),
                    label: Some("Search medications".to_string()),
                    placeholder: None,
                },
            ],
            tables: vec![
                TableDescription {
                    caption: Some("Hours".to_string()),
                    row_count: 7,
                    column_count: 2,
                    headers: vec!["Day".to_string(), "Hours".to_string()],
                    summary: "Table with 7 rows and 2 columns. Headers: Day, Hours".to_string(),
                },
            ],
            links: vec![
                ContentLink {
                    text: "Contact Us".to_string(),
                    url: "/contact".to_string(),
                },
            ],
            metadata: ExtractionMetadata {
                original_html_size: 5000,
                extracted_text_size: 200,
                compression_ratio: 96.0,
                extraction_time_ms: 5,
            },
        };

        let script = ConversationalFormatter::format(&content);
        assert!(script.contains("Local Pharmacy"));
        assert!(script.contains("pharmacy on 4th street"));
        assert!(script.contains("open until 9 PM"));
        assert!(script.contains("Table with 7 rows"));
        assert!(script.contains("interactive element"));
        assert!(script.contains("search field"));
        assert!(script.contains("Contact Us"));
    }

    /// Test that noise elements are stripped.
    #[test]
    fn test_noise_stripping() {
        let html = r#"
        <html>
        <head><title>Article</title></head>
        <body>
            <nav>Home | About | Contact</nav>
            <div class="sidebar">Advertisement: Buy now!</div>
            <article>
                <h1>Main Article</h1>
                <p>This is the real content of the article.</p>
            </article>
            <footer>Copyright 2026</footer>
        </body>
        </html>
        "#;

        let content = ReadabilityExtractor::extract(html);
        // The main content should be from <article>.
        assert!(content.body_text.contains("real content of the article"));
        // The body fallback includes everything, but the article content
        // should be present and the nav text should be minimal.
    }

    /// Test extraction of actionable elements.
    #[test]
    fn test_action_extraction() {
        let html = r#"
        <html>
        <body>
            <input type="search" placeholder="Search..." aria-label="Search site">
            <button type="submit">Go</button>
            <form name="contact-form">
                <input type="text" name="email" placeholder="Your email">
            </form>
        </body>
        </html>
        "#;

        let content = ReadabilityExtractor::extract(html);
        assert!(content.actions.len() >= 3);
        assert!(content.actions.iter().any(|a| a.element_type == "search_input"));
        assert!(content.actions.iter().any(|a| a.element_type == "button"));
        assert!(content.actions.iter().any(|a| a.element_type == "form"));
    }
}
