"""Tests for bluetooth_manager.py — Bluetooth management via mock subprocess."""

from unittest.mock import patch, MagicMock

import pytest

from bluetooth_manager import BluetoothManager, BluetoothDevice


@pytest.fixture
def bm():
    return BluetoothManager()


MOCK_BLUETOOTH_OUTPUT = """Device AA:BB:CC:DD:EE:01 Headphones
Device AA:BB:CC:DD:EE:02 Keyboard
Device AA:BB:CC:DD:EE:03 Mouse"""


class TestListDevices:
    @patch.object(BluetoothManager, "_run")
    def test_list_devices(self, mock_run, bm):
        mock_run.side_effect = [MOCK_BLUETOOTH_OUTPUT, "Connected: no", "Connected: yes", "Connected: no"]
        result = bm.list_devices()
        assert "3 Bluetooth devices" in result or "Headphones" in result

    @patch.object(BluetoothManager, "_run")
    def test_list_empty(self, mock_run, bm):
        mock_run.return_value = ""
        result = bm.list_devices()
        assert "no bluetooth" in result.lower() or "unavailable" in result.lower()

    @patch.object(BluetoothManager, "_run")
    def test_list_no_devices_found(self, mock_run, bm):
        mock_run.return_value = "some garbage\n"
        result = bm.list_devices()
        assert "no paired" in result.lower()


class TestConnect:
    @patch.object(BluetoothManager, "_run")
    @patch("bluetooth_manager.subprocess.run")
    def test_connect_success(self, mock_run_sub, mock_run, bm):
        mock_run.side_effect = [MOCK_BLUETOOTH_OUTPUT, "Connected: no"]
        mock_run_sub.return_value = MagicMock(stdout="Connection successful")
        result = bm.connect("headphones")
        assert "connected" in result.lower()

    @patch.object(BluetoothManager, "_run")
    @patch("bluetooth_manager.subprocess.run")
    def test_connect_not_found(self, mock_run_sub, mock_run, bm):
        mock_run.return_value = MOCK_BLUETOOTH_OUTPUT
        result = bm.connect("nonexistent")
        assert "not found" in result.lower()


class TestDisconnect:
    @patch.object(BluetoothManager, "_run")
    @patch("bluetooth_manager.subprocess.run")
    def test_disconnect(self, mock_run_sub, mock_run, bm):
        mock_run.side_effect = [MOCK_BLUETOOTH_OUTPUT, "Connected: yes"]
        mock_run_sub.return_value = MagicMock()
        result = bm.disconnect("headphones")
        assert "disconnected" in result.lower()


class TestScan:
    @patch("bluetooth_manager.subprocess.run")
    def test_scan_found(self, mock_run, bm):
        mock_run.return_value = MagicMock(
            stdout="Device AA:BB:CC:DD:EE:01 Speaker\nDevice AA:BB:CC:DD:EE:02 Keyboard"
        )
        result = bm.scan()
        assert "found" in result.lower() or "devices" in result.lower()

    @patch("bluetooth_manager.subprocess.run")
    def test_scan_empty(self, mock_run, bm):
        mock_run.return_value = MagicMock(stdout="")
        result = bm.scan()
        assert "no new devices" in result.lower()


class TestPair:
    @patch.object(BluetoothManager, "_run")
    @patch("bluetooth_manager.subprocess.run")
    def test_pair(self, mock_run_sub, mock_run, bm):
        mock_run.return_value = ""
        mock_run_sub.return_value = MagicMock(stdout="Pairing successful")
        result = bm.pair("Speaker")
        assert "paired" in result.lower()
