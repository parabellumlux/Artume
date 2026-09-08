"""Artome Audio-First IDE — Terminal integration.

Provides voice-driven terminal operations through the IDE.
"""

import subprocess
from typing import Optional


class TerminalEngine:
    """Voice-driven terminal operations."""

    def __init__(self):
        self._process: Optional[subprocess.Popen] = None
        self._output_buffer: list[str] = []

    def run(self, command: str, timeout: int = 30) -> str:
        """Execute a shell command and return output."""
        try:
            result = subprocess.run(
                command, shell=True, capture_output=True, text=True, timeout=timeout,
            )
            output = result.stdout.strip() or result.stderr.strip()
            if not output:
                return "Command completed with no output."
            # Truncate to reasonable length for TTS
            lines = output.split("\n")
            if len(lines) > 10:
                output = "\n".join(lines[:10]) + f"\n... and {len(lines) - 10} more lines"
            if len(output) > 500:
                output = output[:500] + "..."
            self._output_buffer.append(output)
            return output
        except subprocess.TimeoutExpired:
            return f"Command timed out after {timeout} seconds."
        except subprocess.CalledProcessError as e:
            return f"Command failed: {e.stderr[:200]}"
        except FileNotFoundError:
            return "Command not found."

    def run_tests(self, test_path: Optional[str] = None) -> str:
        """Run test suite and return results."""
        if test_path:
            cmd = f"cd {test_path} && make test 2>&1 || cargo test 2>&1 || python3 -m pytest 2>&1"
        else:
            cmd = "make test 2>&1 || cargo test 2>&1 || python3 -m pytest 2>&1"
        return self.run(cmd, timeout=120)

    def run_file(self, filepath: str) -> str:
        """Execute current file based on extension."""
        if filepath.endswith(".py"):
            return self.run(f"python3 {filepath}")
        elif filepath.endswith(".rs"):
            return self.run("cargo run --release 2>&1 || cargo run 2>&1")
        elif filepath.endswith(".sh"):
            return self.run(f"bash {filepath}")
        elif filepath.endswith(".js") or filepath.endswith(".mjs"):
            return self.run(f"node {filepath}")
        else:
            return f"Don't know how to run {filepath}"

    def show_terminal(self, lines: int = 10) -> str:
        """Read last N lines of terminal output."""
        if not self._output_buffer:
            return "No terminal output yet."
        recent = self._output_buffer[-lines:]
        return "\n".join(recent)

    def clear_terminal(self) -> str:
        """Clear terminal buffer."""
        self._output_buffer.clear()
        return "Terminal cleared."

    def stop(self) -> str:
        """Interrupt running command."""
        if self._process and self._process.poll() is None:
            self._process.terminate()
            return "Command terminated."
        return "No command running."
