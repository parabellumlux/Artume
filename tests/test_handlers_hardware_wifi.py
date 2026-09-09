"""Tests for WiFi password voice flows (handlers/hardware.py)."""

from unittest.mock import MagicMock, patch

from handlers.hardware import handle


def _ctx(wifi=None, vault=None, phrase=""):
    dialog = MagicMock()
    dialog.capture_phrase.return_value = phrase
    return {
        'tts': MagicMock(),
        'bt_manager': MagicMock(),
        'wifi_manager': wifi if wifi is not None else MagicMock(),
        'audio_switcher': MagicMock(),
        'power_mgr': MagicMock(),
        'confirm_dialog': dialog,
        'vault': vault,
    }


def _connect_ctx(ssid, **kwargs):
    return _ctx(ssid=ssid, **kwargs)


class TestWifiConnect:
    def test_open_network_no_password(self):
        wifi = MagicMock()
        ctx = _ctx(wifi)
        with patch("earcons.play_earcon"):
            assert handle("wifi connect office", ctx) is True
        wifi.connect.assert_called_once_with("office")

    def test_inline_password(self):
        wifi = MagicMock()
        ctx = _ctx(wifi)
        with patch("earcons.play_earcon"):
            handle("wifi connect office password hunter2", ctx)
        wifi.connect.assert_called_once_with("office", "hunter2")

    def test_prompt_fallback_password(self):
        wifi = MagicMock()
        ctx = _ctx(wifi, phrase="hunter2")
        with patch("earcons.play_earcon"):
            handle("wifi connect office", ctx)
        wifi.connect.assert_called_once_with("office", "hunter2")

    def test_vault_password_used(self):
        wifi = MagicMock()
        vault = MagicMock()
        vault.is_unlocked = True
        vault.get.return_value = {"ssid": "office", "password": "vaultpw"}
        ctx = _ctx(wifi, vault=vault)
        with patch("earcons.play_earcon"):
            handle("wifi connect office", ctx)
        vault.get.assert_any_call("wifi_office")
        wifi.connect.assert_called_once_with("office", "vaultpw")

    def test_locked_vault_prompts(self):
        wifi = MagicMock()
        vault = MagicMock()
        vault.is_unlocked = False
        ctx = _ctx(wifi, vault=vault, phrase="hunter2")
        with patch("earcons.play_earcon"):
            handle("wifi connect office", ctx)
        vault.get.assert_not_called()
        wifi.connect.assert_called_once_with("office", "hunter2")

    def test_missing_ssid_prompts(self):
        wifi = MagicMock()
        ctx = _ctx(wifi)
        with patch("earcons.play_earcon"):
            handle("wifi connect", ctx)
        wifi.connect.assert_not_called()