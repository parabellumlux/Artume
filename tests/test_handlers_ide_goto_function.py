"""Tests for the IDE "go to function" voice path (handlers/ide.py)."""

from unittest.mock import MagicMock, patch

from handlers.ide import handle as ide_handle


def _ctx():
    return {
        'tts': MagicMock(),
        'ide': MagicMock(),
        'ai_assistant': MagicMock(),
        'git_engine': MagicMock(),
        'terminal_engine': MagicMock(),
        'confirm_dialog': MagicMock(),
        'state_mgr': MagicMock(),
    }


class TestShowErrors:
    def test_clean_file(self, tmp_path):
        f = tmp_path / "clean.py"
        f.write_text("x = 1\ny = x + 2\nprint(y)\n")
        ide = MagicMock()
        ide.active_file = str(f)
        ctx = _ctx()
        ctx['ide'] = ide
        with patch("earcons.play_earcon"):
            assert ide_handle("show errors", "show errors", "show errors", ctx) is True
        ctx['tts'].speak.assert_any_call("No errors found.")

    def test_undefined_name_reported(self, tmp_path):
        f = tmp_path / "bad.py"
        f.write_text("print(undefined_thing)\n")
        ide = MagicMock()
        ide.active_file = str(f)
        ctx = _ctx()
        ctx['ide'] = ide
        with patch("earcons.play_earcon"):
            ide_handle("show errors", "show errors", "show errors", ctx)
        spoken = [str(c) for c in ctx['tts'].speak.call_args_list]
        assert any("issues found" in s for s in spoken)

    def test_syntax_error_reported(self, tmp_path):
        f = tmp_path / "syntax.py"
        f.write_text("def broken(:\n    pass\n")
        ide = MagicMock()
        ide.active_file = str(f)
        ctx = _ctx()
        ctx['ide'] = ide
        with patch("earcons.play_earcon"):
            ide_handle("check errors", "check errors", "check errors", ctx)
        spoken = [str(c) for c in ctx['tts'].speak.call_args_list]
        assert any("Syntax error" in s for s in spoken)

    def test_no_active_file(self):
        ide = MagicMock()
        ide.active_file = None
        ctx = _ctx()
        ctx['ide'] = ide
        with patch("earcons.play_earcon"):
            ide_handle("show errors", "show errors", "show errors", ctx)
        ctx['tts'].speak.assert_any_call("No active file loaded.")

    def test_daemon_error(self, tmp_path):
        f = tmp_path / "x.py"
        f.write_text("x = 1\n")
        ide = MagicMock()
        ide.active_file = str(f)
        ctx = _ctx()
        ctx['ide'] = ide
        with patch("earcons.play_earcon"), \
             patch("handlers.ide._check_file_errors",
                   side_effect=RuntimeError("boom")):
            ide_handle("show errors", "show errors", "show errors", ctx)
        ctx['tts'].speak.assert_any_call("Error check failed: boom")


class TestGoToFunction:
    def test_navigates_to_named_function(self):
        ctx = _ctx()
        with patch("earcons.play_earcon"), \
             patch("artome_ide.go_to_function",
                   return_value="Moved to function load_file at line 14") as gt:
            assert ide_handle("go to function load_file", "go to function load_file",
                              "go to function load_file", ctx) is True
        gt.assert_called_once_with("load_file")
        ctx['tts'].speak.assert_any_call("Moved to function load_file at line 14")

    def test_jump_to_function_synonym(self):
        ctx = _ctx()
        with patch("earcons.play_earcon"), \
             patch("artome_ide.go_to_function", return_value="Moved to function foo at line 3") as gt:
            ide_handle("jump to function foo", "jump to function foo", "jump to function foo", ctx)
        gt.assert_called_once_with("foo")

    def test_daemon_error_is_spoken(self):
        ctx = _ctx()
        with patch("earcons.play_earcon"), \
             patch("artome_ide.go_to_function", side_effect=RuntimeError("no daemon")) as gt:
            ide_handle("go to function bar", "go to function bar", "go to function bar", ctx)
        gt.assert_called_once_with("bar")
        ctx['tts'].speak.assert_any_call("IDE daemon error: no daemon")