# Artume Audio-First IDE — Comprehensive Plan

## Core Philosophy

> "If you can't use it completely with your eyes closed, it's not good enough."

This is not an IDE with audio bolted on. This is an IDE where **audio is the primary rendering channel** and visual display is optional. Every piece of information a sighted programmer gets from glancing at code must have an audio equivalent that can be consumed at least as fast.

---

## 1. Current State Assessment

### What exists (Python, `artome_core.py` ecosystem):

| Component | Status | Limitation |
|-----------|--------|------------|
| `ide_engine.py` | Basic AST reader | Reads functions/lines aloud. No navigation, no editing, no debugging. |
| `screen_reader.py` | AT-SPI2 tree dump | Dumps raw accessibility tree. No code-specific intelligence. |
| `earcons.py` | Simple earcon system | 4 sounds (listening, thinking, success, error). No code-specific earcons. |
| `audio_engine.py` | Piper TTS + VAD | Piper quality is poor. Barge-in works but is basic. |
| `intent_router.py` | Nemotron routing | Routes to IDE mode. No code-specific intent handling. |
| `command_navigator.py` | Menu system | IDE mode has 3 commands: open, read function, read lines. |

### What's missing (everything):

- **Code structure audio rendering** — no way to hear the shape of a file
- **Multi-file project navigation** — no project tree, no go-to-definition
- **Code editing** — no insert, delete, replace, refactor
- **Debugging** — no breakpoints, step-through, variable inspection
- **Git integration** — no diff, status, commit
- **Terminal** — no integrated shell
- **LSP integration** — no type checking, completions, diagnostics
- **Tree-sitter** — no fast incremental parsing
- **Sonification** — no audio representation of code structure

---

## 2. Architecture

### Layered Design

```
┌─────────────────────────────────────────────────────┐
│                  Voice Interface                      │
│  (wake word → listen → process → TTS + earcons)     │
├─────────────────────────────────────────────────────┤
│                  Intent Router                        │
│  (Nemotron-3 Nano on 1650S → classify intent)        │
├─────────────────────────────────────────────────────┤
│              Audio IDE Controller                     │
│  (manages state, dispatches to subsystems)            │
├──────────┬──────────┬──────────┬──────────┬─────────┤
│  Code    │  Proj    │  Debug   │  Git     │  Term   │
│  Engine  │  Nav     │  Engine  │  Engine  │  Engine │
├──────────┴──────────┴──────────┴──────────┴─────────┤
│              LSP Client (JSON-RPC)                    │
│              Tree-sitter Parser                       │
│              File System Watcher                      │
└─────────────────────────────────────────────────────┘
```

### Integration with Artume OS

- **Rust shell** (`aether_orchestrator`): LSP client, tree-sitter parsing, file watching — performance-critical, runs as daemon
- **Python desktop** (`artome_core.py`): Voice interface, TTS, earcons, screen reader — user-facing
- **Skills system** (`artume_skills.py`): IDE skills registered as pluggable capabilities
- **Dual-GPU**: Nemotron for routing, Llama for code reasoning/editing

---

## 3. Audio Rendering of Code — The Core Innovation

### 3.1 Code Structure Sonification

The fundamental problem: a sighted programmer sees the **shape** of code instantly — indentation depth, function boundaries, comment density. Audio must convey this.

#### Nesting Depth → Pitch

| Depth | Audio Representation |
|-------|---------------------|
| 0 (top-level) | Low C2 (65 Hz) drone |
| 1 (class/module) | C3 (131 Hz) |
| 2 (function/method) | C4 (262 Hz) — default speaking pitch |
| 3 (if/for/while) | C5 (523 Hz) |
| 4+ (nested) | Rising pitch + faster tempo |

When navigating through code, a **background pitch drone** indicates current nesting depth. Moving deeper = pitch rises. Moving shallower = pitch drops. This gives continuous spatial awareness without reading a single word.

#### Scope Boundaries → Earcons

| Event | Earcon |
|-------|--------|
| Enter function | Rising arpeggio (C-E-G) over 150ms |
| Exit function | Falling arpeggio (G-E-C) over 150ms |
| Enter class | Rising major chord held 300ms |
| Exit class | Falling major chord held 300ms |
| Enter loop | Soft pulse (440 Hz, 80ms, repeated) |
| Enter conditional | Short double-beep (660+440 Hz) |
| Comment block | Soft chime (880 Hz, 50ms) |
| Blank line (visual gap) | 100ms silence (proportional to gap size) |

#### Code Density → Texture

- **Dense code** (many tokens per line) → faster tick rate in background
- **Sparse code** (blank lines, comments) → slower tick rate, more silence
- **Long lines** (>80 chars) → warning buzz at end of line

### 3.2 Code Reading Modes

| Mode | Trigger | Behavior |
|------|---------|----------|
| **Skim** | "skim this file" | Read only function/class signatures + line counts. Play structure earcons. 2-3 seconds per 100 lines. |
| **Read** | "read function foo" | Full TTS of code with structural earcons at boundaries. Variable names spelled character-by-character on first mention. |
| **Structure** | "show structure" | Tree-shaped audio: "File has 3 classes. Class Database has 5 methods. Method connect has 2 branches: if-else and try-catch." |
| **Diff** | "what changed" | Read only modified lines with "added"/"removed"/"changed" prefix earcons. |
| **Problems** | "show errors" | Read only lines with LSP diagnostics, prioritized by severity. Error earcon + line content. |

### 3.3 Variable Name Rendering

Variable names are the hardest part of code TTS. Strategy:

1. **First mention**: Spell out character by character at high speed (using phonetics: "alpha, beta, gamma" not "a, b, g")
2. **Subsequent mentions**: Use the spoken name if it's a real word ("count", "name"), spell if it's an abbreviation
3. **CamelCase/snake_case**: Split and speak naturally: "userName" → "user name", "MAX_RETRY_COUNT" → "max retry count"
4. **Single-letter variables**: Always spell: "i" → "eye", "x" → "eks"
5. **Type annotations**: Speak in lower register, faster: `name: str` → "name colon str" (with "colon" whispered)

---

## 4. Voice Navigation

### 4.1 Movement Commands

| Voice Command | Action |
|---------------|--------|
| "go to function [name]" | Jump to function definition |
| "go to class [name]" | Jump to class definition |
| "go to line [number]" | Jump to line number |
| "go to definition" | LSP go-to-definition on symbol under cursor |
| "go back" | Return to previous location |
| "go to next function" | Jump to next function boundary |
| "go to previous function" | Jump to previous function boundary |
| "go to next problem" | Jump to next LSP diagnostic |
| "go to file [name]" | Fuzzy-find file in project |
| "scroll up/down [N] lines" | Move cursor N lines |
| "top of file" / "bottom of file" | Jump to start/end |
| "where am I" | Read current location: "Line 42 of database.py, inside function connect, 3 levels deep" |

### 4.2 Context Queries

| Voice Command | Response |
|---------------|----------|
| "what's around me" | Read ±5 lines with structure earcons |
| "what's this function" | Read signature + docstring of enclosing function |
| "what are the parameters" | Read function signature only |
| "what type is [variable]" | LSP hover type information |
| "who calls this" | LSP find references |
| "show me the callers" | List all call sites with file+line |
| "what's the return type" | Read return type annotation |
| "is this async" | "Yes, function connect is async" |
| "what imports this file" | LSP find references at file level |

### 4.3 Spatial Audio (3D Sound)

Using the existing `aether_audio` spatial mixer:

| Source | Position | Purpose |
|--------|----------|---------|
| Current line | Centre | Primary reading |
| Line above | Soft left | Context before cursor |
| Line below | Soft right | Context after cursor |
| Error/warning | Hard right 45° | Diagnostic alerts |
| Function boundary | Hard left 45° | Scope change cues |
| Background drone | Centre, low volume | Nesting depth awareness |

This creates a **3D audio workspace** where the programmer can perceive code structure spatially without reading.

---

## 5. Code Editing Through Voice

### 5.1 Insertion Commands

| Voice Command | Action |
|---------------|--------|
| "insert [code] before/after" | Insert code at cursor |
| "add line before/after" | Insert blank line |
| "duplicate line" | Copy current line below |
| "paste" | Insert clipboard content |

### 5.2 Deletion Commands

| Voice Command | Action |
|---------------|--------|
| "delete line" | Remove current line |
| "delete [N] lines" | Remove N lines from cursor |
| "delete function" | Remove enclosing function |
| "delete to end of line" | Remove from cursor to EOL |
| "cut" | Delete and copy to clipboard |

### 5.3 Modification Commands

| Voice Command | Action |
|---------------|--------|
| "change [old] to [new]" | Find and replace in selection |
| "rename [old] to [new]" | LSP rename symbol |
| "wrap in try-catch" | Insert try-catch around selection |
| "extract function [name]" | Extract selection to new function |
| "format" | Run formatter on file |
| "indent" / "outdent" | Change indentation |
| "comment line" / "uncomment line" | Toggle comment |

### 5.4 AI-Assisted Editing (Llama 3.1 8B on 1080)

| Voice Command | Action |
|---------------|--------|
| "fix this" | Send current function to LLM with error context, get fix |
| "explain this" | LLM explains current selection |
| "add docstring" | LLM generates docstring for function |
| "add type hints" | LLM adds type annotations |
| "write a test for this" | LLM generates test for current function |
| "refactor this to use [pattern]" | LLM refactors selection |
| "optimize this" | LLM suggests performance improvements |
| "what's wrong with this" | LLM reviews current function |

---

## 6. Debugging Through Audio

### 6.1 Breakpoint Management

| Voice Command | Action |
|---------------|--------|
| "set breakpoint" | Toggle breakpoint on current line |
| "set breakpoint at line [N]" | Set breakpoint at specific line |
| "set conditional breakpoint [expr]" | Set breakpoint with condition |
| "list breakpoints" | Read all breakpoints with file+line |
| "clear breakpoint" | Remove breakpoint on current line |
| "clear all breakpoints" | Remove all breakpoints |

### 6.2 Debug Session

| Voice Command | Action |
|---------------|--------|
| "start debugging" | Launch debug session |
| "continue" | Resume execution |
| "step over" | Step to next line |
| "step into" | Step into function call |
| "step out" | Return from current function |
| "restart" | Restart debug session |
| "stop debugging" | End debug session |

### 6.3 Variable Inspection

| Voice Command | Action |
|---------------|--------|
| "what's [variable]" | Read variable value |
| "show locals" | Read all local variables |
| "watch [expression]" | Add expression to watch list |
| "show watches" | Read all watched expressions |
| "evaluate [expression]" | Evaluate arbitrary expression |
| "show call stack" | Read stack trace with earcons per frame |

### 6.4 Error Audio

| Event | Audio |
|-------|-------|
| Exception thrown | Low growl (100 Hz, 500ms) + "Exception: [type] at [location]" |
| Breakpoint hit | Rising chime + "Breakpoint at [location]" |
| Step complete | Soft click (2ms) |
| Variable changed | Quick ascending tone |
| Watch triggered | Repeating pulse until acknowledged |

---

## 7. Project Navigation

### 7.1 File Tree

| Voice Command | Action |
|---------------|--------|
| "list files" | Read project file tree (depth-controlled) |
| "list files in [directory]" | Read directory contents |
| "open [file]" | Open file by fuzzy name |
| "recent files" | Read recently opened files |
| "close file" | Close current file |
| "next tab" / "previous tab" | Switch between open files |
| "list tabs" | Read all open files |

### 7.2 Git Integration

| Voice Command | Action |
|---------------|--------|
| "git status" | Read changed files with added/modified/deleted earcons |
| "git diff" | Read diff with line-by-line changes |
| "git log" | Read recent commits |
| "git commit [message]" | Stage all and commit |
| "git push" | Push to remote |
| "git pull" | Pull from remote |
| "what branch" | Read current branch name |
| "switch to [branch]" | Checkout branch |
| "show changes in [file]" | Read diff for specific file |

---

## 8. Terminal Integration

| Voice Command | Action |
|---------------|--------|
| "run [command]" | Execute shell command, read output |
| "run tests" | Run test suite, read results |
| "run [file]" | Execute current file |
| "show terminal" | Read last 10 lines of terminal |
| "clear terminal" | Clear terminal buffer |
| "stop" | Interrupt running command |

---

## 9. Implementation Phases

### Phase 1: Foundation (Weeks 1-2)

**Goal**: Replace the basic `ide_engine.py` with a working code reader that can navigate any file.

- [ ] **Tree-sitter integration** (Rust crate `tree-sitter` + Python bindings)
  - Parse Python, Rust, JavaScript, TypeScript, Go, JSON, TOML, YAML
  - Query for functions, classes, methods, conditionals, loops, comments
  - Incremental re-parsing on file change
- [ ] **Code structure audio renderer** (Rust `aether_ide` crate)
  - Nesting depth → pitch mapping
  - Scope boundary earcons
  - Code density → tempo mapping
- [ ] **Enhanced earcon system** (Python)
  - 20+ code-specific earcons (not just 4)
  - Earcon layering (multiple simultaneous sounds)
  - Earcon priority queue (errors interrupt reading)
- [ ] **Multi-mode code reader** (Python)
  - Skim, Read, Structure, Diff, Problems modes
  - Variable name rendering with camelCase/snake_case splitting
  - Type annotation rendering

### Phase 2: Navigation (Weeks 3-4)

**Goal**: Full voice navigation of code and projects.

- [ ] **LSP client** (Rust, `tower-lsp` or custom JSON-RPC)
  - go-to-definition, find-references, hover, completions
  - diagnostics streaming (push from server)
  - rename symbol
- [ ] **Project file indexer** (Rust, using existing `aetherfs-core`)
  - Index all project files by name, path, symbol
  - Fuzzy file search ("open database" → `database.py`, `db_manager.rs`)
  - Symbol search ("go to function connect")
- [ ] **Cursor movement** (Python)
  - Line-based movement with position tracking
  - Function/class boundary jumping
  - Location awareness ("where am I")
- [ ] **Spatial audio rendering** (Rust `aether_audio`)
  - 3D positioning for context lines
  - Background depth drone
  - Error direction cues

### Phase 3: Editing (Weeks 5-6)

**Goal**: Full code editing through voice.

- [ ] **Text buffer management** (Rust)
  - Piece table or rope data structure
  - Undo/redo stack
  - Clipboard
- [ ] **Voice-to-code translation** (Python + Llama 3.1 8B)
  - "insert for i in range(10):" → correct syntax
  - "change count to total" → find and replace
  - "wrap in try-catch" → structural edit
- [ ] **AI-assisted editing** (Llama 3.1 8B on GTX 1080)
  - Fix, explain, refactor, document, test generation
  - Context window: current function + 2 surrounding functions
- [ ] **Format on voice** (Python, `ruff`/`rustfmt`/`prettier`)
  - Format current file
  - Format selection

### Phase 4: Debugging (Weeks 7-8)

**Goal**: Full debugging through audio.

- [ ] **Debug adapter protocol (DAP) client** (Rust)
  - Launch, attach, breakpoints, continue, step
  - Variable evaluation, call stack
  - Watch expressions
- [ ] **Debug audio rendering** (Python + Rust)
  - Breakpoint hit → earcon + location
  - Exception → growl + error speech
  - Variable change → tone
  - Call stack → spatial audio (each frame at different position)
- [ ] **Error navigation** (Python)
  - "go to next error" → jump to next LSP diagnostic
  - "what's wrong" → read all errors in file
  - "fix this" → AI fix suggestion

### Phase 5: Polish (Weeks 9-10)

**Goal**: Speed, reliability, and comfort.

- [ ] **Speed optimization**
  - TTS at 300-400 wpm for experienced users
  - Abbreviation mode ("fn" instead of "function")
  - Parallel earcon + speech rendering
- [ ] **User profile learning**
  - Learn preferred reading speed
  - Learn common navigation patterns
  - Learn frequently accessed files
- [ ] **Onboarding wizard**
  - "Say 'help' to hear available commands"
  - Progressive disclosure of features
  - Speed training mode
- [ ] **Error recovery**
  - "undo" → revert last edit
  - "go back" → return to previous location
  - "start over" → reset to last save

---

## 10. Key Technical Decisions

### 10.1 Tree-sitter over AST

| | AST (stdlib) | Tree-sitter |
|---|---|---|
| Speed | Slow for large files | Incremental, O(n) |
| Error tolerance | Fails on syntax errors | Partial tree on errors |
| Language support | One per stdlib | 50+ languages |
| Query system | Manual walk | Declarative queries |

**Decision**: Use tree-sitter for all parsing. The Rust crate is mature and has Python bindings.

### 10.2 LSP over Custom Protocol

LSP is the standard. Every major language has an LSP server. We build a client, not a server.

**Decision**: Rust LSP client using `tower-lsp` or raw JSON-RPC over stdio. One client process per language server.

### 10.3 DAP over Custom Debugger

Same reasoning as LSP. DAP is the standard debug protocol.

**Decision**: Rust DAP client. One client per debug session.

### 10.4 Rust Core + Python UI

Performance-critical components in Rust (parsing, LSP, DAP, file watching). User-facing voice interface in Python (where the existing Artume OS desktop assistant lives).

**Decision**: New `aether_ide` Rust crate for core. Python `ide_engine.py` rewrite for voice UI. Communication via JSON-RPC over Unix socket (same pattern as `aetherfs-core`).

### 10.5 Sonification Library

Rather than building from scratch, use the existing `aether_audio` spatial mixer and `earcons.py` as foundation. Add a `CodeSonifier` module that maps code structure to audio parameters.

**Decision**: Extend existing audio infrastructure. No new audio dependencies.

---

## 11. File Layout

```
aether_ide/                          # New Rust crate
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── parser.rs                    # Tree-sitter wrapper
│   ├── queries/                     # Tree-sitter queries per language
│   │   ├── python.scm
│   │   ├── rust.scm
│   │   ├── javascript.scm
│   │   └── ...
│   ├── lsp.rs                       # LSP client
│   ├── dap.rs                       # DAP client
│   ├── buffer.rs                    # Text buffer (rope)
│   ├── project.rs                   # Project file index
│   ├── sonifier.rs                  # Code → audio parameters
│   └── ipc.rs                       # JSON-RPC server for Python UI

artome_ide/                          # New Python package
├── __init__.py
├── controller.py                    # Main IDE controller
├── reader.py                        # Code reading modes
├── navigator.py                     # Voice navigation
├── editor.py                        # Voice editing
├── debugger.py                      # Voice debugging
├── sonifier.py                      # Earcon + spatial audio mapping
├── earcons.py                       # Extended earcon definitions
└── skills.py                        # IDE skills for skill system
```

---

## 12. Success Criteria

The IDE is complete when a blind programmer can:

1. **Open a project** — "open project aether_orchestrator" → hears file tree, opens main.rs
2. **Understand structure** — "skim this file" → hears function signatures + structure earcons in 3 seconds
3. **Navigate** — "go to function handle_conversation" → jumps there, hears "line 474, 3 levels deep"
4. **Read code** — "read this function" → hears full code with structural earcons
5. **Find problems** — "show errors" → hears "2 errors, 3 warnings" with locations
6. **Edit** — "change 'warn' to 'info' on line 52" → edit applied, hears confirmation
7. **Debug** — "set breakpoint at line 100" → "continue" → hears "breakpoint hit at database.py:100"
8. **Inspect** — "what's the value of 'count'" → hears "count is 42"
9. **Use Git** — "git status" → hears "2 files modified: main.rs, lib.rs"
10. **Run tests** — "run tests" → hears "12 passed, 0 failed, in 3.2 seconds"

All of this without opening eyes, without a screen, without a mouse, without a keyboard.

---

## 13. Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| TTS speed too slow for productive coding | Support 400+ wpm; use abbreviations; use earcons for structure instead of speech |
| Voice recognition accuracy in noisy environments | Barge-in with VAD; directional mic support; fallback to keyboard |
| LSP server latency | Pre-warm LSP on project open; cache results; show progress earcon |
| Tree-sitter query complexity | Start with Python and Rust only; add languages incrementally |
| User cognitive overload from audio | Progressive disclosure; start simple, add complexity as user learns |
| Debug adapter protocol complexity | Start with Python debugger (debugpy); add others later |
| Multi-file project scale | Use aetherfs-core for indexing; lazy-load files |
