"""Artume Notification Center — Async notification delivery.

Queues notifications during focus, delivers in batch during idle.
Supports: timer expiry, download complete, email arrived, build finished.
"""

import threading
import time
from dataclasses import dataclass, field
from enum import Enum
from typing import Callable, List, Optional


class NotificationPriority(Enum):
    LOW = 0
    NORMAL = 1
    HIGH = 2
    CRITICAL = 3


class NotificationCategory(Enum):
    TIMER = "timer"
    DOWNLOAD = "download"
    EMAIL = "email"
    BUILD = "build"
    SYSTEM = "system"
    REMINDER = "reminder"
    MESSAGE = "message"


@dataclass
class Notification:
    id: str
    category: NotificationCategory
    title: str
    message: str
    priority: NotificationPriority = NotificationPriority.NORMAL
    timestamp: float = field(default_factory=time.time)
    delivered: bool = False
    action: Optional[Callable] = None


class NotificationCenter:
    """Focus-aware notification delivery system."""

    def __init__(self):
        self._queue: List[Notification] = []
        self._history: List[Notification] = []
        self._focus_mode = False
        self._speak_callback: Optional[Callable[[str], None]] = None
        self._lock = threading.Lock()
        self._next_id = 0

    def set_speak_callback(self, callback: Callable[[str], None]):
        """Set the TTS callback for speaking notifications."""
        self._speak_callback = callback

    def set_focus_mode(self, focused: bool):
        """Set whether the user is in focus mode."""
        self._focus_mode = focused
        if not focused:
            self.flush()

    def notify(self, category: NotificationCategory, title: str, message: str,
               priority: NotificationPriority = NotificationPriority.NORMAL,
               action: Optional[Callable] = None) -> str:
        """Queue a notification."""
        with self._lock:
            self._next_id += 1
            notif = Notification(
                id=f"notif_{self._next_id}",
                category=category,
                title=title,
                message=message,
                priority=priority,
                action=action,
            )
            self._queue.append(notif)
            self._history.append(notif)

        # Deliver immediately if high priority or not in focus mode
        if priority == NotificationPriority.CRITICAL or not self._focus_mode:
            self._deliver(notif)
            return f"Notification: {title}. {message}"
        else:
            return f"Notification queued: {title}. I'll tell you when you're free."

    def flush(self):
        """Deliver all queued notifications."""
        with self._lock:
            pending = [n for n in self._queue if not n.delivered]
            self._queue = [n for n in self._queue if n.delivered]

        if not pending:
            return

        # Group by category for concise delivery
        groups = {}
        for n in pending:
            groups.setdefault(n.category, []).append(n)

        for category, notifs in groups.items():
            if len(notifs) == 1:
                n = notifs[0]
                self._deliver(n)
            else:
                summary = f"You have {len(notifs)} {category.value} notifications. "
                for n in notifs[:3]:
                    summary += f"{n.title}. "
                if len(notifs) > 3:
                    summary += f"And {len(notifs) - 3} more."
                self._deliver(notifs[0])  # Use first for callback
                if self._speak_callback:
                    self._speak_callback(summary)

    def _deliver(self, notif: Notification):
        """Deliver a single notification."""
        notif.delivered = True
        if self._speak_callback:
            text = f"{notif.title}: {notif.message}"
            self._speak_callback(text)

    def get_history(self, limit: int = 10) -> str:
        """Get recent notification history."""
        recent = self._history[-limit:]
        if not recent:
            return "No recent notifications."
        lines = [f"{n.category.value}: {n.title}" for n in recent]
        return "Recent notifications: " + ". ".join(lines)

    def clear_history(self):
        """Clear notification history."""
        self._history.clear()
        self._queue.clear()


# Global singleton
_notification_center: Optional[NotificationCenter] = None


def get_notification_center() -> NotificationCenter:
    global _notification_center
    if _notification_center is None:
        _notification_center = NotificationCenter()
    return _notification_center
