#!/usr/bin/env python3
"""Earcons sound generator and player for Artome DE."""

import os
import wave
import struct
import math
import subprocess

SOUNDS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "sounds")
SAMPLE_RATE = 22050

def generate_wave(filename, frequencies, duration_sec=0.15, fade=True):
    """Generate a clean sine-wave chime and save to WAV."""
    filepath = os.path.join(SOUNDS_DIR, filename)
    if os.path.exists(filepath):
        return filepath

    os.makedirs(SOUNDS_DIR, exist_ok=True)
    num_samples = int(SAMPLE_RATE * duration_sec)
    frames = bytearray()

    for i in range(num_samples):
        t = i / SAMPLE_RATE
        # Envelope: smooth fade-in and fade-out
        env = 1.0
        if fade:
            attack = int(num_samples * 0.1)
            decay = int(num_samples * 0.3)
            if i < attack:
                env = i / attack
            elif i > (num_samples - decay):
                env = (num_samples - i) / decay

        val = 0.0
        for f in frequencies:
            val += math.sin(2 * math.pi * f * t)
        val = (val / len(frequencies)) * env * 0.4  # scale volume

        sample = int(val * 32767)
        frames.extend(struct.pack("<h", sample))

    with wave.open(filepath, "w") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(SAMPLE_RATE)
        wf.writeframes(frames)

    return filepath

def init_earcons():
    """Ensure all system earcons exist."""
    # Listening chime: soft rising double-tone (E5 -> B5)
    generate_wave("listening.wav", [659.25, 987.77], duration_sec=0.12)
    # Thinking chime: gentle neutral pulse (440Hz A4)
    generate_wave("thinking.wav", [440.0], duration_sec=0.10)
    # Success chime: major triad chord (C5 -> E5 -> G5)
    generate_wave("success.wav", [523.25, 659.25, 783.99], duration_sec=0.20)
    # Error chime: low double buzz (D3 + F3)
    generate_wave("error.wav", [146.83, 174.61], duration_sec=0.25)

_current_earcon_proc = None

def play_earcon(name):
    """Play an earcon by name non-blockingly."""
    global _current_earcon_proc
    filepath = os.path.join(SOUNDS_DIR, f"{name}.wav")
    if not os.path.exists(filepath):
        init_earcons()
    
    try:
        if _current_earcon_proc and _current_earcon_proc.poll() is None:
            _current_earcon_proc.terminate()
        _current_earcon_proc = subprocess.Popen(
            ["aplay", "-q", filepath],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
    except Exception as e:
        print(f"Earcon play error ({name}): {e}")

if __name__ == "__main__":
    init_earcons()
    print("Earcons initialized cleanly in:", SOUNDS_DIR)
