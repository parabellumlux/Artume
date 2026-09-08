#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# Artume OS — Start Script
# ──────────────────────────────────────────────────────────────────────────────
# Starts the full Artume OS conversational shell with dual-GPU AI pipeline.
#
# Usage:
#   ./start.sh              # Build + run the Rust conversational shell
#   ./start.sh --voice      # Build + run with voice I/O (requires model files)
#   ./start.sh --check      # Just verify prerequisites, don't run
#   ./start.sh --python     # Run the Python desktop assistant instead
#   ./start.sh --help       # This message
# ──────────────────────────────────────────────────────────────────────────────

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

RED='\033[1;31m'; GRN='\033[1;32m'; CYN='\033[1;36m'; YLW='\033[1;33m'; NC='\033[0m'
ok()   { echo -e " ${GRN}✓${NC} $*"; }
info() { echo -e " ${CYN}→${NC} $*"; }
warn() { echo -e " ${YLW}⚠${NC} $*"; }
die()  { echo -e " ${RED}✗${NC} $*" >&2; exit 1; }

# ── Help ────────────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    sed -n '2,/^$/{ s/^#//; s/^ //p }' "$0"
    exit 0
fi

echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║           Artume OS — Starting Up            ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

# ── Python mode (early branch — skips GPU/Ollama checks) ──────────────────
if [[ "${1:-}" == "--python" ]]; then
    info "Starting Artume OS Python desktop assistant..."
    echo ""

    # Check Python
    if ! command -v python3 &>/dev/null; then
        die "python3 not found"
    fi
    ok "Python3 found: $(python3 --version 2>&1)"

    # Check/create virtualenv
    if [ ! -d ".venv" ]; then
        warn "No virtualenv found — creating one..."
        python3 -m venv .venv
    fi
    source .venv/bin/activate
    ok "Virtualenv activated"

    # Install dependencies
    info "Installing Python dependencies..."
    pip install -q -r requirements.txt 2>/dev/null || true
    ok "Python dependencies installed"

    # Check Piper TTS binary
    if [ ! -x "piper/piper" ]; then
        die "Piper TTS binary not found at piper/piper — is it installed?"
    fi
    ok "Piper TTS binary found"

    # Check Piper model file
    if [ ! -f "en_US-lessac-medium.onnx" ]; then
        die "Piper model file not found: en_US-lessac-medium.onnx"
    fi
    ok "Piper model file found"

    # Check aplay (ALSA audio output)
    if ! command -v aplay &>/dev/null; then
        die "aplay not found — install ALSA utils (sudo apt install alsa-utils)"
    fi
    ok "aplay (ALSA) found"

    # Check amixer (volume control)
    if ! command -v amixer &>/dev/null; then
        warn "amixer not found — volume control may not work"
    else
        ok "amixer found"
    fi

    # Check clips directory (Phase 999 pre-recorded responses)
    CLIP_COUNT=$(find clips/ -name "*.wav" 2>/dev/null | wc -l)
    if [ "$CLIP_COUNT" -lt 20 ]; then
        warn "Only $CLIP_COUNT pre-recorded clips found (expected 23) — run scripts/generate_clips.py"
    else
        ok "Pre-recorded clips: $CLIP_COUNT WAV files"
    fi

    # Check whisper model availability (will auto-download on first run)
    ok "Whisper STT: will auto-download tiny.en model on first run"

    echo ""
    info "Starting Artume OS Python desktop assistant..."
    echo ""
    exec python3 artome_core.py
fi

# ── Rust mode: GPU check ───────────────────────────────────────────────────
info "Checking GPUs..."
if ! command -v nvidia-smi &>/dev/null; then
    die "nvidia-smi not found — is the NVIDIA driver installed?"
fi

GPU_COUNT=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | wc -l)
if [ "$GPU_COUNT" -lt 2 ]; then
    warn "Only $GPU_COUNT GPU(s) detected. Expected 2 (1080 + 1650 Super)."
fi

nvidia-smi --query-gpu=index,name,memory.total --format=csv,noheader 2>/dev/null | \
    while IFS=, read -r idx name mem; do
        ok "GPU $idx: $name ($mem)"
    done
echo ""

# ── Rust mode: Check Ollama ────────────────────────────────────────────────
info "Checking Ollama..."
if ! command -v ollama &>/dev/null; then
    die "ollama not found — install it from https://ollama.com"
fi

if ! curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
    warn "Ollama server not running — starting it..."
    ollama serve &>/dev/null &
    OLLAMA_PID=$!
    sleep 2
    if ! curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
        die "Ollama failed to start"
    fi
    ok "Ollama server started (PID $OLLAMA_PID)"
else
    ok "Ollama server running"
fi
echo ""

# ── Rust mode: Check required models ───────────────────────────────────────
info "Checking AI models..."
MODELS=$(ollama list 2>/dev/null)

check_model() {
    local name="$1" gpu="$2" desc="$3"
    if echo "$MODELS" | grep -q "$name"; then
        ok "$desc ($name) on $gpu"
    else
        warn "$desc ($name) not found — pulling..."
        CUDA_VISIBLE_DEVICES="$gpu" ollama pull "$name" 2>&1 | tail -1
        ok "$desc ($name) pulled"
    fi
}

check_model "llama3.1:8b"        "0" "Tier 1 — Reasoning (GTX 1080)"
check_model "nemotron-3-nano:4b" "1" "Tier 2 — Router (GTX 1650S)"
check_model "nomic-embed-text"   "" "Tier 3 — Embeddings (CPU)"
echo ""

# ── Rust mode: Warm up models ──────────────────────────────────────────────
info "Warming up AI models..."
warm_model() {
    local name="$1" desc="$2"
    curl -s http://localhost:11434/api/generate \
        -d "{\"model\":\"$name\",\"prompt\":\"\",\"stream\":false,\"keep_alive\":-1}" \
        >/dev/null 2>&1 && ok "$desc warmed up" || warn "$desc warm-up failed"
}
warm_model "llama3.1:8b"        "Llama 3.1 8B (reasoning/router)"
warm_model "nemotron-3-nano:4b" "Nemotron-3 Nano (legacy router)"
echo ""

# ── Just check mode ────────────────────────────────────────────────────────
if [[ "${1:-}" == "--check" ]]; then
    echo ""
    echo "╔══════════════════════════════════════════════╗"
    echo "║        All prerequisites satisfied            ║"
    echo "╚══════════════════════════════════════════════╝"
    echo ""
    echo "  Run:  ./start.sh              # Rust conversational shell"
    echo "  Run:  ./start.sh --voice      # With voice I/O"
    echo "  Run:  ./start.sh --python    # Python desktop assistant"
    echo ""
    exit 0
fi

# ── Build Rust shell ───────────────────────────────────────────────────────
info "Building Artume OS conversational shell..."
echo ""
cargo build --release -p aether-orchestrator 2>&1 | tail -3
ok "Build complete"
echo ""

# ── Run ────────────────────────────────────────────────────────────────────
VOICE_FLAG=""
if [[ "${1:-}" == "--voice" ]]; then
    VOICE_FLAG="-- --voice"
fi

echo "╔══════════════════════════════════════════════╗"
echo "║        Artume OS — Ready for Input           ║"
echo "╚══════════════════════════════════════════════╝"
echo ""
echo "  Type 'quit' or 'exit' to stop."
echo ""

exec cargo run --release -p aether-orchestrator --bin aether-shell $VOICE_FLAG
