"""Pre-recorded response clip registry for Artume OS.

Maps (action, target_prefix) tuples to pre-recorded WAV clips that play
immediately while the LLM generates the actual response. Reduces perceived
latency from ~2.6s to ~200ms for acknowledge-then-act commands.

Usage:
    from clip_registry import lookup, strip_clip_duplicate, get_clip_text

    clip = lookup("setting_action", "volume_up")
    if clip:
        play_earcon(clip["file"])  # ~100ms vs ~1.5s TTS synthesis
        # ... later, strip duplicate from response:
        response = strip_clip_duplicate(clip, response)
"""

import os

CLIPS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "clips")

# Registry: (action, target_prefix) → clip metadata
# - file: WAV filename (relative to clips/<subdir>/)
# - text: what the clip says (for repetition avoidance)
# - subdir: subdirectory under clips/
_REGISTRY = {
    # --- Volume ---
    ("setting_action", "volume_up"): {
        "file": "hardware/volume_up.wav",
        "text": "Volume up.",
    },
    ("setting_action", "volume_down"): {
        "file": "hardware/volume_down.wav",
        "text": "Volume down.",
    },
    ("setting_action", "set_volume"): {
        "file": "hardware/volume_up.wav",  # generic "volume up" clip
        "text": "Setting volume.",
    },
    ("setting_action", "mute"): {
        "file": "hardware/muted.wav",
        "text": "Muted.",
    },

    # --- Status / System ---
    ("setting_action", "status"): {
        "file": "system/checking_status.wav",
        "text": "Checking your system status.",
    },
    ("setting_action", "timer"): {
        "file": "hardware/timer_set.wav",
        "text": "Timer set.",
    },

    # --- Mode switching ---
    ("switch_mode", ""): {
        "file": "system/switching_mode.wav",
        "text": "Switching modes.",
    },

    # --- File search ---
    ("file_search", ""): {
        "file": "system/searching_files.wav",
        "text": "Searching your files now.",
    },

    # --- Web ---
    ("web_navigate", "url:"): {
        "file": "system/loading_page.wav",
        "text": "Loading that page for you.",
    },
    ("web_navigate", "search:"): {
        "file": "system/searching_files.wav",
        "text": "Searching for that.",
    },
    ("web_navigate", ""): {
        "file": "system/loading_page.wav",
        "text": "Loading that page for you.",
    },

    # --- App launch ---
    ("open_app", ""): {
        "file": "confirm/opening_app.wav",
        "text": "Opening that for you.",
    },

    # --- Email ---
    ("email_action", "inbox"): {
        "file": "system/checking_email.wav",
        "text": "Checking your email.",
    },
    ("email_action", ""): {
        "file": "system/checking_email.wav",
        "text": "Checking your email.",
    },

    # --- Entity lookup ---
    ("entity_lookup", ""): {
        "file": "system/on_it.wav",
        "text": "Looking that up.",
    },

    # --- IDE AI (longest running — highest impact) ---
    ("ide_action", "fix"): {
        "file": "system/looking_into_it.wav",
        "text": "Looking into that.",
    },
    ("ide_action", "explain"): {
        "file": "system/looking_into_it.wav",
        "text": "Looking into that.",
    },
    ("ide_action", "test"): {
        "file": "system/looking_into_it.wav",
        "text": "Looking into that.",
    },
    ("ide_action", "review"): {
        "file": "system/looking_into_it.wav",
        "text": "Looking into that.",
    },
    ("ide_action", "optimize"): {
        "file": "system/looking_into_it.wav",
        "text": "Looking into that.",
    },
    ("ide_action", "refactor"): {
        "file": "system/looking_into_it.wav",
        "text": "Looking into that.",
    },
    ("ide_action", "docstring"): {
        "file": "system/looking_into_it.wav",
        "text": "Looking into that.",
    },
    ("ide_action", "type hints"): {
        "file": "system/looking_into_it.wav",
        "text": "Looking into that.",
    },

    # --- IDE Git ---
    ("ide_action", "git"): {
        "file": "system/working_on_it.wav",
        "text": "Working on that.",
    },

    # --- IDE Terminal ---
    ("ide_action", "run"): {
        "file": "system/running_that_now.wav",
        "text": "Running that now.",
    },
    ("ide_action", "make"): {
        "file": "system/running_that_now.wav",
        "text": "Running that now.",
    },
    ("ide_action", "build"): {
        "file": "system/running_that_now.wav",
        "text": "Running that now.",
    },

    # --- Screen summary ---
    ("screen_summary", ""): {
        "file": "system/reading_screen.wav",
        "text": "Reading your screen.",
    },

    # --- Bluetooth ---
    ("setting_action", "bluetooth:"): {
        "file": "hardware/scanning.wav",
        "text": "Working on Bluetooth.",
    },

    # --- WiFi ---
    ("setting_action", "wifi:"): {
        "file": "system/connecting_wifi.wav",
        "text": "Connecting to WiFi.",
    },

    # --- Audio output ---
    ("setting_action", "audio:"): {
        "file": "system/on_it.wav",
        "text": "Switching audio output.",
    },

    # --- Conversation fallback ---
    ("speak", "help"): {
        "file": "system/on_it.wav",
        "text": "On it.",
    },
}


def lookup(action, target=""):
    """Look up a pre-recorded clip for the given action and target.

    Returns dict with 'file', 'text' keys, or None if no clip matches.
    Matching is prefix-based: ("ide_action", "fix") matches targets
    starting with "fix", "fix this", "fix the", etc.
    """
    target_lower = target.lower().strip()

    # Exact match first
    key = (action, target_lower)
    if key in _REGISTRY:
        return _REGISTRY[key]

    # Prefix match — check if target starts with any registered prefix
    for (act, prefix), clip in _REGISTRY.items():
        if act != action:
            continue
        if prefix and target_lower.startswith(prefix):
            return clip
        if not prefix:
            # Empty prefix matches any target for this action
            return clip

    return None


def strip_clip_duplicate(clip, response):
    """Remove redundant acknowledgment prefix from response if it duplicates the clip.

    If the clip said "Looking into that." and the response starts with
    "Looking into that. Here's what I found...", strip the duplicate.
    """
    if not clip or not response:
        return response

    clip_text = clip.get("text", "").rstrip(". ")
    if not clip_text:
        return response

    resp_lower = response.lower().lstrip()
    clip_lower = clip_text.lower()

    # Direct prefix match
    if resp_lower.startswith(clip_lower):
        stripped = response[len(clip_text):].lstrip(" .,")
        if stripped:
            return stripped

    # Fuzzy: first 3+ words match
    clip_words = clip_lower.split()[:3]
    resp_words = resp_lower.split()[:len(clip_words)]
    if len(clip_words) >= 2 and clip_words == resp_words:
        stripped = response[len(" ".join(clip_words)):].lstrip(" .,")
        if stripped:
            return stripped

    return response


def get_clip_text(clip):
    """Get the text a clip says, for LLM prompt injection."""
    if clip:
        return clip.get("text", "")
    return ""


def list_clips():
    """List all registered clips with their files and text."""
    result = []
    for (action, prefix), clip in sorted(_REGISTRY.items()):
        filepath = os.path.join(CLIPS_DIR, clip["file"])
        exists = os.path.exists(filepath)
        result.append({
            "action": action,
            "prefix": prefix,
            "file": clip["file"],
            "text": clip["text"],
            "exists": exists,
        })
    return result


if __name__ == "__main__":
    clips = list_clips()
    print(f"Registered clips: {len(clips)}")
    for c in clips:
        status = "OK" if c["exists"] else "MISSING"
        print(f"  [{status}] ({c['action']}, {c['prefix']!r}) → {c['file']}: {c['text']}")
