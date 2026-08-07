//! Debug Adapter Protocol (DAP) client module.
//!
//! Provides an async DAP client that communicates with any debug adapter over
//! stdio using JSON-RPC. Supports the core debugging features needed by the
//! audio-first IDE: launch, breakpoints, stepping, stack traces, variables,
//! and expression evaluation.
//!
//! # Protocol notes
//!
//! DAP uses JSON-RPC over stdio with Content-Length framing (identical to LSP).
//! Key differences from LSP:
//! - Request IDs use the field name `seq` instead of `id`
//! - Every message has a `type` field: `"request"`, `"response"`, or `"event"`
//! - Responses carry `request_seq` to correlate with the original request
//! - Events carry an `event` field (e.g. `"stopped"`, `"continued"`, `"exited"`)

use anyhow::{Context, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Configuration mapping language identifiers to debug adapter commands.
///
/// # Example
///
/// ```ignore
/// let config = DapConfig::new()
///     .register("python", &["debugpy-adapter"])
///     .register("rust", &["lldb-vscode"]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct DapConfig {
    /// Maps language id → debug adapter command + args.
    pub adapters: HashMap<String, Vec<String>>,
}

impl DapConfig {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Register a debug adapter command for a language.
    pub fn register(mut self, language: &str, command: &[&str]) -> Self {
        self.adapters.insert(
            language.to_string(),
            command.iter().map(|s| s.to_string()).collect(),
        );
        self
    }

    /// Get the adapter command for a language, if registered.
    pub fn command_for(&self, language: &str) -> Option<&[String]> {
        self.adapters.get(language).map(|v| v.as_slice())
    }
}

/// The current state of a debug session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DebugState {
    /// No debug session active.
    Inactive,
    /// The debugee is running.
    Running,
    /// The debugee is paused at a breakpoint or after a step.
    Paused,
    /// The debug session has been terminated.
    Terminated,
}

/// A breakpoint set in the debugee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    /// DAP breakpoint ID (assigned by the adapter).
    pub id: u64,
    /// Source file path.
    pub file: String,
    /// 1-based line number.
    pub line: u64,
    /// Whether the breakpoint was verified by the adapter.
    pub verified: bool,
    /// Optional break condition expression.
    pub condition: Option<String>,
}

/// A stack frame in the call stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// DAP frame ID (used for variable lookup and evaluation).
    pub id: u64,
    /// Function or method name.
    pub name: String,
    /// Source file path.
    pub file: String,
    /// 1-based line number.
    pub line: u64,
    /// 1-based column number.
    pub column: u64,
}

/// A variable in the current debug context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Variable name.
    pub name: String,
    /// String representation of the variable's value.
    pub value: String,
    /// Type name (e.g. "int", "str", "list").
    pub type_name: String,
    /// Reference for fetching children of composite variables.
    /// 0 means this variable has no children.
    pub variables_reference: u64,
}

/// Events emitted by the debug adapter.
#[derive(Debug, Clone)]
pub enum DapEvent {
    /// The debugee stopped at a breakpoint.
    BreakpointHit {
        thread_id: u64,
        reason: String,
    },
    /// The debugee stopped due to an exception.
    Exception {
        thread_id: u64,
        description: Option<String>,
    },
    /// A step command completed.
    StepComplete {
        thread_id: u64,
    },
    /// A new thread was started.
    ThreadStarted {
        thread_id: u64,
    },
    /// A thread exited.
    ThreadExited {
        thread_id: u64,
    },
    /// The debugee process exited.
    ProcessExited {
        exit_code: i64,
    },
}

/// The debug session state, tracking breakpoints, stack frames, and variables.
#[derive(Debug, Clone)]
pub struct DebugSession {
    /// Current session state.
    pub state: DebugState,
    /// Active breakpoints.
    pub breakpoints: Vec<Breakpoint>,
    /// Current stack frames (populated when paused).
    pub stack_frames: Vec<StackFrame>,
    /// Current visible variables (populated when paused).
    pub variables: Vec<Variable>,
}

impl DebugSession {
    fn new() -> Self {
        Self {
            state: DebugState::Inactive,
            breakpoints: Vec::new(),
            stack_frames: Vec::new(),
            variables: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared state between the client and the background reader task
// ---------------------------------------------------------------------------

/// Shared state accessible by both the DapClient and the background reader.
struct DapShared {
    /// Pending requests keyed by message seq. The reader task completes these
    /// when a response arrives.
    pending: HashMap<u64, tokio::sync::oneshot::Sender<Result<Value>>>,
    /// Events received from the debug adapter, buffered for the client to drain.
    events: Vec<DapEvent>,
}

// ---------------------------------------------------------------------------
// DapClient
// ---------------------------------------------------------------------------

/// An async DAP client that communicates with a debug adapter over stdio.
///
/// # Lifecycle
///
/// 1. Create with [`DapClient::spawn`], which starts the adapter process and
///    performs the DAP initialize handshake.
/// 2. Call methods like [`launch`], [`set_breakpoint`], [`continue_execution`], etc.
/// 3. Call [`terminate`] to cleanly stop the session.
///
/// A background task reads the adapter's stdout, dispatching responses to
/// pending requests and buffering events.
pub struct DapClient {
    /// The child process handle (dropped on terminate).
    process: Option<Child>,
    /// Stdin pipe to the adapter (wrapped in BufWriter for efficiency).
    stdin: Option<BufWriter<tokio::process::ChildStdin>>,
    /// Join handle for the background reader task.
    reader_handle: tokio::task::JoinHandle<()>,
    /// Next request sequence number.
    next_id: u64,
    /// Shared state (pending requests + events).
    shared: Arc<Mutex<DapShared>>,
    /// The current debug session state.
    session: DebugSession,
}

impl DapClient {
    /// Spawn a debug adapter and perform the DAP initialize handshake.
    ///
    /// `command` is the adapter executable and its arguments (e.g.
    /// `&["debugpy-adapter"]`).
    pub async fn spawn(command: &[String]) -> Result<Self> {
        info!("Spawning DAP adapter: {:?}", command);

        let mut child = Command::new(&command[0])
            .args(&command[1..])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn DAP adapter process")?;

        let stdin = child.stdin.take().context("Failed to open stdin")?;
        let stdout = child.stdout.take().context("Failed to open stdout")?;

        let shared = Arc::new(Mutex::new(DapShared {
            pending: HashMap::new(),
            events: Vec::new(),
        }));

        let mut client = Self {
            process: Some(child),
            stdin: Some(BufWriter::new(stdin)),
            reader_handle: tokio::task::spawn(async {}),
            next_id: 1,
            shared: shared.clone(),
            session: DebugSession::new(),
        };

        // Start the background reader task.
        client.reader_handle = tokio::task::spawn(async move {
            Self::reader_loop(stdout, shared).await;
        });

        // Perform the initialize handshake.
        client.initialize().await?;

        Ok(client)
    }

    // ------------------------------------------------------------------
    // Public API methods
    // ------------------------------------------------------------------

    /// Send the DAP initialize request and return the adapter's capabilities.
    ///
    /// This is called automatically by [`spawn`], but can be called manually
    /// if you need to re-initialize.
    pub async fn initialize(&mut self) -> Result<Value> {
        info!("Sending DAP initialize request");

        let args = json!({
            "clientID": "aether-ide",
            "adapterID": "dap",
            "supportsRunInTerminalRequest": false,
            "locale": "en",
        });

        let result = self.send_request("initialize", args).await?;
        info!("DAP adapter initialized successfully");

        // Send initialized event (notification, no response expected).
        let _ = self.send_request("initialized", json!({})).await;

        self.session.state = DebugState::Running;

        Ok(result)
    }

    /// Launch the debugee with the given program path and arguments.
    ///
    /// `program` is the path to the executable or script to debug.
    /// `args` are the command-line arguments to pass to the program.
    /// Additional launch options can be provided via `extra_args`.
    pub async fn launch(
        &mut self,
        program: &str,
        args: &[String],
        extra_args: Option<Value>,
    ) -> Result<Value> {
        info!("Launching debuggee: {} {:?}", program, args);

        let mut launch_args = json!({
            "program": program,
            "args": args,
            "cwd": std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "." .to_string()),
            "console": "internalConsole",
        });

        // Merge any extra arguments.
        if let Some(extra) = extra_args {
            if let Some(obj) = extra.as_object() {
                for (key, val) in obj {
                    launch_args[key] = val.clone();
                }
            }
        }

        let result = self.send_request("launch", launch_args).await?;
        self.session.state = DebugState::Running;

        Ok(result)
    }

    /// Set a breakpoint at the given file and line.
    ///
    /// Returns the breakpoint as confirmed by the adapter.
    /// `file` is the source file path.
    /// `line` is the 1-based line number.
    /// `condition` is an optional break condition expression.
    pub async fn set_breakpoint(
        &mut self,
        file: &str,
        line: u64,
        condition: Option<&str>,
    ) -> Result<Breakpoint> {
        info!("Setting breakpoint at {}:{}", file, line);

        // Collect existing breakpoints for this file.
        let mut bps: Vec<Value> = self
            .session
            .breakpoints
            .iter()
            .filter(|bp| bp.file == file)
            .map(|bp| {
                let mut bp_obj = json!({
                    "line": bp.line,
                });
                if let Some(ref cond) = bp.condition {
                    bp_obj["condition"] = Value::String(cond.clone());
                }
                bp_obj
            })
            .collect();

        // Add the new breakpoint.
        let mut new_bp = json!({
            "line": line,
        });
        if let Some(cond) = condition {
            new_bp["condition"] = Value::String(cond.to_string());
        }
        bps.push(new_bp);

        let args = json!({
            "source": {
                "path": file,
            },
            "breakpoints": bps,
        });

        let result = self.send_request("setBreakpoints", args).await?;

        // Parse the response to extract the confirmed breakpoints.
        let confirmed_bps = result
            .get("breakpoints")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Update our local breakpoint list for this file.
        let mut new_breakpoints: Vec<Breakpoint> = Vec::new();
        for bp_val in &confirmed_bps {
            if let Some(bp) = parse_breakpoint(bp_val, file) {
                new_breakpoints.push(bp);
            }
        }

        // Replace breakpoints for this file, keep others.
        self.session.breakpoints.retain(|bp| bp.file != file);
        self.session.breakpoints.extend(new_breakpoints.clone());

        // Return the last breakpoint (the one we just set).
        new_breakpoints
            .into_iter()
            .last()
            .context("Adapter did not confirm any breakpoints")
    }

    /// Clear a specific breakpoint by file and line.
    pub async fn clear_breakpoint(&mut self, file: &str, line: u64) -> Result<()> {
        info!("Clearing breakpoint at {}:{}", file, line);

        // Remove from our local list.
        self.session
            .breakpoints
            .retain(|bp| !(bp.file == file && bp.line == line));

        // Re-send the remaining breakpoints for this file to the adapter.
        let remaining: Vec<Value> = self
            .session
            .breakpoints
            .iter()
            .filter(|bp| bp.file == file)
            .map(|bp| {
                let mut bp_obj = json!({
                    "line": bp.line,
                });
                if let Some(ref cond) = bp.condition {
                    bp_obj["condition"] = Value::String(cond.clone());
                }
                bp_obj
            })
            .collect();

        let args = json!({
            "source": {
                "path": file,
            },
            "breakpoints": remaining,
        });

        let _result = self.send_request("setBreakpoints", args).await?;

        Ok(())
    }

    /// Return all active breakpoints.
    pub fn list_breakpoints(&self) -> &[Breakpoint] {
        &self.session.breakpoints
    }

    /// Continue execution after a pause.
    pub async fn continue_execution(&mut self) -> Result<Value> {
        info!("Continuing execution");

        // Use threadId 0 (all threads) or the first stopped thread.
        let thread_id = self
            .session
            .stack_frames
            .first()
            .map(|_| 0u64)
            .unwrap_or(0);

        let args = json!({
            "threadId": thread_id,
        });

        let result = self.send_request("continue", args).await?;
        self.session.state = DebugState::Running;

        Ok(result)
    }

    /// Step over (next line).
    pub async fn next(&mut self) -> Result<Value> {
        info!("Step over");

        let thread_id = 1u64; // Default to thread 1; real impl would track this.
        let args = json!({
            "threadId": thread_id,
        });

        let result = self.send_request("next", args).await?;
        self.session.state = DebugState::Running;

        Ok(result)
    }

    /// Step into a function call.
    pub async fn step_in(&mut self) -> Result<Value> {
        info!("Step in");

        let thread_id = 1u64;
        let args = json!({
            "threadId": thread_id,
        });

        let result = self.send_request("stepIn", args).await?;
        self.session.state = DebugState::Running;

        Ok(result)
    }

    /// Step out of the current function.
    pub async fn step_out(&mut self) -> Result<Value> {
        info!("Step out");

        let thread_id = 1u64;
        let args = json!({
            "threadId": thread_id,
        });

        let result = self.send_request("stepOut", args).await?;
        self.session.state = DebugState::Running;

        Ok(result)
    }

    /// Pause execution.
    pub async fn pause(&mut self) -> Result<Value> {
        info!("Pausing execution");

        let thread_id = 1u64;
        let args = json!({
            "threadId": thread_id,
        });

        let result = self.send_request("pause", args).await?;
        self.session.state = DebugState::Paused;

        Ok(result)
    }

    /// Terminate the debug session gracefully.
    pub async fn terminate(&mut self) -> Result<()> {
        info!("Terminating debug session");

        // Send terminate request.
        let _ = self.send_request("terminate", json!({})).await;

        // Send disconnect request.
        let _ = self.send_request("disconnect", json!({})).await;

        // Close stdin to signal EOF.
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.shutdown().await;
        }

        // Wait for the process to exit.
        if let Some(mut process) = self.process.take() {
            let _ = process.wait().await;
        }

        // Abort the reader task.
        self.reader_handle.abort();

        self.session.state = DebugState::Terminated;

        Ok(())
    }

    /// Get the current call stack.
    ///
    /// Returns the stack frames for the given thread (defaults to thread 1).
    pub async fn get_stack_trace(&mut self, thread_id: Option<u64>) -> Result<Vec<StackFrame>> {
        let tid = thread_id.unwrap_or(1);
        info!("Getting stack trace for thread {}", tid);

        let args = json!({
            "threadId": tid,
            "startFrame": 0,
            "levels": 20,
        });

        let result = self.send_request("stackTrace", args).await?;

        let mut frames = Vec::new();
        if let Some(stack_frames) = result.get("stackFrames").and_then(|v| v.as_array()) {
            for frame_val in stack_frames {
                if let Some(frame) = parse_stack_frame(frame_val) {
                    frames.push(frame);
                }
            }
        }

        self.session.stack_frames = frames.clone();
        Ok(frames)
    }

    /// Get variables for a stack frame.
    ///
    /// `frame_id` is the stack frame ID from [`get_stack_trace`].
    /// Returns the top-level variables visible in that frame.
    pub async fn get_variables(&mut self, frame_id: u64) -> Result<Vec<Variable>> {
        info!("Getting variables for frame {}", frame_id);

        let args = json!({
            "variablesReference": frame_id,
        });

        let result = self.send_request("variables", args).await?;

        let mut vars = Vec::new();
        if let Some(variables) = result.get("variables").and_then(|v| v.as_array()) {
            for var_val in variables {
                if let Some(var) = parse_variable(var_val) {
                    vars.push(var);
                }
            }
        }

        self.session.variables = vars.clone();
        Ok(vars)
    }

    /// Get child variables for a composite variable.
    ///
    /// `variables_reference` is the `variables_reference` field from a
    /// [`Variable`] returned by [`get_variables`]. Use this to drill into
    /// objects, arrays, and other composite values.
    pub async fn get_child_variables(&mut self, variables_reference: u64) -> Result<Vec<Variable>> {
        info!(
            "Getting child variables for reference {}",
            variables_reference
        );

        let args = json!({
            "variablesReference": variables_reference,
        });

        let result = self.send_request("variables", args).await?;

        let mut vars = Vec::new();
        if let Some(variables) = result.get("variables").and_then(|v| v.as_array()) {
            for var_val in variables {
                if let Some(var) = parse_variable(var_val) {
                    vars.push(var);
                }
            }
        }

        Ok(vars)
    }

    /// Evaluate an expression in the current debug context.
    ///
    /// `expression` is the expression to evaluate (e.g. `"x + 1"`).
    /// `frame_id` is the stack frame ID to evaluate in (from [`get_stack_trace`]).
    pub async fn evaluate(&mut self, expression: &str, frame_id: u64) -> Result<Variable> {
        info!("Evaluating expression: '{}' in frame {}", expression, frame_id);

        let args = json!({
            "expression": expression,
            "frameId": frame_id,
            "context": "repl",
        });

        let result = self.send_request("evaluate", args).await?;

        let name = expression.to_string();
        let value = result
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("<no result>")
            .to_string();
        let type_name = result
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let variables_reference = result
            .get("variablesReference")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok(Variable {
            name,
            value,
            type_name,
            variables_reference,
        })
    }

    /// Drain any buffered events from the debug adapter.
    ///
    /// Call this periodically (e.g. after each debugger command) to process
    /// events like breakpoint hits, exceptions, and thread changes.
    pub async fn drain_events(&mut self) -> Vec<DapEvent> {
        let mut shared = self.shared.lock().await;
        let events = std::mem::take(&mut shared.events);
        drop(shared);

        // Update session state based on events.
        for event in &events {
            match event {
                &DapEvent::BreakpointHit { .. }
                | &DapEvent::Exception { .. }
                | &DapEvent::StepComplete { .. } => {
                    self.session.state = DebugState::Paused;
                }
                &DapEvent::ProcessExited { .. } => {
                    self.session.state = DebugState::Terminated;
                }
                _ => {}
            }
        }

        events
    }

    /// Get a reference to the current debug session state.
    pub fn session(&self) -> &DebugSession {
        &self.session
    }

    /// Get a mutable reference to the current debug session state.
    pub fn session_mut(&mut self) -> &mut DebugSession {
        &mut self.session
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Send a DAP request and await the response.
    async fn send_request(&mut self, command: &str, arguments: Value) -> Result<Value> {
        let seq = self.next_id;
        self.next_id += 1;

        let (tx, rx) = tokio::sync::oneshot::channel();

        // Register the pending request in shared state.
        {
            let mut shared = self.shared.lock().await;
            shared.pending.insert(seq, tx);
        }

        let msg = build_dap_message(seq, command, arguments);
        self.write_message(&msg).await?;

        // Await the response from the reader task.
        rx.await
            .context("DAP adapter closed before responding")?
    }

    /// Write a DAP message to the adapter's stdin.
    async fn write_message(&mut self, msg: &str) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("DAP stdin not available (session may be terminated)")?;

        let header = format!("Content-Length: {}\r\n\r\n", msg.len());
        stdin
            .write_all(header.as_bytes())
            .await
            .context("Failed to write DAP header")?;
        stdin
            .write_all(msg.as_bytes())
            .await
            .context("Failed to write DAP body")?;
        stdin
            .flush()
            .await
            .context("Failed to flush DAP stdin")?;

        Ok(())
    }

    /// Background reader loop that processes adapter stdout.
    ///
    /// Reads Content-Length framed JSON-RPC messages, dispatches responses
    /// to pending requests, and buffers events for the client to drain.
    async fn reader_loop(
        stdout: tokio::process::ChildStdout,
        shared: Arc<Mutex<DapShared>>,
    ) {
        let mut reader = BufReader::new(stdout);
        let mut buf = String::new();

        loop {
            buf.clear();

            // Read the header line: "Content-Length: N\r\n"
            match reader.read_line(&mut buf).await {
                Ok(0) => {
                    info!("DAP adapter stdout closed");
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("Error reading DAP header: {}", e);
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
                warn!("Error reading DAP body: {}", e);
                break;
            }

            let text = String::from_utf8_lossy(&body);
            let msg: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Failed to parse DAP message: {} — body: {}", e, text);
                    continue;
                }
            };

            // Determine message type.
            let msg_type = msg
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            match msg_type {
                "response" => {
                    // Response to a request.
                    let request_seq = msg["request_seq"].as_u64().unwrap_or(0);
                    let success = msg.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

                    let result = if success {
                        Ok(msg.get("body").cloned().unwrap_or(Value::Null))
                    } else {
                        let err_msg = msg
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error");
                        Err(anyhow::anyhow!("DAP error: {}", err_msg))
                    };

                    // Complete the pending request via shared state.
                    let mut shared = shared.lock().await;
                    if let Some(sender) = shared.pending.remove(&request_seq) {
                        let _ = sender.send(result);
                    } else {
                        warn!(
                            "Received response for unknown request seq: {}",
                            request_seq
                        );
                    }
                }
                "event" => {
                    // Event from the adapter.
                    let event_name = msg
                        .get("event")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let body = msg.get("body");

                    let dap_event = parse_dap_event(event_name, body);
                    if let Some(event) = dap_event {
                        let mut shared = shared.lock().await;
                        shared.events.push(event);
                    } else {
                        info!("Received unhandled DAP event: {}", event_name);
                    }
                }
                _ => {
                    warn!("Unknown DAP message type: {}", msg_type);
                }
            }
        }
    }
}

impl Drop for DapClient {
    fn drop(&mut self) {
        // The process has kill_on_drop(true), so it will be killed when
        // the Child handle is dropped.
        self.reader_handle.abort();
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC message building
// ---------------------------------------------------------------------------

/// Build a DAP JSON-RPC request message string.
///
/// DAP messages use `seq` instead of `id`, and have a `type` field set to
/// `"request"`. Arguments are placed in the `arguments` field.
fn build_dap_message(seq: u64, command: &str, arguments: Value) -> String {
    let mut msg = serde_json::Map::new();
    msg.insert("seq".to_string(), Value::Number(serde_json::Number::from(seq)));
    msg.insert(
        "type".to_string(),
        Value::String("request".to_string()),
    );
    msg.insert(
        "command".to_string(),
        Value::String(command.to_string()),
    );
    msg.insert("arguments".to_string(), arguments);
    serde_json::to_string(&Value::Object(msg)).expect("DAP message must serialize")
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

/// Parse a DAP breakpoint value into a `Breakpoint`.
fn parse_breakpoint(val: &Value, file: &str) -> Option<Breakpoint> {
    let obj = val.as_object()?;
    let id = obj.get("id").and_then(|v| v.as_u64())?;
    let line = obj.get("line").and_then(|v| v.as_u64())?;
    let verified = obj.get("verified").and_then(|v| v.as_bool()).unwrap_or(false);
    let condition = obj
        .get("condition")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(Breakpoint {
        id,
        file: file.to_string(),
        line,
        verified,
        condition,
    })
}

/// Parse a DAP stack frame value into a `StackFrame`.
fn parse_stack_frame(val: &Value) -> Option<StackFrame> {
    let obj = val.as_object()?;
    let id = obj.get("id").and_then(|v| v.as_u64())?;
    let name = obj.get("name").and_then(|v| v.as_str())?.to_string();
    let line = obj.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
    let column = obj.get("column").and_then(|v| v.as_u64()).unwrap_or(0);

    // Source path may be nested under "source.path".
    let file = obj
        .get("source")
        .and_then(|s| s.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>")
        .to_string();

    Some(StackFrame {
        id,
        name,
        file,
        line,
        column,
    })
}

/// Parse a DAP variable value into a `Variable`.
fn parse_variable(val: &Value) -> Option<Variable> {
    let obj = val.as_object()?;
    let name = obj.get("name").and_then(|v| v.as_str())?.to_string();
    let value = obj
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("<unavailable>")
        .to_string();
    let type_name = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let variables_reference = obj
        .get("variablesReference")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Some(Variable {
        name,
        value,
        type_name,
        variables_reference,
    })
}

/// Parse a DAP event into a `DapEvent`.
fn parse_dap_event(event_name: &str, body: Option<&Value>) -> Option<DapEvent> {
    match event_name {
        "stopped" => {
            let body = body?;
            let reason = body
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let thread_id = body.get("threadId").and_then(|v| v.as_u64()).unwrap_or(0);

            match reason.as_str() {
                "breakpoint" | "entry" | "data breakpoint" | "function breakpoint" => {
                    Some(DapEvent::BreakpointHit { thread_id, reason })
                }
                "exception" => {
                    let description = body
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    Some(DapEvent::Exception {
                        thread_id,
                        description,
                    })
                }
                "step" => Some(DapEvent::StepComplete { thread_id }),
                _ => Some(DapEvent::BreakpointHit { thread_id, reason }),
            }
        }
        "continued" => {
            // The debugee resumed execution; we don't need a specific event
            // for this since the client tracks state via its own commands.
            None
        }
        "thread" => {
            let body = body?;
            let reason = body
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("started");
            let thread_id = body.get("threadId").and_then(|v| v.as_u64()).unwrap_or(0);

            match reason {
                "started" => Some(DapEvent::ThreadStarted { thread_id }),
                "exited" => Some(DapEvent::ThreadExited { thread_id }),
                _ => None,
            }
        }
        "exited" => {
            let exit_code = body
                .and_then(|b| b.get("exitCode"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            Some(DapEvent::ProcessExited { exit_code })
        }
        "terminated" => {
            // The debug session ended; treat as process exit with code 0.
            Some(DapEvent::ProcessExited { exit_code: 0 })
        }
        "output" => {
            // Console output from the debugee — we log it but don't emit
            // a DapEvent for it. The caller can use drain_events to get
            // structured events only.
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_dap_message() {
        let msg = build_dap_message(
            1,
            "initialize",
            json!({"clientID": "aether-ide", "adapterID": "python"}),
        );
        let parsed: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["seq"], 1);
        assert_eq!(parsed["type"], "request");
        assert_eq!(parsed["command"], "initialize");
        assert_eq!(parsed["arguments"]["clientID"], "aether-ide");
    }

    #[test]
    fn test_parse_content_length() {
        assert_eq!(parse_content_length("Content-Length: 123\r\n"), Some(123));
        assert_eq!(parse_content_length("content-length: 456"), Some(456));
        assert_eq!(parse_content_length("Content-Length: abc"), None);
        assert_eq!(parse_content_length(""), None);
    }

    #[test]
    fn test_parse_breakpoint() {
        let json = json!({
            "id": 1,
            "line": 42,
            "verified": true,
            "condition": "x > 5",
        });
        let bp = parse_breakpoint(&json, "/test.py").unwrap();
        assert_eq!(bp.id, 1);
        assert_eq!(bp.file, "/test.py");
        assert_eq!(bp.line, 42);
        assert!(bp.verified);
        assert_eq!(bp.condition.unwrap(), "x > 5");
    }

    #[test]
    fn test_parse_breakpoint_no_condition() {
        let json = json!({
            "id": 2,
            "line": 10,
            "verified": false,
        });
        let bp = parse_breakpoint(&json, "/test.rs").unwrap();
        assert_eq!(bp.id, 2);
        assert_eq!(bp.line, 10);
        assert!(!bp.verified);
        assert!(bp.condition.is_none());
    }

    #[test]
    fn test_parse_stack_frame() {
        let json = json!({
            "id": 1001,
            "name": "main",
            "source": { "path": "/test.py" },
            "line": 42,
            "column": 5,
        });
        let frame = parse_stack_frame(&json).unwrap();
        assert_eq!(frame.id, 1001);
        assert_eq!(frame.name, "main");
        assert_eq!(frame.file, "/test.py");
        assert_eq!(frame.line, 42);
        assert_eq!(frame.column, 5);
    }

    #[test]
    fn test_parse_stack_frame_no_source() {
        let json = json!({
            "id": 1002,
            "name": "helper",
            "line": 10,
            "column": 0,
        });
        let frame = parse_stack_frame(&json).unwrap();
        assert_eq!(frame.file, "<unknown>");
    }

    #[test]
    fn test_parse_variable() {
        let json = json!({
            "name": "x",
            "value": "42",
            "type": "int",
            "variablesReference": 0,
        });
        let var = parse_variable(&json).unwrap();
        assert_eq!(var.name, "x");
        assert_eq!(var.value, "42");
        assert_eq!(var.type_name, "int");
        assert_eq!(var.variables_reference, 0);
    }

    #[test]
    fn test_parse_variable_with_children() {
        let json = json!({
            "name": "my_list",
            "value": "[1, 2, 3]",
            "type": "list",
            "variablesReference": 5,
        });
        let var = parse_variable(&json).unwrap();
        assert_eq!(var.name, "my_list");
        assert_eq!(var.variables_reference, 5);
    }

    #[test]
    fn test_parse_dap_event_breakpoint_hit() {
        let body = json!({
            "reason": "breakpoint",
            "threadId": 1,
        });
        let event = parse_dap_event("stopped", Some(&body)).unwrap();
        match event {
            DapEvent::BreakpointHit { thread_id, reason } => {
                assert_eq!(thread_id, 1);
                assert_eq!(reason, "breakpoint");
            }
            _ => panic!("Expected BreakpointHit event"),
        }
    }

    #[test]
    fn test_parse_dap_event_exception() {
        let body = json!({
            "reason": "exception",
            "threadId": 1,
            "description": "division by zero",
        });
        let event = parse_dap_event("stopped", Some(&body)).unwrap();
        match event {
            DapEvent::Exception {
                thread_id,
                description,
            } => {
                assert_eq!(thread_id, 1);
                assert_eq!(description.unwrap(), "division by zero");
            }
            _ => panic!("Expected Exception event"),
        }
    }

    #[test]
    fn test_parse_dap_event_step_complete() {
        let body = json!({
            "reason": "step",
            "threadId": 1,
        });
        let event = parse_dap_event("stopped", Some(&body)).unwrap();
        match event {
            DapEvent::StepComplete { thread_id } => {
                assert_eq!(thread_id, 1);
            }
            _ => panic!("Expected StepComplete event"),
        }
    }

    #[test]
    fn test_parse_dap_event_thread_started() {
        let body = json!({
            "reason": "started",
            "threadId": 2,
        });
        let event = parse_dap_event("thread", Some(&body)).unwrap();
        match event {
            DapEvent::ThreadStarted { thread_id } => {
                assert_eq!(thread_id, 2);
            }
            _ => panic!("Expected ThreadStarted event"),
        }
    }

    #[test]
    fn test_parse_dap_event_thread_exited() {
        let body = json!({
            "reason": "exited",
            "threadId": 2,
        });
        let event = parse_dap_event("thread", Some(&body)).unwrap();
        match event {
            DapEvent::ThreadExited { thread_id } => {
                assert_eq!(thread_id, 2);
            }
            _ => panic!("Expected ThreadExited event"),
        }
    }

    #[test]
    fn test_parse_dap_event_process_exited() {
        let body = json!({
            "exitCode": 0,
        });
        let event = parse_dap_event("exited", Some(&body)).unwrap();
        match event {
            DapEvent::ProcessExited { exit_code } => {
                assert_eq!(exit_code, 0);
            }
            _ => panic!("Expected ProcessExited event"),
        }
    }

    #[test]
    fn test_parse_dap_event_terminated() {
        let event = parse_dap_event("terminated", None).unwrap();
        match event {
            DapEvent::ProcessExited { exit_code } => {
                assert_eq!(exit_code, 0);
            }
            _ => panic!("Expected ProcessExited event"),
        }
    }

    #[test]
    fn test_dap_config() {
        let config = DapConfig::new()
            .register("python", &["debugpy-adapter"])
            .register("rust", &["lldb-vscode"]);
        assert_eq!(
            config.command_for("python").unwrap(),
            &["debugpy-adapter"]
        );
        assert_eq!(
            config.command_for("rust").unwrap(),
            &["lldb-vscode"]
        );
        assert!(config.command_for("go").is_none());
    }

    #[test]
    fn test_debug_session_initial_state() {
        let session = DebugSession::new();
        assert_eq!(session.state, DebugState::Inactive);
        assert!(session.breakpoints.is_empty());
        assert!(session.stack_frames.is_empty());
        assert!(session.variables.is_empty());
    }

    #[test]
    fn test_dap_config_default() {
        let config = DapConfig::default();
        assert!(config.adapters.is_empty());
    }
}
