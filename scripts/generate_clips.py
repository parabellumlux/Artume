#!/usr/bin/env python3
"""Generate pre-recorded response clips for Artume OS.

Usage: python3 scripts/generate_clips.py

Generates WAV files from text phrases using PiperTTS, saved to clips/.
Run once during setup. Generated WAVs are committed to the repo.
"""

import os
import subprocess
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(SCRIPT_DIR)
PIPER_BIN = os.path.join(PROJECT_ROOT, "piper", "piper")
MODEL_FILE = os.path.join(PROJECT_ROOT, "en_US-lessac-medium.onnx")

# (filename, phrase, subdirectory)
CLIPS = [
    # System clips
    ("on_it.wav", "On it.", "system"),
    ("working_on_it.wav", "Working on that.", "system"),
    ("checking_status.wav", "Checking your system status.", "system"),
    ("searching_files.wav", "Searching your files now.", "system"),
    ("loading_page.wav", "Loading that page for you.", "system"),
    ("switching_mode.wav", "Switching modes.", "system"),
    ("looking_into_it.wav", "Looking into that.", "system"),
    ("running_that_now.wav", "Running that now.", "system"),
    ("checking_email.wav", "Checking your email.", "system"),
    ("connecting_wifi.wav", "Connecting to WiFi.", "system"),
    ("reading_screen.wav", "Reading your screen.", "system"),

    # Hardware clips
    ("volume_up.wav", "Volume up.", "hardware"),
    ("volume_down.wav", "Volume down.", "hardware"),
    ("muted.wav", "Muted.", "hardware"),
    ("unmuted.wav", "Unmuted.", "hardware"),
    ("bluetooth_on.wav", "Bluetooth on.", "hardware"),
    ("bluetooth_off.wav", "Bluetooth off.", "hardware"),
    ("scanning.wav", "Scanning for devices.", "hardware"),
    ("timer_set.wav", "Timer set.", "hardware"),

    # Confirmation clips
    ("confirming.wav", "Confirmed.", "confirm"),
    ("cancelled.wav", "Cancelled.", "confirm"),
    ("opening_app.wav", "Opening that for you.", "confirm"),
    ("not_sure.wav", "I'm not sure what you meant.", "confirm"),
]


def generate_clip(filename, phrase, subdir):
    """Generate a single WAV clip using PiperTTS."""
    out_dir = os.path.join(PROJECT_ROOT, "clips", subdir)
    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, filename)

    if os.path.exists(out_path):
        print(f"  [skip] {filename} (exists)")
        return True

    try:
        proc = subprocess.run(
            [PIPER_BIN, "-m", MODEL_FILE, "--output_file", out_path],
            input=phrase.encode("utf-8"),
            capture_output=True,
            timeout=30,
        )
        if proc.returncode != 0:
            print(f"  [FAIL] {filename}: {proc.stderr.decode()[:100]}")
            return False
        size = os.path.getsize(out_path) if os.path.exists(out_path) else 0
        print(f"  [OK]   {filename} ({size} bytes)")
        return True
    except subprocess.TimeoutExpired:
        print(f"  [FAIL] {filename}: timeout")
        return False
    except FileNotFoundError:
        print(f"  [FAIL] Piper binary not found at {PIPER_BIN}")
        return False


def main():
    print(f"Piper binary: {PIPER_BIN}")
    print(f"Model: {MODEL_FILE}")
    print(f"Generating {len(CLIPS)} clips...\n")

    if not os.path.exists(PIPER_BIN):
        print(f"ERROR: Piper binary not found at {PIPER_BIN}")
        print("Make sure the piper/ directory exists with the binary and model.")
        sys.exit(1)

    ok = 0
    fail = 0
    for filename, phrase, subdir in CLIPS:
        if generate_clip(filename, phrase, subdir):
            ok += 1
        else:
            fail += 1

    print(f"\nDone: {ok} generated, {fail} failed.")
    return 0 if fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
