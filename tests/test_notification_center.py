"""Tests for notification_center.py — async notification delivery."""

import threading
from unittest.mock import MagicMock, call

import pytest

from notification_center import (
    NotificationCenter,
    Notification,
    NotificationCategory,
    NotificationPriority,
    get_notification_center,
)


@pytest.fixture
def nc():
    return NotificationCenter()


class TestNotify:
    def test_notify_immediately_when_not_focused(self, nc):
        speak = MagicMock()
        nc.set_speak_callback(speak)
        nc.set_focus_mode(False)
        result = nc.notify(NotificationCategory.TIMER, "Timer", "Done")
        assert "timer" in result.lower() or "Timer" in result
        speak.assert_called_once()

    def test_notify_queued_when_focused(self, nc):
        speak = MagicMock()
        nc.set_speak_callback(speak)
        nc.set_focus_mode(True)
        result = nc.notify(NotificationCategory.EMAIL, "Email", "New mail")
        assert "queued" in result.lower()
        speak.assert_not_called()

    def test_critical_notifies_even_when_focused(self, nc):
        speak = MagicMock()
        nc.set_speak_callback(speak)
        nc.set_focus_mode(True)
        result = nc.notify(
            NotificationCategory.SYSTEM, "Alert", "Disk full",
            priority=NotificationPriority.CRITICAL
        )
        speak.assert_called_once()

    def test_notify_increments_id(self, nc):
        r1 = nc.notify(NotificationCategory.TIMER, "A", "msg")
        r2 = nc.notify(NotificationCategory.TIMER, "B", "msg")
        assert "notif_1" in str(nc._history[0].id)
        assert "notif_2" in str(nc._history[1].id)


class TestFocusMode:
    def test_exit_focus_flushes(self, nc):
        speak = MagicMock()
        nc.set_speak_callback(speak)
        nc.set_focus_mode(True)
        nc.notify(NotificationCategory.EMAIL, "Email", "msg1")
        nc.notify(NotificationCategory.EMAIL, "Email", "msg2")
        speak.reset_mock()
        nc.set_focus_mode(False)
        assert speak.call_count >= 1

    def test_enter_focus_does_not_flush(self, nc):
        speak = MagicMock()
        nc.set_speak_callback(speak)
        nc.set_focus_mode(False)
        speak.reset_mock()
        nc.set_focus_mode(True)
        speak.assert_not_called()


class TestFlush:
    def test_flush_delivers_pending(self, nc):
        speak = MagicMock()
        nc.set_speak_callback(speak)
        nc.set_focus_mode(True)
        nc.notify(NotificationCategory.TIMER, "Timer", "Done")
        nc.notify(NotificationCategory.TIMER, "Timer", "Done2")
        speak.reset_mock()
        nc.flush()
        assert speak.call_count >= 1

    def test_flush_empty_queue(self, nc):
        nc.flush()
        assert len(nc._queue) == 0


class TestHistory:
    def test_get_history(self, nc):
        nc.notify(NotificationCategory.TIMER, "Timer", "Done")
        result = nc.get_history()
        assert "Timer" in result

    def test_get_history_empty(self, nc):
        result = nc.get_history()
        assert "no" in result.lower()

    def test_clear_history(self, nc):
        nc.notify(NotificationCategory.TIMER, "Timer", "Done")
        nc.clear_history()
        assert len(nc._history) == 0
        assert len(nc._queue) == 0

    def test_history_limit(self, nc):
        for i in range(15):
            nc.notify(NotificationCategory.TIMER, f"Timer{i}", "msg")
        result = nc.get_history(limit=5)
        assert "Timer" in result


class TestSingleton:
    def test_get_notification_center_singleton(self):
        nc1 = get_notification_center()
        nc2 = get_notification_center()
        assert nc1 is nc2
