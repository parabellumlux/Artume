"""Tests for artome_ide/__init__.py — IdeClient JSON-RPC with mock socket."""

import json
from unittest.mock import patch, MagicMock

import pytest

from artome_ide import IdeClient


@pytest.fixture
def client():
    return IdeClient(socket_path="/tmp/test-ide.sock")


def _make_jsonrpc_response(result=None, error=None):
    resp = {"id": 1}
    if error:
        resp["error"] = error
    else:
        resp["result"] = result or {}
    return json.dumps(resp).encode() + b"\n"


class TestIdeClientCall:
    @patch("artome_ide.socket.socket")
    def test_call_sends_request(self, mock_socket_cls, client):
        mock_sock = MagicMock()
        mock_socket_cls.return_value = mock_sock
        mock_sock.recv.side_effect = [
            _make_jsonrpc_response({"path": "/tmp/test.py"}),
            b"",
        ]
        result = client.open_file("/tmp/test.py")
        assert result == {"path": "/tmp/test.py"}

    @patch("artome_ide.socket.socket")
    def test_call_raises_on_error(self, mock_socket_cls, client):
        mock_sock = MagicMock()
        mock_socket_cls.return_value = mock_sock
        mock_sock.recv.side_effect = [
            _make_jsonrpc_response(error={"code": -1, "message": "file not found"}),
            b"",
        ]
        with pytest.raises(RuntimeError, match="file not found"):
            client.open_file("/tmp/missing.py")

    @patch("artome_ide.socket.socket")
    def test_call_raises_on_socket_error(self, mock_socket_cls, client):
        mock_sock = MagicMock()
        mock_socket_cls.return_value = mock_sock
        mock_sock.connect.side_effect = OSError("Connection refused")
        with pytest.raises(RuntimeError, match="connection failed"):
            client.open_file("/tmp/test.py")

    @patch("artome_ide.socket.socket")
    def test_call_increments_id(self, mock_socket_cls, client):
        mock_sock = MagicMock()
        mock_socket_cls.return_value = mock_sock
        mock_sock.recv.side_effect = [
            _make_jsonrpc_response({}),
            b"",
            _make_jsonrpc_response({}),
            b"",
        ]
        client.get_structure()
        assert client._id == 2


class TestIdeClientMethods:
    @patch.object(IdeClient, "_call")
    def test_open_file(self, mock_call, client):
        mock_call.return_value = {"path": "/tmp/a.py"}
        result = client.open_file("/tmp/a.py")
        mock_call.assert_called_with("open_file", {"path": "/tmp/a.py"})

    @patch.object(IdeClient, "_call")
    def test_get_structure(self, mock_call, client):
        mock_call.return_value = {"symbols": []}
        client.get_structure()
        mock_call.assert_called_with("get_structure")

    @patch.object(IdeClient, "_call")
    def test_set_cursor(self, mock_call, client):
        mock_call.return_value = {}
        client.set_cursor(10, 5)
        mock_call.assert_called_with("set_cursor", {"line": 10, "column": 5})

    @patch.object(IdeClient, "_call")
    def test_get_cursor(self, mock_call, client):
        mock_call.return_value = {"line": 1, "column": 0}
        result = client.get_cursor()
        mock_call.assert_called_with("get_cursor")

    @patch.object(IdeClient, "_call")
    def test_shutdown(self, mock_call, client):
        mock_call.return_value = {}
        result = client.shutdown()
        assert result is True

    @patch.object(IdeClient, "_call")
    def test_list_skills(self, mock_call, client):
        mock_call.return_value = {"skills": ["fix", "explain"]}
        result = client.list_skills()
        assert result == ["fix", "explain"]
