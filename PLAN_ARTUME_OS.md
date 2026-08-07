# Artume OS — Complete System Audit & n8n Workflow Map

> A complete rethinking of how a blind person accomplishes normal everyday computer tasks — conversationally, not visually.
> If a tool doesn't exist, we build it. If a flow has a gap, we bridge it.

---

## Table of Contents

1. [System Architecture Overview](#1-system-architecture-overview)
2. [Complete Module Inventory](#2-complete-module-inventory)
3. [n8n Workflow Map — User Journeys](#3-n8n-workflow-map)
4. [Intent Router — Current vs Needed](#4-intent-router)
5. [Gap Analysis — What's Missing](#5-gap-analysis)
6. [Build Order — Priority Queue](#6-build-order)
7. [The Golden Rule](#7-the-golden-rule)

---

## 1. System Architecture Overview

```
                    ┌─────────────────────────────────────┐
                    │         WAKE WORD DETECTOR           │
                    │  (Python: wakeword_engine.py)         │
                    │  (Rust: aether_audio::wake_word)     │
                    │  Words: "Artume", "Hey Artume"       │
                    └──────────────┬──────────────────────┘
                                 │
                    ┌──────────────▼──────────────────────┐
                    │         AUDIO CAPTURE                │
                    │  (Python: audio_engine.py VAD)       │
                    │  (Rust: aether_audio::capture)       │
                    │  Barge-in capable                     │
                    └──────────────┬──────────────────────┘
                                 │
                    ┌──────────────▼──────────────────────┐
                    │         SPEECH-TO-TEXT               │
                    │  (Python: faster-whisper tiny.en)    │
                    │  (Rust: aether_orchestrator::stt)    │
                    └──────────────┬──────────────────────┘
                                 │
                    ┌──────────────▼──────────────────────┐
                    │         INTENT ROUTER                │
                    │  (Python: intent_router.py)           │
                    │  (Rust: aether_orchestrator::router) │
                    │  Nemotron-3 Nano on GTX 1650S        │
                    └──────┬──────┬──────┬──────┬─────────┘
                          │      │      │      │
          ┌───────────────┼──────┼──────┼──────┼─────────────────┐
          │               │      │      │      │                 │
          ▼               ▼      ▼      ▼      ▼                 ▼
   ┌──────────┐   ┌──────────┐ ┌──────┐ ┌──────┐          ┌──────────┐
   │CONVERSATE│   │ EXECUTE  │ │SYSTEM│ │FILE  │          │  WEB     │
   │(Llama 8B)│   │ ACTION   │ │CMD   │ │SEARCH│          │  FETCH   │
   └──────────┘   └──────────┘ └──────┘ └──────┘          └──────────┘
                        │
          ┌─────────────┼──────────────┬──────────────────┐
          ▼              ▼              ▼                  ▼
   ┌──────────┐   ┌──────────┐   ┌──────────┐      ┌──────────┐
   │  BROWSER │   │  EMAIL   │   │   IDE    │      │  FILES   │
   │  MODE    │   │  MODE    │   │  MODE    │      │  MODE    │
   ├──────────┤   ├──────────┤   ├──────────┤      ├──────────┤
   │browser_  │   │mail_     │   │ide_      │      │file_     │
   │engine.py │   │engine.py │   │engine.py │      │browser.py│
   └──────────┘   └──────────┘   └──────────┘      └──────────┘

   ┌──────────┐   ┌──────────┐   ┌──────────┐      ┌──────────┐
   │   DOCS   │   │  EBOOK   │   │ SETTINGS │      │ SCREEN   │
   │  MODE    │   │  MODE    │   │  MODE    │      │ READER   │
   ├──────────┤   ├──────────┤   ├──────────┤      ├──────────┤
   │doc_writer│   │ebook_    │   │system_   │      │screen_   │
   │engine.py │   │engine.py │   │settings  │      │reader.py │
   └──────────┘   └──────────┘   └──────────┘      └──────────┘

                    ┌─────────────────────────────────────┐
                    │         TEXT-TO-SPEECH               │
                    │  (Python: Piper TTS)                  │
                    │  (Rust: Kokoro-82M via subprocess)    │
                    │  (Rust: aether_audio::output)        │
                    └──────────────┬──────────────────────┘
                                   │
                    ┌──────────────▼──────────────────────┐
                    │         AUDIO OUTPUT                  │
                    │  (cpal / PulseAudio / PipeWire)       │
                    │  Spatial mixer for 3D audio           │
                    └─────────────────────────────────────┘
```

---

## 2. Complete Module Inventory

### 2.1 Rust Crates (Performance-Critical Core)

| Crate | Module | Status | Lines | Purpose |
|-------|--------|--------|-------|---------|
| **aether_orchestrator** | | | | **Conversational AI pipeline** |
| | `router.rs` | ✅ Done | 214 | Intent classification via Nemotron-3 Nano |
| | `conversation.rs` | ✅ Done | 825 | Main loop: listen → classify → dispatch → respond |
| | `ollama.rs` | ✅ Done | 381 | Ollama HTTP client for dual-GPU inference |
| | `tts.rs` | ✅ Done | 254 | Kokoro-82M TTS via Python subprocess |
| | `tts_service.rs` | ✅ Done | ~80 | Background TTS thread |
| | `stt.rs` | ✅ Done | ~100 | Whisper STT via whisper-rs |
| | `file_search.rs` | ✅ Done | 159 | gRPC client for aetherfs-core |
| | `profile.rs` | ✅ Done | 294 | User profile (identity, preferences, routines) |
| | `skills/mod.rs` | ✅ Done | 446 | Pluggable skill system |
| | `bin/shell.rs` | ✅ Done | 309 | Main conversational shell |
| | `bin/tts_test.rs` | ✅ Done | ~50 | TTS test binary |
| **aether_audio** | | | | **Audio I/O** |
| | `output.rs` | ✅ Done | ~150 | AudioOutput with cpal + PulseAudio sink |
| | `capture.rs` | ✅ Done | ~120 | Mic capture with VAD |
| | `wake_word.rs` | ✅ Done | ~100 | OpenWakeWord detector |
| | `spatial_mixer.rs` | ✅ Done | ~200 | Binaural HRTF spatial audio mixer |
| | `context_stack.rs` | ✅ Done | ~80 | Audio context for interruption handling |
| **aether_browser** | | ✅ Done | ~300 | HTTP fetch + Readability + conversational formatting |
| **aether_buffer** | | ✅ Done | ~200 | NER entity extraction + transcript ring buffer |
| **aether_attention** | | ✅ Done | ~200 | Cognitive load evaluator + notification queue |
| **aetherfs-core** | | ✅ Done | ~800 | Background file indexing daemon (SQLite FTS5 + Qdrant) |
| **aether_ide** | | | | **Audio-First IDE (NEW)** |
| | `parser.rs` | ✅ Done | 360 | Tree-sitter parsing (6 languages) |
| | `sonifier.rs` | ✅ Done | 250 | Code structure → audio parameters |
| | `lsp.rs` | ✅ Done | 1,036 | LSP client (12/12 tests) |
| | `dap.rs` | ✅ Done | 1,200 | DAP client (18/18 tests) |
| | `project.rs` | ✅ Done | 637 | Project scanner + fuzzy search |
| | `navigation.rs` | ✅ Done | 724 | Cursor + 3D spatial audio workspace |
| | `buffer.rs` | ✅ Done | 350 | Text buffer with undo/redo |
| | `editor.rs` | ✅ Done | 460 | VoiceEditor (insert/delete/comment/indent/extract) |
| | `ipc.rs` | ✅ Done | 820 | JSON-RPC server (50+ methods) |
| | `bin/daemon.rs` | ✅ Done | 56 | Daemon entry point |

### 2.2 Python Modules (Voice UI Layer)

| Module | Status | Lines | Purpose |
|--------|--------|-------|---------|
| `artome_core.py` | ✅ Done | 358 | Main daemon loop — wake word → listen → route → TTS |
| `intent_router.py` | ✅ Done | 138 | AI intent routing via Nemotron + Llama |
| `command_navigator.py` | ✅ Done | 125 | Menu system for 8 modes |
| `audio_engine.py` | ✅ Done | 157 | Piper TTS + VAD listener with barge-in |
| `earcons.py` | ✅ Done | 84 | 4 basic earcons (listening, thinking, success, error) |
| `wakeword_engine.py` | ✅ Done | 46 | Wake word detection (keyword-based) |
| `screen_reader.py` | ✅ Done | 103 | AT-SPI2 accessibility tree inspector |
| `browser_engine.py` | ✅ Done | 131 | Web page → audio DOM (search, headings, links) |
| `mail_engine.py` | ✅ Done | 134 | IMAP/SMTP email client |
| `file_browser_engine.py` | ✅ Done | 233 | File browser + AetherFS search |
| `ide_engine.py` | ✅ Done | 106 | Basic AST code reader |
| `doc_writer_engine.py` | ✅ Done | 153 | Document writer (TXT/MD/HTML/DOCX/PDF) |
| `ebook_engine.py` | ✅ Done | 166 | EPUB/PDF/TXT reader |
| `system_settings_engine.py` | ✅ Done | 85 | Volume, battery, WiFi, timer |
| `artume_skills.py` | ✅ Done | 200 | Python skill system |
| **artome_ide/** | | | **Audio-First IDE Python (NEW)** |
| `__init__.py` | ✅ Done | 171 | IdeClient + convenience functions |
| `earcons.py` | ✅ Done | 198 | 20+ code-specific earcons |
| `ai_assistant.py` | ✅ Done | 123 | Llama 3.1 for fix/explain/test/refactor |
| `git_engine.py` | ✅ Done | 81 | Voice-driven Git operations |
| `terminal_engine.py` | ✅ Done | 81 | Voice-driven terminal |

### 2.3 Configuration & Identity

| File | Status | Purpose |
|------|--------|---------|
| `soul.md` | ✅ Done | Artume's identity prompt (54 lines) |
| `start.sh` | ✅ Done | Unified startup script |
| `PLAN_AUDIO_IDE.md` | ✅ Done | Audio IDE plan document |
| `README.md` | ✅ Done | Project README |

---

## 3. n8n Workflow Map — User Journeys

### 3.1 Wake Word → Task Completion (Master Flow)

```
[USER SPEAKS]
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│ WAKE WORD DETECTION                                          │
│                                                              │
│ Python path: wakeword_engine.py → keyword match             │
│ Rust path:   aether_audio::wake_word → OpenWakeWord model   │
│                                                              │
│ Words: "Artume", "Hey Artume", "Hey R2"                     │
│                                                              │
│ If wake word disabled → always listening mode                │
└────────────────────────────────┬────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────┐
│ AUDIO CAPTURE                                                │
│                                                              │
│ Python: audio_engine.py DynamicVADListener                   │
│   - Captures until silence (800ms threshold)                 │
│   - Barge-in: interrupts TTS if user speaks                  │
│   - Max duration: 12 seconds                                 │
│                                                              │
│ Rust: aether_audio::capture::capture_until_silence           │
│   - 16kHz f32 PCM samples                                    │
│   - Energy-based VAD                                         │
└────────────────────────────────┬────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────┐
│ SPEECH-TO-TEXT                                               │
│                                                              │
│ Python: faster-whisper tiny.en (CPU, int8)                   │
│ Rust:   whisper-rs (feature-gated)                           │
│                                                              │
│ Output: transcribed text string                              │
└────────────────────────────────┬────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────┐
│ INTENT CLASSIFICATION                                        │
│                                                              │
│ Python: intent_router.py → Nemotron-3 Nano on GTX 1650S     │
│ Rust:   router.rs → Nemotron-3 Nano via Ollama              │
│                                                              │
│ Intents:                                                     │
│   Conversation  → Llama 3.1 8B on GTX 1080                   │
│   EntityLookup → aether_buffer NER                          │
│   WebFetch     → aether_browser HTTP + Readability           │
│   FileSearch   → aetherfs-core gRPC daemon                   │
│   ExecuteAction → dispatch to mode handler                    │
│   SystemCommand → volume, settings, help                     │
│   Unknown      → fallback conversation                      │
└────────────────────────────────┬────────────────────────────┘
                                 │
                    ┌────────────┴────────────┐
                    │                         │
                    ▼                         ▼
          ┌──────────────────┐      ┌──────────────────┐
          │ MODE: DESKTOP    │      │ MODE: SPECIFIC   │
          │ (default)        │      │ (browser/email/  │
          │                  │      │  ide/files/docs/  │
          │ Direct action    │      │  ebook/settings)  │
          │ or mode switch   │      │                  │
          └────────┬─────────┘      └────────┬─────────┘
                   │                         │
                   ▼                         ▼
          ┌──────────────────────────────────────────────┐
          │ ACTION EXECUTION                              │
          │                                               │
          │ artome_core.py → execute_action(intent)       │
          │   → calls the appropriate engine module       │
          │   → plays earcon (success/error)              │
          │   → speaks response via TTS                   │
          └──────────────────────────────────────────────┘
```

### 3.2 Web Browsing Flow

```
"Artume, search for python tutorials"
    │
    ▼
[Wake Word] → [Capture] → [STT] → [Router: WebFetch]
    │
    ▼
[Switch to BROWSER mode]
    │
    ▼
[browser_engine.py: search("python tutorials")]
    │
    ├── DuckDuckGo HTML search
    ├── Parse results (title + URL)
    └── Speak: "Found 5 results. Result 1: Python for Beginners. Result 2: ..."
    │
    ▼
"Open result 1"
    │
    ▼
[browser_engine.py: load_url(url)]
    │
    ├── Fetch page, parse with BeautifulSoup
    ├── Extract: title, headings, links, article text
    └── Speak: "Loaded page: Python for Beginners. 12 headings, 45 links."
    │
    ▼
"Read the page"
    │
    ▼
[Readability extraction → TTS of article text]
    │
    ▼
"Scroll down" / "Next section" / "What are the links?"
    │
    ▼
[Headings list / Links list / Click link N]
```

**GAPS:**
- ❌ No "read from where I left off" after interruption
- ❌ No "go back" in browser history
- ❌ No form filling ("type in the search box")
- ❌ No multi-tab support
- ❌ No bookmark management
- ❌ No download management

### 3.3 Email Flow

```
"Artume, check my email"
    │
    ▼
[Router: ExecuteAction → EMAIL mode]
    │
    ▼
[mail_engine.py: fetch_inbox(server, user, pass)]
    │
    ├── IMAP SSL connection
    ├── Search UNSEEN emails
    └── Speak: "You have 3 unread emails. Email 1 from Alice: Subject: Meeting..."
    │
    ▼
"Read email 1"
    │
    ▼
[mail_engine.py: read_email_audio(1)]
    │
    └── Speak: "Reading email 1 from Alice. Subject: Meeting. Message reads: ..."
    │
    ▼
"Reply: Sounds good, I'll be there."
    │
    ▼
[mail_engine.py: prepare_draft("Alice", "Re: Meeting", "Sounds good...")]
    │
    └── Speak: "Prepared draft. Say 'send' to confirm or 'cancel'."
    │
    ▼
"Send"
    │
    ▼
[mail_engine.py: send_draft(smtp_server, ...)]
    │
    └── Speak: "Email sent to Alice."
```

**GAPS:**
- ❌ No credentials stored (must speak IMAP/SMTP password every time)
- ❌ No contact list / address book
- ❌ No attachment handling
- ❌ No email search
- ❌ No folder management (sent, spam, trash)
- ❌ No multi-account support

### 3.4 File Management Flow

```
"Artume, open files"
    │
    ▼
[Router: ExecuteAction → FILES mode]
    │
    ▼
[file_browser_engine.py: list_contents_audio()]
    │
    └── Speak: "Directory contains 3 folders and 8 files. Folders: src, docs, tests..."
    │
    ▼
"Go to src"
    │
    ▼
[file_browser_engine.py: change_dir("src")]
    │
    └── Speak: "Entered folder src. Directory contains 2 folders and 5 files..."
    │
    ▼
"Read file main.py"
    │
    ▼
[file_browser_engine.py: read_file_audio("main.py")]
    │
    └── Speak: "File main.py, size 2.4 KB. Content preview: import os..."
    │
    ▼
"Search for tax documents"
    │
    ▼
[file_browser_engine.py: search_files_audio("tax")]
    │
    ├── AetherFS daemon search (if running)
    ├── Fallback: os.walk
    └── Speak: "Found 3 matching files: tax_2024.pdf, tax_notes.txt..."
```

**GAPS:**
- ❌ No file copy/move/rename/delete
- ❌ No file creation
- ❌ No directory creation
- ❌ No file compression/extraction
- ❌ No trash/recycle bin
- ❌ No file type filtering ("show me only PDFs")
- ❌ No file sorting ("sort by date")
- ❌ No file properties ("how big is this folder")
- ❌ No external drive / USB mounting

### 3.5 Document Writing Flow

```
"Artume, open documents"
    │
    ▼
[Router: ExecuteAction → DOCS mode]
    │
    ▼
[doc_writer_engine.py: start_new_doc("Project Report")]
    │
    └── Speak: "Started new document titled 'Project Report'."
    │
    ▼
"Add heading Executive Summary"
    │
    ▼
[doc_writer_engine.py: add_heading("Executive Summary")]
    │
    └── Speak: "Added heading: Executive Summary."
    │
    ▼
"Add paragraph: The project is progressing well..."
    │
    ▼
[doc_writer_engine.py: add_paragraph("The project is...")]
    │
    └── Speak: "Added paragraph."
    │
    ▼
"Read draft"
    │
    ▼
[doc_writer_engine.py: read_draft_audio()]
    │
    └── Speak: "Reading document 'Project Report'. Contains 2 sections..."
    │
    ▼
"Export all"
    │
    ▼
[doc_writer_engine.py: export_all_formats()]
    │
    └── Speak: "Exported to TXT, Markdown, HTML, Word DOCX, PDF."
```

**GAPS:**
- ❌ No edit/insert between sections
- ❌ No delete section
- ❌ No reorder sections
- ❌ No spell check
- ❌ No template system
- ❌ No dictation mode (continuous speech-to-text)
- ❌ No revision history
- ❌ No collaborative editing

### 3.6 EBook Reading Flow

```
"Artume, open books"
    │
    ▼
[Router: ExecuteAction → EBOOK mode]
    │
    ▼
"Open book /path/to/book.epub"
    │
    ▼
[ebook_engine.py: load_book(path)]
    │
    ├── EPUB: extract chapters via ebooklib
    ├── PDF: extract pages via pypdf
    ├── TXT: split into sections
    └── Speak: "Loaded EPUB 'The Great Novel' by Author. 24 chapters."
    │
    ▼
"Read chapter 1"
    │
    ▼
[ebook_engine.py: read_chapter_audio(1)]
    │
    └── TTS of chapter content
    │
    ▼
"Next chapter" / "Set bookmark" / "Search for 'castle'"
```

**GAPS:**
- ❌ No "read from bookmark" on app restart
- ❌ No reading progress tracking
- ❌ No text-to-speech speed control per book
- ❌ No annotation / highlighting
- ❌ No dictionary lookup
- ❌ No table of contents navigation by voice
- ❌ No audiobook sync (Audible, Libby)

### 3.7 IDE / Coding Flow

```
"Artume, open code"
    │
    ▼
[Router: ExecuteAction → IDE mode]
    │
    ▼
"Open file main.rs"
    │
    ▼
[ide_engine.py: load_file("main.rs")]
    │
    └── AST parse → Speak: "Opened main.rs. 142 lines. 3 functions: main, parse, format."
    │
    ▼
"Read function parse"
    │
    ▼
[ide_engine.py: read_function("parse")]
    │
    └── TTS of function source code
    │
    ▼
"Read lines 10 to 20"
    │
    ▼
[ide_engine.py: read_lines(10, 20)]
    │
    └── TTS of line range
```

**GAPS (the new aether_ide fills most of these):**
- ✅ Tree-sitter parsing (6 languages) — DONE
- ✅ Code sonification (depth→pitch, scope→earcons) — DONE
- ✅ LSP client (go-to-def, hover, completions) — DONE (not wired to IPC)
- ✅ DAP client (breakpoints, step, variables) — DONE (not wired to IPC)
- ✅ Project indexer (fuzzy file/symbol search) — DONE
- ✅ Cursor navigation (function/class jumping) — DONE
- ✅ Text buffer with undo/redo — DONE
- ✅ Voice editing (insert/delete/comment/indent/extract) — DONE
- ✅ AI assistant (fix/explain/test/refactor) — DONE
- ✅ Git integration — DONE
- ✅ Terminal integration — DONE
- ✅ Enhanced earcons (20+ code-specific) — DONE
- ❌ LSP wired to IPC — NOT DONE (Send issue)
- ❌ DAP wired to IPC — NOT DONE (Send issue)
- ❌ Earcons played during navigation — NOT WIRED
- ❌ Spatial audio workspace rendering — NOT WIRED

### 3.8 System Settings Flow

```
"Artume, what's my status"
    │
    ▼
[Router: SystemCommand]
    │
    ▼
[system_settings_engine.py: get_system_status_audio()]
    │
    └── Speak: "Time is 2:30 PM. Battery at 85%. Connected to WiFi 'HomeNetwork'."
    │
    ▼
"Volume up" / "Set volume to 50" / "Set timer for 10 minutes"
```

**GAPS:**
- ❌ No Bluetooth device management
- ❌ No WiFi network selection ("connect to CafeWiFi")
- ❌ No display settings (brightness, night mode)
- ❌ No audio output device switching ("switch to headphones")
- ❌ No microphone settings
- ❌ No keyboard shortcuts management
- ❌ No accessibility settings (TTS speed, verbosity)
- ❌ No power management (sleep, shutdown, restart)
- ❌ No user account management
- ❌ No printer management
- ❌ No app installation/removal

### 3.9 Screen Reading Flow

```
"Artume, what's on my screen"
    │
    ▼
[Router: ExecuteAction → screen_summary]
    │
    ▼
[screen_reader.py: generate_screen_summary_payload()]
    │
    ├── AT-SPI2 accessibility tree traversal
    ├── Active window title via xdotool
    └── → Llama 3.1 8B summarizes for blind user
    │
    ▼
Speak: "Active window is Firefox. Page has a search box, navigation bar with 5 links..."
```

**GAPS:**
- ❌ No element-level navigation ("click the login button")
- ❌ No text selection and reading
- ❌ No form field interaction ("type in the search box")
- ❌ No scroll support
- ❌ No table navigation
- ❌ No dynamic content change detection
- ❌ No mouse cursor tracking
- ❌ No multi-monitor support

---

## 4. Intent Router — Current vs Needed

### Current Intents (Rust `router.rs`)

```rust
pub enum Intent {
    Conversation,    // → Llama 3.1 8B chat
    EntityLookup,    // → aether_buffer NER
    WebFetch,        // → aether_browser HTTP
    FileSearch,      // → aetherfs-core gRPC
    ExecuteAction,   // → mode dispatch (stub)
    SystemCommand,   // → volume/settings (stub)
    Unknown,         // → fallback conversation
}
```

### Current Intents (Python `intent_router.py`)

```python
# Actions: speak, open_app, type_text, press_key, run_cmd, switch_mode
#          web_navigate, email_action, ide_action, file_action
#          doc_action, setting_action, ebook_action, screen_summary
# Modes: DESKTOP, BROWSER, EMAIL, IDE, FILES, DOCS, EBOOK, SETTINGS
```

### Needed Intents (Complete)

```rust
pub enum Intent {
    // Existing
    Conversation,
    EntityLookup,
    WebFetch,
    FileSearch,
    ExecuteAction,
    SystemCommand,
    
    // NEW — Mode switching
    SwitchMode,         // "switch to browser" / "open files"
    
    // NEW — Application management
    LaunchApp,          // "open firefox" / "launch terminal"
    QuitApp,            // "close firefox"
    ListApps,           // "what apps are running"
    
    // NEW — Window management
    FocusWindow,        // "switch to the terminal window"
    ListWindows,        // "what windows are open"
    MinimizeWindow,     // "minimize this"
    MaximizeWindow,     // "maximize"
    CloseWindow,        // "close window"
    
    // NEW — Clipboard
    CopySelection,      // "copy this"
    PasteClipboard,     // "paste"
    SelectAll,          // "select all"
    
    // NEW — Navigation
    ScrollUp,           // "scroll up" / "scroll down"
    ScrollDown,
    GoBack,             // "go back" (browser/file history)
    GoForward,          // "go forward"
    
    // NEW — Media
    PlayPause,          // "play" / "pause"
    VolumeUp,           // "volume up"
    VolumeDown,         // "volume down"
    MuteUnmute,         // "mute"
    NextTrack,          // "next track"
    PreviousTrack,      // "previous track"
    
    // NEW — System
    Shutdown,           // "shutdown" / "restart" / "sleep"
    Restart,
    Sleep,
    LockScreen,         // "lock my computer"
    
    // NEW — IDE (from aether_ide)
    OpenFile,           // "open main.rs"
    GoToDefinition,     // "go to definition of parse"
    FindReferences,     // "who calls this"
    RenameSymbol,       // "rename count to total"
    FormatCode,         // "format this file"
    RunTests,           // "run tests"
    DebugStart,         // "start debugging"
    DebugStep,          // "step over"
    DebugContinue,      // "continue"
    ToggleBreakpoint,   // "set breakpoint at line 42"
    
    // NEW — Git
    GitStatus,          // "git status"
    GitDiff,            // "what changed"
    GitCommit,          // "commit this"
    GitPush,            // "push"
    GitPull,            // "pull"
    GitBranch,          // "what branch am I on"
    GitSwitch,          // "switch to main"
    
    // NEW — Terminal
    RunCommand,         // "run make build"
    RunFile,            // "run this file"
    ShowTerminal,       // "show terminal"
    StopCommand,        // "stop"
    
    // NEW — Screen reader
    ReadScreen,         // "what's on my screen"
    ClickElement,       // "click the login button"
    ReadElement,        // "read that"
    TypeText,           // "type hello"
    
    // NEW — Email
    CheckEmail,         // "check my email"
    ReadEmail,          // "read email 1"
    ComposeEmail,       // "compose to alice"
    SendEmail,          // "send"
    
    // NEW — Documents
    NewDocument,        // "new document"
    AddHeading,         // "add heading Summary"
    AddParagraph,       // "add paragraph text"
    ReadDraft,          // "read draft"
    ExportDocument,     // "export"
    
    // NEW — EBook
    OpenBook,           // "open book.epub"
    ReadChapter,        // "read chapter 1"
    NextChapter,        // "next chapter"
    SetBookmark,        // "set bookmark"
    SearchBook,         // "search for castle"
    
    // NEW — Settings
    SystemStatus,       // "what's my status"
    SetTimer,           // "set timer 10 minutes"
    ConnectWiFi,        // "connect to CafeWiFi"
    BluetoothDevices,   // "list bluetooth devices"
    SwitchAudioOutput,  // "switch to headphones"
}
```

---

## 5. Gap Analysis — What's Missing

### 5.1 Critical Infrastructure Gaps

| Gap | Impact | Priority |
|-----|--------|----------|
| **No credential manager** — user must speak passwords | Every email, IMAP, SMTP, WiFi action requires re-entering credentials | 🔴 P0 |
| **No persistent state** — modes, history, bookmarks lost on restart | All context resets when Artume restarts | 🔴 P0 |
| **No notification system** — no way to alert user asynchronously | Can't notify when download finishes, timer expires, email arrives | 🔴 P0 |
| **No multi-turn confirmation** — no "are you sure?" pattern for destructive actions | Risk of accidental file deletion, email send, shutdown | 🔴 P0 |
| **LSP/DAP not wired to IPC** — Send issue with std::process::ChildStdin | Full IDE intelligence exists but can't be used | 🔴 P0 |
| **Earcons not wired to navigation** — 20+ sounds generated but never played | No audio feedback during code reading/navigation | 🟡 P1 |

### 5.2 Missing Modules (Need to Build)

| Module | Purpose | Priority |
|--------|---------|----------|
| **Credential Vault** | Encrypted storage for IMAP/SMTP/WiFi credentials. Unlock with voice PIN or biometric. | 🔴 P0 |
| **Notification Center** | Async notification delivery: download complete, timer expired, email arrived, build finished. Queue during focus, deliver during idle. | 🔴 P0 |
| **Confirmation Dialog** | "Are you sure?" pattern for destructive actions. "Say yes to confirm, say cancel to abort." | 🔴 P0 |
| **Window Manager** | List, focus, minimize, maximize, close windows via EWMH/NetWM. | 🔴 P0 |
| **App Launcher** | Launch, list, quit applications. Desktop file parser. | 🔴 P0 |
| **Clipboard Manager** | Copy, paste, select all, clipboard history. | 🔴 P0 |
| **Bluetooth Manager** | List, connect, disconnect Bluetooth devices. | 🟡 P1 |
| **WiFi Manager** | List networks, connect, disconnect, save credentials. | 🟡 P1 |
| **Audio Output Switcher** | List PulseAudio sinks, switch default output. | 🟡 P1 |
| **Download Manager** | Track downloads, notify on completion, open downloaded files. | 🟡 P1 |
| **Calendar Integration** | Read calendar events, set reminders, check schedule. | 🟡 P1 |
| **Contacts / Address Book** | Store and retrieve contacts for email/phone. | 🟡 P1 |
| **Calculator** | Voice calculator ("what's 15% of 340") | 🟢 P2 |
| **Unit Converter** | "convert 5 miles to kilometers" | 🟢 P2 |
| **Weather** | "what's the weather today" | 🟢 P2 |
| **Dictionary / Thesaurus** | "define serendipity" | 🟢 P2 |
| **Password Generator** | "generate a strong password" | 🟢 P2 |
| **Notes / Scratchpad** | Quick voice notes, read back later | 🟢 P2 |
| **Alarm Clock** | Set, list, dismiss alarms | 🟢 P2 |
| **Pomodoro Timer** | Focus timer with work/break cycles | 🟢 P2 |

### 5.3 Integration Gaps (Built But Not Connected)

| What | Status | Action Needed |
|------|--------|---------------|
| LSP client → IPC server | Built, not wired | Refactor LspClient to use tokio::process for Send safety |
| DAP client → IPC server | Built, not wired | Same Send issue as LSP |
| Python earcons → code reading | 20+ WAVs generated | Wire play_earcon() calls into skim/read/navigate |
| Python AI assistant → voice routing | Module exists | Connect to intent router for "fix this" / "explain" |
| Python Git → voice routing | Module exists | Connect to intent router for "git status" |
| Python terminal → voice routing | Module exists | Connect to intent router for "run tests" |
| Spatial audio workspace → audio output | SpatialMixer configured | Wire to actual audio playback during navigation |
| aether_ide daemon → Python UI | IPC server running | Python IdeClient needs to be called from artome_core.py |

---

## 6. Build Order — Priority Queue

### Phase 0: Critical Fixes (Do First)

```
[ ] Fix LSP/DAP Send issue — refactor to tokio::process
[ ] Wire LSP handlers in IPC server
[ ] Wire DAP handlers in IPC server
[ ] Wire earcons into code navigation
[ ] Wire Python AI assistant to intent router
[ ] Wire Python Git to intent router
[ ] Wire Python terminal to intent router
[ ] Wire aether_ide daemon startup into artome_core.py
```

### Phase 1: Infrastructure (Week 1-2)

```
[ ] Credential Vault — encrypted storage, voice PIN unlock
[ ] Notification Center — async delivery, focus-aware queuing
[ ] Confirmation Dialog — "are you sure?" pattern
[ ] Persistent State — save/restore modes, history, bookmarks
[ ] Window Manager — EWMH/NetWM window control
[ ] App Launcher — desktop file parser, launch/quit/list
[ ] Clipboard Manager — copy/paste/select/select all
```

### Phase 2: System Control (Week 3-4)

```
[ ] Bluetooth Manager — list/connect/disconnect devices
[ ] WiFi Manager — list/connect/disconnect/save networks
[ ] Audio Output Switcher — list sinks, switch default
[ ] Download Manager — track, notify, open
[ ] Power Management — shutdown/restart/sleep/lock
[ ] Display Settings — brightness, night mode
[ ] Microphone Settings — input device, gain
```

### Phase 3: Productivity (Week 5-6)

```
[ ] Calendar Integration — read events, set reminders
[ ] Contacts / Address Book — store, retrieve, search
[ ] Calculator — voice math expressions
[ ] Unit Converter — measurements, currency
[ ] Weather — forecast, current conditions
[ ] Dictionary / Thesaurus — word lookup
[ ] Password Generator — create, store, retrieve
```

### Phase 4: Daily Tools (Week 7-8)

```
[ ] Notes / Scratchpad — quick capture, read back
[ ] Alarm Clock — set, list, dismiss
[ ] Pomodoro Timer — focus/break cycles
[ ] Screen Reader — element-level navigation, click, type
[ ] Form Filler — web form interaction
[ ] Multi-tab Browser — tab management
[ ] Email Attachments — send, receive, download
```

### Phase 5: IDE Integration (Week 9-10)

```
[ ] Wire spatial audio workspace → actual 3D playback
[ ] Wire IDE skim/read/structure modes → earcons + TTS
[ ] Wire AI assistant → "fix this" / "explain" voice commands
[ ] Wire Git → "git status" / "commit" voice commands
[ ] Wire terminal → "run tests" / "run file" voice commands
[ ] Wire debug → "set breakpoint" / "step over" voice commands
[ ] Complete intent router with all new intents
```

---

## 7. The Golden Rule

> **"If you can't use it completely with your eyes closed, it's not good enough."**

Every feature must pass this test:

1. **Can a blind user discover it exists?** — "What can I say?" should list every available command in the current mode.
2. **Can a blind user invoke it?** — Voice command, not keyboard shortcut, not mouse click.
3. **Can a blind user understand the result?** — Audio feedback (speech + earcon) for every action.
4. **Can a blind user recover from errors?** — "That didn't work. Try saying it differently." / "Undo."
5. **Can a blind user learn it without sighted help?** — Onboarding wizard that teaches commands conversationally.

### Design Principles

1. **Voice-native, not voice-bolted-on.** Every action has a voice path. If you need a keyboard or mouse, it's not done.
2. **Confirm before destroy.** Delete, send, shutdown, purchase — always confirm. "Say yes to confirm."
3. **Narrate, don't silence.** "Fetching that page...", "Searching your files...", "Thinking..." — the user should never wonder if you heard them.
4. **Summarize, don't dump.** Long content gets condensed. "I found 24 results. The top 3 are..." The user can ask for more.
5. **Remember, don't ask again.** Learned credentials, preferred TTS speed, common file locations — store them.
6. **Queue, don't interrupt.** When the user is focused, queue notifications. Deliver them in a batch during idle moments.
7. **Barge-in always works.** If the user speaks while Artume is talking, Artume stops and listens. Always.
8. **Mode-aware context.** "Read this" means different things in BROWSER mode (read article) vs FILES mode (read file) vs EBOOK mode (read chapter). The mode disambiguates.

---

## Appendix: File Map

```
/home/damon/Desktop/aetherfs_engine/
│
├── aether_orchestrator/          # Rust — Conversational AI pipeline
│   ├── src/
│   │   ├── router.rs            # Intent classification
│   │   ├── conversation.rs      # Main loop
│   │   ├── ollama.rs            # Ollama HTTP client
│   │   ├── tts.rs               # Kokoro TTS
│   │   ├── tts_service.rs       # Background TTS
│   │   ├── stt.rs               # Whisper STT
│   │   ├── file_search.rs       # AetherFS gRPC client
│   │   ├── profile.rs           # User profile
│   │   ├── skills/mod.rs        # Skill system
│   │   └── bin/shell.rs         # Main shell
│   └── soul.md                  # Artume identity
│
├── aether_audio/                # Rust — Audio I/O
│   ├── src/
│   │   ├── output.rs            # Audio playback
│   │   ├── capture.rs           # Mic capture
│   │   ├── wake_word.rs         # Wake word detection
│   │   ├── spatial_mixer.rs     # 3D audio mixer
│   │   └── context_stack.rs     # Interruption handling
│
├── aether_browser/              # Rust — Web fetching
├── aether_buffer/               # Rust — NER + ring buffer
├── aether_attention/            # Rust — Cognitive load
├── aetherfs-core/               # Rust — File indexing daemon
│
├── aether_ide/                  # Rust — Audio-First IDE (NEW)
│   ├── src/
│   │   ├── parser.rs            # Tree-sitter parsing
│   │   ├── sonifier.rs          # Code → audio
│   │   ├── lsp.rs               # LSP client
│   │   ├── dap.rs               # DAP client
│   │   ├── project.rs           # Project indexer
│   │   ├── navigation.rs        # Cursor + spatial audio
│   │   ├── buffer.rs            # Text buffer
│   │   ├── editor.rs            # Voice editing
│   │   ├── ipc.rs               # JSON-RPC server
│   │   └── bin/daemon.rs        # Daemon entry
│
├── artome_core.py               # Python — Main daemon
├── intent_router.py             # Python — AI routing
├── command_navigator.py         # Python — Menu system
├── audio_engine.py              # Python — Piper TTS + VAD
├── earcons.py                   # Python — 4 basic earcons
├── wakeword_engine.py           # Python — Wake word
├── screen_reader.py             # Python — AT-SPI2
├── browser_engine.py            # Python — Web browser
├── mail_engine.py               # Python — Email
├── file_browser_engine.py       # Python — File browser
├── ide_engine.py                # Python — Basic IDE
├── doc_writer_engine.py         # Python — Document writer
├── ebook_engine.py              # Python — EBook reader
├── system_settings_engine.py    # Python — Settings
├── artume_skills.py             # Python — Skill system
│
├── artome_ide/                  # Python — Audio IDE UI (NEW)
│   ├── __init__.py              # IdeClient
│   ├── earcons.py               # 20+ code earcons
│   ├── ai_assistant.py          # Llama 3.1 AI
│   ├── git_engine.py            # Git operations
│   └── terminal_engine.py       # Terminal operations
│
├── scripts/kokoro_tts.py        # Python — Kokoro bridge
├── start.sh                     # Startup script
├── PLAN_AUDIO_IDE.md            # Audio IDE plan
└── README.md                    # Project README
```

---

*Generated: 2026-08-01 — Complete system audit of Artume OS*
