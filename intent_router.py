#!/usr/bin/env python3
"""AI Intent Router for Artome DE — uses GPU-backed Ollama models.

Tier 1: Llama 3.1 8B on GTX 1080 (GPU 0) — main reasoning/conversation
Tier 2: Nemotron-3 Nano on GTX 1650S (GPU 1) — router/tool-caller/guardrails
Tier 3: nomic-embed-text on CPU — embeddings
"""

import json
import re
import requests
import subprocess
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


def ask_artome_ai(user_speech, mode="DESKTOP"):
    """Query AI with context and request JSON intent payload.

    Modes: DESKTOP, BROWSER, EMAIL, IDE, FILES, DOCS, SETTINGS, EBOOK, SCREEN_SUMMARY
    Uses Nemotron-3 Nano on GTX 1650S for routing/classification.
    """
    window_title = get_active_window()
    low_speech = user_speech.lower().strip()

    # COMMAND:SCREEN_SUMMARY handler
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
            return {"action": "screen_summary", "speech": f"Screen summary: {ai_summary}", "target": "COMMAND:SCREEN_SUMMARY"}
        except Exception as e:
            return {"action": "screen_summary", "speech": f"Active window is {window_title}. AT-SPI elements: {accessibility_tree[:150]}", "target": "COMMAND:SCREEN_SUMMARY"}

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
- "ide_action": IDE operations (open_code, read_function, read_lines).
- "file_action": File operations (list_dir, change_dir, read_file, search_file).
- "doc_action": Document Writer (new_doc, add_heading, add_paragraph, export_all).
- "setting_action": System Settings (volume_up, volume_down, status, set_timer).
- "ebook_action": EBook Reader (open_book, list_chapters, read_chapter).
- "screen_summary": Summarize current screen (target "COMMAND:SCREEN_SUMMARY").

User speech: "{user_speech}"
Response JSON:"""

    try:
        res = requests.post(
            OLLAMA_URL,
            json={
                "model": ROUTER_MODEL,
                "prompt": system_prompt,
                "stream": False,
                "options": {"temperature": 0.2, "num_predict": 120},
            },
            timeout=12,
        )
        raw_text = res.json()["response"].strip()

        # Attempt JSON extraction
        json_match = re.search(r"\{.*\}", raw_text, re.DOTALL)
        if json_match:
            return json.loads(json_match.group(0))

        # Fallback keyword matching
        if "screen" in low_speech:
            return {"action": "screen_summary", "speech": f"Active window: {window_title}", "target": "COMMAND:SCREEN_SUMMARY"}
        if "open" in low_speech and "file" not in low_speech and "folder" not in low_speech and "book" not in low_speech:
            app = user_speech.split("open")[-1].strip()
            return {"action": "open_app", "speech": f"Opening {app}", "target": app}

        return {"action": "speak", "speech": raw_text.replace("COMMAND:TELL", "").strip(), "target": ""}

    except Exception as e:
        return {"action": "speak", "speech": f"Connection error: {str(e)[:40]}", "target": ""}


# Alias ask_ai for compatibility
ask_ai = ask_artome_ai

if __name__ == "__main__":
    print(ask_artome_ai("what is on screen"))
