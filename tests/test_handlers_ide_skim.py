"""Tests for spoken structure skim (artome_ide.skim_structure + routing)."""

from unittest.mock import MagicMock, patch

import artome_ide

from tests.test_handlers_ide_goto_function import _ctx
from handlers.ide import handle as ide_handle


def _structure():
    return {
        "total_lines": 100,
        "symbols": [
            {"name": "helper", "kind": "function", "start_line": 3, "end_line": 20},
            {"name": "Widget", "kind": "class", "start_line": 50, "end_line": 100},
            {"name": "render", "kind": "method", "start_line": 60, "end_line": 80},
        ],
    }


class TestSkimStructure:
    def test_concise_toc(self):
        client = MagicMock()
        client.get_structure.return_value = _structure()
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.skim_structure()
        assert "1 function, 1 class in 100 lines." in result
        assert "helper, line 3" in result
        assert "Widget, line 50" in result
        assert "render" not in result  # methods excluded by default

    def test_includes_methods_when_requested(self):
        client = MagicMock()
        client.get_structure.return_value = _structure()
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.skim_structure(kinds=("function", "method", "class"))
        assert "render, line 60" in result

    def test_empty_file(self):
        client = MagicMock()
        client.get_structure.return_value = {"total_lines": 40, "symbols": []}
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.skim_structure()
        assert "no top-level symbols" in result

    def test_truncates_long_files(self):
        client = MagicMock()
        client.get_structure.return_value = {
            "total_lines": 500,
            "symbols": [{"name": f"fn{i}", "kind": "function",
                         "start_line": i, "end_line": i}
                        for i in range(1, 25)],
        }
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.skim_structure(kinds=("function",))
        assert "First 15 of 24." in result
        assert "fn1, line 1" in result
        assert "fn24" not in result

    def test_daemon_error(self):
        client = MagicMock()
        client.get_structure.side_effect = RuntimeError("no daemon")
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.skim_structure()
        assert result.startswith("Error:")


class TestHandlerSkim:
    def test_skim_phrase_speaks_toc(self):
        ctx = _ctx()
        with patch("handlers.ide.play_earcon") as pe, \
             patch("artome_ide.skim_structure") as skim:
            skim.return_value = "1 function, 1 class in 100 lines."
            assert ide_handle("skim code", "skim code", "", ctx) is True
        assert ctx["tts"].speak.call_args_list[0][0][0] == (
            "1 function, 1 class in 100 lines.")
        pe.assert_any_call("info")


class TestRouterSkim:
    def test_routes_to_ide(self):
        from intent_router import ask_artome_ai
        for phrase in ["skim code", "code outline", "list functions",
                       "table of contents"]:
            result = ask_artome_ai(phrase, "DESKTOP")
            assert result["action"] == "ide_action", phrase
            assert result["target"].startswith("skim:"), phrase