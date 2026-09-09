"""Tests for the item-7 correctness batch (confirm-before-destroy, screen summary)."""

from unittest.mock import MagicMock, patch

from artome_core import execute_action
from handlers.desktop import handle as desktop_handle


def _ctx(mail=None, confirm=True, **extra):
    dialog = MagicMock()
    dialog.confirm_destructive.return_value = confirm
    navigator = MagicMock()
    navigator.select_menu_option.return_value = (None, "")
    ctx = {
        'tts': MagicMock(),
        'mail_client': mail if mail is not None else MagicMock(),
        'confirm_dialog': dialog,
        'navigator': navigator,
        'file_browser': MagicMock(),
        'screen_reader': MagicMock(),
        'notification_center': MagicMock(),
        'clipboard': MagicMock(),
        'window_mgr': MagicMock(),
        'app_launcher': MagicMock(),
        'vault': MagicMock(),
    }
    ctx.update(extra)
    return ctx


def _desktop_ctx(confirm=True, **extra):
    ctx = _ctx(confirm=confirm)
    ctx['bt_manager'] = MagicMock()
    ctx['audio_switcher'] = MagicMock()
    ctx.update(extra)
    return ctx


class TestDeleteFile:
    def test_delete_confirmed(self):
        ctx = _ctx(confirm=True)
        with patch("earcons.play_earcon"):
            execute_action(
                {"action": "file_action", "speech": "Deleting", "target": "delete:notes.txt"},
                "delete notes.txt", "DESKTOP", ctx)
        ctx['file_browser'].delete_file.assert_called_once_with("notes.txt")

    def test_delete_cancelled(self):
        ctx = _ctx(confirm=False)
        with patch("earcons.play_earcon"):
            execute_action(
                {"action": "file_action", "speech": "Deleting", "target": "delete:notes.txt"},
                "delete notes.txt", "DESKTOP", ctx)
        ctx['file_browser'].delete_file.assert_not_called()


class TestCloseQuit:
    def test_close_window_confirmed(self):
        ctx = _desktop_ctx(confirm=True)
        with patch("earcons.play_earcon"):
            assert desktop_handle("close window", ctx) is True
        ctx['window_mgr'].close_window.assert_called_once_with()

    def test_close_window_cancelled(self):
        ctx = _desktop_ctx(confirm=False)
        with patch("earcons.play_earcon"):
            desktop_handle("close window", ctx)
        ctx['window_mgr'].close_window.assert_not_called()

    def test_quit_app_confirmed(self):
        ctx = _desktop_ctx(confirm=True)
        with patch("earcons.play_earcon"):
            desktop_handle("quit app firefox", ctx)
        ctx['app_launcher'].quit_app.assert_called_once_with("firefox")

    def test_quit_app_cancelled(self):
        ctx = _desktop_ctx(confirm=False)
        with patch("earcons.play_earcon"):
            desktop_handle("quit app firefox", ctx)
        ctx['app_launcher'].quit_app.assert_not_called()


class TestScreenSummary:
    def test_runs_reader(self):
        ctx = _ctx()
        ctx['screen_reader'].generate_screen_summary_payload.return_value = "two windows open"
        with patch("earcons.play_earcon"), \
             patch("requests.post", return_value=MagicMock(
                 **{"json.return_value": {"response": "Two windows open, dock visible."}})):
            execute_action(
                {"action": "screen_summary", "speech": "Summarizing screen", "target": "COMMAND:SCREEN_SUMMARY"},
                "screen summary", "DESKTOP", ctx)
        ctx['screen_reader'].generate_screen_summary_payload.assert_called_once_with()
        ctx['tts'].speak.assert_any_call("Screen summary: Two windows open, dock visible.")

    def test_falls_back_to_raw_payload(self):
        ctx = _ctx()
        ctx['screen_reader'].generate_screen_summary_payload.return_value = "two windows open"
        with patch("earcons.play_earcon"), \
             patch("requests.post", side_effect=RuntimeError("ollama down")):
            execute_action(
                {"action": "screen_summary", "speech": "Summarizing screen", "target": "COMMAND:SCREEN_SUMMARY"},
                "screen summary", "DESKTOP", ctx)
        ctx['tts'].speak.assert_any_call("Screen summary: two windows open")

    def test_falls_back_when_reader_fails(self):
        ctx = _ctx()
        ctx['screen_reader'].generate_screen_summary_payload.side_effect = RuntimeError("no display")
        with patch("earcons.play_earcon"):
            execute_action(
                {"action": "screen_summary", "speech": "Screen reader offline", "target": "COMMAND:SCREEN_SUMMARY"},
                "screen summary", "DESKTOP", ctx)
        ctx['tts'].speak.assert_any_call("Screen reader offline")