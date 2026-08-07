"""Artume Window Manager — EWMH/NetWM window control.

List, focus, minimize, maximize, close windows via xdotool and wmctrl.
"""

import subprocess
import re
from typing import List, Optional


class WindowInfo:
    def __init__(self, wid: str, title: str, desktop: str = ""):
        self.wid = wid
        self.title = title
        self.desktop = desktop

    def __repr__(self):
        return f"{self.title} (window {self.wid})"


class WindowManager:
    """Voice-driven window management."""

    def __init__(self):
        self._cache: List[WindowInfo] = []

    def _run(self, cmd: List[str]) -> str:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=3)
            return result.stdout.strip()
        except Exception:
            return ""

    def list_windows(self) -> str:
        """List all visible windows."""
        output = self._run(["wmctrl", "-l"])
        if not output:
            return "Window manager not available."

        self._cache = []
        for line in output.split("\n"):
            parts = line.split(None, 3)
            if len(parts) >= 4:
                wid, desktop, _, title = parts
                self._cache.append(WindowInfo(wid, title, desktop))

        if not self._cache:
            return "No windows found."

        speech = f"You have {len(self._cache)} open windows. "
        for i, w in enumerate(self._cache[:10], 1):
            speech += f"Window {i}: {w.title}. "
        return speech

    def focus_window(self, query: str) -> str:
        """Focus a window by title or number."""
        # Try number first
        if query.isdigit():
            idx = int(query) - 1
            if 0 <= idx < len(self._cache):
                wid = self._cache[idx].wid
                self._run(["wmctrl", "-i", "-a", wid])
                return f"Focused window: {self._cache[idx].title}."
            return f"Invalid window number {query}."

        # Try title match
        for w in self._cache:
            if query.lower() in w.title.lower():
                self._run(["wmctrl", "-i", "-a", w.wid])
                return f"Focused window: {w.title}."

        return f"No window found matching '{query}'."

    def minimize_window(self, query: Optional[str] = None) -> str:
        """Minimize current or specified window."""
        if query:
            return self.focus_window(query)
        self._run(["xdotool", "getactivewindow", "windowminimize"])
        return "Window minimized."

    def maximize_window(self) -> str:
        """Toggle maximize current window."""
        self._run(["wmctrl", "-r", ":ACTIVE:", "-b", "toggle,maximized_vert,maximized_horz"])
        return "Window maximized."

    def close_window(self, query: Optional[str] = None) -> str:
        """Close current or specified window."""
        if query:
            for w in self._cache:
                if query.lower() in w.title.lower():
                    self._run(["wmctrl", "-i", "-c", w.wid])
                    return f"Closed window: {w.title}."
            return f"No window found matching '{query}'."
        self._run(["xdotool", "getactivewindow", "windowkill"])
        return "Window closed."

    def get_active_window(self) -> str:
        """Get the title of the active window."""
        title = self._run(["xdotool", "getactivewindow", "getwindowname"])
        if title:
            return f"Active window: {title}."
        return "Could not determine active window."

    def switch_desktop(self, desktop_num: int) -> str:
        """Switch to a specific desktop/workspace."""
        self._run(["wmctrl", "-s", str(desktop_num - 1)])
        return f"Switched to desktop {desktop_num}."

    def list_desktops(self) -> str:
        """List all virtual desktops."""
        output = self._run(["wmctrl", "-d"])
        if not output:
            return "Desktop information not available."
        lines = output.split("\n")
        current = None
        for line in lines:
            if "*" in line:
                parts = line.split()
                current = parts[0] if parts else "?"
        return f"You are on desktop {current}. Total desktops: {len(lines)}."


# Global singleton
_window_manager: Optional[WindowManager] = None


def get_window_manager() -> WindowManager:
    global _window_manager
    if _window_manager is None:
        _window_manager = WindowManager()
    return _window_manager
