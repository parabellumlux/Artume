# Artume OS

**A voice-native operating system for the blind. No screen reader. No visual interface. Just dialogue.**

*"Read to you, me."*

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-prototype-orange.svg)]()

---

## The Name

**Artume** is the Etruscan goddess of the night — a deity who moved through darkness not with clarity, but with purpose.

Phonetically: **R** (read) • **Tu** (to you) • **Me** (me)

*Read to you, me.* A dialogue between the sighted world and those who navigate it through sound, touch, and language.

---

## Quickstart

### Prerequisites

- Linux x86_64 with NVIDIA GPU(s)
- [Rust](https://rustup.rs/) (2021 edition)
- [Ollama](https://ollama.com/) for local LLM inference
- NVIDIA driver + CUDA

### One-command start

```bash
git clone https://github.com/parabellumlux/Artume.git
cd Artume
./start.sh
```

That's it. The script will:

1. Check your NVIDIA GPUs
2. Start Ollama if not running
3. Pull required models (Llama 3.1 8B, Nemotron-3 Nano, nomic-embed-text)
4. Build the Rust conversational shell
5. Launch the interactive prompt

### Flags

| Flag | Description |
|------|-------------|
| `./start.sh` | Build + run Rust conversational shell (text mode) |
| `./start.sh --voice` | Build + run with voice I/O (requires model files) |
| `./start.sh --check` | Verify prerequisites without running |
| `./start.sh --python` | Run the Python desktop assistant instead |
| `./start.sh --help` | Show usage |

---

## GPU Pipeline

| Tier | GPU | Model | Purpose |
|------|-----|-------|---------|
| **1 — Reasoning** | GTX 1080 (GPU 0) | `llama3.1:8b` | Main conversation, summarization |
| **2 — Router** | GTX 1650S (GPU 1) | `nemotron-3-nano:4b` | Intent classification, routing |
| **3 — Embeddings** | CPU | `nomic-embed-text` | Vector search, RAG |

---

## Architecture

```
User Input (text or voice)
    │
    ▼
┌─────────────────────────────────────────────┐
│  Intent Router (Nemotron-3 Nano on 1650S)    │
│  ┌──────────────────────────────────────┐   │
│  │ Conversation  → Llama 3.1 8B (1080) │   │
│  │ EntityLookup  → aether_buffer (NER) │   │
│  │ WebFetch      → aether_browser      │   │
│  │ FileSearch    → aetherfs-core (gRPC) │   │
│  │ ExecuteAction → system call         │   │
│  │ SystemCommand → settings/volume     │   │
│  └──────────────────────────────────────┘   │
│              │                               │
│              ▼                               │
│  ┌──────────────────────────────────────┐   │
│  │ Optional: TTS → Spatial Audio Mixer  │   │
│  │ (Kokoro-82M)  (binaural HRTF)        │   │
│  └──────────────────────────────────────┘   │
│              │                               │
│              ▼                               │
│  ┌──────────────────────────────────────┐   │
│  │ Attention Manager (cognitive load)   │   │
│  │ Queue notifications during focus     │   │
│  │ Batch summarize on idle              │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| `aether_orchestrator` | Conversational AI pipeline — intent routing, GPU inference, history |
| `aether_browser` | Headless web fetcher + Readability content extraction |
| `aether_buffer` | Rolling transcript ring buffer + regex NER entity resolution |
| `aether_audio` | Spatial audio mixer (binaural HRTF) + interruption context stack |
| `aether_attention` | Cognitive load evaluator + pending notification queue |
| `aetherfs-core` | Background file indexing daemon (SQLite + Qdrant + BLAKE3 dedup) |
| `aetherfs-cli` | CLI client for file search queries |
| `aetherfs-proto` | gRPC protobuf definitions |

## File Search Daemon

For file search capabilities, start the background indexing daemon in another terminal:

```bash
cargo run --bin aetherfs-core
```

This watches your filesystem, indexes files into SQLite + Qdrant, and exposes a gRPC endpoint over Unix Domain Socket (`/tmp/aetherfs.sock`). The conversational shell connects to it automatically.

## Python Desktop Assistant

The original Python-based desktop assistant is still available:

```bash
./start.sh --python
```

This provides AT-SPI2 screen reading, audio web browsing, email, IDE, file browser, document writer, ebook reader, and system settings — all voice-controlled.

## Voice Commands

Once the shell is running, try:

- `"Hello"` — starts a conversation
- `"What's the time?"` — checks the time
- `"Read me https://example.com"` — fetches and summarizes a web page
- `"Copy that tracking number"` — looks up entities from conversation
- `"Find my tax documents"` — searches the file index (requires aetherfs-core)
- `"Help"` — shows available commands

## License

MIT
