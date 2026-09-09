#!/usr/bin/env python3
"""Voice-driven confirmation dialogs for dangerous actions in Artume OS."""

import time
import numpy as np
import sounddevice as sd
from typing import Optional
from earcons import play_earcon

_singleton = None

def get_dialog(tts_engine=None, whisper_model=None):
    global _singleton
    if _singleton is None:
        _singleton = ConfirmationDialog(tts_engine, whisper_model)
    return _singleton

class ConfirmationDialog:
    """Voice-driven yes/no confirmation for dangerous actions."""

    SAMPLE_RATE = 16000
    CONFIRM_WORDS = {"yes", "yeah", "yep", "confirm", "do it", "go", "ok", "okay", "affirmative"}
    DENY_WORDS = {"no", "nope", "nah", "cancel", "abort", "stop", "negative", "nevermind", "never"}

    def __init__(self, tts_engine=None, whisper_model=None):
        self.tts = tts_engine
        self.whisper = whisper_model

    def _speak(self, text):
        if self.tts:
            self.tts.speak(text)
            while self.tts.is_speaking:
                time.sleep(0.05)

    def _listen(self, timeout=8):
        buffer = []
        silent_chunks = 0
        chunk_size = int(self.SAMPLE_RATE * 0.25)
        max_silent = int(timeout / 0.25)
        start = time.time()

        with sd.InputStream(samplerate=self.SAMPLE_RATE, channels=1, dtype="float32") as stream:
            while time.time() - start < timeout:
                chunk, _ = stream.read(chunk_size)
                audio = chunk.flatten()
                rms = np.sqrt(np.mean(audio ** 2))
                if rms >= 0.015:
                    buffer.append(audio)
                    silent_chunks = 0
                elif buffer:
                    buffer.append(audio)
                    silent_chunks += 1
                    if silent_chunks >= max_silent:
                        break

        if not buffer:
            return None
        audio_data = np.concatenate(buffer).astype(np.float32)
        segments, _ = self.whisper.transcribe(audio_data, language="en", beam_size=1)
        return " ".join(s.text.strip().lower() for s in segments)

    def _parse_response(self, text):
        if not text:
            return None
        words = set(text.split())
        if words & self.CONFIRM_WORDS:
            return True
        if words & self.DENY_WORDS:
            return False
        return None

    def ask(self, question: str, timeout: int = 8) -> bool:
        """Speak question, listen for yes/no, return True for yes, False for no/timeout."""
        self._speak(question)
        time.sleep(0.15)
        play_earcon("listening")
        text = self._listen(timeout)
        result = self._parse_response(text)
        if result is True:
            play_earcon("success")
            return True
        play_earcon("error")
        return False

    def confirm_action(self, action_name: str, details: str = "") -> bool:
        """Convenience: 'Are you sure you want to {action_name}?'"""
        q = f"Are you sure you want to {action_name}?"
        if details:
            q += f" {details}"
        return self.ask(q)

    def confirm_destructive(self, action_name: str, target: str = "") -> bool:
        """Destructive action: warning earcon + stern confirmation."""
        play_earcon("warning")
        time.sleep(0.4)
        q = f"Warning. This will {action_name}."
        if target:
            q += f" Target: {target}."
        q += " Are you sure?"
        return self.ask(q)

    def capture_phrase(self, prompt: str, timeout: int = 8) -> Optional[str]:
        """Speak a prompt, listen for one spoken phrase, and return it (or None)."""
        self._speak(prompt)
        time.sleep(0.15)
        play_earcon("listening")
        return self._listen(timeout)
