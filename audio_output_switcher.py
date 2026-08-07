"""Artume Audio Output Switcher — List and switch PulseAudio sinks."""

import subprocess
import re
from typing import List, Optional


class AudioSink:
    def __init__(self, index: int, name: str, description: str, active: bool = False):
        self.index = index
        self.name = name
        self.description = description
        self.active = active


class AudioOutputSwitcher:
    """Voice-driven audio output device switching."""

    def _run(self, cmd: list) -> str:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
            return result.stdout.strip()
        except Exception:
            return ""

    def list_sinks(self) -> str:
        """List available audio output devices."""
        output = self._run(["pactl", "list", "sinks", "short"])
        if not output:
            return "Audio system not available."

        sinks = []
        for line in output.split("\n"):
            parts = line.split()
            if len(parts) >= 2:
                try:
                    idx = int(parts[0])
                    name = parts[1]
                    sinks.append(AudioSink(idx, name, name))
                except ValueError:
                    continue

        # Get descriptions and active status
        long_output = self._run(["pactl", "list", "sinks"])
        current_idx = None
        for line in long_output.split("\n"):
            m = re.search(r"Sink #(\d+)", line)
            if m:
                current_idx = int(m.group(1))
            m2 = re.search(r"Description: (.+)", line)
            if m2 and current_idx is not None:
                for s in sinks:
                    if s.index == current_idx:
                        s.description = m2.group(1).strip()
            if "*" in line and "State: RUNNING" in line and current_idx is not None:
                for s in sinks:
                    if s.index == current_idx:
                        s.active = True

        if not sinks:
            return "No audio outputs found."

        speech = f"Found {len(sinks)} audio outputs. "
        for s in sinks:
            status = "active" if s.active else "available"
            speech += f"{s.description} ({status}). "
        return speech

    def switch_to(self, name: str) -> str:
        """Switch default audio output to a named sink."""
        output = self._run(["pactl", "list", "sinks", "short"])
        target_idx = None
        for line in output.split("\n"):
            parts = line.split()
            if len(parts) >= 2 and name.lower() in parts[1].lower():
                try:
                    target_idx = parts[0]
                    break
                except ValueError:
                    continue

        if not target_idx:
            # Try description match
            long_output = self._run(["pactl", "list", "sinks"])
            current_idx = None
            for line in long_output.split("\n"):
                m = re.search(r"Sink #(\d+)", line)
                if m:
                    current_idx = m.group(1)
                m2 = re.search(r"Description: (.+)", line)
                if m2 and current_idx and name.lower() in m2.group(1).lower():
                    target_idx = current_idx
                    break

        if not target_idx:
            return f"Audio output '{name}' not found."

        try:
            subprocess.run(
                ["pactl", "set-default-sink", str(target_idx)],
                timeout=3
            )
            return f"Switched audio to {name}."
        except Exception as e:
            return f"Failed to switch audio: {str(e)[:50]}"

    def current(self) -> str:
        """Get current audio output device."""
        output = self._run(["pactl", "info"])
        for line in output.split("\n"):
            if "Default Sink:" in line:
                name = line.split(":", 1)[1].strip()
                return f"Current audio output: {name}."
        return "Could not determine current audio output."


# Global singleton
_audio_switcher: Optional[AudioOutputSwitcher] = None


def get_audio_switcher() -> AudioOutputSwitcher:
    global _audio_switcher
    if _audio_switcher is None:
        _audio_switcher = AudioOutputSwitcher()
    return _audio_switcher
