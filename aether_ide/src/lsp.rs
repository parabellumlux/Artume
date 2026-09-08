//! Language Server Protocol (LSP) client module.
//!
//! Provides an async LSP client that communicates with any LSP server over
//! stdio using JSON-RPC. Supports the core language intelligence features
//! needed by the audio-first IDE: go-to-definition, hover info, completions,
//! diagnostics, document symbols, references, rename, and formatting.

use anyhow::{Context, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, Mutex};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Configuration mapping language identifiers to LSP server commands.
///
/// # Example
///
/// ```ignore
/// let config = LspConfig::new()
///     .register("python", &["pylsp"])
///     .register("rust", &["rust-analyzer"]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct LspConfig {
    /// Maps language id → server command + args.
    pub servers: HashMap<String, Vec<String>>,
}

impl LspConfig {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    /// Register a language server command.
    pub fn register(mut self, language: &str, command: &[&str]) -> Self {
        self.servers.insert(
            language.to_string(),
            command.iter().map(|s| s.to_string()).collect(),
        );
        self
    }

    /// Get the server command for a language, if registered.
    pub fn command_for(&self, language: &str) -> Option<&[String]> {
        self.servers.get(language).map(|v| v.as_slice())
    }
}

/// A diagnostic message from the language server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDiagnostic {
    /// Severity: 1=error, 2=warning, 3=info, 4=hint.
    pub severity: u8,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Source file URI.
    pub file: String,
    /// 0-indexed line number.
    pub line: usize,
    /// 0-indexed column number.
    pub column: usize,
}

/// A source-code location (file + position).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspLocation {
    /// Source file URI.
    pub file: String,
    /// 0-indexed line number.
    pub line: usize,
    /// 0-indexed column number.
    pub column: usize,
}

/// A completion item returned by the language server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspCompletionItem {
    /// Display label.
    pub label: String,
    /// Optional detail (e.g. type signature).
    pub detail: Option<String>,
    /// Completion item kind (1=Text, 2=Method, 3=Function, 4=Constructor,
    /// 5=Field, 6=Variable, 7=Class, 8=Interface, 9=Module, 10=Property,
    /// 11=Unit, 12=Value, 13=Enum, 14=Keyword, 15=Snippet, 16=Color,
    /// 17=File, 18=Reference, 19=Folder, 20=EnumMember, 21=Constant,
    /// 22=Struct, 23=Event, 24=Operator, 25=TypeParameter).
    pub kind: Option<u32>,
}

/// A document symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSymbol {
    /// Symbol name.
    pub name: String,
    /// Symbol kind (same numbering as completion kinds).
    pub kind: u32,
    /// Source file URI.
    pub file: String,
    /// 0-indexed line number of the definition.
    pub line: usize,
    /// 0-indexed column number of the definition.
    pub column: usize,
}

/// A text edit produced by formatting or rename.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspTextEdit {
    /// 0-indexed start line.
    pub start_line: usize,
    /// 0-indexed start column.
    pub start_column: usize,
    /// 0-indexed end line.
    pub end_line: usize,
    /// 0-indexed end column.
    pub end_column: usize,
    /// Replacement text.
    pub new_text: String,
}

/// A workspace edit returned by rename.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspWorkspaceEdit {
    /// Changes keyed by file URI.
    pub changes: HashMap<String, Vec<LspTextEdit>>,
}

// ---------------------------------------------------------------------------
// Shared state between the client and the background reader task
// ---------------------------------------------------------------------------

/// Shared state accessible by both the LspClient and the background reader.
struct LspShared {
    /// Pending requests keyed by message ID. The reader task completes these
    /// when a response arrives.
    pending: HashMap<u64, oneshot::Sender<Result<Value>>>,
    /// Diagnostics store keyed by file URI.
    diagnostics: HashMap<String, Vec<LspDiagnostic>>,
}

// ---------------------------------------------------------------------------
// LspClient
// ---------------------------------------------------------------------------

/// An async LSP client that communicates with a language server over stdio.
///
/// # Lifecycle
///
/// 1. Create with [`LspClient::spawn`], which starts the server process and
///    performs the LSP initialize handshake.
/// 2. Call methods like [`go_to_definition`], [`hover`], etc.
/// 3. Call [`shutdown`] to cleanly stop the server.
///
/// A background task reads the server's stdout, dispatching responses to
/// pending requests and storing diagnostics notifications.
pub struct LspClient {
    /// The child process handle (dropped on shutdown).
    _process: Option<Child>,
    /// Stdin pipe to the server.
    stdin: ChildStdin,
    /// Join handle for the background reader task.
    reader_handle: tokio::task::JoinHandle<()>,
    /// Next request ID.
    next_id: u64,
    /// Shared state (pending requests + diagnostics).
    shared: Arc<Mutex<LspShared>>,
}

impl LspClient {
    /// Spawn a language server and perform the LSP initialize handshake.
    ///
    /// `command` is the server executable and its arguments (e.g.
    /// `&["rust-analyzer"]`).
    pub async fn spawn(command: &[String]) -> Result<Self> {
        info!("Spawning LSP server: {:?}", command);

        let mut child = Command::new(&command[0])
            .args(&command[1..])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn LSP server process")?;

        let stdin = child.stdin.take().context("Failed to open stdin")?;
        let stdout = child.stdout.take().context("Failed to open stdout")?;

        let shared = Arc::new(Mutex::new(LspShared {
            pending: HashMap::new(),
            diagnostics: HashMap::new(),
        }));

        let mut client = Self {
            _process: Some(child),
            stdin,
            reader_handle: tokio::task::spawn(Self::reader_loop(stdout, shared.clone())),
            next_id: 1,
            shared: shared.clone(),
        };

        // Wait for reader loop to be ready, then perform initialize handshake.
        client.initialize().await?;

        Ok(client)
    }

    // ------------------------------------------------------------------
    // Public API methods
    // ------------------------------------------------------------------

    /// Go to definition at the given position.
    ///
    /// Returns the location of the definition, or `None` if not found.
    pub async fn go_to_definition(
        &mut self,
        file_uri: &str,
        line: usize,
        column: usize,
    ) -> Result<Option<LspLocation>> {
        let params = json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": line, "character": column },
        });
        let result = self.send_request("textDocument/definition", params).await?;

        // Response can be a single Location or an array of Locations.
        if let Some(single) = result.as_object() {
            Ok(Some(location_from_value(single, file_uri)))
        } else if let Some(arr) = result.as_array() {
            if let Some(first) = arr.first().and_then(|v| v.as_object()) {
                Ok(Some(location_from_value(first, file_uri)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Get hover information at the given position.
    ///
    /// Returns a tuple of `(type_info, docstring)` where either may be empty.
    pub async fn hover(
        &mut self,
        file_uri: &str,
        line: usize,
        column: usize,
    ) -> Result<(String, String)> {
        let params = json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": line, "character": column },
        });
        let result = self.send_request("textDocument/hover", params).await?;

        let mut type_info = String::new();
        let mut docstring = String::new();

        if let Some(contents) = result.get("contents") {
            parse_hover_contents(contents, &mut type_info, &mut docstring);
        }

        Ok((type_info, docstring))
    }

    /// Get completions at the given position.
    pub async fn completions(
        &mut self,
        file_uri: &str,
        line: usize,
        column: usize,
    ) -> Result<Vec<LspCompletionItem>> {
        let params = json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": line, "character": column },
            "context": { "triggerKind": 1 },
        });
        let result = self.send_request("textDocument/completion", params).await?;

        let mut items = Vec::new();

        // Response can be a CompletionList or an array of CompletionItems.
        if let Some(list) = result.get("items").and_then(|v| v.as_array()) {
            for item in list {
                if let Some(item) = parse_completion_item(item) {
                    items.push(item);
                }
            }
        } else if let Some(arr) = result.as_array() {
            for item in arr {
                if let Some(item) = parse_completion_item(item) {
                    items.push(item);
                }
            }
        }

        Ok(items)
    }

    /// Get the current diagnostics for a file.
    ///
    /// Diagnostics are accumulated from `textDocument/publishDiagnostics`
    /// notifications sent by the server.
    pub async fn diagnostics(&self, file_uri: &str) -> Vec<LspDiagnostic> {
        let shared = self.shared.lock().await;
        shared
            .diagnostics
            .get(file_uri)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all stored diagnostics, keyed by file URI.
    pub async fn all_diagnostics(&self) -> HashMap<String, Vec<LspDiagnostic>> {
        let shared = self.shared.lock().await;
        shared.diagnostics.clone()
    }

    /// Get document symbols for a file.
    pub async fn document_symbols(&mut self, file_uri: &str) -> Result<Vec<LspSymbol>> {
        let params = json!({
            "textDocument": { "uri": file_uri },
        });
        let result = self
            .send_request("textDocument/documentSymbol", params)
            .await?;

        let mut symbols = Vec::new();

        if let Some(arr) = result.as_array() {
            for item in arr {
                if let Some(sym) = parse_symbol(item, file_uri) {
                    symbols.push(sym);
                }
            }
        }

        Ok(symbols)
    }

    /// Find all references to the symbol at the given position.
    pub async fn references(
        &mut self,
        file_uri: &str,
        line: usize,
        column: usize,
    ) -> Result<Vec<LspLocation>> {
        let params = json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": line, "character": column },
            "context": { "includeDeclaration": true },
        });
        let result = self.send_request("textDocument/references", params).await?;

        let mut locations = Vec::new();
        if let Some(arr) = result.as_array() {
            for loc in arr {
                if let Some(obj) = loc.as_object() {
                    locations.push(location_from_value(obj, file_uri));
                }
            }
        }

        Ok(locations)
    }

    /// Rename the symbol at the given position.
    ///
    /// Returns a workspace edit with changes across all affected files.
    pub async fn rename(
        &mut self,
        file_uri: &str,
        line: usize,
        column: usize,
        new_name: &str,
    ) -> Result<LspWorkspaceEdit> {
        let params = json!({
            "textDocument": { "uri": file_uri },
            "position": { "line": line, "character": column },
            "newName": new_name,
        });
        let result = self.send_request("textDocument/rename", params).await?;

        let mut changes: HashMap<String, Vec<LspTextEdit>> = HashMap::new();

        if let Some(changes_obj) = result.get("changes").and_then(|v| v.as_object()) {
            for (uri, edits) in changes_obj {
                let mut text_edits = Vec::new();
                if let Some(edits_arr) = edits.as_array() {
                    for edit in edits_arr {
                        if let Some(te) = parse_text_edit(edit) {
                            text_edits.push(te);
                        }
                    }
                }
                changes.insert(uri.clone(), text_edits);
            }
        }

        Ok(LspWorkspaceEdit { changes })
    }

    /// Format the entire document.
    ///
    /// Returns a list of text edits to apply.
    pub async fn formatting(&mut self, file_uri: &str) -> Result<Vec<LspTextEdit>> {
        let params = json!({
            "textDocument": { "uri": file_uri },
            "options": {
                "tabSize": 4,
                "insertSpaces": true,
            },
        });
        let result = self.send_request("textDocument/formatting", params).await?;

        let mut edits = Vec::new();
        if let Some(arr) = result.as_array() {
            for edit in arr {
                if let Some(te) = parse_text_edit(edit) {
                    edits.push(te);
                }
            }
        }

        Ok(edits)
    }

    /// Shut down the language server gracefully.
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down LSP server");

        // Send shutdown request.
        let _ = self.send_request("shutdown", json!(null)).await;

        // Send exit notification.
        let exit_msg = build_message(None::<u64>, "exit", json!(null));
        if let Err(e) = self.write_message(&exit_msg).await {
            warn!("Failed to send exit notification: {}", e);
        }

        // Close stdin to signal EOF.
        let _ = self.stdin.shutdown().await;

        // Wait for the process to exit.
        if let Some(mut process) = self._process.take() {
            let _ = process.wait().await;
        }

        // Abort the reader task.
        self.reader_handle.abort();

        Ok(())
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Perform the LSP initialize handshake.
    async fn initialize(&mut self) -> Result<()> {
        info!("Sending LSP initialize request");

        let params = json!({
            "processId": null,
            "capabilities": {},
            "rootUri": null,
        });

        let _result = self.send_request("initialize", params).await?;
        info!("LSP server initialized successfully");

        // Send initialized notification.
        let notif = build_message(None::<u64>, "initialized", json!({}));
        self.write_message(&notif).await?;

        Ok(())
    }

    /// Send a JSON-RPC request and await the response.
    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let (tx, rx) = oneshot::channel();

        // Register the pending request in shared state.
        {
            let mut shared = self.shared.lock().await;
            shared.pending.insert(id, tx);
        }

        let msg = build_message(Some(id), method, params);
        self.write_message(&msg).await?;

        // Await the response from the reader task.
        rx.await.context("LSP server closed before responding")?
    }

    /// Write a JSON-RPC message to the server's stdin.
    async fn write_message(&mut self, msg: &str) -> Result<()> {
        let header = format!("Content-Length: {}\r\n\r\n", msg.len());
        self.stdin
            .write_all(header.as_bytes())
            .await
            .context("Failed to write LSP header")?;
        self.stdin
            .write_all(msg.as_bytes())
            .await
            .context("Failed to write LSP body")?;
        self.stdin
            .flush()
            .await
            .context("Failed to flush LSP stdin")?;
        Ok(())
    }

    /// Background reader loop that processes server stdout.
    ///
    /// Reads Content-Length framed JSON-RPC messages, dispatches responses
    /// to pending requests, and stores diagnostics notifications.
    async fn reader_loop(stdout: ChildStdout, shared: Arc<Mutex<LspShared>>) {
        let mut reader = BufReader::new(stdout);
        let mut buf = String::new();

        loop {
            buf.clear();

            // Read the header line: "Content-Length: N\r\n"
            match reader.read_line(&mut buf).await {
                Ok(0) => {
                    info!("LSP server stdout closed");
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("Error reading LSP header: {}", e);
                    break;
                }
            };

            let content_length = match parse_content_length(&buf) {
                Some(n) => n,
                None => {
                    warn!("Failed to parse Content-Length from: {:?}", buf);
                    continue;
                }
            };

            // Read the blank line "\r\n".
            buf.clear();
            if reader.read_line(&mut buf).await.is_err() {
                break;
            }

            // Read the JSON body.
            let mut body = vec![0u8; content_length];
            if let Err(e) = reader.read_exact(&mut body).await {
                warn!("Error reading LSP body: {}", e);
                break;
            }

            let text = String::from_utf8_lossy(&body);
            let msg: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Failed to parse LSP message: {} — body: {}", e, text);
                    continue;
                }
            };

            // Dispatch based on whether it's a response or notification.
            if msg.get("id").is_some() && msg.get("method").is_none() {
                // Response to a request.
                let id = msg["id"].as_u64().unwrap_or(0);
                let result = if let Some(error) = msg.get("error") {
                    let code = error["code"].as_i64().unwrap_or(-1);
                    let message = error["message"].as_str().unwrap_or("unknown error");
                    Err(anyhow::anyhow!("LSP error {}: {}", code, message))
                } else {
                    Ok(msg.get("result").cloned().unwrap_or(Value::Null))
                };

                // Complete the pending request via shared state.
                let mut shared = shared.lock().await;
                if let Some(sender) = shared.pending.remove(&id) {
                    let _ = sender.send(result);
                } else {
                    warn!("Received response for unknown request id: {}", id);
                }
            } else if let Some(method) = msg.get("method").and_then(|v| v.as_str()) {
                // Notification.
                if method == "textDocument/publishDiagnostics" {
                    if let Some(params) = msg.get("params") {
                        let diags = parse_diagnostics(params);
                        let mut shared = shared.lock().await;
                        // Extract the file URI from the params.
                        if let Some(uri) = params.get("uri").and_then(|v| v.as_str()) {
                            shared.diagnostics.insert(uri.to_string(), diags);
                        }
                    }
                } else {
                    info!("Received LSP notification: {}", method);
                }
            }
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // The process has kill_on_drop(true), so it will be killed when
        // the Child handle is dropped.
        self.reader_handle.abort();
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC message building
// ---------------------------------------------------------------------------

/// Build a JSON-RPC message string.
///
/// If `id` is `Some`, it's a request; if `None`, it's a notification.
fn build_message(id: Option<u64>, method: &str, params: Value) -> String {
    let mut msg = serde_json::Map::new();
    msg.insert("jsonrpc".to_string(), Value::String("2.0".to_string()));
    msg.insert("method".to_string(), Value::String(method.to_string()));
    msg.insert("params".to_string(), params);
    if let Some(id) = id {
        msg.insert(
            "id".to_string(),
            Value::Number(serde_json::Number::from(id)),
        );
    }
    serde_json::to_string(&Value::Object(msg)).unwrap_or_default()
}

/// Parse the Content-Length value from a header line.
fn parse_content_length(header: &str) -> Option<usize> {
    let header = header.trim();
    if let Some(rest) = header.strip_prefix("Content-Length:") {
        rest.trim().parse::<usize>().ok()
    } else if let Some(rest) = header.strip_prefix("content-length:") {
        rest.trim().parse::<usize>().ok()
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Response parsing helpers
// ---------------------------------------------------------------------------

/// Parse a Location object into an `LspLocation`.
fn location_from_value(obj: &serde_json::Map<String, Value>, _fallback_uri: &str) -> LspLocation {
    let file = obj
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or(_fallback_uri)
        .to_string();

    let (line, column) = obj
        .get("range")
        .and_then(|r| r.get("start"))
        .map(|start| {
            let l = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let c = start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            (l, c)
        })
        .unwrap_or((0, 0));

    LspLocation { file, line, column }
}

/// Parse hover contents into type_info and docstring strings.
fn parse_hover_contents(contents: &Value, type_info: &mut String, _docstring: &mut String) {
    match contents {
        Value::Object(map) => {
            // MarkupContent: { kind: "markdown" | "plaintext", value: "..." }
            if let Some(value) = map.get("value").and_then(|v| v.as_str()) {
                *type_info = value.to_string();
            }
            // MarkedString (old format): { language: "...", value: "..." }
            if let Some(lang) = map.get("language").and_then(|v| v.as_str()) {
                if let Some(value) = map.get("value").and_then(|v| v.as_str()) {
                    *type_info = format!("{}: {}", lang, value);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                if let Some(obj) = item.as_object() {
                    if let Some(value) = obj.get("value").and_then(|v| v.as_str()) {
                        if !type_info.is_empty() {
                            type_info.push('\n');
                        }
                        type_info.push_str(value);
                    }
                } else if let Some(s) = item.as_str() {
                    if !type_info.is_empty() {
                        type_info.push('\n');
                    }
                    type_info.push_str(s);
                }
            }
        }
        Value::String(s) => {
            *type_info = s.clone();
        }
        _ => {}
    }
}

/// Parse a single CompletionItem value.
fn parse_completion_item(item: &Value) -> Option<LspCompletionItem> {
    let obj = item.as_object()?;
    let label = obj.get("label")?.as_str()?.to_string();
    let detail = obj
        .get("detail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let kind = obj.get("kind").and_then(|v| v.as_u64()).map(|k| k as u32);
    Some(LspCompletionItem {
        label,
        detail,
        kind,
    })
}

/// Parse a single SymbolInformation or DocumentSymbol value.
fn parse_symbol(item: &Value, file_uri: &str) -> Option<LspSymbol> {
    let obj = item.as_object()?;

    let name = obj.get("name")?.as_str()?.to_string();
    let kind = obj.get("kind").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    // DocumentSymbol has range/selectionRange; SymbolInformation has location.
    let (line, column) = if let Some(range) = obj.get("range") {
        let start = range.get("start")?;
        let l = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let c = start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        (l, c)
    } else if let Some(location) = obj.get("location") {
        let start = location.get("range").and_then(|r| r.get("start"))?;
        let l = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let c = start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        (l, c)
    } else {
        (0, 0)
    };

    let file = obj
        .get("location")
        .and_then(|loc| loc.get("uri"))
        .and_then(|v| v.as_str())
        .unwrap_or(file_uri)
        .to_string();

    Some(LspSymbol {
        name,
        kind,
        file,
        line,
        column,
    })
}

/// Parse a TextEdit value.
fn parse_text_edit(edit: &Value) -> Option<LspTextEdit> {
    let obj = edit.as_object()?;
    let range = obj.get("range")?;
    let start = range.get("start")?;
    let end = range.get("end")?;

    let start_line = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let start_column = start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let end_line = end.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let end_column = end.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let new_text = obj
        .get("newText")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(LspTextEdit {
        start_line,
        start_column,
        end_line,
        end_column,
        new_text,
    })
}

/// Parse a `textDocument/publishDiagnostics` params into `LspDiagnostic` vec.
fn parse_diagnostics(params: &Value) -> Vec<LspDiagnostic> {
    let file = params
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let mut diags = Vec::new();

    if let Some(items) = params.get("diagnostics").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(obj) = item.as_object() {
                let severity = obj.get("severity").and_then(|v| v.as_u64()).unwrap_or(4) as u8;
                let message = obj
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let (line, column) = if let Some(range) = obj.get("range") {
                    let start = range.get("start");
                    let l = start
                        .and_then(|s| s.get("line"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let c = start
                        .and_then(|s| s.get("character"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    (l, c)
                } else {
                    (0, 0)
                };

                diags.push(LspDiagnostic {
                    severity,
                    message,
                    file: file.clone(),
                    line,
                    column,
                });
            }
        }
    }

    diags
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request() {
        let msg = build_message(
            Some(1),
            "textDocument/definition",
            json!({"textDocument": {"uri": "file:///test.rs"}, "position": {"line": 0, "character": 5}}),
        );
        let parsed: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["method"], "textDocument/definition");
    }

    #[test]
    fn test_build_notification() {
        let msg = build_message(None::<u64>, "initialized", json!({}));
        let parsed: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert!(parsed.get("id").is_none());
        assert_eq!(parsed["method"], "initialized");
    }

    #[test]
    fn test_parse_content_length() {
        assert_eq!(parse_content_length("Content-Length: 123\r\n"), Some(123));
        assert_eq!(parse_content_length("content-length: 456"), Some(456));
        assert_eq!(parse_content_length("Content-Length: abc"), None);
        assert_eq!(parse_content_length(""), None);
    }

    #[test]
    fn test_parse_diagnostics() {
        let json = json!({
            "uri": "file:///test.rs",
            "diagnostics": [
                {
                    "severity": 1,
                    "message": "unused variable",
                    "range": {
                        "start": {"line": 5, "character": 0},
                        "end": {"line": 5, "character": 10}
                    }
                }
            ]
        });
        let diags = parse_diagnostics(&json);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, 1);
        assert_eq!(diags[0].message, "unused variable");
        assert_eq!(diags[0].file, "file:///test.rs");
        assert_eq!(diags[0].line, 5);
        assert_eq!(diags[0].column, 0);
    }

    #[test]
    fn test_parse_completion_item() {
        let json = json!({
            "label": "println!",
            "detail": "macro_rules! println",
            "kind": 15
        });
        let item = parse_completion_item(&json).unwrap();
        assert_eq!(item.label, "println!");
        assert_eq!(item.detail.unwrap(), "macro_rules! println");
        assert_eq!(item.kind.unwrap(), 15);
    }

    #[test]
    fn test_parse_text_edit() {
        let json = json!({
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 1, "character": 0}
            },
            "newText": "fn main() {\n"
        });
        let edit = parse_text_edit(&json).unwrap();
        assert_eq!(edit.start_line, 0);
        assert_eq!(edit.start_column, 0);
        assert_eq!(edit.end_line, 1);
        assert_eq!(edit.end_column, 0);
        assert_eq!(edit.new_text, "fn main() {\n");
    }

    #[test]
    fn test_lsp_config() {
        let config = LspConfig::new()
            .register("rust", &["rust-analyzer"])
            .register("python", &["pylsp"]);
        assert_eq!(config.command_for("rust").unwrap(), &["rust-analyzer"]);
        assert_eq!(config.command_for("python").unwrap(), &["pylsp"]);
        assert!(config.command_for("go").is_none());
    }

    #[test]
    fn test_parse_symbol_document_symbol() {
        let json = json!({
            "name": "main",
            "kind": 12,
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 3, "character": 1}
            },
            "selectionRange": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 4}
            }
        });
        let sym = parse_symbol(&json, "file:///test.rs").unwrap();
        assert_eq!(sym.name, "main");
        assert_eq!(sym.kind, 12);
        assert_eq!(sym.line, 0);
        assert_eq!(sym.column, 0);
    }

    #[test]
    fn test_parse_symbol_symbol_information() {
        let json = json!({
            "name": "MyStruct",
            "kind": 7,
            "location": {
                "uri": "file:///test.rs",
                "range": {
                    "start": {"line": 10, "character": 0},
                    "end": {"line": 20, "character": 1}
                }
            }
        });
        let sym = parse_symbol(&json, "file:///test.rs").unwrap();
        assert_eq!(sym.name, "MyStruct");
        assert_eq!(sym.kind, 7);
        assert_eq!(sym.line, 10);
        assert_eq!(sym.column, 0);
    }

    #[test]
    fn test_location_from_value() {
        let json = json!({
            "uri": "file:///test.rs",
            "range": {
                "start": {"line": 42, "character": 7},
                "end": {"line": 42, "character": 15}
            }
        });
        let loc = location_from_value(json.as_object().unwrap(), "file:///fallback.rs");
        assert_eq!(loc.file, "file:///test.rs");
        assert_eq!(loc.line, 42);
        assert_eq!(loc.column, 7);
    }

    #[test]
    fn test_parse_hover_contents_markup() {
        let json = json!({
            "contents": {
                "kind": "markdown",
                "value": "**fn** `foo()`\n\nDoes something."
            }
        });
        let mut type_info = String::new();
        let mut docstring = String::new();
        parse_hover_contents(&json["contents"], &mut type_info, &mut docstring);
        assert!(type_info.contains("foo"));
    }

    #[test]
    fn test_parse_hover_contents_string() {
        let json = json!({
            "contents": "plain text hover"
        });
        let mut type_info = String::new();
        let mut docstring = String::new();
        parse_hover_contents(&json["contents"], &mut type_info, &mut docstring);
        assert_eq!(type_info, "plain text hover");
    }
}
