//! Live test for BrowserEngine::search — run with:
//!   cargo run -p aether-browser --example search_test
use aether_browser::BrowserEngine;

#[tokio::main]
async fn main() {
    let engine = BrowserEngine::new().unwrap();
    match engine.search("latest AI news", 5).await {
        Ok(results) => {
            println!("Got {} results:", results.len());
            for (i, r) in results.iter().enumerate() {
                println!("  {}. {} -> {}", i + 1, r.title, r.url);
            }
        }
        Err(e) => println!("ERROR: {e}"),
    }
}
