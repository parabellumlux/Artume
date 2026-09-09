"""Tests for DAP step-through voice — client introspection + IDE handler readout."""

from unittest.mock import MagicMock, patch

from lsp_client import DAPClient

from tests.test_handlers_ide_goto_function import _ctx
from handlers.ide import handle as ide_handle


def _ctx_with_ide():
    ide = MagicMock()
    ide.active_file = "/tmp/foo.py"
    ide.read_lines.return_value = "line text"
    ctx = _ctx()
    ctx["ide"] = ide
    return ctx


class TestDAPClientIntrospection:
    def _client(self, **send_results):
        c = DAPClient("localhost", 4711)
        c._send = MagicMock()
        if send_results:
            c._send.side_effect = list(send_results.values())
        else:
            c._send.return_value = {}
        return c

    def test_when_paused_speaks_location(self):
        c = self._client(
            stackTrace={"stackFrames": [
                {"line": 12, "name": "run", "source": {"path": "/tmp/app.py"}},
            ]})
        assert c.where() == "Paused at line 12 in run, file app.py."

    def test_when_not_paused(self):
        c = self._client(stackTrace={"stackFrames": []})
        assert "not paused" in c.where()

    def test_list_locals(self):
        c = self._client(
            scopes={"scopes": [{"name": "Locals", "variablesReference": 7}]},
            variables={"variables": [
                {"name": "x", "value": "5"}, {"name": "y", "value": "None"},
            ]})
        assert c.list_locals() == "Locals: x = 5, y = None."

    def test_list_locals_empty_scope(self):
        c = self._client(scopes={"scopes": [{"name": "Globals", "variablesReference": 7}]})
        assert c.list_locals() == "No locals in scope."

    def test_pause(self):
        c = self._client(pause={})
        assert c.pause() == "Paused."

    def test_stopped_event_captures_thread(self):
        c = DAPClient("localhost", 4711)
        c._sock = MagicMock()
        messages = iter([
            {"type": "event", "event": "stopped",
             "body": {"threadId": 9, "reason": "breakpoint"}},
            {"type": "response", "body": {}},
            {"type": "response", "body": {"stackFrames": [{"line": 3}]}},
        ])
        c._read_message = MagicMock(side_effect=lambda: next(messages))
        c._running = True
        assert c._send("next", {"threadId": 1}) == {}
        assert c._stopped_thread == 9
        assert c._send("stackTrace", {}) == {"stackFrames": [{"line": 3}]}
        assert c._stopped_thread == 9


class TestIdeHandlerDebugReadout:
    def _live_client(self):
        client = MagicMock()
        client.step_over.return_value = "Stepping over."
        client.where.return_value = "Paused at line 10 in main, file app.py."
        client.list_locals.return_value = "Locals: x = 5."
        return client

    def test_step_over_then_read_position(self):
        ctx = _ctx_with_ide()
        client = self._live_client()
        with patch("earcons.play_earcon"), \
             patch("debug_adapter.get_debug_adapter") as da:
            da.return_value.session_client.return_value = client
            ide_handle("step over", "step over", "", ctx)
        ctx["tts"].speak.assert_any_call("Stepping over.")
        ctx["tts"].speak.assert_any_call("Paused at line 10 in main, file app.py.")

    def test_list_locals(self):
        ctx = _ctx_with_ide()
        client = self._live_client()
        with patch("earcons.play_earcon"), \
             patch("debug_adapter.get_debug_adapter") as da:
            da.return_value.session_client.return_value = client
            ide_handle("list locals", "list locals", "", ctx)
        ctx["tts"].speak.assert_any_call("Locals: x = 5.")

    def test_where_is_debugger(self):
        ctx = _ctx_with_ide()
        client = self._live_client()
        with patch("earcons.play_earcon"), \
             patch("debug_adapter.get_debug_adapter") as da:
            da.return_value.session_client.return_value = client
            ide_handle("where is the debugger", "where is the debugger", "", ctx)
        ctx["tts"].speak.assert_any_call("Paused at line 10 in main, file app.py.")

    def test_debug_readout_needs_session(self):
        ctx = _ctx_with_ide()
        with patch("earcons.play_earcon"), \
             patch("debug_adapter.get_debug_adapter") as da:
            da.return_value.session_client.return_value = None
            ide_handle("list locals", "list locals", "", ctx)
        ctx["tts"].speak.assert_any_call("No active debug session.")


class TestRouterDebugKeywords:
    def test_router_maps_debug_introspection(self):
        from intent_router import ask_artome_ai
        cases = {
            "list locals": "list_locals",
            "show locals": "list_locals",
            "where is the debugger": "where",
            "debugger position": "where",
            "pause execution": "pause",
        }
        for phrase, target in cases.items():
            result = ask_artome_ai(phrase, "DESKTOP")
            assert result["action"] == "dap_action", phrase
            assert result["target"] == target, phrase