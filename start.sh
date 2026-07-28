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

# ── 1. Check NVIDIA GPUs ────────────────────────────────────────────────────
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

# ── 2. Check Ollama ─────────────────────────────────────────────────────────
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

# ── 3. Check required models ────────────────────────────────────────────────
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

# ── 4. Check Python deps (for --python mode) ────────────────────────────────
if [[ "${1:-}" == "--python" ]]; then
    info "Checking Python dependencies..."
    if [ ! -d ".venv" ]; then
        warn "No virtualenv found — creating one..."
        python3 -m venv .venv
    fi
    source .venv/bin/activate
    pip install -q -r requirements.txt 2>/dev/null || true
    ok "Python environment ready"
    echo ""

    info "Starting Artume OS Python desktop assistant..."
    echo ""
    exec python3 artome_core.py
fi

# ── 5. Just check mode ──────────────────────────────────────────────────────
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

# ── 6. Build Rust shell ─────────────────────────────────────────────────────
info "Building Artume OS conversational shell..."
echo ""
cargo build --release -p aether-orchestrator 2>&1 | tail -3
ok "Build complete"
echo ""

# ── 7. Run ──────────────────────────────────────────────────────────────────
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
