"""Artome Audio-First IDE — Python voice interface.

Communicates with the aether-ide-daemon (Rust) over a Unix socket
via JSON-RPC. Provides voice-driven code reading, navigation, and editing.
"""

import json
import logging
import os
import socket
import subprocess
import time
from pathlib import Path
from typing import Optional

log = logging.getLogger(__name__)

SOCKET_PATH = os.environ.get("AETHER_IDE_SOCKET", "/tmp/aether-ide.sock")


class IdeClient:
    """JSON-RPC client for the aether-ide-daemon."""

    def __init__(self, socket_path: str = SOCKET_PATH):
        self.socket_path = socket_path
        self._id = 0

    def _call(self, method: str, params: dict = None) -> dict:
        """Send a JSON-RPC request and return the result."""
        self._id += 1
        request = {
            "id": self._id,
            "method": method,
            "params": params or {},
        }

        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.settimeout(10)
            sock.connect(self.socket_path)
            payload = json.dumps(request) + "\n"
            sock.sendall(payload.encode())

            response = b""
            while True:
                chunk = sock.recv(4096)
                if not chunk:
                    break
                response += chunk
                if b"\n" in response:
                    break
            sock.close()

            data = json.loads(response.decode().strip())
            if data.get("error"):
                raise RuntimeError(data["error"]["message"])
            return data.get("result", {})
        except (socket.error, json.JSONDecodeError) as e:
            raise RuntimeError(f"IDE daemon connection failed: {e}")

    def open_file(self, path: str) -> dict:
        """Open a file for reading and navigation."""
        return self._call("open_file", {"path": str(Path(path).resolve())})

    def get_structure(self) -> dict:
        """Get the code structure (symbols, boundaries)."""
        return self._call("get_structure")

    def get_sonification(self) -> dict:
        """Get audio parameters for every line."""
        return self._call("get_sonification")

    def get_summary(self) -> dict:
        """Get a text summary of the code structure."""
        return self._call("get_summary")

    def get_tree(self) -> dict:
        """Get a tree-shaped structure description."""
        return self._call("get_tree")

    def set_cursor(self, line: int, column: int = 0) -> dict:
        """Set the cursor position (1-indexed line)."""
        return self._call("set_cursor", {"line": line, "column": column})

    def get_cursor(self) -> dict:
        """Get the current cursor position."""
        return self._call("get_cursor")

    def shutdown(self) -> bool:
        """Send a JSON-RPC shutdown request and close the connection."""
        try:
            self._call("shutdown")
            log.info("IDE daemon shutdown requested via JSON-RPC.")
            return True
        except Exception as e:
            log.warning("IDE daemon JSON-RPC shutdown failed: %s", e)
            return False

    def list_skills(self) -> list:
        """List available IDE skills."""
        return self._call("list_skills").get("skills", [])


# ---------------------------------------------------------------------------
# Convenience functions for voice commands
# ---------------------------------------------------------------------------

_client: Optional[IdeClient] = None


def get_client() -> IdeClient:
    global _client
    if _client is None:
        _client = IdeClient()
    return _client


def open_file(path: str) -> str:
    """Open a file and return a spoken summary."""
    try:
        result = get_client().open_file(path)
        summary = get_client().get_summary()
        return summary.get("summary", f"Opened {result.get('path', path)}")
    except RuntimeError as e:
        return f"Error opening file: {e}"


def get_structure() -> str:
    """Get the code structure as spoken text."""
    try:
        result = get_client().get_tree()
        return result.get("tree", "No structure available")
    except RuntimeError as e:
        return f"Error: {e}"


def get_summary() -> str:
    """Get a text summary of the code structure."""
    try:
        result = get_client().get_summary()
        return result.get("summary", "No summary available")
    except RuntimeError as e:
        return f"Error: {e}"


def go_to_function(name: str) -> str:
    """Navigate to a function by name."""
    try:
        structure = get_client().get_structure()
        for sym in structure.get("symbols", []):
            if sym["name"].lower() == name.lower():
                get_client().set_cursor(sym["start_line"])
                return f"Moved to function {name} at line {sym['start_line']}"
        return f"Function {name} not found"
    except RuntimeError as e:
        return f"Error: {e}"


def go_to_line(line: int) -> str:
    """Navigate to a specific line."""
    try:
        get_client().set_cursor(line)
        return f"Moved to line {line}"
    except RuntimeError as e:
        return f"Error: {e}"


def where_am_i() -> str:
    """Get current location description."""
    try:
        cursor = get_client().get_cursor()
        structure = get_client().get_structure()
        line = cursor.get("line", 0)
        total = structure.get("total_lines", 0)

        # Find enclosing function/class
        for sym in structure.get("symbols", []):
            if sym["start_line"] <= line <= sym["end_line"]:
                return (
                    f"Line {line} of {total}, inside {sym['kind']} "
                    f"{sym['name']} (lines {sym['start_line']}-{sym['end_line']})"
                )
        return f"Line {line} of {total}"
    except RuntimeError as e:
        return f"Error: {e}"


# ---------------------------------------------------------------------------
# Daemon lifecycle
# ---------------------------------------------------------------------------

def _find_daemon_binary() -> Optional[str]:
    """Locate the aether-ide-daemon binary (debug first, then release)."""
    base = Path(__file__).resolve().parent.parent
    for profile in ("debug", "release"):
        candidate = base / "target" / profile / "aether-ide-daemon"
        if candidate.is_file():
            return str(candidate)
    return None


def start_daemon(timeout: float = 3.0) -> bool:
    """Start the aether-ide-daemon subprocess and wait for the socket."""
    binary = _find_daemon_binary()
    if binary is None:
        log.warning("aether-ide-daemon binary not found; daemon features unavailable.")
        return False

    try:
        subprocess.Popen(
            [binary],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        log.info("Spawned aether-ide-daemon from %s", binary)
    except Exception as e:
        log.error("Failed to spawn aether-ide-daemon: %s", e)
        return False

    elapsed = 0.0
    interval = 0.2
    while elapsed < timeout:
        if os.path.exists(SOCKET_PATH):
            log.info("IDE daemon socket ready at %s", SOCKET_PATH)
            return True
        time.sleep(interval)
        elapsed += interval

    log.error("Timed out waiting for IDE daemon socket at %s", SOCKET_PATH)
    return False


def stop_daemon() -> None:
    """Shut down the IDE daemon gracefully, then kill as safety net."""
    client = get_client()
    client.shutdown()
    try:
        subprocess.run(
            ["pkill", "-f", "aether-ide-daemon"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except Exception:
        pass
    log.info("IDE daemon stopped.")
