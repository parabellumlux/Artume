"""Tests for spoken code navigation (artome_ide movement + IDE handler routing)."""

from unittest.mock import MagicMock, patch

import artome_ide
from handlers.ide import handle as ide_handle

from tests.test_handlers_ide_lsp import _ctx


def _structure():
    return {
        "total_lines": 100,
        "symbols": [
            {"name": "helper", "kind": "function", "start_line": 3, "end_line": 20},
            {"name": "Widget", "kind": "class", "start_line": 50, "end_line": 100},
            {"name": "render", "kind": "method", "start_line": 60, "end_line": 80},
        ],
    }


def _client(cursor_line=1):
    client = MagicMock()
    client.get_structure.return_value = _structure()
    client.get_cursor.return_value = {"line": cursor_line, "column": 0}
    return client


class TestArtomeIdeMovement:
    def test_next_function(self):
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.next_symbol(1)
            assert result == {"speech": "Function helper, line 3",
                              "line": 3, "total": 100}
        client.set_cursor.assert_called_once_with(3)

    def test_previous_symbol(self):
        client = _client(55)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.next_symbol(-1)
            assert result["speech"] == "Class Widget, line 50"
        client.set_cursor.assert_called_once_with(50)

    def test_no_next_at_end(self):
        client = _client(100)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.next_symbol(1, kinds=("function",))
        assert result["speech"] == "Already at the end of the file."
        assert result["line"] == 0
        client.set_cursor.assert_not_called()

    def test_no_previous_at_start(self):
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.next_symbol(-1, kinds=("function",))
        assert result["speech"] == "Already at the start of the file."
        assert client.set_cursor.call_count == 0

    def test_next_method_uses_method_symbols(self):
        client = _client(55)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.next_symbol(1, kinds=("function", "method"))
            assert result["speech"] == "Method render, line 60"

    def test_move_lines_down(self):
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.move_lines(10)
        assert result == {"speech": "Line 11", "line": 11, "total": 100}
        client.set_cursor.assert_called_once_with(11)

    def test_move_lines_clamped_to_top(self):
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.move_lines(-5)
        assert result["speech"] == "Already at the top of the file."
        client.set_cursor.assert_not_called()

    def test_move_lines_clamped_to_end(self):
        client = _client(100)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.move_lines(10)
        assert result["speech"] == "Already at the end of the file."
        client.set_cursor.assert_not_called()

    def test_go_to_edge(self):
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.go_to_edge(-1)
            assert result["line"] == 1
            result = artome_ide.go_to_edge(1)
            assert result["line"] == 100
        assert client.set_cursor.call_count == 2

    def test_no_symbols(self):
        client = _client(1)
        client.get_structure.return_value = {"total_lines": 5, "symbols": []}
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.next_symbol()
        assert result["speech"].startswith("No symbols found.")


class TestIdeHandlerNavigation:
    def _ctx_with_ide(self):
        ide = MagicMock()
        ide.active_file = "/tmp/foo.py"
        ide.read_lines.return_value = "line text"
        ctx = _ctx()
        ctx['ide'] = ide
        return ctx

    def test_next_function_block(self):
        ctx = self._ctx_with_ide()
        with patch("earcons.play_earcon"), \
             patch("artome_ide.next_symbol",
                   return_value={"speech": "Function helper, line 3",
                                 "line": 3, "total": 100}), \
             patch("os.path.exists", return_value=True):
            assert ide_handle("next function", "next function", "", ctx) is True
        ctx['tts'].speak.assert_any_call("Function helper, line 3")
        ctx['ide'].read_lines.assert_any_call(start_line=3, count=1)

    def test_previous_class_block(self):
        ctx = self._ctx_with_ide()
        with patch("earcons.play_earcon"), \
             patch("artome_ide.next_symbol",
                   return_value={"speech": "Class Widget, line 50",
                                 "line": 50, "total": 100}), \
             patch("os.path.exists", return_value=True):
            ide_handle("previous class", "previous class", "", ctx)
        ctx['tts'].speak.assert_any_call("Class Widget, line 50")
        ctx['ide'].read_lines.assert_any_call(start_line=50, count=1)

    def test_go_to_line_block(self):
        ctx = self._ctx_with_ide()
        with patch("earcons.play_earcon"), \
             patch("artome_ide.go_to_line", return_value="Moved to line 42"), \
             patch("os.path.exists", return_value=True):
            ide_handle("go to line 42", "go to line 42", "", ctx)
        ctx['tts'].speak.assert_any_call("Moved to line 42")
        ctx['ide'].read_lines.assert_any_call(start_line=42, count=1)

    def test_next_line_block(self):
        ctx = self._ctx_with_ide()
        with patch("earcons.play_earcon"), \
             patch("artome_ide.move_lines",
                   return_value={"speech": "Line 11", "line": 11, "total": 100}), \
             patch("os.path.exists", return_value=True):
            ide_handle("next line", "next line", "", ctx)
        ctx['tts'].speak.assert_any_call("Line 11")
        ctx['ide'].read_lines.assert_any_call(start_line=11, count=1)

    def test_end_of_file_block(self):
        ctx = self._ctx_with_ide()
        with patch("earcons.play_earcon"), \
             patch("artome_ide.go_to_edge",
                   return_value={"speech": "Line 100 of 100",
                                 "line": 100, "total": 100}), \
             patch("os.path.exists", return_value=True):
            ide_handle("end of file", "end of file", "", ctx)
        ctx['tts'].speak.assert_any_call("Line 100 of 100")
        ctx['ide'].read_lines.assert_any_call(start_line=100, count=1)

    def test_step_over_without_session(self):
        ctx = self._ctx_with_ide()
        with patch("earcons.play_earcon"), \
             patch("debug_adapter.get_debug_adapter") as da:
            da.return_value.session_client.return_value = None
            ide_handle("step over", "step over", "", ctx)
        ctx['tts'].speak.assert_any_call("No active debug session.")

    def test_next_line_without_session_falls_through_to_nav(self):
        ctx = self._ctx_with_ide()
        with patch("earcons.play_earcon"), \
             patch("debug_adapter.get_debug_adapter") as da, \
             patch("artome_ide.move_lines",
                   return_value={"speech": "Line 11", "line": 11, "total": 100}), \
             patch("os.path.exists", return_value=True):
            da.return_value.session_client.return_value = None
            assert ide_handle("next line", "next line", "", ctx) is True
        ctx['tts'].speak.assert_any_call("Line 11")

    def test_next_line_steps_when_session_live(self):
        ctx = self._ctx_with_ide()
        client = MagicMock()
        client.step_over.return_value = "Stepped"
        with patch("earcons.play_earcon"), \
             patch("debug_adapter.get_debug_adapter") as da:
            da.return_value.session_client.return_value = client
            ide_handle("next line", "next line", "", ctx)
        ctx['tts'].speak.assert_any_call("Stepped")


class TestRouterNavigation:
    def test_nav_phrases_route_to_ide(self):
        from intent_router import ask_artome_ai
        for phrase in ["next function", "previous class", "next line",
                       "go to line 42", "end of file", "where am i",
                       "next method"]:
            result = ask_artome_ai(phrase, "DESKTOP")
            assert result["action"] == "ide_action", phrase
            assert result["target"].startswith("navigate:"), phrase

    def test_debug_step_still_dap(self):
        from intent_router import ask_artome_ai
        result = ask_artome_ai("step over", "IDE")
        assert result["target"] == "step_over"