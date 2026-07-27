#!/usr/bin/env python3
"""Artome Audio Wake-Word Engine - Lightweight voice activation detector."""

import re

class WakeWordDetector:
    """Lightweight wake-word & activation phrase detector for Artome OS."""

    def __init__(self, wake_words=None):
        self.wake_words = wake_words or ["artome", "hey artome", "r2", "hey r2", "assistant", "computer"]
        self.wake_word_enabled = True

    def toggle_wake_word(self, enable=None):
        """Toggle between Wake-Word Mode and Always-Listening Mode."""
        if enable is not None:
            self.wake_word_enabled = enable
        else:
            self.wake_word_enabled = not self.wake_word_enabled

        status = "enabled" if self.wake_word_enabled else "disabled (always listening)"
        return f"Wake-word mode is now {status}."

    def check_wake_word(self, transcribed_text):
        """Check if transcribed speech contains an activation wake word.

        Returns (is_activated, clean_command_after_wakeword)
        """
        if not self.wake_word_enabled:
            return True, transcribed_text.strip()

        text_lower = transcribed_text.lower().strip()

        for ww in self.wake_words:
            if ww in text_lower:
                # Extract command text appearing after wake word
                parts = re.split(re.escape(ww), text_lower, maxsplit=1, flags=re.IGNORECASE)
                command_after = parts[1].strip() if len(parts) > 1 else ""
                return True, command_after

        return False, ""

if __name__ == "__main__":
    detector = WakeWordDetector()
    print("Testing Wake-Word Detection...")
    activated, cmd = detector.check_wake_word("Hey Artome open firefox")
    print(f"Result: Activated={activated}, Command='{cmd}'")
