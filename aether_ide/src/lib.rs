//! AetherOS Audio-First IDE
//!
//! Core library for the audio-first IDE. Provides:
//! - Tree-sitter based code parsing and structure extraction
//! - Code sonification (nesting depth → pitch, scope boundaries → earcons)
//! - LSP client for language intelligence
//! - DAP client for debugging
//! - JSON-RPC IPC for communication with the Python voice UI

#![recursion_limit = "256"]

pub mod buffer;
pub mod dap;
pub mod editor;
pub mod ipc;
pub mod lsp;
pub mod navigation;
pub mod parser;
pub mod project;
pub mod sonifier;

use anyhow::Result;
use std::sync::Arc;
use parking_lot::RwLock;

/// The IDE state, shared between the IPC server and all subsystems.
pub struct IdeState {
    /// Currently open file path.
    pub current_file: Arc<RwLock<Option<String>>>,
    /// Current cursor position (0-indexed line, column).
    pub cursor: Arc<RwLock<(usize, usize)>>,
    /// The parsed tree for the current file.
    pub tree: Arc<RwLock<Option<parser::CodeTree>>>,
}

impl IdeState {
    pub fn new() -> Self {
        Self {
            current_file: Arc::new(RwLock::new(None)),
            cursor: Arc::new(RwLock::new((0, 0))),
            tree: Arc::new(RwLock::new(None)),
        }
    }
}
