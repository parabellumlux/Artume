"""Tests for voice edit-in-place (artome_ide edits + IDE handler routing)."""

from unittest.mock import MagicMock, patch

import artome_ide
from handlers.ide import handle as ide_handle

from tests.test_handlers_ide_goto_function import _ctx


def _client(cursor_line=1):
    client = MagicMock()
    client.get_cursor.return_value = {"line": cursor_line, "column": 0}
    return client


class TestArtomeIdeEditFunctions:
    def test_insert_after_cursor_line(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\n")
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.edit_insert_line(str(f), "extra")
        assert result["speech"] == "Inserted line 1: extra."
        assert result["total"] == 4
        assert f.read_text().splitlines() == ["a", "extra", "b", "c"]
        client.set_cursor.assert_called_once_with(1)

    def test_insert_before_line(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\n")
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.edit_insert_line(str(f), "zero",
                                                 at_line=1, after=False)
        assert result["speech"] == "Inserted line 1: zero."
        assert f.read_text().splitlines() == ["zero", "a", "b", "c"]

    def test_insert_at_explicit_line(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\n")
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.edit_insert_line(str(f), "x", at_line=2)
        assert result["speech"] == "Inserted line 2: x."
        assert f.read_text().splitlines() == ["a", "b", "x", "c"]

    def test_replace_line(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\n")
        client = _client(3)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.edit_replace_line(str(f), 2, "changed")
        assert result["speech"] == "Replaced line 2: changed."
        assert f.read_text().splitlines() == ["a", "changed", "c"]
        client.set_cursor.assert_called_once_with(2)

    def test_replace_out_of_range(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\n")
        client = _client(1)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.edit_replace_line(str(f), 99, "nope")
        assert result["changed"] is False
        assert result["speech"] == "Line 99 does not exist."
        client.set_cursor.assert_not_called()

    def test_delete_range(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\nd\n")
        client = _client(4)
        with patch("artome_ide.get_client", return_value=client):
            result = artome_ide.edit_delete_lines(str(f), 2, 3)
        assert result["speech"] == "Deleted lines 2 to 3. Total now 2."
        assert f.read_text().splitlines() == ["a", "d"]
        client.set_cursor.assert_called_once_with(2)


class TestIdeHandlerEditing:
    def _handle(self, phrase, file, cursor_line=1, target=None):
        ide = MagicMock()
        ide.active_file = str(file)
        ide.read_lines.return_value = "read back line"
        ctx = _ctx()
        ctx["ide"] = ide
        client = _client(cursor_line)
        with patch("earcons.play_earcon"), \
             patch("artome_ide.get_client", return_value=client):
            handled = ide_handle(phrase, (target or phrase).lower(),
                                 target or phrase, ctx)
        return handled, ide, ctx

    def test_insert_line(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\n")
        handled, ide, ctx = self._handle("insert line return 0", f,
                                         target="edit:insert line return 0")
        assert handled is True
        assert f.read_text().splitlines() == ["a", "return 0", "b", "c"]
        ctx["tts"].speak.assert_any_call("Inserted line 1: return 0.")
        ide.read_lines.assert_called_once_with(start_line=1, count=1)

    def test_insert_after_line(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\n")
        handled, _, ctx = self._handle(
            "insert after line 5 alpha", f,
            target="edit:insert after line 5 alpha")
        assert handled is True
        assert "Inserted line" in ctx["tts"].speak.call_args_list[-2][0][0]
        assert f.read_text().splitlines()[-1] == "alpha"

    def test_replace_line(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\n")
        handled, ide, ctx = self._handle(
            "replace line 2 with x = 9", f,
            target="edit:replace line 2 with x = 9")
        assert handled is True
        assert f.read_text().splitlines() == ["a", "x = 9", "c"]
        ctx["tts"].speak.assert_any_call("Replaced line 2: x = 9.")
        ide.read_lines.assert_called_once_with(start_line=2, count=1)

    def test_delete_lines_range(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\nd\n")
        handled, _, ctx = self._handle("delete lines 2 to 3", f,
                                       target="edit:delete lines 2 to 3")
        assert handled is True
        assert f.read_text().splitlines() == ["a", "d"]
        ctx["tts"].speak.assert_any_call("Deleted lines 2 to 3. Total now 2.")

    def test_edit_does_not_swallow_navigation(self, tmp_path):
        f = tmp_path / "code.py"
        f.write_text("a\nb\nc\n")
        ide = MagicMock()
        ide.active_file = str(f)
        ctx = _ctx()
        ctx["ide"] = ide
        ctx["tts"].speak.return_value = None
        with patch("earcons.play_earcon"), \
             patch("artome_ide.get_client", return_value=_client(1)):
            handled = ide_handle("next function", "next function",
                                 "next function", ctx)
        assert handled is True
        assert "Inserted" not in " ".join(
            str(c) for c in ctx["tts"].speak.call_args_list)