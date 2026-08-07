#!/usr/bin/env python3
"""AI Intent Router for Artome DE — uses GPU-backed Ollama models.

Tier 1: Llama 3.1 8B on GTX 1080 (GPU 0) — main reasoning/conversation
Tier 2: Nemotron-3 Nano on GTX 1650S (GPU 1) — router/tool-caller/guardrails
Tier 3: nomic-embed-text on CPU — embeddings

DESIGN (v2 — two-stage, deterministic dispatch):

  The v1 router asked a 4B model to do two hard things at once: classify the
  intent AND emit a full JSON action payload. Empirically Nemotron-3 Nano
  returns empty for most utterances, so the router silently fell back to
  "conversation" — the exact bug this rewrite fixes.

  v2 splits the problem:

    Stage 1 — CLASSIFY (LLM, label-only, greedy, 8s timeout)
        Llama 3.1 8B returns ONE label word. This is the one thing the model
        does reliably (verified 8/8 on live tests). Nemotron is demoted to a
        non-critical fallback because it returns empty for nearly everything.

    Stage 2 — DISPATCH (deterministic, no LLM)
        A pure-Python dispatcher maps (label + mode + keyword extraction) to a
        concrete action dict. No model involved, so it can never return empty.

  Fallback ladder (never returns empty speech):
      1. Keyword pre-classification (fast path, zero latency, no model)
      2. Llama 3.1 8B label classification (reliable)
      3. Nemotron label classification (best-effort, usually empty)
      4. Safe default conversational response

  Both the Python and Rust shells converge on this same design so they behave
  identically.
"""

import json
import re
import requests
import subprocess
import time
from screen_reader import AtspiScreenReader

OLLAMA_URL = "http://localhost:11434/api/generate"
ROUTER_MODEL = "llama3.1:8b"          # Tier 1 — GTX 1080 (reliable classifier)
LEGACY_ROUTER_MODEL = "nemotron-3-nano:4b"  # Tier 2 — GTX 1650S (best-effort)
REASONING_MODEL = "llama3.1:8b"       # Tier 1 — GTX 1080
EMBED_MODEL = "nomic-embed-text"      # Tier 3 — CPU

# Intent labels the classifier can return. Keep in sync with Rust router.rs.
INTENT_LABELS = [
    "conversation",
    "entity_lookup",
    "web_fetch",
    "file_search",
    "execute_action",
    "system_command",
    "switch_mode",
    "unknown",
]

screen_reader = AtspiScreenReader()


def get_active_window():
    """Retrieve title of currently focused desktop window."""
    try:
        res = subprocess.check_output(
            ["xdotool", "getactivewindow", "getwindowname"],
            stderr=subprocess.STDOUT, text=True,
        )
        return res.strip()
    except Exception:
        return "Unknown Desktop Window"


def _query_ollama(model: str, prompt: str, timeout: int = 12, temperature: float = 0.2, num_predict: int = 120) -> str:
    """Query Ollama with retry logic. Returns empty string on failure."""
    for attempt in range(2):
        try:
            res = requests.post(
                OLLAMA_URL,
                json={
                    "model": model,
                    "prompt": prompt,
                    "stream": False,
                    "options": {
                        "temperature": temperature,
                        "num_predict": num_predict,
                    },
                },
                timeout=timeout,
            )
            data = res.json()
            response = data.get("response", "").strip()
            if response:
                return response
            # Empty response — model may still be loading
            if attempt == 0:
                time.sleep(2)  # Give model time to load
                continue
        except requests.Timeout:
            if attempt == 0:
                time.sleep(1)
                continue
        except Exception:
            if attempt == 0:
                time.sleep(1)
                continue
    return ""


# ---------------------------------------------------------------------------
# Stage 1 — Classification
# ---------------------------------------------------------------------------

def _classify_prompt(utterance: str) -> str:
    """Build the label-only classification prompt (the format that works)."""
    return (
        "Classify this user utterance into exactly one intent label. "
        "Return ONLY the label word, nothing else.\n\n"
        f"Labels: {', '.join(INTENT_LABELS)}\n\n"
        f"Utterance: {utterance}\n\n"
        "Intent:"
    )


def _normalize_label(raw: str) -> str:
    """Map a raw model output to a known label, or 'unknown'."""
    low = raw.strip().lower()
    # Strip punctuation / trailing words
    low = re.sub(r"[^a-z_]", "", low)
    if low in INTENT_LABELS:
        return low
    # Fuzzy: label embedded in a longer response
    for label in INTENT_LABELS:
        if label in low:
            return label
    return "unknown"


def _classify_llm(utterance: str) -> str:
    """Classify via Llama 3.1 8B (reliable). Returns a label or 'unknown'."""
    raw = _query_ollama(
        ROUTER_MODEL, _classify_prompt(utterance),
        timeout=8, temperature=0.0, num_predict=32,
    )
    if not raw:
        return "unknown"
    return _normalize_label(raw)


def _classify_legacy(utterance: str) -> str:
    """Best-effort classification via Nemotron (usually empty)."""
    raw = _query_ollama(
        LEGACY_ROUTER_MODEL, _classify_prompt(utterance),
        timeout=6, temperature=0.0, num_predict=32,
    )
    if not raw:
        return "unknown"
    return _normalize_label(raw)


# ---------------------------------------------------------------------------
# Stage 2 — Deterministic dispatch
# ---------------------------------------------------------------------------

def _extract_after(text: str, prefixes) -> str:
    """Return the substring after the first matching prefix, stripped."""
    low = text.lower()
    for prefix in prefixes:
        if prefix in low:
            return text[low.index(prefix) + len(prefix):].strip()
    return text


def _dispatch(label: str, text: str, mode: str) -> dict:
    """Deterministically map (label, mode, text) to a concrete action dict.

    No LLM involved — this can never return empty speech.
    """
    low = text.lower().strip()

    # --- switch_mode: explicit mode change ---
    if label == "switch_mode" or any(w in low for w in ["switch to", "change to", "go to", "open"]) and \
            any(m in low for m in ["browser", "email", "ide", "files", "docs", "settings", "ebook", "desktop"]):
        for m in ["browser", "email", "ide", "files", "docs", "settings", "ebook", "desktop"]:
            if m in low:
                return {"action": "switch_mode", "speech": f"Switching to {m} mode", "target": m.upper()}

    # --- entity_lookup ---
    if label == "entity_lookup" or any(w in low for w in ["copy that", "tracking number", "look up", "entity"]):
        return {"action": "speak", "speech": "Looking up that entity from our conversation.", "target": "entity_lookup"}

    # --- web_fetch ---
    if label == "web_fetch" or any(w in low for w in ["read me", "read http", "fetch", "browse", "open http", ".com", ".org"]):
        url = _extract_after(text, ["read me", "read", "fetch", "browse", "open", "go to"])
        if "http" in low or ".com" in low or ".org" in low:
            return {"action": "web_navigate", "speech": f"Loading {url}", "target": f"url:{url}"}
        return {"action": "web_navigate", "speech": "Reading page content", "target": "read"}

    # --- file_search ---
    if label == "file_search" or any(w in low for w in ["find my", "find", "search for", "search files", "look for"]):
        query = _extract_after(text, ["find my", "find", "search for", "search", "look for"])
        return {"action": "file_action", "speech": f"Searching for {query}", "target": f"search_file:{query}"}

    # --- system_command (settings) ---
    if label == "system_command" or any(w in low for w in ["volume", "status", "timer", "bluetooth", "wifi", "audio", "sound", "speaker", "headphone", "battery", "time"]):
        if "volume" in low:
            if "up" in low or "increase" in low:
                return {"action": "setting_action", "speech": "Increasing volume", "target": "volume_up"}
            if "down" in low or "decrease" in low:
                return {"action": "setting_action", "speech": "Decreasing volume", "target": "volume_down"}
            if "set" in low or any(c.isdigit() for c in low):
                return {"action": "setting_action", "speech": "Setting volume", "target": f"set_volume:{text}"}
        if any(w in low for w in ["status", "battery", "system"]):
            return {"action": "setting_action", "speech": "Checking system status", "target": "status"}
        if "timer" in low:
            return {"action": "setting_action", "speech": f"Setting timer", "target": f"set_timer:{text}"}
        if "bluetooth" in low:
            return {"action": "setting_action", "speech": f"Bluetooth: {text}", "target": f"bluetooth:{text}"}
        if "wifi" in low:
            return {"action": "setting_action", "speech": f"WiFi: {text}", "target": f"wifi:{text}"}
        if any(w in low for w in ["audio", "sound", "speaker", "headphone"]):
            return {"action": "setting_action", "speech": f"Audio: {text}", "target": f"audio:{text}"}
        if "time" in low:
            return {"action": "setting_action", "speech": "Checking the time", "target": "status"}

    # --- execute_action (open app / launch) ---
    if label == "execute_action" or any(w in low for w in ["open ", "launch", "start ", "run "]):
        app = _extract_after(text, ["open", "launch", "start", "run"])
        if app and app != text:
            return {"action": "open_app", "speech": f"Opening {app}", "target": app}
        return {"action": "speak", "speech": f"Executing: {text}", "target": ""}

    # --- conversation (default) ---
    if any(w in low for w in ["hello", "hi ", "hey", "good morning", "good evening", "what's up", "how are"]):
        return {"action": "speak", "speech": "Hello! How can I help you?", "target": ""}

    # --- help ---
    if any(w in low for w in ["help", "what can i say", "what can i do", "commands"]):
        return {"action": "speak", "speech": "I can help you with browsing, email, files, documents, books, settings, and coding. Say 'switch to browser' or just tell me what you want to do.", "target": "help"}

    # --- screen summary ---
    if any(w in low for w in ["screen", "what is on", "read screen", "what's on"]):
        return {"action": "screen_summary", "speech": "Reading screen", "target": "COMMAND:SCREEN_SUMMARY"}

    # --- mode-specific keyword fallbacks ---
    mode_result = _mode_keywords(text, mode)
    if mode_result:
        return mode_result

    # --- safe default (never empty) ---
    return {"action": "speak", "speech": f"I heard: {text[:100]}. I'm not sure what to do with that. Try saying 'help' or 'what can I say' for available commands.", "target": ""}


def _mode_keywords(text: str, mode: str) -> dict:
    """Mode-specific keyword dispatch (used when label is ambiguous)."""
    low = text.lower().strip()

    if mode == "BROWSER":
        if any(w in low for w in ["search", "find", "look up", "google", "duckduckgo"]):
            query = _extract_after(text, ["search for", "search", "find", "look up", "google", "duckduckgo"])
            return {"action": "web_navigate", "speech": f"Searching for {query}", "target": f"search:{query}"}
        if "heading" in low or "headings" in low:
            return {"action": "web_navigate", "speech": "Listing headings", "target": "headings"}
        if "link" in low:
            return {"action": "web_navigate", "speech": "Listing links", "target": "links"}

    if mode == "EMAIL":
        if any(w in low for w in ["check", "inbox", "unread", "email"]):
            return {"action": "email_action", "speech": "Checking your inbox", "target": "inbox"}
        if "read" in low and ("email" in low or any(c.isdigit() for c in low)):
            return {"action": "email_action", "speech": "Reading email", "target": "read"}
        if any(w in low for w in ["compose", "send", "reply", "write"]):
            return {"action": "email_action", "speech": "Preparing to compose", "target": "compose"}

    if mode == "IDE":
        if any(w in low for w in ["open", "load", "read"]) and ("file" in low or ".py" in low or ".rs" in low):
            return {"action": "ide_action", "speech": f"Opening {text}", "target": f"open_code:{text}"}
        if "function" in low or "def " in low:
            return {"action": "ide_action", "speech": f"Reading function", "target": f"read_function:{text}"}
        if "line" in low and any(c.isdigit() for c in low):
            return {"action": "ide_action", "speech": f"Reading lines", "target": f"read_lines:{text}"}
        if any(w in low for w in ["fix", "explain", "test", "review", "optimize", "docstring", "refactor"]):
            return {"action": "ide_action", "speech": f"AI: {text}", "target": f"{text}"}
        if "git" in low:
            return {"action": "ide_action", "speech": f"Git: {text}", "target": f"git_{text}"}
        if any(w in low for w in ["run", "test", "build", "make"]):
            return {"action": "ide_action", "speech": f"Running: {text}", "target": f"run:{text}"}

    if mode == "FILES":
        if any(w in low for w in ["list", "show", "what", "contents", "files", "folders"]):
            return {"action": "file_action", "speech": "Listing directory contents", "target": "list_dir"}
        if any(w in low for w in ["go to", "open", "enter", "cd", "change"]) and "folder" in low:
            return {"action": "file_action", "speech": f"Navigating to {text}", "target": f"change_dir:{text}"}
        if "read" in low or "open" in low:
            return {"action": "file_action", "speech": f"Reading file", "target": f"read_file:{text}"}

    if mode == "DOCS":
        if any(w in low for w in ["new", "start", "create"]) and "doc" in low:
            return {"action": "doc_action", "speech": "Starting new document", "target": "new_doc"}
        if "heading" in low:
            return {"action": "doc_action", "speech": "Adding heading", "target": "add_heading"}
        if "paragraph" in low or "add" in low:
            return {"action": "doc_action", "speech": "Adding paragraph", "target": "add_paragraph"}
        if "export" in low or "save" in low:
            return {"action": "doc_action", "speech": "Exporting document", "target": "export_all"}

    if mode == "SETTINGS":
        if "volume" in low:
            if "up" in low or "increase" in low:
                return {"action": "setting_action", "speech": "Increasing volume", "target": "volume_up"}
            if "down" in low or "decrease" in low:
                return {"action": "setting_action", "speech": "Decreasing volume", "target": "volume_down"}
            if "set" in low or any(c.isdigit() for c in low):
                return {"action": "setting_action", "speech": f"Setting volume", "target": f"set_volume:{text}"}
        if any(w in low for w in ["status", "time", "battery", "system"]):
            return {"action": "setting_action", "speech": "Checking system status", "target": "status"}
        if "timer" in low:
            return {"action": "setting_action", "speech": f"Setting timer", "target": f"set_timer:{text}"}
        if "bluetooth" in low:
            return {"action": "setting_action", "speech": f"Bluetooth: {text}", "target": f"bluetooth:{text}"}
        if "wifi" in low:
            return {"action": "setting_action", "speech": f"WiFi: {text}", "target": f"wifi:{text}"}
        if any(w in low for w in ["audio", "sound", "speaker", "headphone"]):
            return {"action": "setting_action", "speech": f"Audio: {text}", "target": f"audio:{text}"}

    if mode == "EBOOK":
        if any(w in low for w in ["open", "load"]) and ("book" in low or ".epub" in low or ".pdf" in low):
            return {"action": "ebook_action", "speech": f"Opening book", "target": f"open_book:{text}"}
        if "chapter" in low:
            return {"action": "ebook_action", "speech": f"Reading chapter", "target": f"read_chapter:{text}"}
        if "list" in low or "chapters" in low:
            return {"action": "ebook_action", "speech": "Listing chapters", "target": "list_chapters"}
        if "bookmark" in low:
            return {"action": "ebook_action", "speech": "Setting bookmark", "target": "set_bookmark"}

    return {}


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------

def ask_artome_ai(user_speech, mode="DESKTOP"):
    """Route a user utterance to a concrete action.

    Two-stage pipeline:
      1. CLASSIFY — keyword fast-path, then Llama 3.1 8B label, then Nemotron.
      2. DISPATCH — deterministic mapping to an action dict.

    Never returns empty speech.
    """
    window_title = get_active_window()
    low_speech = user_speech.lower().strip()

    if not low_speech:
        return {"action": "speak", "speech": "I didn't catch that. Can you say it again?", "target": ""}

    # ====================================================================
    # TIER 0: Fast path — screen summary (bypasses model entirely)
    # ====================================================================
    if "screen" in low_speech or "what is on screen" in low_speech or "read screen" in low_speech or mode == "SCREEN_SUMMARY":
        accessibility_tree = screen_reader.generate_screen_summary_payload()
        summary_prompt = f"""You are Artome OS voice assistant. Summarize what is on the screen for a blind user in 2 concise sentences based on this UI accessibility tree.

Active Window: "{window_title}"
Accessibility UI Elements:
{accessibility_tree}

Spoken summary for blind user:"""

        try:
            res = requests.post(
                OLLAMA_URL,
                json={
                    "model": REASONING_MODEL,
                    "prompt": summary_prompt,
                    "stream": False,
                    "options": {"temperature": 0.3, "num_predict": 90},
                },
                timeout=12,
            )
            ai_summary = res.json()["response"].strip()
            if ai_summary:
                return {"action": "screen_summary", "speech": f"Screen summary: {ai_summary}", "target": "COMMAND:SCREEN_SUMMARY"}
        except Exception:
            pass
        return {"action": "screen_summary", "speech": f"Active window is {window_title}.", "target": "COMMAND:SCREEN_SUMMARY"}

    # ====================================================================
    # STAGE 1: Classify
    # ====================================================================
    # 1a. Keyword fast-path — covers the ~80% of simple commands with zero latency.
    #     Mode-specific keywords first (so "run tests" in IDE mode stays an IDE
    #     action), then generic dispatch. Trust any result that isn't the generic
    #     "I heard..." fallback.
    mode_result = _mode_keywords(user_speech, mode)
    if mode_result:
        return mode_result
    keyword_result = _dispatch("", user_speech, mode)
    if keyword_result.get("speech", "").strip() and not keyword_result["speech"].startswith("I heard:"):
        return keyword_result

    # 1b. Llama 3.1 8B label classification (reliable).
    label = _classify_llm(user_speech)
    if label != "unknown":
        result = _dispatch(label, user_speech, mode)
        if result.get("speech", "").strip():
            return result

    # 1c. Nemotron best-effort (usually empty, but try).
    label = _classify_legacy(user_speech)
    if label != "unknown":
        result = _dispatch(label, user_speech, mode)
        if result.get("speech", "").strip():
            return result

    # ====================================================================
    # STAGE 2: Safe default — never return empty speech
    # ====================================================================
    return {"action": "speak", "speech": f"I heard: {user_speech[:80]}. I'm not sure how to help with that. Try saying 'help' or 'what can I say'.", "target": ""}


# Alias ask_ai for compatibility
ask_ai = ask_artome_ai

if __name__ == "__main__":
    print(ask_artome_ai("what is on screen"))
