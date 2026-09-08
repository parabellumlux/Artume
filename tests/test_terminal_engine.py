"""Tests for artome_ide/terminal_engine.py — Terminal operations with mock subprocess."""

from unittest.mock import patch, MagicMock
import subprocess

import pytest

from artome_ide.terminal_engine import TerminalEngine


@pytest.fixture
def te():
    return TerminalEngine()


class TestRun:
    @patch("artome_ide.terminal_engine.subprocess.run")
    def test_run_success(self, mock_run, te):
        mock_run.return_value = MagicMock(stdout="hello world", stderr="", returncode=0)
        result = te.run("echo hello")
        assert "hello world" in result

    @patch("artome_ide.terminal_engine.subprocess.run")
    def test_run_no_output(self, mock_run, te):
        mock_run.return_value = MagicMock(stdout="", stderr="", returncode=0)
        result = te.run("true")
        assert "no output" in result.lower()

    @patch("artome_ide.terminal_engine.subprocess.run")
    def test_run_timeout(self, mock_run, te):
        mock_run.side_effect = subprocess.TimeoutExpired("cmd", 5)
        result = te.run("sleep 100", timeout=5)
        assert "timed out" in result.lower()

    @patch("artome_ide.terminal_engine.subprocess.run")
    def test_run_truncates_long_output(self, mock_run, te):
        long_output = "\n".join(f"line {i}" for i in range(20))
        mock_run.return_value = MagicMock(stdout=long_output, stderr="", returncode=0)
        result = te.run("seq 20")
        assert "more lines" in result

    @patch("artome_ide.terminal_engine.subprocess.run")
    def test_run_stderr_fallback(self, mock_run, te):
        mock_run.return_value = MagicMock(stdout="", stderr="error message", returncode=1)
        result = te.run("bad_cmd")
        assert "error message" in result


class TestRunFile:
    @patch.object(TerminalEngine, "run")
    def test_run_python(self, mock_run, te):
        mock_run.return_value = "output"
        result = te.run_file("test.py")
        mock_run.assert_called_once()
        assert "python3" in mock_run.call_args[0][0]

    @patch.object(TerminalEngine, "run")
    def test_run_shell(self, mock_run, te):
        mock_run.return_value = "output"
        te.run_file("script.sh")
        assert "bash" in mock_run.call_args[0][0]

    @patch.object(TerminalEngine, "run")
    def test_run_javascript(self, mock_run, te):
        mock_run.return_value = "output"
        te.run_file("app.js")
        assert "node" in mock_run.call_args[0][0]

    def test_run_unknown_extension(self, te):
        result = te.run_file("data.xyz")
        assert "don't know" in result.lower()


class TestShowAndClearTerminal:
    def test_show_empty(self, te):
        result = te.show_terminal()
        assert "no terminal output" in result.lower()

    def test_show_with_content(self, te):
        te._output_buffer = ["line1", "line2", "line3"]
        result = te.show_terminal(lines=2)
        assert "line2" in result
        assert "line3" in result

    def test_clear(self, te):
        te._output_buffer = ["stuff"]
        result = te.clear_terminal()
        assert "cleared" in result.lower()
        assert te._output_buffer == []


class TestStop:
    def test_stop_no_process(self, te):
        result = te.stop()
        assert "no command" in result.lower()

    def test_stop_running(self, te):
        mock_proc = MagicMock()
        mock_proc.poll.return_value = None
        te._process = mock_proc
        result = te.stop()
        assert "terminated" in result.lower()
        mock_proc.terminate.assert_called_once()
