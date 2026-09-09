"""Artume Debug Adapter lifecycle.

Spawns a Debug Adapter Protocol server (debugpy) so the voice DAP commands —
start debugging, set breakpoint, continue, step, evaluate — work end-to-end.

The DAP server runs as ``debugpy --listen <host>:<port> --wait-for-client <target>``
and the :class:`~lsp_client.DAPClient` connects to it. The target only starts
after ``run()`` sends ``configurationDone``, so breakpoints can be set first
(attach-then-edit-debug voice flow).
"""

import logging
import os
import socket
import subprocess
import sys
import time
from typing import List, Optional

log = logging.getLogger(__name__)

DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4711


class DebugAdapter:
    """Owns the debug adapter process and the connected DAP client."""

    def __init__(self, host: str = DEFAULT_HOST, port: int = DEFAULT_PORT):
        self.host = host
        self.port = port
        self._proc: Optional[subprocess.Popen] = None
        self.client = None  # DAPClient
        self._configured = False

    @staticmethod
    def _debugpy_available() -> bool:
        import importlib.util
        return importlib.util.find_spec("debugpy") is not None

    def _wait_for_port(self, timeout: float = 10.0) -> bool:
        """Block until the debug adapter accepts TCP connections."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                with socket.create_connection((self.host, self.port), timeout=1.0):
                    return True
            except OSError:
                time.sleep(0.2)
        return False

    def start(self, program: str, args: Optional[List[str]] = None) -> str:
        """Launch the debug adapter for a program and prepare the session.

        Breakpoints can be set between ``start`` and ``run``; the target program
        does not execute until ``run()`` sends ``configurationDone``.
        """
        if self.is_active():
            return "A debug session is already running."
        if not self._debugpy_available():
            return "Debugging requires debugpy. Install it with: pip install debugpy"
        if not program or not os.path.exists(program):
            return f"Debug target not found: {program}"

        listen_addr = self.host if ":" in f"{self.host}:{self.port}" else f"{self.host}:{self.port}"
        cmd = [sys.executable, "-m", "debugpy",
               "--listen", listen_addr,
               "--wait-for-client", program] + list(args or [])
        try:
            self._proc = subprocess.Popen(
                cmd,
                cwd=os.path.dirname(os.path.abspath(program)) or None,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except Exception as e:
            self._proc = None
            return f"Failed to start debug session: {str(e)[:60]}"

        if not self._wait_for_port():
            self.stop()
            return "Debug adapter did not become ready in time."

        from lsp_client import DAPClient
        client = DAPClient(self.host, self.port)
        if not client.connect():
            self.stop()
            return "Could not connect to the debug adapter."
        if not client.initialize():
            self.stop()
            return "Debug adapter rejected the session."

        launch_msg = client.launch(program, args or [],
                                   os.path.dirname(os.path.abspath(program)))
        if "Failed" in launch_msg:
            self.stop()
            return launch_msg
        self.client = client
        self._configured = False
        return "Debug session ready. Say set breakpoint, then run."

    def run(self) -> str:
        """Start the target (configurationDone), or continue if already running."""
        client = self.session_client()
        if not client:
            return "No active debug session. Say debug file first."
        if not self._configured:
            if client.configuration_done():
                self._configured = True
                return "Running."
            return "Failed to start the program."
        return client.continue_execution()

    def is_active(self) -> bool:
        return (self.client is not None
                and self._proc is not None
                and self._proc.poll() is None)

    def session_client(self):
        """Return the live DAP client, or None if the session is gone."""
        if not self.is_active():
            return None
        return self.client

    def stop(self):
        """Tear down the client and adapter process."""
        if self.client:
            try:
                self.client.stop()
            except Exception:
                pass
            self.client = None
        if self._proc:
            try:
                self._proc.kill()
            except Exception:
                pass
            self._proc = None
        self._configured = False


_debug_adapter: Optional[DebugAdapter] = None


def get_debug_adapter() -> DebugAdapter:
    global _debug_adapter
    if _debug_adapter is None:
        _debug_adapter = DebugAdapter()
    return _debug_adapter