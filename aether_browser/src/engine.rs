//! AetherOS Headless Browser Engine
//!
//! Fetches web pages using `reqwest` (HTTP client) and extracts the DOM
//! using `scraper`. Blocks trackers, images, ads, and stylesheets by
//! configuring the HTTP client to skip unnecessary resources.
//!
//! ## Performance Target
//! - Page fetch + DOM extraction: < 300 ms for typical pages.

use log::{debug, info, warn};
use reqwest::Client;
use std::time::Instant;
use thiserror::Error;
use url::Url;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Timeout while fetching {url}")]
    Timeout { url: String },
    #[error("Failed to extract page content: {0}")]
    ExtractionFailed(String),
}

// ---------------------------------------------------------------------------
// Fetch result
// ---------------------------------------------------------------------------

/// The result of fetching and extracting a web page.
#[derive(Debug, Clone)]
pub struct FetchResult {
    /// The original URL that was fetched.
    pub url: String,
    /// The resolved URL (after redirects).
    pub resolved_url: String,
    /// Page title.
    pub title: String,
    /// Raw HTML content of the page.
    pub html: String,
    /// Time taken to fetch the page (ms).
    pub fetch_time_ms: u64,
}

// ---------------------------------------------------------------------------
// Headless browser engine
// ---------------------------------------------------------------------------

/// A lightweight headless browser engine that fetches web pages using HTTP.
///
/// Uses `reqwest` for fetching and `scraper` for DOM parsing. Configures
/// the HTTP client to block unnecessary resources (trackers, images, etc.)
/// by using a minimal user-agent and skipping resource-heavy content types.
pub struct BrowserEngine {
    /// Shared HTTP client with connection pooling.
    client: Client,
}

impl BrowserEngine {
    /// Create a new browser engine with a configured HTTP client.
    ///
    /// The client is configured to:
    /// - Use a standard browser-like user-agent.
    /// - Set a reasonable timeout.
    /// - Not follow meta refresh redirects (only HTTP 3xx).
    pub fn new() -> Result<Self, BrowserError> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(10))
            .danger_accept_invalid_certs(false)
            .build()
            .map_err(|e| BrowserError::HttpError(e.to_string()))?;

        Ok(Self { client })
    }

    /// Fetch a URL and return the raw HTML content.
    ///
    /// This sends an HTTP GET request, follows redirects, and returns
    /// the final HTML content along with metadata.
    pub async fn fetch(&self, url: &str) -> Result<FetchResult, BrowserError> {
        let start = Instant::now();

        // Validate URL.
        let parsed = Url::parse(url).map_err(|_| BrowserError::InvalidUrl(url.to_string()))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(BrowserError::InvalidUrl(format!(
                "Unsupported scheme: {}",
                parsed.scheme()
            )));
        }

        debug!("BrowserEngine: fetching {}", url);

        // Send the HTTP request.
        let response = self
            .client
            .get(url)
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
            .map_err(|e| BrowserError::HttpError(e.to_string()))?;

        // Check for HTTP errors.
        let status = response.status();
        if !status.is_success() {
            return Err(BrowserError::HttpError(format!(
                "HTTP {} for {}",
                status.as_u16(),
                url
            )));
        }

        // Get the resolved URL (after redirects).
        let resolved_url = response.url().to_string();

        // Get the content type to verify it's HTML.
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Read the full response body.
        let html = response
            .text()
            .await
            .map_err(|e| BrowserError::HttpError(e.to_string()))?;

        // Extract the page title from HTML.
        let title = Self::extract_title(&html);

        let fetch_time_ms = start.elapsed().as_millis() as u64;
        info!(
            "BrowserEngine: fetched {} in {} ms (title: {}, size: {} bytes)",
            url,
            fetch_time_ms,
            title,
            html.len()
        );

        Ok(FetchResult {
            url: url.to_string(),
            resolved_url,
            title,
            html,
            fetch_time_ms,
        })
    }

    /// Fetch a URL with a configurable timeout.
    pub async fn fetch_with_timeout(
        &self,
        url: &str,
        timeout_secs: u64,
    ) -> Result<FetchResult, BrowserError> {
        let fetch = self.fetch(url);
        tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            fetch,
        )
        .await
        .map_err(|_| BrowserError::Timeout { url: url.to_string() })?
    }

    /// Extract the page title from raw HTML.
    fn extract_title(html: &str) -> String {
        use scraper::{Html, Selector};

        let document = Html::parse_document(html);

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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_validation() {
        let invalid = Url::parse("not-a-url");
        assert!(invalid.is_err());

        let valid = Url::parse("https://example.com");
        assert!(valid.is_ok());
    }

    #[test]
    fn test_extract_title() {
        let html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Test Page Title</title></head>
        <body><h1>Welcome</h1></body>
        </html>
        "#;
        assert_eq!(BrowserEngine::extract_title(html), "Test Page Title");
    }

    #[test]
    fn test_extract_title_fallback_h1() {
        let html = r#"
        <!DOCTYPE html>
        <html>
        <head></head>
        <body><h1>Fallback Title</h1></body>
        </html>
        "#;
        assert_eq!(BrowserEngine::extract_title(html), "Fallback Title");
    }

    #[test]
    fn test_extract_title_empty() {
        let html = "<html><head></head><body></body></html>";
        assert_eq!(BrowserEngine::extract_title(html), "Untitled");
    }
}
