"""Tests for wifi_manager.py — WiFi management via mock subprocess."""

from unittest.mock import patch, MagicMock

import pytest

from wifi_manager import WiFiManager, WiFiNetwork


@pytest.fixture
def wm():
    return WiFiManager()


MOCK_WIFI_LIST = "MyNetwork:85:WPA2\nGuestWiFi:42:WPA\nCoffeeShop:15:"


class TestListNetworks:
    @patch.object(WiFiManager, "_run")
    def test_list_networks(self, mock_run, wm):
        mock_run.return_value = MOCK_WIFI_LIST
        result = wm.list_networks()
        assert "3 networks" in result or "MyNetwork" in result

    @patch.object(WiFiManager, "_run")
    def test_list_empty(self, mock_run, wm):
        mock_run.return_value = ""
        result = wm.list_networks()
        assert "not available" in result.lower()

    @patch.object(WiFiManager, "_run")
    def test_list_strong_medium_weak(self, mock_run, wm):
        mock_run.return_value = MOCK_WIFI_LIST
        result = wm.list_networks()
        assert "strong" in result.lower()


class TestConnect:
    @patch("wifi_manager.subprocess.run")
    def test_connect_with_password(self, mock_run, wm):
        mock_run.return_value = MagicMock(stdout="Connection successfully activated")
        result = wm.connect("MyNetwork", password="pass123")
        assert "connected" in result.lower()
        cmd = mock_run.call_args[0][0]
        assert "password" in cmd

    @patch("wifi_manager.subprocess.run")
    def test_connect_without_password(self, mock_run, wm):
        mock_run.return_value = MagicMock(stdout="Connection successfully activated")
        result = wm.connect("OpenNetwork")
        assert "connected" in result.lower()

    @patch("wifi_manager.subprocess.run")
    def test_connect_failure(self, mock_run, wm):
        mock_run.return_value = MagicMock(stdout="Error", stderr="auth failed")
        result = wm.connect("WrongPassword")
        assert "failed" in result.lower()


class TestDisconnect:
    @patch("wifi_manager.subprocess.run")
    def test_disconnect(self, mock_run, wm):
        result = wm.disconnect()
        assert "disconnected" in result.lower()


class TestStatus:
    @patch.object(WiFiManager, "_run")
    def test_status_connected(self, mock_run, wm):
        mock_run.return_value = "yes:MyNetwork:85"
        result = wm.status()
        assert "MyNetwork" in result
        assert "85" in result

    @patch.object(WiFiManager, "_run")
    def test_status_not_connected(self, mock_run, wm):
        mock_run.return_value = ""
        result = wm.status()
        assert "not connected" in result.lower()


class TestToggle:
    @patch.object(WiFiManager, "_run")
    def test_turn_on(self, mock_run, wm):
        result = wm.turn_on()
        assert "turned on" in result.lower()

    @patch.object(WiFiManager, "_run")
    def test_turn_off(self, mock_run, wm):
        result = wm.turn_off()
        assert "turned off" in result.lower()


class TestSaveCredentials:
    @patch("wifi_manager.subprocess.run")
    def test_save(self, mock_run, wm):
        mock_run.return_value = MagicMock(stdout="Connection successfully added")
        result = wm.save_credentials("MyNetwork", "pass123")
        assert "saved" in result.lower()
