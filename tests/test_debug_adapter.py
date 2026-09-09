"""Tests for debug_adapter.py — DAP debug session lifecycle."""

from unittest.mock import MagicMock, patch

from debug_adapter import DebugAdapter, get_debug_adapter


class TestStart:
    def test_start_missing_target(self):
        adapter = DebugAdapter()
        with patch.object(DebugAdapter, "_debugpy_available", return_value=True):
            result = adapter.start("/no/such/file.py")
        assert "not found" in result.lower()

    def test_start_requires_debugpy(self, tmp_path):
        adapter = DebugAdapter()
        target = tmp_path / "a.py"
        target.write_text("print(1)\n")
        with patch.object(DebugAdapter, "_debugpy_available", return_value=False):
            result = adapter.start(str(target))
        assert "debugpy" in result

    def test_start_timeout(self, tmp_path):
        adapter = DebugAdapter()
        target = tmp_path / "b.py"
        target.write_text("x = 1\n")
        with patch.object(DebugAdapter, "_debugpy_available", return_value=True), \
             patch.object(DebugAdapter, "_wait_for_port", return_value=False), \
             patch("debug_adapter.subprocess.Popen"):
            result = adapter.start(str(target))
        assert "did not become ready" in result.lower()
        assert not adapter.is_active()

    def test_start_success(self, tmp_path):
        adapter = DebugAdapter()
        target = tmp_path / "c.py"
        target.write_text("print(1)\n")
        proc = MagicMock()
        proc.poll.return_value = None
        client = MagicMock()
        client.connect.return_value = True
        client.initialize.return_value = True
        client.launch.return_value = "Debug session ready."
        with patch.object(DebugAdapter, "_debugpy_available", return_value=True), \
             patch("debug_adapter.subprocess.Popen", return_value=proc), \
             patch("debug_adapter.socket.create_connection", return_value=MagicMock()), \
             patch("lsp_client.DAPClient", return_value=client):
            result = adapter.start(str(target))
        assert "debug session ready" in result.lower()
        assert adapter.is_active()
        assert adapter.session_client() is client

    def test_start_rejects_second_session(self, tmp_path):
        adapter = DebugAdapter()
        proc = MagicMock()
        proc.poll.return_value = None
        adapter.client = MagicMock()
        adapter._proc = proc
        result = adapter.start(str(tmp_path / "c.py"))
        assert "already running" in result.lower()


class TestRun:
    def test_run_without_session(self):
        adapter = DebugAdapter()
        assert "no active debug session" in adapter.run().lower()

    def test_run_first_time_sends_configuration_done(self):
        adapter = DebugAdapter()
        client = MagicMock()
        client.configuration_done.return_value = True
        adapter.client = client
        proc = MagicMock()
        proc.poll.return_value = None
        adapter._proc = proc
        result = adapter.run()
        assert result == "Running."
        client.configuration_done.assert_called_once_with()
        assert adapter._configured

    def test_run_second_time_continues(self):
        adapter = DebugAdapter()
        client = MagicMock()
        client.continue_execution.return_value = "Continuing."
        adapter.client = client
        proc = MagicMock()
        proc.poll.return_value = None
        adapter._proc = proc
        adapter._configured = True
        result = adapter.run()
        assert "continuing" in result.lower()
        client.continue_execution.assert_called_once_with()


class TestSession:
    def test_stop_cleans_up(self):
        adapter = DebugAdapter()
        client = MagicMock()
        adapter.client = client
        proc = MagicMock()
        adapter._proc = proc
        adapter._configured = True
        adapter.stop()
        client.stop.assert_called_once()
        proc.kill.assert_called_once()
        assert adapter.client is None
        assert adapter._proc is None
        assert not adapter._configured

    def test_session_client_requires_live_process(self):
        adapter = DebugAdapter()
        adapter.client = MagicMock()
        proc = MagicMock()
        proc.poll.return_value = 0
        adapter._proc = proc
        assert adapter.session_client() is None

    def test_singleton(self):
        assert get_debug_adapter() is get_debug_adapter()