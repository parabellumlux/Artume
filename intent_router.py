#!/usr/bin/env python3
"""AI Intent Router for Artome DE — uses GPU-backed Ollama models.

Tier 1: Llama 3.1 8B on GTX 1080 (GPU 0) — main reasoning/conversation
Tier 2: Nemotron-3 Nano on GTX 1650S (GPU 1) — router/tool-caller/guardrails
Tier 3: nomic-embed-text on CPU — embeddings

FIXES:
- Handles empty model responses (Nemotron returns "" for simple inputs)
- Multi-tier fallback: Nemotron → keyword → Llama → safe default
- Response validation — never returns empty speech
- Retry logic for transient failures
- Timeout handling
"""

import json
import re
import requests
import subprocess
import time
from screen_reader import AtspiScreenReader

OLLAMA_URL = "http://localhost:11434/api/generate"
ROUTER_MODEL = "nemotron-3-nano:4b"   # Tier 2 — GTX 1650S
REASONING_MODEL = "llama3.1:8b"       # Tier 1 — GTX 1080
EMBED_MODEL = "nomic-embed-text"      # Tier 3 — CPU

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


def _keyword_classify(text: str, mode: str) -> dict:
    """Fast keyword-based classification as fallback when model returns empty."""
    low = text.lower().strip()

    # Mode-specific commands
    if mode == "BROWSER":
        if any(w in low for w in ["search", "find", "look up", "google", "duckduckgo"]):
            query = text
            for prefix in ["search for", "search", "find", "look up", "google", "duckduckgo"]:
                if prefix in low:
                    query = text.lower().split(prefix, 1)[-1].strip()
                    break
            return {"action": "web_navigate", "speech": f"Searching for {query}", "target": f"search:{query}"}
        if any(w in low for w in ["open", "load", "go to"]) and ("http" in low or ".com" in low or ".org" in low):
            return {"action": "web_navigate", "speech": f"Loading {text}", "target": f"url:{text}"}
        if "heading" in low or "headings" in low:
            return {"action": "web_navigate", "speech": "Listing headings", "target": "headings"}
        if "link" in low:
            return {"action": "web_navigate", "speech": "Listing links", "target": "links"}
        if "read" in low or "article" in low:
            return {"action": "web_navigate", "speech": "Reading page content", "target": "read"}

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
        if "run" in low or "test" in low or "build" in low or "make" in low:
            return {"action": "ide_action", "speech": f"Running: {text}", "target": f"run:{text}"}

    if mode == "FILES":
        if any(w in low for w in ["list", "show", "what", "contents", "files", "folders"]):
            return {"action": "file_action", "speech": "Listing directory contents", "target": "list_dir"}
        if any(w in low for w in ["go to", "open", "enter", "cd", "change"]) and "folder" in low:
            return {"action": "file_action", "speech": f"Navigating to {text}", "target": f"change_dir:{text}"}
        if "read" in low or "open" in low:
            return {"action": "file_action", "speech": f"Reading file", "target": f"read_file:{text}"}
        if "search" in low or "find" in low:
            return {"action": "file_action", "speech": f"Searching files", "target": f"search_file:{text}"}

    if mode == "DOCS":
        if any(w in low for w in ["new", "start", "create"]) and "doc" in low:
            return {"action": "doc_action", "speech": "Starting new document", "target": "new_doc"}
        if "heading" in low:
            return {"action": "doc_action", "speech": "Adding heading", "target": "add_heading"}
        if "paragraph" in low or "add" in low:
            return {"action": "doc_action", "speech": "Adding paragraph", "target": "add_paragraph"}
        if "export" in low or "save" in low:
            return {"action": "doc_action", "speech": "Exporting document", "target": "export_all"}
        if "read" in low or "draft" in low:
            return {"action": "doc_action", "speech": "Reading draft", "target": "read_draft"}

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
        if "audio" in low or "sound" in low or "speaker" in low or "headphone" in low:
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
        if "search" in low:
            return {"action": "ebook_action", "speech": f"Searching book", "target": f"search_book:{text}"}

    # Cross-mode commands
    if any(w in low for w in ["screen", "what is on", "read screen", "what's on"]):
        return {"action": "screen_summary", "speech": "Reading screen", "target": "COMMAND:SCREEN_SUMMARY"}

    if any(w in low for w in ["help", "what can i say", "what can i do", "commands"]):
        return {"action": "speak", "speech": "I can help you with browsing, email, files, documents, books, settings, and coding. Say 'switch to browser' or just tell me what you want to do.", "target": "help"}

    if any(w in low for w in ["switch to", "change to", "go to", "open"]) and any(m in low for m in
            ["browser", "email", "ide", "files", "docs", "settings", "ebook", "desktop"]):
        for m in ["browser", "email", "ide", "files", "docs", "settings", "ebook", "desktop"]:
            if m in low:
                return {"action": "switch_mode", "speech": f"Switching to {m} mode", "target": m.upper()}

    if any(w in low for w in ["hello", "hi ", "hey", "good morning", "good evening", "what's up"]):
        return {"action": "speak", "speech": "Hello! How can I help you?", "target": ""}

    # Generic fallback — return a safe conversational response
    return {"action": "speak", "speech": f"I heard: {text[:100]}. I'm not sure what to do with that. Try saying 'help' or 'what can I say' for available commands.", "target": ""}


def ask_artome_ai(user_speech, mode="DESKTOP"):
    """Query AI with context and request JSON intent payload.

    Multi-tier fallback:
    1. Try Nemotron-3 Nano (router model on 1650S)
    2. If empty/fails, try keyword classification
    3. If keyword fails, try Llama 3.1 8B (reasoning model on 1080)
    4. If all fail, return safe default response
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
    # TIER 1: Try Nemotron-3 Nano (router model on GTX 1650S)
    # ====================================================================
    system_prompt = f"""You are Artome OS, an AI voice desktop assistant for a blind user.
Current Mode: {mode}
Active Window: "{window_title}"

Respond strictly with a single JSON object. No preamble.

Format:
{{
  "action": "<ACTION_NAME>",
  "speech": "<text to speak>",
  "target": "<target app/file/query/cmd/mode>"
}}

Available actions:
- "speak": Speak information to user.
- "open_app": Launch application (target e.g. "firefox").
- "type_text": Type text into active field.
- "press_key": Send keypress (target e.g. "Return", "ctrl+c").
- "run_cmd": Run shell command.
- "switch_mode": Change mode (target "browser", "email", "ide", "files", "docs", "settings", "ebook", or "desktop").
- "web_navigate": Web operations (search, URL, headings, links).
- "email_action": Mail operations (inbox, read, compose).
- "ide_action": IDE operations (open_code, read_function, read_lines, fix_code, explain_code, generate_tests, review_code, optimize_code, add_docstring, add_type_hints, refactor_code, git_status, git_commit, git_push, git_pull, git_diff, git_log, git_branch, git_switch, run_tests, run_file, run_make, run_command, stop_terminal, show_terminal, clear_terminal, ide_structure, ide_summary, cursor_position).
- "file_action": File operations (list_dir, change_dir, read_file, search_file).
- "doc_action": Document Writer (new_doc, add_heading, add_paragraph, export_all).
- "setting_action": System Settings (volume_up, volume_down, status, set_timer).
- "ebook_action": EBook Reader (open_book, list_chapters, read_chapter).
- "screen_summary": Summarize current screen (target "COMMAND:SCREEN_SUMMARY").

User speech: "{user_speech}"
Response JSON:"""

    raw_text = _query_ollama(ROUTER_MODEL, system_prompt, timeout=10, temperature=0.2, num_predict=120)

    # Try to parse JSON from model output
    if raw_text:
        json_match = re.search(r"\{.*\}", raw_text, re.DOTALL)
        if json_match:
            try:
                result = json.loads(json_match.group(0))
                # Validate the result has non-empty speech
                speech = result.get("speech", "").strip()
                if speech:
                    return result
            except (json.JSONDecodeError, KeyError):
                pass

    # ====================================================================
    # TIER 2: Keyword-based classification (fast, reliable fallback)
    # ====================================================================
    keyword_result = _keyword_classify(user_speech, mode)
    if keyword_result.get("speech", "").strip():
        return keyword_result

    # ====================================================================
    # TIER 3: Try Llama 3.1 8B (reasoning model on GTX 1080)
    # ====================================================================
    llama_prompt = f"""You are Artome OS, a voice assistant for a blind user.
Current mode: {mode}
User said: "{user_speech}"

Respond with a short, helpful spoken response (1-2 sentences). Be direct and natural."""

    llama_response = _query_ollama(REASONING_MODEL, llama_prompt, timeout=15, temperature=0.3, num_predict=80)
    if llama_response:
        return {"action": "speak", "speech": llama_response, "target": ""}

    # ====================================================================
    # TIER 4: Safe default — never return empty speech
    # ====================================================================
    return {"action": "speak", "speech": f"I heard: {user_speech[:80]}. I'm not sure how to help with that. Try saying 'help' or 'what can I say'.", "target": ""}


# Alias ask_ai for compatibility
ask_ai = ask_artome_ai

if __name__ == "__main__":
    print(ask_artome_ai("what is on screen"))
