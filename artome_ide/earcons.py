"""Artome Audio-First IDE — Enhanced earcon system.

20+ code-specific audio icons for code structure awareness.
Generates WAV files using sine wave synthesis.
"""

import math
import os
import struct
import wave
from pathlib import Path

SAMPLE_RATE = 24000
SOUNDS_DIR = Path(__file__).parent / "sounds"


def _generate_wav(filename: str, frequencies: list[float], duration: float, envelope: str = "fade") -> Path:
    """Generate a WAV file with given frequencies and envelope."""
    path = SOUNDS_DIR / filename
    if path.exists():
        return path

    SOUNDS_DIR.mkdir(parents=True, exist_ok=True)
    num_samples = int(SAMPLE_RATE * duration)
    frames = bytearray()

    for i in range(num_samples):
        t = i / SAMPLE_RATE
        # Envelope
        env = 1.0
        attack = int(num_samples * 0.05)
        decay = int(num_samples * 0.2)
        if envelope == "fade":
            if i < attack:
                env = i / attack
            elif i > (num_samples - decay):
                env = (num_samples - i) / decay
        elif envelope == "pulse":
            env = abs(math.sin(math.pi * i / (num_samples * 0.1)))
        elif envelope == "growl":
            env = 1.0 - (i / num_samples) * 0.7  # fade out

        val = 0.0
        for f in frequencies:
            val += math.sin(2 * math.pi * f * t)
        val = (val / len(frequencies)) * env * 0.3
        sample = int(val * 32767)
        frames.extend(struct.pack("<h", sample))

    with wave.open(str(path), "w") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(SAMPLE_RATE)
        wf.writeframes(frames)

    return path


def init_earcons():
    """Generate all 20+ code-specific earcons."""
    earcons = {}

    # --- Scope boundaries ---
    # Enter function: rising arpeggio (C-E-G) 150ms
    earcons["scope_enter_function"] = _generate_wav(
        "scope_enter_function.wav", [261.63, 329.63, 392.00], 0.15
    )
    # Exit function: falling arpeggio (G-E-C) 150ms
    earcons["scope_exit_function"] = _generate_wav(
        "scope_exit_function.wav", [392.00, 329.63, 261.63], 0.15
    )
    # Enter class: rising major chord held 300ms
    earcons["scope_enter_class"] = _generate_wav(
        "scope_enter_class.wav", [261.63, 329.63, 392.00, 523.25], 0.30
    )
    # Exit class: falling major chord held 300ms
    earcons["scope_exit_class"] = _generate_wav(
        "scope_exit_class.wav", [523.25, 392.00, 329.63, 261.63], 0.30
    )
    # Enter loop: soft pulse (440 Hz, 80ms, repeated)
    earcons["loop_iteration"] = _generate_wav(
        "loop_iteration.wav", [440.0], 0.08, envelope="pulse"
    )
    # Enter conditional: short double-beep (660+440 Hz)
    earcons["conditional"] = _generate_wav(
        "conditional.wav", [660.0, 440.0], 0.10
    )
    # Enter try: rising minor third
    earcons["scope_enter_try"] = _generate_wav(
        "scope_enter_try.wav", [261.63, 311.13], 0.15
    )
    # Enter catch: falling minor third
    earcons["scope_enter_catch"] = _generate_wav(
        "scope_enter_catch.wav", [311.13, 261.63], 0.15
    )

    # --- Code elements ---
    # Comment block: soft chime (880 Hz, 50ms)
    earcons["comment"] = _generate_wav(
        "comment.wav", [880.0], 0.05
    )
    # Blank line: 100ms silence (represented as very quiet tone)
    earcons["blank_line"] = _generate_wav(
        "blank_line.wav", [20.0], 0.05  # sub-bass, barely audible
    )
    # Long line warning: buzz (150 Hz, 100ms)
    earcons["long_line"] = _generate_wav(
        "long_line.wav", [150.0, 151.0], 0.10  # slight detune for buzz
    )

    # --- Diagnostics ---
    # Error: low growl (100 Hz, 500ms)
    earcons["error"] = _generate_wav(
        "error.wav", [100.0, 105.0], 0.50, envelope="growl"
    )
    # Warning: mid tone (440 Hz, 200ms)
    earcons["warning"] = _generate_wav(
        "warning.wav", [440.0], 0.20
    )
    # Info: soft click (2ms)
    earcons["info"] = _generate_wav(
        "info.wav", [1000.0], 0.002
    )

    # --- Debugging ---
    # Breakpoint hit: rising chime
    earcons["breakpoint_hit"] = _generate_wav(
        "breakpoint_hit.wav", [523.25, 659.25, 783.99], 0.20
    )
    # Step complete: soft click
    earcons["step_complete"] = _generate_wav(
        "step_complete.wav", [1000.0], 0.005
    )
    # Exception: low growl (100 Hz, 500ms)
    earcons["exception"] = _generate_wav(
        "exception.wav", [80.0, 85.0], 0.50, envelope="growl"
    )
    # Variable changed: quick ascending tone
    earcons["variable_changed"] = _generate_wav(
        "variable_changed.wav", [440.0, 880.0], 0.10
    )
    # Watch triggered: repeating pulse
    earcons["watch_triggered"] = _generate_wav(
        "watch_triggered.wav", [660.0], 0.30, envelope="pulse"
    )

    # --- Git ---
    # File added: bright ascending
    earcons["git_added"] = _generate_wav(
        "git_added.wav", [523.25, 659.25], 0.12
    )
    # File modified: mid tone
    earcons["git_modified"] = _generate_wav(
        "git_modified.wav", [392.00], 0.12
    )
    # File deleted: descending
    earcons["git_deleted"] = _generate_wav(
        "git_deleted.wav", [392.00, 261.63], 0.15
    )
    # File untracked: question tone
    earcons["git_untracked"] = _generate_wav(
        "git_untracked.wav", [440.0, 550.0], 0.15
    )

    # --- Navigation ---
    # Found match: triple ascending
    earcons["found_match"] = _generate_wav(
        "found_match.wav", [392.00, 523.25, 659.25], 0.20
    )
    # No match: descending
    earcons["no_match"] = _generate_wav(
        "no_match.wav", [659.25, 523.25, 392.00], 0.20
    )
    # Boundary reached: thud
    earcons["boundary"] = _generate_wav(
        "boundary.wav", [80.0], 0.10
    )

    return earcons


def play_earcon(name: str) -> None:
    """Play an earcon by name using aplay."""
    import subprocess
    path = SOUNDS_DIR / f"{name}.wav"
    if not path.exists():
        init_earcons()
    try:
        subprocess.Popen(
            ["aplay", "-q", str(path)],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
    except Exception:
        pass  # silently fail if aplay not available


# Pre-generate on import
init_earcons()
