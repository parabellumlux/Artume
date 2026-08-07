"""Artume Clipboard Manager — Copy, paste, select all, clipboard history.

Uses xclip for clipboard operations.
"""

import subprocess
from typing import List, Optional


class ClipboardManager:
    """Voice-driven clipboard operations."""

    def __init__(self):
        self._history: List[str] = []
        self._max_history = 20

    def _run(self, cmd: list) -> str:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=3)
            return result.stdout.strip()
        except Exception:
            return ""

    def copy(self, text: str) -> str:
        """Copy text to clipboard."""
        try:
            proc = subprocess.Popen(
                ["xclip", "-selection", "clipboard"],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            proc.communicate(input=text.encode(), timeout=3)
            self._history.append(text)
            if len(self._history) > self._max_history:
                self._history.pop(0)
            return "Copied to clipboard."
        except Exception:
            return "Failed to copy to clipboard."

    def paste(self) -> str:
        """Get clipboard contents."""
        content = self._run(["xclip", "-selection", "clipboard", "-o"])
        if content:
            preview = content[:200].replace("\n", " ")
            return f"Clipboard contains: {preview}"
        return "Clipboard is empty."

    def select_all(self) -> str:
        """Select all text in the current window (Ctrl+A)."""
        try:
            subprocess.run(
                ["xdotool", "key", "ctrl+a"],
                timeout=2,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            return "Selected all."
        except Exception:
            return "Failed to select all."

    def copy_selection(self) -> str:
        """Copy current selection (Ctrl+C)."""
        try:
            subprocess.run(
                ["xdotool", "key", "ctrl+c"],
                timeout=2,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            return self.paste()
        except Exception:
            return "Failed to copy selection."

    def paste_clipboard(self) -> str:
        """Paste clipboard contents (Ctrl+V)."""
        try:
            subprocess.run(
                ["xdotool", "key", "ctrl+v"],
                timeout=2,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            return "Pasted."
        except Exception:
            return "Failed to paste."

    def history(self) -> str:
        """Get clipboard history."""
        if not self._history:
            return "Clipboard history is empty."
        speech = f"Clipboard history: {len(self._history)} items. "
        for i, item in enumerate(self._history[-5:], 1):
            preview = item[:50].replace("\n", " ")
            speech += f"Item {i}: {preview}. "
        return speech

    def clear_history(self) -> str:
        """Clear clipboard history."""
        self._history.clear()
        return "Clipboard history cleared."


# Global singleton
_clipboard: Optional[ClipboardManager] = None


def get_clipboard() -> ClipboardManager:
    global _clipboard
    if _clipboard is None:
        _clipboard = ClipboardManager()
    return _clipboard
