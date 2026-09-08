"""Artume App Launcher — Launch, list, and quit applications.

Uses XDG desktop files and xdg-open for application management.
"""

import subprocess
from pathlib import Path
from typing import Dict, Optional


class AppInfo:
    def __init__(self, name: str, exec_cmd: str, categories: str = "", icon: str = ""):
        self.name = name
        self.exec_cmd = exec_cmd
        self.categories = categories
        self.icon = icon


class AppLauncher:
    """Voice-driven application launcher."""

    def __init__(self):
        self._apps: Dict[str, AppInfo] = {}
        self._scan_dirs = [
            Path("/usr/share/applications"),
            Path("/usr/local/share/applications"),
            Path.home() / ".local/share/applications",
        ]

    def _scan_apps(self):
        """Scan XDG desktop files for installed applications."""
        self._apps = {}
        for app_dir in self._scan_dirs:
            if not app_dir.exists():
                continue
            for desktop_file in app_dir.glob("*.desktop"):
                try:
                    content = desktop_file.read_text()
                    name = ""
                    exec_cmd = ""
                    categories = ""
                    icon = ""
                    for line in content.split("\n"):
                        if line.startswith("Name=") and not name:
                            name = line[5:].strip()
                        elif line.startswith("Exec="):
                            exec_cmd = line[5:].strip()
                            # Remove field codes like %f, %F, %u, %U
                            exec_cmd = " ".join(
                                p for p in exec_cmd.split()
                                if not p.startswith("%")
                            )
                        elif line.startswith("Categories="):
                            categories = line[11:].strip()
                        elif line.startswith("Icon="):
                            icon = line[5:].strip()
                    if name and exec_cmd:
                        self._apps[name.lower()] = AppInfo(name, exec_cmd, categories, icon)
                except Exception:
                    continue

    def list_apps(self, category: Optional[str] = None) -> str:
        """List installed applications, optionally filtered by category."""
        if not self._apps:
            self._scan_apps()

        apps = list(self._apps.values())
        if category:
            apps = [a for a in apps if category.lower() in a.categories.lower()]

        if not apps:
            return "No applications found."

        speech = f"Found {len(apps)} applications. "
        for a in apps[:15]:
            speech += f"{a.name}. "
        return speech

    def launch(self, name: str) -> str:
        """Launch an application by name."""
        if not self._apps:
            self._scan_apps()

        # Exact match
        if name.lower() in self._apps:
            app = self._apps[name.lower()]
            try:
                subprocess.Popen(
                    app.exec_cmd.split(),
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
                return f"Launched {app.name}."
            except Exception as e:
                return f"Failed to launch {app.name}: {str(e)[:50]}"

        # Fuzzy match
        matches = [a for a_name, a in self._apps.items() if name.lower() in a_name]
        if matches:
            app = matches[0]
            try:
                subprocess.Popen(
                    app.exec_cmd.split(),
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
                return f"Launched {app.name}."
            except Exception as e:
                return f"Failed to launch {app.name}: {str(e)[:50]}"

        # Try xdg-open as fallback
        try:
            subprocess.Popen(
                ["xdg-open", name],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            return f"Opened {name}."
        except Exception:
            return f"Application '{name}' not found."

    def quit_app(self, name: str) -> str:
        """Quit an application by name."""
        try:
            subprocess.run(["pkill", "-f", name], timeout=3)
            return f"Quit {name}."
        except Exception:
            return f"Failed to quit {name}."

    def search_apps(self, query: str) -> str:
        """Search for applications matching a query."""
        if not self._apps:
            self._scan_apps()

        matches = [a for a_name, a in self._apps.items() if query.lower() in a_name]
        if not matches:
            return f"No applications matching '{query}'."

        speech = f"Found {len(matches)} applications matching {query}. "
        for a in matches[:5]:
            speech += f"{a.name}. "
        return speech


# Global singleton
_app_launcher: Optional[AppLauncher] = None


def get_app_launcher() -> AppLauncher:
    global _app_launcher
    if _app_launcher is None:
        _app_launcher = AppLauncher()
    return _app_launcher
