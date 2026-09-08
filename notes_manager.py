"""Artume Notes / Scratchpad — Quick voice notes, read back later."""

import json
import os
import time
from typing import List, Optional


class NotesManager:
    """Voice-driven notes and scratchpad."""

    def __init__(self):
        self._notes_path = os.path.expanduser("~/.config/artume/notes.json")
        self._notes: List[dict] = []
        self._load()

    def _load(self):
        if os.path.exists(self._notes_path):
            try:
                with open(self._notes_path, "r") as f:
                    self._notes = json.load(f)
            except Exception:
                self._notes = []

    def _save(self):
        os.makedirs(os.path.dirname(self._notes_path), exist_ok=True)
        with open(self._notes_path, "w") as f:
            json.dump(self._notes, f, indent=2)

    def add(self, text: str) -> str:
        """Add a quick note."""
        note = {
            "id": len(self._notes) + 1,
            "text": text,
            "timestamp": time.time(),
            "created": time.strftime("%Y-%m-%d %H:%M"),
        }
        self._notes.append(note)
        self._save()
        preview = text[:60].replace("\n", " ")
        return f"Note added: {preview}."

    def list_notes(self, limit: int = 10) -> str:
        """List recent notes."""
        if not self._notes:
            return "No notes saved."

        recent = self._notes[-limit:]
        speech = f"You have {len(self._notes)} notes. "
        for i, note in enumerate(reversed(recent), 1):
            preview = note["text"][:80].replace("\n", " ")
            speech += f"Note {i}: {preview}. "
        return speech

    def read_note(self, index: int) -> str:
        """Read a specific note by index."""
        if not self._notes:
            return "No notes saved."

        if 1 <= index <= len(self._notes):
            note = self._notes[-index]  # Reverse order (newest first)
            return f"Note {index}: {note['text']}. Created {note['created']}."
        return f"Note {index} not found. You have {len(self._notes)} notes."

    def search(self, query: str) -> str:
        """Search notes by text."""
        if not self._notes:
            return "No notes saved."

        matches = [n for n in self._notes if query.lower() in n["text"].lower()]
        if not matches:
            return f"No notes matching '{query}'."

        speech = f"Found {len(matches)} matching notes. "
        for n in matches[-5:]:
            preview = n["text"][:60].replace("\n", " ")
            speech += f"Note {n['id']}: {preview}. "
        return speech

    def delete(self, index: int) -> str:
        """Delete a note by index."""
        if not self._notes:
            return "No notes saved."

        if 1 <= index <= len(self._notes):
            note = self._notes.pop(-index)
            self._save()
            return f"Deleted note: {note['text'][:40]}."
        return f"Note {index} not found."

    def clear(self) -> str:
        """Delete all notes."""
        count = len(self._notes)
        self._notes = []
        self._save()
        return f"Deleted all {count} notes."


# Global singleton
_notes: Optional[NotesManager] = None


def get_notes() -> NotesManager:
    global _notes
    if _notes is None:
        _notes = NotesManager()
    return _notes
