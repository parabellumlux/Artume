"""Tests for window_manager.py — EWMH window control via mock subprocess."""

from unittest.mock import patch, MagicMock

import pytest

from window_manager import WindowManager, WindowInfo


@pytest.fixture
def wm():
    return WindowManager()


MOCK_WMCTRL_OUTPUT = """0x04000007  0 terminal  hostname Terminal
0x04000008  0 browser  hostname Firefox
0x04000009  1 code     hostname VS Code"""


class TestListWindows:
    @patch.object(WindowManager, "_run")
    def test_list_windows_parses_output(self, mock_run, wm):
        mock_run.return_value = MOCK_WMCTRL_OUTPUT
        result = wm.list_windows()
        assert "3 open windows" in result
        assert "Terminal" in result

    @patch.object(WindowManager, "_run")
    def test_list_windows_empty(self, mock_run, wm):
        mock_run.return_value = ""
        result = wm.list_windows()
        assert "not available" in result.lower()

    @patch.object(WindowManager, "_run")
    def test_list_windows_no_matches(self, mock_run, wm):
        mock_run.return_value = "bad data\n"
        result = wm.list_windows()
        assert "no windows" in result.lower()


class TestFocusWindow:
    @patch.object(WindowManager, "_run")
    def test_focus_by_number(self, mock_run, wm):
        mock_run.return_value = MOCK_WMCTRL_OUTPUT
        wm.list_windows()
        mock_run.reset_mock()
        result = wm.focus_window("2")
        assert "Focused window: Firefox" in result

    @patch.object(WindowManager, "_run")
    def test_focus_by_title(self, mock_run, wm):
        mock_run.return_value = MOCK_WMCTRL_OUTPUT
        wm.list_windows()
        mock_run.reset_mock()
        result = wm.focus_window("firefox")
        assert "Focused window: Firefox" in result

    @patch.object(WindowManager, "_run")
    def test_focus_invalid_number(self, mock_run, wm):
        mock_run.return_value = MOCK_WMCTRL_OUTPUT
        wm.list_windows()
        result = wm.focus_window("99")
        assert "invalid" in result.lower()

    @patch.object(WindowManager, "_run")
    def test_focus_no_match(self, mock_run, wm):
        mock_run.return_value = MOCK_WMCTRL_OUTPUT
        wm.list_windows()
        result = wm.focus_window("notepad")
        assert "no window" in result.lower()


class TestCloseWindow:
    @patch.object(WindowManager, "_run")
    def test_close_by_title(self, mock_run, wm):
        mock_run.return_value = MOCK_WMCTRL_OUTPUT
        wm.list_windows()
        mock_run.reset_mock()
        result = wm.close_window("terminal")
        assert "Closed window: Terminal" in result

    @patch.object(WindowManager, "_run")
    def test_close_current(self, mock_run, wm):
        result = wm.close_window()
        assert "closed" in result.lower()


class TestMaximize:
    @patch.object(WindowManager, "_run")
    def test_maximize(self, mock_run, wm):
        result = wm.maximize_window()
        assert "maximized" in result.lower()


class TestSwitchDesktop:
    @patch.object(WindowManager, "_run")
    def test_switch_desktop(self, mock_run, wm):
        result = wm.switch_desktop(3)
        assert "desktop 3" in result
        cmd = mock_run.call_args[0][0]
        assert "wmctrl" in cmd


class TestGetActiveWindow:
    @patch.object(WindowManager, "_run")
    def test_get_active(self, mock_run, wm):
        mock_run.return_value = "Firefox"
        result = wm.get_active_window()
        assert "Firefox" in result

    @patch.object(WindowManager, "_run")
    def test_get_active_none(self, mock_run, wm):
        mock_run.return_value = ""
        result = wm.get_active_window()
        assert "could not" in result.lower()
