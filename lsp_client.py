"""Lightweight LSP client for Artume IDE — communicates with language servers via JSON-RPC over stdio."""

import json
import subprocess
import threading
import os
from typing import Optional, Dict, Any, List


class LSPClient:
    """Client for Language Server Protocol over stdio."""

    def __init__(self, server_cmd: List[str], root_uri: str = ""):
        self.server_cmd = server_cmd
        self.root_uri = root_uri or f"file://{os.getcwd()}"
        self._proc: Optional[subprocess.Popen] = None
        self._request_id = 0
        self._lock = threading.Lock()
        self._response_buffer: Dict[int, Any] = {}
        self._reader_thread: Optional[threading.Thread] = None

    def start(self) -> bool:
        """Start the language server process."""
        try:
            self._proc = subprocess.Popen(
                self.server_cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                bufsize=0,
            )
            self._reader_thread = threading.Thread(target=self._read_loop, daemon=True)
            self._reader_thread.start()
            self._initialize()
            return True
        except Exception:
            return False

    def _read_loop(self):
        """Read JSON-RPC responses from stdout."""
        while self._proc and self._proc.poll() is None:
            try:
                header = self._proc.stdout.readline()
                if not header or not header.startswith("Content-Length:"):
                    continue
                length = int(header.split(":")[1].strip())
                self._proc.stdout.readline()  # empty line
                body = self._proc.stdout.read(length)
                msg = json.loads(body)
                if "id" in msg:
                    self._response_buffer[msg["id"]] = msg
            except Exception:
                break

    def _send(self, method: str, params: Dict[str, Any]) -> Optional[Dict]:
        """Send a JSON-RPC request and wait for response."""
        if not self._proc or self._proc.poll() is not None:
            return None
        with self._lock:
            self._request_id += 1
            req_id = self._request_id
        msg = json.dumps({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params,
        })
        content = f"Content-Length: {len(msg)}\r\n\r\n{msg}"
        try:
            self._proc.stdin.write(content)
            self._proc.stdin.flush()
        except Exception:
            return None
        # Wait for response (up to 5s)
        import time
        for _ in range(50):
            time.sleep(0.1)
            if req_id in self._response_buffer:
                resp = self._response_buffer.pop(req_id)
                return resp.get("result")
        return None

    def _initialize(self):
        """Send initialize request."""
        self._send("initialize", {
            "processId": os.getpid(),
            "rootUri": self.root_uri,
            "capabilities": {
                "textDocument": {
                    "hover": {"contentFormat": ["plaintext"]},
                    "definition": {},
                    "references": {},
                    "rename": {},
                    "completion": {},
                }
            },
        })
        self._send("initialized", {})

    def hover(self, file_path: str, line: int, character: int) -> str:
        """Get hover information at position."""
        uri = f"file://{file_path}"
        result = self._send("textDocument/hover", {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
        })
        if result and "contents" in result:
            contents = result["contents"]
            if isinstance(contents, dict):
                return contents.get("value", "")
            return str(contents)
        return "No hover information available."

    def go_to_definition(self, file_path: str, line: int, character: int) -> str:
        """Go to definition at position."""
        uri = f"file://{file_path}"
        result = self._send("textDocument/definition", {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
        })
        if result:
            if isinstance(result, list) and result:
                loc = result[0]
                target = loc.get("targetUri", loc.get("uri", ""))
                target_line = loc.get("targetRange", loc.get("range", {})).get("start", {}).get("line", 0)
                return f"Definition at {target.replace('file://', '')} line {target_line + 1}"
            elif isinstance(result, dict):
                target = result.get("targetUri", result.get("uri", ""))
                target_line = result.get("targetRange", result.get("range", {})).get("start", {}).get("line", 0)
                return f"Definition at {target.replace('file://', '')} line {target_line + 1}"
        return "No definition found."

    def find_references(self, file_path: str, line: int, character: int) -> str:
        """Find all references at position."""
        uri = f"file://{file_path}"
        result = self._send("textDocument/references", {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
            "context": {"includeDeclaration": True},
        })
        if result and isinstance(result, list):
            count = len(result)
            if count == 0:
                return "No references found."
            locations = []
            for ref in result[:5]:
                uri = ref.get("uri", "").replace("file://", "")
                line = ref.get("range", {}).get("start", {}).get("line", 0) + 1
                locations.append(f"{os.path.basename(uri)} line {line}")
            summary = f"Found {count} reference{'s' if count != 1 else ''}."
            if locations:
                summary += " Including: " + ", ".join(locations)
            return summary
        return "No references found."

    def rename(self, file_path: str, line: int, character: int, new_name: str) -> str:
        """Rename symbol at position."""
        uri = f"file://{file_path}"
        result = self._send("textDocument/rename", {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
            "newName": new_name,
        })
        if result and "changes" in result:
            changes = result["changes"]
            total_edits = sum(len(edits) for edits in changes.values())
            return f"Renamed to {new_name}. {total_edits} edit{'s' if total_edits != 1 else ''} across {len(changes)} file{'s' if len(changes) != 1 else ''}."
        return f"Rename to {new_name} completed."

    def stop(self):
        """Stop the language server."""
        if self._proc:
            try:
                self._send("shutdown", {})
                self._send("exit", {})
                self._proc.terminate()
                self._proc.wait(timeout=5)
            except Exception:
                self._proc.kill()
            self._proc = None


class DAPClient:
    """Client for Debug Adapter Protocol over TCP."""

    def __init__(self, host: str = "127.0.0.1", port: int = 4711):
        self.host = host
        self.port = port
        self._sock = None
        self._seq = 0
        self._running = False

    def connect(self) -> bool:
        """Connect to the debug adapter."""
        try:
            import socket
            self._sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self._sock.connect((self.host, self.port))
            self._running = True
            return True
        except Exception:
            return False

    def _send(self, command: str, arguments: Optional[Dict] = None) -> Optional[Dict]:
        """Send a DAP request."""
        if not self._sock or not self._running:
            return None
        self._seq += 1
        msg = json.dumps({
            "seq": self._seq,
            "type": "request",
            "command": command,
            "arguments": arguments or {},
        })
        try:
            content = f"Content-Length: {len(msg)}\r\n\r\n{msg}"
            self._sock.sendall(content.encode())
        except Exception:
            return None
        # Read response
        try:
            header = b""
            while b"\r\n\r\n" not in header:
                chunk = self._sock.recv(1)
                if not chunk:
                    return None
                header += chunk
            length = int(header.split(b"Content-Length:")[1].split(b"\r\n")[0])
            body = b""
            while len(body) < length:
                chunk = self._sock.recv(length - len(body))
                if not chunk:
                    return None
                body += chunk
            resp = json.loads(body)
            return resp.get("body", resp)
        except Exception:
            return None

    def launch(self, program: str, args: Optional[List[str]] = None, cwd: Optional[str] = None) -> str:
        """Launch a debug session."""
        result = self._send("launch", {
            "program": program,
            "args": args or [],
            "cwd": cwd or os.getcwd(),
            "console": "integratedTerminal",
        })
        return "Debug session started." if result else "Failed to start debug session."

    def set_breakpoint(self, file_path: str, line: int) -> str:
        """Set a breakpoint at a line."""
        result = self._send("setBreakpoints", {
            "source": {"path": file_path},
            "breakpoints": [{"line": line}],
        })
        if result and "breakpoints" in result:
            bp = result["breakpoints"][0]
            if bp.get("verified"):
                return f"Breakpoint set at line {line}."
            return f"Breakpoint not verified at line {line}."
        return "Could not set breakpoint."

    def continue_execution(self) -> str:
        """Continue execution."""
        result = self._send("continue", {"threadId": 1})
        return "Continuing." if result else "Failed to continue."

    def step_over(self) -> str:
        """Step over next line."""
        result = self._send("next", {"threadId": 1})
        return "Stepping over." if result else "Failed to step."

    def step_into(self) -> str:
        """Step into function."""
        result = self._send("stepIn", {"threadId": 1})
        return "Stepping into." if result else "Failed to step."

    def step_out(self) -> str:
        """Step out of function."""
        result = self._send("stepOut", {"threadId": 1})
        return "Stepping out." if result else "Failed to step."

    def evaluate(self, expression: str) -> str:
        """Evaluate expression in current stack frame."""
        result = self._send("evaluate", {
            "expression": expression,
            "frameId": 0,
            "context": "repl",
        })
        if result and "result" in result:
            return f"{expression} = {result['result']}"
        return f"Cannot evaluate {expression}."

    def stop(self):
        """Stop the debug session."""
        self._send("terminate", {})
        self._running = False
        if self._sock:
            try:
                self._sock.close()
            except Exception:
                pass
            self._sock = None
