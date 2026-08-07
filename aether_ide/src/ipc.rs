//! JSON-RPC IPC server for communication with the Python voice UI.
//!
//! Listens on a Unix domain socket and handles requests from the
//! Python `artome_ide` package. Uses serde_json for message serialization.

use crate::buffer::TextBuffer;
use crate::editor::VoiceEditor;
use crate::dap::DapClient;
use crate::lsp::LspClient;
use crate::navigation::{Cursor, CursorNavigation};
use crate::parser;
use crate::project::{ProjectConfig, ProjectIndex};
use crate::sonifier;
use crate::IdeState;
use anyhow::{Context, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use parking_lot::RwLock;

/// A JSON-RPC request from the Python UI.
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC response to the Python UI.
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// Global shared state for the IDE server.
pub struct IdeServerState {
    pub ide: IdeState,
    pub project: RwLock<Option<ProjectIndex>>,
    pub navigation: RwLock<Option<CursorNavigation>>,
    pub buffer: RwLock<Option<TextBuffer>>,
    pub editor: RwLock<Option<VoiceEditor>>,
    pub lsp: RwLock<Option<LspClient>>,
    pub dap: RwLock<Option<DapClient>>,
}

impl IdeServerState {
    pub fn new() -> Self {
        Self {
            ide: IdeState::new(),
            project: RwLock::new(None),
            navigation: RwLock::new(None),
            buffer: RwLock::new(None),
            editor: RwLock::new(None),
            lsp: RwLock::new(None),
            dap: RwLock::new(None),
        }
    }
}

/// Start the IPC server on a Unix domain socket.
pub async fn start_ipc_server(state: Arc<IdeServerState>, socket_path: &str) -> Result<()> {
    let path = PathBuf::from(socket_path);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&path)
        .with_context(|| format!("Failed to bind to {}", socket_path))?;

    info!("IDE IPC server listening on {}", socket_path);

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let state = state.clone();
                if let Err(e) = handle_client(stream, state).await {
                    warn!("IPC client error: {e}");
                }
            }
            Err(e) => {
                warn!("IPC accept error: {e}");
            }
        }
    }
}

/// Handle a single client connection.
async fn handle_client(stream: UnixStream, state: Arc<IdeServerState>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let request: RpcRequest = match serde_json::from_str(&trimmed) {
            Ok(r) => r,
            Err(e) => {
                let response = RpcResponse {
                    id: 0,
                    result: None,
                    error: Some(RpcError {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                    }),
                };
                let mut buf = serde_json::to_string(&response)?;
                buf.push('\n');
                writer.write_all(buf.as_bytes()).await?;
                line.clear();
                continue;
            }
        };

        let response = handle_request(&request, &state).await;
        let mut buf = serde_json::to_string(&response)?;
        buf.push('\n');
        writer.write_all(buf.as_bytes()).await?;
        line.clear();
    }

    Ok(())
}

/// Dispatch a request to the appropriate handler.
async fn handle_request(request: &RpcRequest, state: &IdeServerState) -> RpcResponse {
    let result = match request.method.as_str() {
        // File operations
        "open_file" => handle_open_file(request, state).await,
        "get_structure" => handle_get_structure(request, state).await,
        "get_sonification" => handle_get_sonification(request, state).await,
        "get_summary" => handle_get_summary(request, state).await,
        "get_tree" => handle_get_tree(request, state).await,
        "get_cursor" => handle_get_cursor(request, state).await,
        "set_cursor" => handle_set_cursor(request, state).await,
        "where_am_i" => handle_where_am_i(request, state).await,
        "get_context_lines" => handle_get_context_lines(request, state).await,

        // LSP operations
        "lsp_go_to_definition" => handle_lsp_go_to_definition(request, state).await,
        "lsp_hover" => handle_lsp_hover(request, state).await,
        "lsp_completions" => handle_lsp_completions(request, state).await,
        "lsp_diagnostics" => handle_lsp_diagnostics(request, state).await,
        "lsp_document_symbols" => handle_lsp_document_symbols(request, state).await,
        "lsp_references" => handle_lsp_references(request, state).await,
        "lsp_rename" => handle_lsp_rename(request, state).await,
        "lsp_format" => handle_lsp_format(request, state).await,

        // Project operations
        "project_scan" => handle_project_scan(request, state).await,
        "project_find_file" => handle_project_find_file(request, state).await,
        "project_find_symbol" => handle_project_find_symbol(request, state).await,
        "project_file_tree" => handle_project_file_tree(request, state).await,

        // Navigation operations
        "nav_go_to_line" => handle_nav_go_to_line(request, state).await,
        "nav_go_to_function" => handle_nav_go_to_function(request, state).await,
        "nav_go_to_class" => handle_nav_go_to_class(request, state).await,
        "nav_go_to_symbol" => handle_nav_go_to_symbol(request, state).await,
        "nav_next_function" => handle_nav_next_function(request, state).await,
        "nav_prev_function" => handle_nav_prev_function(request, state).await,
        "nav_next_problem" => handle_nav_next_problem(request, state).await,
        "nav_go_back" => handle_nav_go_back(request, state).await,

        // Buffer/Editor operations
        "buffer_load" => handle_buffer_load(request, state).await,
        "buffer_save" => handle_buffer_save(request, state).await,
        "buffer_get_line" => handle_buffer_get_line(request, state).await,
        "buffer_get_lines" => handle_buffer_get_lines(request, state).await,
        "buffer_undo" => handle_buffer_undo(request, state).await,
        "buffer_redo" => handle_buffer_redo(request, state).await,
        "editor_insert_line" => handle_editor_insert_line(request, state).await,
        "editor_delete_line" => handle_editor_delete_line(request, state).await,
        "editor_change_text" => handle_editor_change_text(request, state).await,
        "editor_comment_lines" => handle_editor_comment_lines(request, state).await,
        "editor_indent_lines" => handle_editor_indent_lines(request, state).await,
        "editor_wrap_try_catch" => handle_editor_wrap_try_catch(request, state).await,
        "editor_extract_function" => handle_editor_extract_function(request, state).await,

        // DAP operations
        "dap_start" => handle_dap_start(request, state).await,
        "dap_stop" => handle_dap_stop(request, state).await,
        "dap_set_breakpoint" => handle_dap_set_breakpoint(request, state).await,
        "dap_clear_breakpoint" => handle_dap_clear_breakpoint(request, state).await,
        "dap_list_breakpoints" => handle_dap_list_breakpoints(request, state).await,
        "dap_continue" => handle_dap_continue(request, state).await,
        "dap_next" => handle_dap_next(request, state).await,
        "dap_step_in" => handle_dap_step_in(request, state).await,
        "dap_step_out" => handle_dap_step_out(request, state).await,
        "dap_stack_trace" => handle_dap_stack_trace(request, state).await,
        "dap_variables" => handle_dap_variables(request, state).await,
        "dap_evaluate" => handle_dap_evaluate(request, state).await,

        // Meta
        "list_skills" => handle_list_skills().await,
        _ => Err(anyhow::anyhow!("Unknown method: {}", request.method)),
    };

    match result {
        Ok(value) => RpcResponse {
            id: request.id,
            result: Some(value),
            error: None,
        },
        Err(e) => RpcResponse {
            id: request.id,
            result: None,
            error: Some(RpcError {
                code: -1,
                message: e.to_string(),
            }),
        },
    }
}

// =========================================================================
// File operations
// =========================================================================

async fn handle_open_file(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let path = request.params.get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

    let file_path = PathBuf::from(path);
    let tree = parser::parse_file(&file_path)?;

    *state.ide.current_file.write() = Some(path.to_string());
    *state.ide.tree.write() = Some(tree.clone());
    *state.ide.cursor.write() = (0, 0);

    // Also load into buffer
    let buffer = TextBuffer::load(path)?;
    *state.buffer.write() = Some(buffer.clone());
    *state.editor.write() = Some(VoiceEditor::new(buffer));

    // Set up navigation
    let cursor = Cursor::new(path.to_string());
    let mut nav = CursorNavigation::new(cursor);
    nav.tree = Some(tree.clone());
    *state.navigation.write() = Some(nav);

    Ok(serde_json::json!({
        "path": tree.path,
        "language": tree.language,
        "total_lines": tree.total_lines,
        "symbols": tree.symbols.len(),
        "boundaries": tree.boundaries.len(),
    }))
}

async fn handle_get_structure(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let tree = state.ide.tree.read();
    let tree = tree.as_ref().ok_or_else(|| anyhow::anyhow!("No file open"))?;

    let symbols: Vec<serde_json::Value> = tree.symbols.iter().map(|s| {
        serde_json::json!({
            "name": s.name,
            "kind": format!("{:?}", s.kind).to_lowercase(),
            "start_line": s.start_line + 1,
            "end_line": s.end_line + 1,
            "depth": s.depth,
        })
    }).collect();

    Ok(serde_json::json!({
        "path": tree.path,
        "language": tree.language,
        "total_lines": tree.total_lines,
        "symbols": symbols,
    }))
}

async fn handle_get_sonification(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let tree = state.ide.tree.read();
    let tree = tree.as_ref().ok_or_else(|| anyhow::anyhow!("No file open"))?;

    let audio = sonifier::sonify_tree(tree);
    let lines: Vec<serde_json::Value> = audio.iter().map(|l| {
        serde_json::json!({
            "line": l.line + 1,
            "drone_pitch": l.drone_pitch,
            "drone_volume": l.drone_volume,
            "earcon": l.earcon.as_ref().map(|e| format!("{:?}", e)),
            "tempo": l.tempo,
            "speak": l.speak,
        })
    }).collect();

    Ok(serde_json::json!({
        "path": tree.path,
        "total_lines": tree.total_lines,
        "lines": lines,
    }))
}

async fn handle_get_summary(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let tree = state.ide.tree.read();
    let tree = tree.as_ref().ok_or_else(|| anyhow::anyhow!("No file open"))?;

    let summary = sonifier::structure_summary(tree);
    let tree_summary = sonifier::tree_summary(tree);

    Ok(serde_json::json!({
        "summary": summary,
        "tree": tree_summary,
    }))
}

async fn handle_get_tree(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let tree = state.ide.tree.read();
    let tree = tree.as_ref().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    let tree_text = sonifier::tree_summary(tree);
    Ok(serde_json::json!({ "tree": tree_text }))
}

async fn handle_get_cursor(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let cursor = *state.ide.cursor.read();
    Ok(serde_json::json!({
        "line": cursor.0 + 1,
        "column": cursor.1,
    }))
}

async fn handle_set_cursor(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let line = request.params.get("line")
        .and_then(|v| v.as_u64())
        .map(|l| l.saturating_sub(1) as usize)
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'line' parameter"))?;

    let column = request.params.get("column")
        .and_then(|v| v.as_u64())
        .map(|c| c as usize)
        .unwrap_or(0);

    *state.ide.cursor.write() = (line, column);

    // Update navigation cursor
    if let Some(ref mut nav) = *state.navigation.write() {
        nav.cursor.line = line;
        nav.cursor.column = column;
    }

    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_where_am_i(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let nav = state.navigation.read();
    let nav = nav.as_ref().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    Ok(serde_json::json!({ "description": nav.where_am_i() }))
}

async fn handle_get_context_lines(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let n = request.params.get("count").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let nav = state.navigation.read();
    let nav = nav.as_ref().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    let context = nav.get_context_lines(n);
    // Convert to serializable format (SpatialPosition doesn't impl Serialize)
    let lines: Vec<serde_json::Value> = context.into_iter().map(|(line, text, pos)| {
        serde_json::json!({
            "line": line + 1,
            "text": text,
            "position": format!("{:?}", pos),
        })
    }).collect();
    Ok(serde_json::json!({ "lines": lines }))
}

// =========================================================================
// LSP operations
// =========================================================================

async fn handle_lsp_go_to_definition(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let column = request.params.get("column").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'column' parameter"))? as usize;
    let mut lsp = state.lsp.write();
    let lsp = lsp.as_mut().ok_or_else(|| anyhow::anyhow!("LSP not initialized"))?;
    let result = lsp.go_to_definition(file, line, column).await?;
    Ok(serde_json::json!({ "location": result }))
}

async fn handle_lsp_hover(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let column = request.params.get("column").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'column' parameter"))? as usize;
    let mut lsp = state.lsp.write();
    let lsp = lsp.as_mut().ok_or_else(|| anyhow::anyhow!("LSP not initialized"))?;
    let (type_info, docstring) = lsp.hover(file, line, column).await?;
    Ok(serde_json::json!({ "type_info": type_info, "docstring": docstring }))
}

async fn handle_lsp_completions(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let column = request.params.get("column").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'column' parameter"))? as usize;
    let mut lsp = state.lsp.write();
    let lsp = lsp.as_mut().ok_or_else(|| anyhow::anyhow!("LSP not initialized"))?;
    let items = lsp.completions(file, line, column).await?;
    Ok(serde_json::json!({ "completions": items }))
}

async fn handle_lsp_diagnostics(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let lsp = state.lsp.read();
    let lsp = lsp.as_ref().ok_or_else(|| anyhow::anyhow!("LSP not initialized"))?;
    let diags = lsp.all_diagnostics().await;
    Ok(serde_json::json!({ "diagnostics": diags }))
}

async fn handle_lsp_document_symbols(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let mut lsp = state.lsp.write();
    let lsp = lsp.as_mut().ok_or_else(|| anyhow::anyhow!("LSP not initialized"))?;
    let symbols = lsp.document_symbols(file).await?;
    Ok(serde_json::json!({ "symbols": symbols }))
}

async fn handle_lsp_references(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let column = request.params.get("column").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'column' parameter"))? as usize;
    let mut lsp = state.lsp.write();
    let lsp = lsp.as_mut().ok_or_else(|| anyhow::anyhow!("LSP not initialized"))?;
    let locations = lsp.references(file, line, column).await?;
    Ok(serde_json::json!({ "references": locations }))
}

async fn handle_lsp_rename(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let column = request.params.get("column").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'column' parameter"))? as usize;
    let new_name = request.params.get("new_name").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'new_name' parameter"))?;
    let mut lsp = state.lsp.write();
    let lsp = lsp.as_mut().ok_or_else(|| anyhow::anyhow!("LSP not initialized"))?;
    let edit = lsp.rename(file, line, column, new_name).await?;
    Ok(serde_json::json!({ "workspace_edit": edit }))
}

async fn handle_lsp_format(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let mut lsp = state.lsp.write();
    let lsp = lsp.as_mut().ok_or_else(|| anyhow::anyhow!("LSP not initialized"))?;
    let edits = lsp.formatting(file).await?;
    Ok(serde_json::json!({ "edits": edits }))
}

// =========================================================================
// Project operations
// =========================================================================

async fn handle_project_scan(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let root = request.params.get("root").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'root' parameter"))?;
    let mut config = ProjectConfig::default();
    config.root_dir = PathBuf::from(root);
    let project = ProjectIndex::scan(&config)?;
    let count = project.files.len();
    *state.project.write() = Some(project);
    Ok(serde_json::json!({ "files_indexed": count }))
}

async fn handle_project_find_file(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let project = state.project.read();
    let project = project.as_ref().ok_or_else(|| anyhow::anyhow!("No project scanned"))?;
    let name = request.params.get("name").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
    let results = project.fuzzy_find_file(name);
    let files: Vec<serde_json::Value> = results.iter().map(|f| {
        serde_json::json!({
            "path": f.path.to_string_lossy(),
            "language": f.language,
            "line_count": f.line_count,
        })
    }).collect();
    Ok(serde_json::json!({ "files": files }))
}

async fn handle_project_find_symbol(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let project = state.project.read();
    let project = project.as_ref().ok_or_else(|| anyhow::anyhow!("No project scanned"))?;
    let name = request.params.get("name").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
    let results = project.find_symbol(name);
    let symbols: Vec<serde_json::Value> = results.iter().map(|s| {
        serde_json::json!({
            "name": s.name,
            "kind": format!("{:?}", s.kind),
            "file": s.file_path.to_string_lossy(),
            "start_line": s.start_line + 1,
            "end_line": s.end_line + 1,
        })
    }).collect();
    Ok(serde_json::json!({ "symbols": symbols }))
}

async fn handle_project_file_tree(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let project = state.project.read();
    let project = project.as_ref().ok_or_else(|| anyhow::anyhow!("No project scanned"))?;
    let tree = project.get_file_tree();
    Ok(serde_json::json!({ "tree": tree }))
}

// =========================================================================
// Navigation operations
// =========================================================================

async fn handle_nav_go_to_line(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let mut nav = state.navigation.write();
    let nav = nav.as_mut().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    nav.move_to_line(line.saturating_sub(1)).map_err(|e| anyhow::anyhow!("{}", e))?;
    *state.ide.cursor.write() = (line.saturating_sub(1), 0);
    Ok(serde_json::json!({ "ok": true, "line": line }))
}

async fn handle_nav_go_to_function(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let name = request.params.get("name").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
    let mut nav = state.navigation.write();
    let nav = nav.as_mut().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    let line = nav.move_to_function(name).map_err(|e| anyhow::anyhow!("{}", e))?;
    *state.ide.cursor.write() = (line, 0);
    Ok(serde_json::json!({ "ok": true, "line": line + 1 }))
}

async fn handle_nav_go_to_class(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let name = request.params.get("name").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
    let mut nav = state.navigation.write();
    let nav = nav.as_mut().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    let line = nav.move_to_class(name).map_err(|e| anyhow::anyhow!("{}", e))?;
    *state.ide.cursor.write() = (line, 0);
    Ok(serde_json::json!({ "ok": true, "line": line + 1 }))
}

async fn handle_nav_go_to_symbol(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let name = request.params.get("name").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
    let mut nav = state.navigation.write();
    let nav = nav.as_mut().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    let line = nav.move_to_symbol(name).map_err(|e| anyhow::anyhow!("{}", e))?;
    *state.ide.cursor.write() = (line, 0);
    Ok(serde_json::json!({ "ok": true, "line": line + 1 }))
}

async fn handle_nav_next_function(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut nav = state.navigation.write();
    let nav = nav.as_mut().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    let line = nav.move_next_function().map_err(|e| anyhow::anyhow!("{}", e))?;
    *state.ide.cursor.write() = (line, 0);
    Ok(serde_json::json!({ "ok": true, "line": line + 1 }))
}

async fn handle_nav_prev_function(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut nav = state.navigation.write();
    let nav = nav.as_mut().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    let line = nav.move_prev_function().map_err(|e| anyhow::anyhow!("{}", e))?;
    *state.ide.cursor.write() = (line, 0);
    Ok(serde_json::json!({ "ok": true, "line": line + 1 }))
}

async fn handle_nav_next_problem(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut nav = state.navigation.write();
    let nav = nav.as_mut().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    let line = nav.move_next_problem().map_err(|e| anyhow::anyhow!("{}", e))?;
    *state.ide.cursor.write() = (line, 0);
    Ok(serde_json::json!({ "ok": true, "line": line + 1 }))
}

async fn handle_nav_go_back(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut nav = state.navigation.write();
    let nav = nav.as_mut().ok_or_else(|| anyhow::anyhow!("No file open"))?;
    if nav.go_back() {
        let line = nav.cursor.line;
        let col = nav.cursor.column;
        *state.ide.cursor.write() = (line, col);
        Ok(serde_json::json!({ "ok": true, "line": line + 1, "file": nav.cursor.file }))
    } else {
        Err(anyhow::anyhow!("No previous location"))
    }
}

// =========================================================================
// Buffer/Editor operations
// =========================================================================

async fn handle_buffer_load(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let path = request.params.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
    let buffer = TextBuffer::load(path)?;
    let info = format!("Loaded {}: {} lines, {} chars", path, buffer.line_count(), buffer.total_chars());
    *state.buffer.write() = Some(buffer.clone());
    *state.editor.write() = Some(VoiceEditor::new(buffer));
    Ok(serde_json::json!({ "ok": true, "info": info }))
}

async fn handle_buffer_save(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let buffer = state.buffer.read();
    let buffer = buffer.as_ref().ok_or_else(|| anyhow::anyhow!("No buffer"))?;
    buffer.save()?;
    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_buffer_get_line(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let buffer = state.buffer.read();
    let buffer = buffer.as_ref().ok_or_else(|| anyhow::anyhow!("No buffer"))?;
    let text = buffer.get_line(line).unwrap_or("").to_string();
    Ok(serde_json::json!({ "text": text }))
}

async fn handle_buffer_get_lines(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let start = request.params.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let end = request.params.get("end").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let buffer = state.buffer.read();
    let buffer = buffer.as_ref().ok_or_else(|| anyhow::anyhow!("No buffer"))?;
    let text = buffer.get_lines(start, end);
    Ok(serde_json::json!({ "text": text }))
}

async fn handle_buffer_undo(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    editor.undo();
    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_buffer_redo(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    editor.redo();
    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_editor_insert_line(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let text = request.params.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let after = request.params.get("after").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    if after {
        editor.insert_line_after(line, text);
    } else {
        editor.insert_line_before(line, text);
    }
    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_editor_delete_line(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))? as usize;
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    editor.delete_line(line);
    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_editor_change_text(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let old = request.params.get("old").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'old' parameter"))?;
    let new = request.params.get("new").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'new' parameter"))?;
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    let count = editor.change_text(old, new);
    Ok(serde_json::json!({ "ok": true, "replacements": count }))
}

async fn handle_editor_comment_lines(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let start = request.params.get("start").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'start' parameter"))? as usize;
    let end = request.params.get("end").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'end' parameter"))? as usize;
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    editor.comment_lines(start, end);
    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_editor_indent_lines(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let start = request.params.get("start").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'start' parameter"))? as usize;
    let end = request.params.get("end").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'end' parameter"))? as usize;
    let outdent = request.params.get("outdent").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    if outdent {
        editor.outdent_lines(start, end);
    } else {
        editor.indent_lines(start, end);
    }
    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_editor_wrap_try_catch(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let start = request.params.get("start").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'start' parameter"))? as usize;
    let end = request.params.get("end").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'end' parameter"))? as usize;
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    editor.wrap_in_try_catch(start, end);
    Ok(serde_json::json!({ "ok": true }))
}

async fn handle_editor_extract_function(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let start = request.params.get("start").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'start' parameter"))? as usize;
    let end = request.params.get("end").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'end' parameter"))? as usize;
    let name = request.params.get("name").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
    let mut editor = state.editor.write();
    let editor = editor.as_mut().ok_or_else(|| anyhow::anyhow!("No editor"))?;
    editor.extract_function(start, end, name);
    Ok(serde_json::json!({ "ok": true }))
}

// =========================================================================
// DAP operations
// =========================================================================

async fn handle_dap_start(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let program = request.params.get("program").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'program' parameter"))?;
    let args: Vec<String> = request.params.get("args")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let result = dap.launch(program, &args, None).await?;
    Ok(serde_json::json!({ "ok": true, "result": result }))
}

async fn handle_dap_stop(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    dap.terminate().await?;
    Ok(serde_json::json!({ "ok": true, "message": "Debug session stopped" }))
}

async fn handle_dap_set_breakpoint(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))?;
    let condition = request.params.get("condition").and_then(|v| v.as_str());
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let bp = dap.set_breakpoint(file, line, condition).await?;
    Ok(serde_json::json!({ "ok": true, "breakpoint": bp }))
}

async fn handle_dap_clear_breakpoint(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let file = request.params.get("file").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'file' parameter"))?;
    let line = request.params.get("line").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'line' parameter"))?;
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    dap.clear_breakpoint(file, line).await?;
    Ok(serde_json::json!({ "ok": true, "cleared_line": line }))
}

async fn handle_dap_list_breakpoints(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let dap = state.dap.read();
    let dap = dap.as_ref().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let bps = dap.list_breakpoints();
    Ok(serde_json::json!({ "breakpoints": bps }))
}

async fn handle_dap_continue(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let result = dap.continue_execution().await?;
    Ok(serde_json::json!({ "ok": true, "result": result }))
}

async fn handle_dap_next(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let result = dap.next().await?;
    Ok(serde_json::json!({ "ok": true, "result": result }))
}

async fn handle_dap_step_in(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let result = dap.step_in().await?;
    Ok(serde_json::json!({ "ok": true, "result": result }))
}

async fn handle_dap_step_out(_request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let result = dap.step_out().await?;
    Ok(serde_json::json!({ "ok": true, "result": result }))
}

async fn handle_dap_stack_trace(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let thread_id = request.params.get("thread_id").and_then(|v| v.as_u64());
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let frames = dap.get_stack_trace(thread_id).await?;
    Ok(serde_json::json!({ "stack_frames": frames }))
}

async fn handle_dap_variables(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let frame_id = request.params.get("frame_id").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'frame_id' parameter"))?;
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let vars = dap.get_variables(frame_id).await?;
    Ok(serde_json::json!({ "variables": vars }))
}

async fn handle_dap_evaluate(request: &RpcRequest, state: &IdeServerState) -> Result<serde_json::Value> {
    let expression = request.params.get("expression").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'expression' parameter"))?;
    let frame_id = request.params.get("frame_id").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'frame_id' parameter"))?;
    let mut dap = state.dap.write();
    let dap = dap.as_mut().ok_or_else(|| anyhow::anyhow!("DAP not initialized"))?;
    let var = dap.evaluate(expression, frame_id).await?;
    Ok(serde_json::json!({ "result": var }))
}

// =========================================================================
// Meta
// =========================================================================

async fn handle_list_skills() -> Result<serde_json::Value> {
    let skills: Vec<serde_json::Value> = vec![
        serde_json::json!({"name": "open_file", "description": "Open a file for reading and navigation", "params": ["path"]}),
        serde_json::json!({"name": "get_structure", "description": "Get the code structure (symbols, boundaries)", "params": []}),
        serde_json::json!({"name": "get_sonification", "description": "Get audio parameters for every line", "params": []}),
        serde_json::json!({"name": "get_summary", "description": "Get a text summary of the code structure", "params": []}),
        serde_json::json!({"name": "get_tree", "description": "Get a tree-shaped structure description", "params": []}),
        serde_json::json!({"name": "set_cursor", "description": "Set the cursor position", "params": ["line", "column"]}),
        serde_json::json!({"name": "get_cursor", "description": "Get the current cursor position", "params": []}),
        serde_json::json!({"name": "where_am_i", "description": "Get spoken description of current location", "params": []}),
        serde_json::json!({"name": "get_context_lines", "description": "Get lines around cursor for spatial audio", "params": ["count"]}),
        serde_json::json!({"name": "lsp_go_to_definition", "description": "Go to definition of symbol at cursor", "params": ["file", "line", "column"]}),
        serde_json::json!({"name": "lsp_hover", "description": "Get type info and docs for symbol at cursor", "params": ["file", "line", "column"]}),
        serde_json::json!({"name": "lsp_completions", "description": "Get code completions at position", "params": ["file", "line", "column"]}),
        serde_json::json!({"name": "lsp_diagnostics", "description": "Get all diagnostics for open files", "params": []}),
        serde_json::json!({"name": "lsp_document_symbols", "description": "Get symbols in document", "params": ["file"]}),
        serde_json::json!({"name": "lsp_references", "description": "Find all references to symbol", "params": ["file", "line", "column"]}),
        serde_json::json!({"name": "lsp_rename", "description": "Rename symbol across project", "params": ["file", "line", "column", "new_name"]}),
        serde_json::json!({"name": "lsp_format", "description": "Format current document", "params": ["file"]}),
        serde_json::json!({"name": "project_scan", "description": "Scan a project directory", "params": ["root"]}),
        serde_json::json!({"name": "project_find_file", "description": "Fuzzy-find a file by name", "params": ["name"]}),
        serde_json::json!({"name": "project_find_symbol", "description": "Find a symbol across the project", "params": ["name"]}),
        serde_json::json!({"name": "project_file_tree", "description": "Get project file tree for audio rendering", "params": []}),
        serde_json::json!({"name": "nav_go_to_line", "description": "Go to a specific line number", "params": ["line"]}),
        serde_json::json!({"name": "nav_go_to_function", "description": "Go to a function by name", "params": ["name"]}),
        serde_json::json!({"name": "nav_go_to_class", "description": "Go to a class by name", "params": ["name"]}),
        serde_json::json!({"name": "nav_go_to_symbol", "description": "Go to any symbol by name", "params": ["name"]}),
        serde_json::json!({"name": "nav_next_function", "description": "Go to next function boundary", "params": []}),
        serde_json::json!({"name": "nav_prev_function", "description": "Go to previous function boundary", "params": []}),
        serde_json::json!({"name": "nav_next_problem", "description": "Go to next LSP diagnostic", "params": []}),
        serde_json::json!({"name": "nav_go_back", "description": "Return to previous cursor position", "params": []}),
        serde_json::json!({"name": "buffer_load", "description": "Load file into editable buffer", "params": ["path"]}),
        serde_json::json!({"name": "buffer_save", "description": "Save buffer to disk", "params": []}),
        serde_json::json!({"name": "buffer_get_line", "description": "Get text of a specific line", "params": ["line"]}),
        serde_json::json!({"name": "buffer_get_lines", "description": "Get range of lines", "params": ["start", "end"]}),
        serde_json::json!({"name": "buffer_undo", "description": "Undo last edit", "params": []}),
        serde_json::json!({"name": "buffer_redo", "description": "Redo last undone edit", "params": []}),
        serde_json::json!({"name": "editor_insert_line", "description": "Insert a line before or after", "params": ["line", "text", "after"]}),
        serde_json::json!({"name": "editor_delete_line", "description": "Delete a line", "params": ["line"]}),
        serde_json::json!({"name": "editor_change_text", "description": "Find and replace text", "params": ["old", "new"]}),
        serde_json::json!({"name": "editor_comment_lines", "description": "Toggle comment on line range", "params": ["start", "end"]}),
        serde_json::json!({"name": "editor_indent_lines", "description": "Indent or outdent line range", "params": ["start", "end", "outdent"]}),
        serde_json::json!({"name": "editor_wrap_try_catch", "description": "Wrap selection in try-catch", "params": ["start", "end"]}),
        serde_json::json!({"name": "editor_extract_function", "description": "Extract selection to new function", "params": ["start", "end", "name"]}),
        serde_json::json!({"name": "dap_start", "description": "Start a debug session", "params": ["program"]}),
        serde_json::json!({"name": "dap_stop", "description": "Stop debug session", "params": []}),
        serde_json::json!({"name": "dap_set_breakpoint", "description": "Set a breakpoint", "params": ["file", "line"]}),
        serde_json::json!({"name": "dap_clear_breakpoint", "description": "Clear a breakpoint", "params": ["line"]}),
        serde_json::json!({"name": "dap_list_breakpoints", "description": "List all breakpoints", "params": []}),
        serde_json::json!({"name": "dap_continue", "description": "Continue execution", "params": []}),
        serde_json::json!({"name": "dap_next", "description": "Step over", "params": []}),
        serde_json::json!({"name": "dap_step_in", "description": "Step into function", "params": []}),
        serde_json::json!({"name": "dap_step_out", "description": "Step out of function", "params": []}),
        serde_json::json!({"name": "dap_stack_trace", "description": "Get call stack", "params": []}),
        serde_json::json!({"name": "dap_variables", "description": "Get variables for stack frame", "params": []}),
        serde_json::json!({"name": "dap_evaluate", "description": "Evaluate expression", "params": ["expression"]}),
    ];
    Ok(serde_json::json!({ "skills": skills }))
}
