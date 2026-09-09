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

import re
import requests
import subprocess
import time

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
    "ide_action",
    "lsp_action",
    "dap_action",
    "unknown",
]

_screen_reader = None


def _get_screen_reader():
    global _screen_reader
    if _screen_reader is None:
        from screen_reader import AtspiScreenReader

        _screen_reader = AtspiScreenReader()
    return _screen_reader


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

    if any(w in low for w in ["next heading", "previous heading", "prev heading"]):
        return {"action": "web_navigate", "speech": "Navigating headings", "target": "navigate_headings"}

    if any(w in low for w in ["next link", "previous link", "prev link"]):
        return {"action": "web_navigate", "speech": "Navigating links", "target": "navigate_links"}

    # --- web_fetch ---
    if label == "web_fetch" or any(w in low for w in ["read me", "read http", "fetch", "browse", "open http", ".com", ".org"]):
        # Browser navigation commands
        if any(w in low for w in ["go back", "back", "previous page"]):
            return {"action": "web_navigate", "speech": "Going back", "target": "go_back"}
        if any(w in low for w in ["go forward", "forward", "next page"]):
            return {"action": "web_navigate", "speech": "Going forward", "target": "go_forward"}
        if any(w in low for w in ["bookmark this", "save bookmark", "bookmark page"]):
            return {"action": "web_navigate", "speech": "Saving bookmark", "target": "bookmark"}
        if any(w in low for w in ["list bookmarks", "show bookmarks", "my bookmarks"]):
            return {"action": "web_navigate", "speech": "Listing bookmarks", "target": "list_bookmarks"}
        if any(w in low for w in ["open bookmark"]):
            nums = re.findall(r'\d+', low)
            idx = int(nums[0]) if nums else 1
            return {"action": "web_navigate", "speech": f"Opening bookmark {idx}", "target": f"open_bookmark:{idx}"}
        url = _extract_after(text, ["read me", "read", "fetch", "browse", "open", "go to"])
        if "http" in low or ".com" in low or ".org" in low:
            return {"action": "web_navigate", "speech": f"Loading {url}", "target": f"url:{url}"}
        return {"action": "web_navigate", "speech": "Reading page content", "target": "read"}

    # --- file_search ---
    if label == "file_search" or any(w in low for w in ["find my", "find", "search for", "search files", "look for"]):
        query = _extract_after(text, ["find my", "find", "search for", "search", "look for"])
        return {"action": "file_action", "speech": f"Searching for {query}", "target": f"search_file:{query}"}

    # --- file operations (copy, move, rename, delete, create, sort) ---
    if any(w in low for w in ["copy file", "copy "]):
        parts = re.sub(r'^(copy file|copy)\s*', '', low).split(" to ")
        if len(parts) == 2:
            return {"action": "file_op", "speech": f"Copying {parts[0].strip()}", "target": f"copy:{parts[0].strip()}:{parts[1].strip()}"}
        return {"action": "file_op", "speech": "Copy requires source and destination", "target": "copy_help"}

    if any(w in low for w in ["move file", "move "]):
        parts = re.sub(r'^(move file|move)\s*', '', low).split(" to ")
        if len(parts) == 2:
            return {"action": "file_op", "speech": f"Moving {parts[0].strip()}", "target": f"move:{parts[0].strip()}:{parts[1].strip()}"}
        return {"action": "file_op", "speech": "Move requires source and destination", "target": "move_help"}

    if any(w in low for w in ["rename file", "rename "]):
        parts = re.sub(r'^(rename file|rename)\s*', '', low).split(" to ")
        if len(parts) == 2:
            return {"action": "file_op", "speech": f"Renaming {parts[0].strip()}", "target": f"rename:{parts[0].strip()}:{parts[1].strip()}"}
        return {"action": "file_op", "speech": "Rename requires old and new name", "target": "rename_help"}

    if any(w in low for w in ["delete file", "delete ", "remove file", "remove "]) \
            and not any(w in low for w in ["delete line", "delete lines",
                                           "remove line", "remove lines",
                                           "delete current line", "cut line"]):
        name = re.sub(r'^(delete file|delete|remove file|remove)\s*', '', low)
        return {"action": "file_op", "speech": f"Deleting {name}", "target": f"delete:{name}"}

    if any(w in low for w in ["create file", "new file", "make file"]):
        name = re.sub(r'^(create file|new file|make file)\s*', '', low)
        return {"action": "file_op", "speech": f"Creating file {name}", "target": f"create_file:{name}"}

    if any(w in low for w in ["create folder", "new folder", "make folder", "mkdir"]):
        name = re.sub(r'^(create folder|new folder|make folder|mkdir)\s*', '', low)
        return {"action": "file_op", "speech": f"Creating folder {name}", "target": f"create_folder:{name}"}

    if any(w in low for w in ["sort by name", "sort by date", "sort by size", "sort files"]):
        by = "name"
        if "date" in low:
            by = "date"
        elif "size" in low:
            by = "size"
        return {"action": "file_op", "speech": f"Sorting by {by}", "target": f"sort:{by}"}

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
            return {"action": "setting_action", "speech": "Setting timer", "target": f"set_timer:{text}"}
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

    # --- IDE commands from any mode (AI, Git, Terminal) ---
    if any(w in low for w in ["fix this", "fix code", "fix error", "explain this", "explain code",
                               "what does this do", "generate tests", "write tests", "add tests",
                               "review code", "review this", "optimize code", "optimize this",
                               "add docstring", "generate docstring", "add type hints",
                               "refactor code", "refactor this"]):
        return {"action": "ide_action", "speech": f"AI: {text}", "target": text}

    if any(w in low for w in ["git status", "git changes", "git diff", "git log", "git history",
                               "git branch", "current branch", "git commit", "git push",
                               "git pull", "git switch", "checkout branch"]):
        return {"action": "ide_action", "speech": f"Git: {text}", "target": text}

    if any(w in low for w in ["run tests", "run test", "run file", "run this file",
                               "run make", "run build", "make build", "run command",
                               "show terminal", "terminal output", "clear terminal"]):
        return {"action": "ide_action", "speech": f"Terminal: {text}", "target": text}

    # --- LSP commands (go to definition, find references, rename, hover) ---
    if any(w in low for w in ["go to definition", "jump to definition", "find definition",
                               "where is this defined", "where defined"]):
        return {"action": "lsp_action", "speech": "Finding definition", "target": "go_to_definition"}

    if any(w in low for w in ["find references", "show references", "who uses this",
                               "where is this used", "all references"]):
        return {"action": "lsp_action", "speech": "Finding references", "target": "find_references"}

    if any(w in low for w in ["rename symbol", "rename this", "rename to",
                               "rename function", "rename variable", "rename class"]):
        new_name = re.sub(r'^(rename (symbol|this|function|variable|class)?)\s*', '', low) if 'rename' in low else ""
        if not new_name:
            new_name = re.sub(r'^rename\s*', '', low)
        return {"action": "lsp_action", "speech": f"Renaming to {new_name}", "target": f"rename:{new_name}"}

    if any(w in low for w in ["hover", "what is this", "type info", "type of",
                               "what type", "what kind"]):
        return {"action": "lsp_action", "speech": "Checking type info", "target": "hover"}

    # --- Spoken navigation (any mode) ---
    if any(w in low for w in ["next function", "next class", "next method",
                              "next symbol", "previous function", "previous class",
                              "previous method", "previous symbol", "prev function",
                              "prev class", "prev symbol", "next line",
                              "previous line", "go to line", "jump to line",
                              "jump down", "jump up", "move up", "move down",
                              "go up one line", "go down one line",
                              "start of file", "top of file", "beginning of file",
                              "end of file", "bottom of file", "where am i",
                              "cursor position"]):
        return {"action": "ide_action", "speech": "IDE navigation",
                "target": f"navigate:{text}"}

    # --- Spoken editing (any mode, edit-in-place) ---
    if any(w in low for w in ["insert line", "insert after", "insert before",
                              "insert at line", "add line", "add new line",
                              "replace line", "replace current line", "change line",
                              "change current line", "rewrite line", "set line",
                              "delete line", "remove line", "delete lines",
                              "remove lines", "cut line"]):
        return {"action": "ide_action", "speech": "Editing code",
                "target": f"edit:{text}"}

    # --- DAP commands (debug, step, continue, breakpoint) ---
    if any(w in low for w in ["start debugging", "debug this", "debug file", "run debugger",
                               "start debug session"]):
        return {"action": "dap_action", "speech": "Starting debug session", "target": "launch"}

    if any(w in low for w in ["set breakpoint", "add breakpoint", "break here",
                               "pause here", "stop here"]):
        nums = re.findall(r'\d+', low)
        line = int(nums[0]) if nums else 1
        return {"action": "dap_action", "speech": f"Setting breakpoint at line {line}", "target": f"breakpoint:{line}"}

    if any(w in low for w in ["continue", "resume", "keep running", "go on"]):
        return {"action": "dap_action", "speech": "Continuing execution", "target": "continue"}

    if any(w in low for w in ["step over", "next line", "step", "step next"]):
        return {"action": "dap_action", "speech": "Stepping over", "target": "step_over"}

    if any(w in low for w in ["step into", "go into", "enter function"]):
        return {"action": "dap_action", "speech": "Stepping into", "target": "step_into"}

    if any(w in low for w in ["step out", "exit function", "return from"]):
        return {"action": "dap_action", "speech": "Stepping out", "target": "step_out"}

    if any(w in low for w in ["list locals", "show locals", "local variables",
                              "inspect locals", "list variables", "read locals"]):
        return {"action": "dap_action", "speech": "Listing locals", "target": "list_locals"}

    if any(w in low for w in ["where is the debugger", "debugger position",
                              "debugger location", "pause position",
                              "where are we", "where is execution"]):
        return {"action": "dap_action", "speech": "Debugger position", "target": "where"}

    if any(w in low for w in ["pause execution", "pause debugger", "pause code",
                              "hold execution", "pause the debugger"]):
        return {"action": "dap_action", "speech": "Pausing debugger", "target": "pause"}

    if any(w in low for w in ["evaluate", "inspect", "watch", "print variable"]):
        expr = re.sub(r'^(evaluate|inspect|watch|print variable)\s*', '', low)
        return {"action": "dap_action", "speech": f"Evaluating {expr}", "target": f"evaluate:{expr}"}

    # --- screen summary ---
    if any(w in low for w in ["screen", "what is on", "read screen", "what's on"]):
        return {"action": "screen_summary", "speech": "Reading screen", "target": "COMMAND:SCREEN_SUMMARY"}

    # --- calculator ---
    if any(w in low for w in ["calculate", "compute", "what is ", "what's ", "how much is ", "math"]):
        return {"action": "calculator", "speech": "Calculating", "target": text}

    # --- weather ---
    if any(w in low for w in ["weather", "forecast", "temperature", "outside"]):
        return {"action": "weather", "speech": "Checking weather", "target": text}

    # --- notes ---
    if any(w in low for w in ["note", "take a note", "save note", "read note", "list notes",
                               "search note", "delete note", "clear notes"]):
        return {"action": "notes", "speech": "Managing notes", "target": text}

    # --- email folder/search/attachment commands ---
    if any(w in low for w in ["list folders", "email folders", "show folders", "go to inbox",
                               "go to sent", "switch folder"]):
        if "inbox" in low:
            return {"action": "email_folder", "speech": "Switching to inbox", "target": "INBOX"}
        if "sent" in low:
            return {"action": "email_folder", "speech": "Switching to sent", "target": "Sent"}
        return {"action": "email_folder", "speech": "Listing folders", "target": "list_folders"}

    if any(w in low for w in ["search email", "find email", "search inbox", "email about",
                               "email from", "email matching"]):
        query = re.sub(r'^(search (email|inbox)|find email|email (about|from|matching))\s*', '', low)
        return {"action": "email_search", "speech": f"Searching emails for {query}", "target": query}

    if any(w in low for w in ["read email", "read inbox", "read mail", "check email",
                               "check inbox", "what emails", "any emails"]):
        return {"action": "email_action", "speech": "Checking email", "target": "fetch_inbox"}

    if any(w in low for w in ["open email", "read message", "open message"]):
        nums = re.findall(r'\d+', low)
        idx = int(nums[0]) if nums else 1
        return {"action": "email_action", "speech": f"Reading email {idx}", "target": f"read_email:{idx}"}

    if any(w in low for w in ["attachments", "list attachments", "read attachments"]):
        nums = re.findall(r'\d+', low)
        idx = int(nums[0]) if nums else 1
        return {"action": "email_action", "speech": f"Checking attachments for email {idx}", "target": f"attachments:{idx}"}

    if any(w in low for w in ["send email", "send the email", "send email now", "send the draft"]):
        return {"action": "email_action", "speech": "Sending email", "target": "send_draft"}

    if any(w in low for w in ["compose email", "compose an email", "new email", "write email",
                              "draft email", "compose a message", "write a message"]):
        return {"action": "email_action", "speech": "Preparing to compose", "target": "compose"}

    if any(w in low for w in ["cancel draft", "discard email", "delete draft", "discard draft"]):
        return {"action": "email_action", "speech": "Cancelling email draft", "target": "cancel_draft"}

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
        if any(w in low for w in ["check", "inbox", "unread"]):
            return {"action": "email_action", "speech": "Checking your inbox", "target": "inbox"}
        if "send" in low:
            return {"action": "email_action", "speech": "Sending email", "target": "send_draft"}
        if "read" in low and ("email" in low or any(c.isdigit() for c in low)):
            return {"action": "email_action", "speech": "Reading email", "target": "read"}
        if any(w in low for w in ["compose", "write"]):
            return {"action": "email_action", "speech": "Preparing to compose", "target": "compose"}

    if mode == "IDE":
        if any(w in low for w in ["open", "load", "read"]) and ("file" in low or ".py" in low or ".rs" in low):
            return {"action": "ide_action", "speech": f"Opening {text}", "target": f"open_code:{text}"}
        if "function" in low or "def " in low:
            return {"action": "ide_action", "speech": "Reading function", "target": f"read_function:{text}"}
        if "line" in low and any(c.isdigit() for c in low):
            return {"action": "ide_action", "speech": "Reading lines", "target": f"read_lines:{text}"}
        if any(w in low for w in ["fix", "explain", "test", "review", "optimize", "docstring", "refactor"]):
            return {"action": "ide_action", "speech": f"AI: {text}", "target": f"{text}"}
        if "git" in low:
            return {"action": "ide_action", "speech": f"Git: {text}", "target": f"git_{text}"}
        if any(w in low for w in ["run", "test", "build", "make"]):
            return {"action": "ide_action", "speech": f"Running: {text}", "target": f"run:{text}"}

    # --- IDE commands from ANY mode (AI, Git, Terminal) ---
    if any(w in low for w in ["fix this", "fix code", "fix error", "explain this", "explain code",
                               "what does this do", "generate tests", "write tests", "add tests",
                               "review code", "review this", "optimize code", "optimize this",
                               "add docstring", "generate docstring", "add type hints",
                               "refactor code", "refactor this"]):
        return {"action": "ide_action", "speech": f"AI: {text}", "target": text}

    if any(w in low for w in ["git status", "git changes", "git diff", "git log", "git history",
                               "git branch", "current branch", "git commit", "git push",
                               "git pull", "git switch", "checkout branch"]):
        return {"action": "ide_action", "speech": f"Git: {text}", "target": text}

    if any(w in low for w in ["run tests", "run test", "run file", "run this file",
                               "run make", "run build", "make build", "run command",
                               "show terminal", "terminal output", "clear terminal"]):
        return {"action": "ide_action", "speech": f"Terminal: {text}", "target": text}

    if mode == "FILES":
        if any(w in low for w in ["list", "show", "what", "contents", "files", "folders"]):
            return {"action": "file_action", "speech": "Listing directory contents", "target": "list_dir"}
        if any(w in low for w in ["go up", "parent folder", "parent directory", "go back",
                                   "up one level", "one level up"]):
            return {"action": "file_action", "speech": "Moving up", "target": "go_up"}
        if any(w in low for w in ["next file", "next folder", "next entry", "next",
                                   "previous file", "previous folder", "previous entry",
                                   "previous", "prev file", "prev folder", "prev entry", "prev"]):
            return {"action": "file_action", "speech": "Moving to next file", "target": "navigate_entries"}
        if any(w in low for w in ["go to", "open", "enter", "cd", "change"]) and "folder" in low:
            return {"action": "file_action", "speech": f"Navigating to {text}", "target": f"change_dir:{text}"}
        if "read" in low or "open" in low:
            return {"action": "file_action", "speech": "Reading file", "target": f"read_file:{text}"}

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
                return {"action": "setting_action", "speech": "Setting volume", "target": f"set_volume:{text}"}
        if any(w in low for w in ["status", "time", "battery", "system"]):
            return {"action": "setting_action", "speech": "Checking system status", "target": "status"}
        if "timer" in low:
            return {"action": "setting_action", "speech": "Setting timer", "target": f"set_timer:{text}"}
        if "bluetooth" in low:
            return {"action": "setting_action", "speech": f"Bluetooth: {text}", "target": f"bluetooth:{text}"}
        if "wifi" in low:
            return {"action": "setting_action", "speech": f"WiFi: {text}", "target": f"wifi:{text}"}
        if any(w in low for w in ["audio", "sound", "speaker", "headphone"]):
            return {"action": "setting_action", "speech": f"Audio: {text}", "target": f"audio:{text}"}

    if mode == "EBOOK":
        if any(w in low for w in ["open", "load"]) and ("book" in low or ".epub" in low or ".pdf" in low):
            return {"action": "ebook_action", "speech": "Opening book", "target": f"open_book:{text}"}
        if "chapter" in low:
            return {"action": "ebook_action", "speech": "Reading chapter", "target": f"read_chapter:{text}"}
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
    low_speech = user_speech.lower().strip()

    if not low_speech:
        return {"action": "speak", "speech": "I didn't catch that. Can you say it again?", "target": ""}

    # ====================================================================
    # TIER 0: Fast path — screen summary (bypasses model entirely)
    # ====================================================================
    if "screen" in low_speech or "what is on screen" in low_speech or "read screen" in low_speech or mode == "SCREEN_SUMMARY":
        # The actual summarisation happens once, in execute_action's
        # screen_summary branch (payload -> Ollama -> spoken fallback), so the
        # router stays fast and there is a single generation path.
        return {"action": "screen_summary", "speech": "Summarizing screen", "target": "COMMAND:SCREEN_SUMMARY"}

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
