"""Tests for daemon-backed LSP routing (artome_ide + handlers/ide.py)."""

from unittest.mock import MagicMock, patch

import artome_ide
from handlers.ide import handle as ide_handle

from tests.test_handlers_ide_goto_function import _ctx


class TestArtomeIdeLsp:
    def setup_method(self):
        artome_ide._lsp_initialized = False

    def test_lsp_ensure_inits_once(self):
        client = MagicMock()
        client.lsp_go_to_definition.return_value = {
            "location": {"file": "file:///tmp/foo.py", "line": 1}}
        with patch("artome_ide.get_client", return_value=client):
            artome_ide.lsp_go_to_definition("/tmp/foo.py", 0, 0)
            artome_ide.lsp_hover("/tmp/foo.py", 0, 0)
        assert client.lsp_init.call_count == 1
        client.lsp_init.assert_called_once_with(["pylsp"])

    def test_lsp_init_retries_on_failure(self):
        client = MagicMock()
        client.lsp_init.side_effect = RuntimeError("offline")
        with patch("artome_ide.get_client", return_value=client):
            try:
                artome_ide.lsp_go_to_definition("/tmp/foo.py", 0, 0)
            except RuntimeError:
                pass
        assert artome_ide._lsp_initialized is False
        assert client.lsp_init.call_count == 1

    def test_definition_phrase(self):
        client = MagicMock()
        client.lsp_go_to_definition.return_value = {
            "location": {"file": "file:///tmp/foo.py", "line": 2}}
        with patch("artome_ide.get_client", return_value=client):
            out = artome_ide.lsp_go_to_definition("/tmp/foo.py", 0, 0)
        assert out == "Definition at /tmp/foo.py line 3"

    def test_no_definition(self):
        client = MagicMock()
        client.lsp_go_to_definition.return_value = {"location": None}
        with patch("artome_ide.get_client", return_value=client):
            out = artome_ide.lsp_go_to_definition("/tmp/foo.py", 0, 0)
        assert out == "No definition found."

    def test_references_phrase(self):
        client = MagicMock()
        client.lsp_references.return_value = {
            "references": [
                {"file": "file:///tmp/foo.py", "line": 4},
                {"file": "file:///tmp/bar.py", "line": 9},
            ]}
        with patch("artome_ide.get_client", return_value=client):
            out = artome_ide.lsp_references("/tmp/foo.py", 0, 0)
            out = artome_ide.lsp_references("/tmp/foo.py", 0, 0)
        assert out == "Found 2 references. Including: foo.py line 5, bar.py line 10"

    def test_hover_phrase(self):
        client = MagicMock()
        client.lsp_hover.return_value = {"type_info": "int", "docstring": "An integer"}
        with patch("artome_ide.get_client", return_value=client):
            out = artome_ide.lsp_hover("/tmp/foo.py", 0, 0)
        assert out == "int. An integer"

    def test_rename_phrase(self):
        client = MagicMock()
        client.lsp_rename.return_value = {
            "workspace_edit": {"changes": {"file:///tmp/foo.py": ["e1", "e2"]}}}
        with patch("artome_ide.get_client", return_value=client):
            out = artome_ide.lsp_rename("/tmp/foo.py", 0, 0, "bar")
        assert out == "Renamed to bar. 2 edits across 1 file."


class TestIdeHandlerLspRouting:
    def test_uses_daemon_when_available(self):
        f = MagicMock()
        f.as_posix.return_value = "/tmp/foo.py"
        f.__str__ = lambda self: "/tmp/foo.py"
        f.__fspath__ = lambda self: "/tmp/foo.py"
        ide = MagicMock()
        ide.active_file = "/tmp/foo.py"
        ctx = _ctx()
        ctx['ide'] = ide
        with patch("earcons.play_earcon"), \
             patch("os.path.exists", return_value=True), \
             patch("artome_ide.lsp_go_to_definition",
                   return_value="Definition at /tmp/foo.py line 3") as lsp, \
             patch("handlers.ide._cursor_position", return_value=(0, 3)):
            assert ide_handle("go to definition", "go to definition", "SEMANTIC", ctx) is True
        lsp.assert_called_once_with("/tmp/foo.py", 0, 3)
        ctx['tts'].speak.assert_any_call("Definition at /tmp/foo.py line 3")

    def test_falls_back_to_python_client_when_daemon_down(self):
        ide = MagicMock()
        ide.active_file = "/tmp/foo.py"
        ctx = _ctx()
        ctx['ide'] = ide
        with patch("earcons.play_earcon"), \
             patch("os.path.exists", return_value=True), \
             patch("artome_ide.lsp_go_to_definition",
                   side_effect=RuntimeError("offline")), \
             patch("handlers.ide._cursor_position", return_value=(0, 3)) as pos, \
             patch("lsp_client.LSPClient.start", return_value=False):
            ide_handle("go to definition", "go to definition", "SEMANTIC", ctx)
        pos.assert_called_once_with()
        ctx['tts'].speak.assert_any_call(
            "LSP server not available. Install pylsp.")

    def test_no_active_file(self):
        ctx = _ctx()
        ctx['ide'].active_file = None
        with patch("earcons.play_earcon"):
            ide_handle("hover", "hover", "SEMANTIC", ctx)
        ctx['tts'].speak.assert_any_call("No active file loaded.")