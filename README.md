# Artome OS: Voice-First Conversational Desktop Environment for the Blind

> **Artome OS** is an AI-driven, audio-first Desktop Environment (DE) designed for complete visual independence. Built for blind developers, professionals, and users, Artome replaces complex visual screen reader keybindings with natural, context-aware spoken conversations, real-time speech barge-in, AT-SPI2 screen perception, and local AI reasoning.

---

## 🌟 Key Architecture & Capabilities

```
                  ┌──────────────────────────────────────────────┐
                  │           Artome Voice OS Core               │
                  │   (Whisper STT / Piper TTS / Silero VAD)     │
                  └──────────────────────┬───────────────────────┘
                                         │
 ┌───────────────────┬───────────────────┼───────────────────┬───────────────────┐
 │                   │                   │                   │                   │
 ▼                   ▼                   ▼                   ▼                   ▼
🌐 Audio Browser    📧 Audio Mail       💻 Audio IDE        📁 File Manager     📝 Document Writer
(DuckDuckGo/DOM)   (IMAP/SMTP)         (AST Code Tree)     (Voice Tree)        (PDF/DOCX/MD/TXT)
 │                   │                   │                   │                   │
 ├───────────────────┼───────────────────┼───────────────────┼───────────────────┤
 ▼                   ▼                   ▼                   ▼                   ▼
📚 EBook Reader    ⚙️ Audio Settings   🎙️ Wake Word       🔊 Speech Barge-In  👁️ AT-SPI2 Screen Reader
(EPUB / PDF)        (Volume/Battery)    (Hey Artome)        (Instant Stop)      (Accessibility Tree)
```

---

## 📊 Current Project Status

| Module | File | Implementation Status | Features |
| :--- | :--- | :--- | :--- |
| **OS Core Daemon** | [`artome_core.py`](file:///home/damon/r2me/artome_core.py) | ✅ Fully Integrated | Multi-mode event loop, mode routing, earcons |
| **Audio Engine** | [`audio_engine.py`](file:///home/damon/r2me/audio_engine.py) | ✅ Fully Integrated | Piper TTS, continuous dynamic VAD, **Barge-In speech interruption** |
| **Earcons Engine** | [`earcons.py`](file:///home/damon/r2me/earcons.py) | ✅ Fully Integrated | Chimes for listening, thinking, success, error |
| **Wake-Word Detector**| [`wakeword_engine.py`](file:///home/damon/r2me/wakeword_engine.py) | ✅ Fully Integrated | Hands-free detection (*"Hey Artome"*, *"Computer"*, *"Hey R2"*) |
| **AT-SPI2 Screen Reader**| [`screen_reader.py`](file:///home/damon/r2me/screen_reader.py) | ✅ Fully Integrated | Linux `at-spi2-core` accessibility tree inspector |
| **AI Intent Router** | [`intent_router.py`](file:///home/damon/r2me/intent_router.py) | ✅ Fully Integrated | Ollama JSON structured intent schemas & `COMMAND:SCREEN_SUMMARY` |
| **Audio Web Browser** | [`browser_engine.py`](file:///home/damon/r2me/browser_engine.py) | ✅ Fully Integrated | DuckDuckGo audio web search, DOM article reading, links/headings |
| **Audio Email Client**| [`mail_engine.py`](file:///home/damon/r2me/mail_engine.py) | ✅ Fully Integrated | IMAP unread mail reader, voice dictation, SMTP sender |
| **Audio AI IDE** | [`ide_engine.py`](file:///home/damon/r2me/ide_engine.py) | ✅ Fully Integrated | Python AST code symbol navigator, function reader, traceback summarizer |
| **Audio File Manager**| [`file_browser_engine.py`](file:///home/damon/r2me/file_browser_engine.py) | ✅ Fully Integrated | Conversational directory tree navigator, audio file previews & search |
| **Document Writer** | [`doc_writer_engine.py`](file:///home/damon/r2me/doc_writer_engine.py) | ✅ Fully Integrated | Voice authoring, multi-format export (**PDF, DOCX, MD, TXT, HTML**), Dropbox sync |
| **EBook Reader** | [`ebook_engine.py`](file:///home/damon/r2me/ebook_engine.py) | ✅ Fully Integrated | EPUB & PDF audio reader, TOC chapter navigation, audio bookmarks, search |
| **System Settings** | [`system_settings_engine.py`](file:///home/damon/r2me/system_settings_engine.py) | ✅ Fully Integrated | Master volume control, battery % check, WiFi status, audio timers |
| **Command Navigator**| [`command_navigator.py`](file:///home/damon/r2me/command_navigator.py) | ✅ Fully Integrated | Context-aware audio help & 8-mode interactive audio OS menu |

---

## 🚀 Quickstart Guide

### 1. Prerequisites
Ensure system dependencies and local AI models are ready:
* Linux OS (Linux x86_64)
* Python 3.10+
* `piper` TTS binary and voice model (`en_US-lessac-medium.onnx`)
* `xdotool`, `aplay`, `amixer`
* Local `ollama` daemon running `tinyllama` or `llama3.2` model (`ollama run tinyllama`)

### 2. Running Artome OS
Run the main daemon from the virtual environment:

```bash
cd /home/damon/r2me
./venv/bin/python artome_core.py
```

---

## 🗣️ Voice Commands Reference Guide

### 1. Wake Word & Navigation
* **Wake Word**: `"Hey Artome"`, `"Artome"`, `"Computer"`, or `"Hey R2"`.
* **Toggle Wake Word**: `"Enable wake word"` or `"Disable wake word"`.
* **Audio Help**: `"Help"` or `"What can I say?"` (speaks mode-specific help).
* **Main Menu**: `"Main menu"` or `"Navigation menu"` (speaks 8-mode menu).
* **Barge-In**: Speak anytime while Artome is talking to instantly interrupt TTS and issue a new command.

### 2. Screen Reader & Perception
* `"What is on screen?"` / `"Read screen"` $\rightarrow$ Triggers AT-SPI2 accessibility tree inspection and speaks an AI-summarized screen description.

### 3. Audio Web Browser (`BROWSER` mode)
* `"Search for [query]"` $\rightarrow$ Searches DuckDuckGo and reads result summaries out loud.
* `"Open [URL]"` $\rightarrow$ Fetches and parses webpage into audio structure.
* `"Headings"` / `"Links"` $\rightarrow$ Lists headings or numbered links.
* `"Click link 1"` $\rightarrow$ Follows link by index.

### 4. Audio Email Client (`EMAIL` mode)
* `"Check inbox"` $\rightarrow$ Speaks unread email count and sender summaries.
* `"Read email 1"` $\rightarrow$ Speaks email body.
* `"Compose email"` $\rightarrow$ Initiates guided voice dictation and confirmation loop.

### 5. Audio AI IDE (`IDE` mode)
* `"Open code [file.py]"` $\rightarrow$ Parses Python AST and reads class/function outline.
* `"Read function [func_name]"` $\rightarrow$ Reads function source code out loud.
* `"Read lines 1 to 20"` $\rightarrow$ Reads line range.

### 6. Audio File Browser (`FILES` mode)
* `"Where am I"` $\rightarrow$ Speaks current directory location.
* `"List folders"` / `"List files"` $\rightarrow$ Speaks contents of current folder.
* `"Go to [folder]"` / `"Go up"` $\rightarrow$ Navigates folder tree.
* `"Search file [pattern]"` $\rightarrow$ Locates files across directory tree.

### 7. Document Writer (`DOCS` mode)
* `"New document [title]"` $\rightarrow$ Initializes new draft.
* `"Add heading [text]"` / `"Add paragraph [text]"` $\rightarrow$ Dictates text into draft.
* `"Export all"` $\rightarrow$ Simultaneously exports to **PDF, Word DOCX, Markdown, TXT, and HTML**.
* `"Share to cloud"` $\rightarrow$ Syncs exported files to Dropbox/cloud directory.

### 8. EBook Reader (`EBOOK` mode)
* `"Open book [file.epub / file.pdf]"` $\rightarrow$ Loads ebook and speaks chapter count.
* `"Read chapter [num]"` / `"Next chapter"` / `"Previous chapter"`.
* `"Set bookmark"` / `"Go to bookmark"`.
* `"Search book for [keyword]"` $\rightarrow$ Searches full text of book.

### 9. System Settings (`SETTINGS` mode)
* `"Status"` $\rightarrow$ Speaks time, date, battery level %, and WiFi network status.
* `"Volume up"` / `"Volume down"`.
* `"Set timer for [N] minutes"`.

---

## 🔮 Next Steps & Roadmap

- [ ] **Systemd Session Integration**: Package Artome as a systemd user service (`artome.service`) to auto-boot as the primary audio desktop session on startup.
- [ ] **Hardware Push-to-Talk & Panic Key**: Bind a physical keyboard key (`Caps Lock` or `Pause/Break`) to instantly toggle microphone mute or force-silence audio.
- [ ] **Bluetooth Audio Output Switcher**: Voice commands to switch audio output between built-in speakers, Bluetooth headsets, and USB DACs.
- [ ] **Multi-Language Speech Models**: Add support for non-English Whisper models and multilingual Piper TTS voices.
- [ ] **Offline LLM Fine-Tuning**: Fine-tune a lightweight 1B/3B local model specifically on Artome JSON action schemas for ultra-fast, offline inference.
