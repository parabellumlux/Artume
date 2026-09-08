"""Tests for power_manager.py — power management via mock subprocess/filesystem."""

from unittest.mock import patch, MagicMock, mock_open

import pytest

from power_manager import PowerManager


@pytest.fixture
def pm():
    return PowerManager()


class TestBatteryStatus:
    def test_battery_status(self, pm):
        m = mock_open(read_data="75\n")
        m.side_effect = [
            mock_open(read_data="75\n")(),
            mock_open(read_data="Discharging\n")(),
        ]
        with patch("builtins.open", m):
            result = pm.get_battery_status()
        assert "75" in result

    def test_no_battery(self, pm):
        with patch("builtins.open", side_effect=FileNotFoundError):
            result = pm.get_battery_status()
        assert "no battery" in result.lower()


class TestScheduledActions:
    @patch("power_manager.subprocess.run")
    def test_suspend(self, mock_run, pm):
        result = pm.suspend()
        assert "suspend" in result.lower()
        assert "5 seconds" in result

    @patch("power_manager.subprocess.run")
    def test_hibernate(self, mock_run, pm):
        result = pm.hibernate()
        assert "hibernate" in result.lower()

    @patch("power_manager.subprocess.run")
    def test_shutdown(self, mock_run, pm):
        result = pm.shutdown()
        assert "shutdown" in result.lower()

    @patch("power_manager.subprocess.run")
    def test_reboot(self, mock_run, pm):
        result = pm.reboot()
        assert "reboot" in result.lower()


class TestCancel:
    def test_cancel_nothing(self, pm):
        result = pm.cancel()
        assert "nothing to cancel" in result.lower()

    def test_cancel_pending(self, pm):
        mock_timer = MagicMock()
        mock_timer.is_alive.return_value = True
        pm._pending_timer = mock_timer
        pm._action_pending = True
        result = pm.cancel()
        assert "cancelled" in result.lower()
        mock_timer.cancel.assert_called_once()

    def test_only_one_pending_action(self, pm):
        # A second power action while one is scheduled must be refused.
        mock_timer = MagicMock()
        mock_timer.is_alive.return_value = True
        pm._pending_timer = mock_timer
        pm._action_pending = True
        with patch("power_manager.subprocess.run") as mock_run:
            result = pm.reboot()
        assert "already pending" in result.lower()
        mock_run.assert_not_called()


class TestLockScreen:
    @patch("power_manager.subprocess.run")
    def test_lock_screen_success(self, mock_run):
        mock_run.return_value = MagicMock(returncode=0)
        pm = PowerManager()
        result = pm.lock_screen()
        assert "locked" in result.lower()

    @patch("power_manager.subprocess.run", side_effect=Exception("fail"))
    def test_lock_screen_failure(self, mock_run):
        pm = PowerManager()
        result = pm.lock_screen()
        assert "could not" in result.lower()


class TestPowerInfo:
    def test_power_info(self, pm):
        with patch.object(pm, "get_battery_status", return_value="Battery at 80%"):
            with patch("builtins.open", side_effect=FileNotFoundError):
                result = pm.get_power_info()
        assert "80%" in result
