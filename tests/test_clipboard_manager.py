"""Tests for clipboard_manager.py — clipboard operations via mock subprocess."""

from unittest.mock import patch, MagicMock
import subprocess

import pytest

from clipboard_manager import ClipboardManager


@pytest.fixture
def cm():
    return ClipboardManager()


class TestCopy:
    @patch("clipboard_manager.subprocess.Popen")
    def test_copy_calls_xclip(self, mock_popen, cm):
        mock_proc = MagicMock()
        mock_proc.communicate.return_value = (None, None)
        mock_popen.return_value = mock_proc
        result = cm.copy("hello world")
        assert "copied" in result.lower()
        mock_popen.assert_called_once()
        cmd = mock_popen.call_args[0][0]
        assert "xclip" in cmd
        assert "clipboard" in cmd

    @patch("clipboard_manager.subprocess.Popen", side_effect=Exception("fail"))
    def test_copy_failure(self, mock_popen, cm):
        result = cm.copy("test")
        assert "failed" in result.lower()

    def test_copy_stores_history(self, cm):
        with patch("clipboard_manager.subprocess.Popen") as mock_popen:
            mock_proc = MagicMock()
            mock_proc.communicate.return_value = (None, None)
            mock_popen.return_value = mock_proc
            cm.copy("item1")
            cm.copy("item2")
        assert cm._history == ["item1", "item2"]


class TestPaste:
    @patch("clipboard_manager.subprocess.run")
    def test_paste_returns_content(self, mock_run, cm):
        mock_run.return_value = MagicMock(stdout="clipboard text", returncode=0)
        result = cm.paste()
        assert "clipboard text" in result

    @patch("clipboard_manager.subprocess.run", side_effect=Exception("fail"))
    def test_paste_empty(self, mock_run, cm):
        result = cm.paste()
        assert "empty" in result.lower()


class TestSelectAll:
    @patch("clipboard_manager.subprocess.run")
    def test_select_all(self, mock_run, cm):
        result = cm.select_all()
        assert "selected all" in result.lower()
        cmd = mock_run.call_args[0][0]
        assert "ctrl+a" in cmd

    @patch("clipboard_manager.subprocess.run", side_effect=Exception("fail"))
    def test_select_all_failure(self, mock_run, cm):
        result = cm.select_all()
        assert "failed" in result.lower()


class TestHistory:
    def test_history_empty(self, cm):
        result = cm.history()
        assert "empty" in result.lower()

    @patch("clipboard_manager.subprocess.Popen")
    def test_history_with_items(self, mock_popen, cm):
        mock_proc = MagicMock()
        mock_proc.communicate.return_value = (None, None)
        mock_popen.return_value = mock_proc
        for i in range(5):
            cm.copy(f"item{i}")
        result = cm.history()
        assert "5 items" in result

    def test_clear_history(self, cm):
        cm._history = ["a", "b"]
        result = cm.clear_history()
        assert "cleared" in result.lower()
        assert cm._history == []


class TestHistoryLimit:
    @patch("clipboard_manager.subprocess.Popen")
    def test_max_history_enforced(self, mock_popen, cm):
        mock_proc = MagicMock()
        mock_proc.communicate.return_value = (None, None)
        mock_popen.return_value = mock_proc
        for i in range(25):
            cm.copy(f"item{i}")
        assert len(cm._history) == cm._max_history
