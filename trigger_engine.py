"""Artume Trigger Engine — First-run, timed, and event-driven triggers.

Supports:
- First-run onboarding (detect first launch, walk through setup)
- Timed/cron triggers (every X minutes, at specific times, one-shot timers)
- Event-driven triggers (file changes, email arrival, system events, process completion)
"""

import json
import os
import threading
import time
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any, Callable, Dict, List, Optional


class TriggerType(Enum):
    FIRST_RUN = "first_run"
    TIMED = "timed"
    CRON = "cron"
    FILE_WATCH = "file_watch"
    EMAIL = "email"
    SYSTEM_EVENT = "system_event"
    PROCESS_COMPLETE = "process_complete"
    IDLE = "idle"


class TriggerPriority(Enum):
    LOW = 0
    NORMAL = 1
    HIGH = 2
    CRITICAL = 3


@dataclass
class Trigger:
    id: str
    trigger_type: TriggerType
    name: str
    description: str
    priority: TriggerPriority = TriggerPriority.NORMAL
    enabled: bool = True
    one_shot: bool = False
    last_fired: Optional[float] = None
    fire_count: int = 0
    cooldown_seconds: float = 0.0
    params: Dict[str, Any] = field(default_factory=dict)
    action: Optional[Callable] = None
    action_name: str = ""


@dataclass
class TriggerEvent:
    trigger_id: str
    trigger_name: str
    trigger_type: TriggerType
    timestamp: float
    data: Dict[str, Any] = field(default_factory=dict)


class TriggerEngine:
    """Central trigger engine for Artume OS."""

    def __init__(self, state_path: Optional[str] = None):
        self.state_path = state_path or os.path.expanduser("~/.config/artume/triggers.json")
        self._triggers: Dict[str, Trigger] = {}
        self._event_callbacks: List[Callable[[TriggerEvent], None]] = []
        self._running = False
        self._timed_thread: Optional[threading.Thread] = None
        self._file_watch_threads: Dict[str, threading.Thread] = {}
        self._lock = threading.Lock()
        self._first_run_detected = False
        self._onboarding_complete = False
        os.makedirs(os.path.dirname(self.state_path), exist_ok=True)
        self._load_state()
        self._register_builtin_triggers()

    def _load_state(self):
        """Load trigger state from disk."""
        try:
            if os.path.exists(self.state_path):
                with open(self.state_path, "r") as f:
                    data = json.load(f)
                self._first_run_detected = data.get("first_run_detected", False)
                self._onboarding_complete = data.get("onboarding_complete", False)
                for t_data in data.get("triggers", []):
                    t = Trigger(
                        id=t_data["id"],
                        trigger_type=TriggerType(t_data["type"]),
                        name=t_data["name"],
                        description=t_data.get("description", ""),
                        priority=TriggerPriority(t_data.get("priority", 1)),
                        enabled=t_data.get("enabled", True),
                        one_shot=t_data.get("one_shot", False),
                        last_fired=t_data.get("last_fired"),
                        fire_count=t_data.get("fire_count", 0),
                        cooldown_seconds=t_data.get("cooldown_seconds", 0),
                        params=t_data.get("params", {}),
                        action_name=t_data.get("action_name", ""),
                    )
                    self._triggers[t.id] = t
        except Exception:
            pass

    def _save_state(self):
        """Save trigger state to disk."""
        try:
            data = {
                "first_run_detected": self._first_run_detected,
                "onboarding_complete": self._onboarding_complete,
                "triggers": [
                    {
                        "id": t.id,
                        "type": t.trigger_type.value,
                        "name": t.name,
                        "description": t.description,
                        "priority": t.priority.value,
                        "enabled": t.enabled,
                        "one_shot": t.one_shot,
                        "last_fired": t.last_fired,
                        "fire_count": t.fire_count,
                        "cooldown_seconds": t.cooldown_seconds,
                        "params": t.params,
                        "action_name": t.action_name,
                    }
                    for t in self._triggers.values()
                ],
            }
            with open(self.state_path, "w") as f:
                json.dump(data, f, indent=2)
        except Exception:
            pass

    def _register_builtin_triggers(self):
        """Register built-in system triggers."""
        builtins = [
            Trigger(
                id="first-run-onboarding",
                trigger_type=TriggerType.FIRST_RUN,
                name="First Run Onboarding",
                description="Welcome wizard on first launch — set up voice PIN, preferences, and learn commands",
                priority=TriggerPriority.HIGH,
                one_shot=True,
                action_name="start_onboarding",
            ),
            Trigger(
                id="daily-briefing",
                trigger_type=TriggerType.CRON,
                name="Daily Morning Briefing",
                description="Time, weather, calendar events, unread emails, system status",
                priority=TriggerPriority.NORMAL,
                params={"cron": "0 8 * * *", "time": "08:00"},
                action_name="daily_briefing",
            ),
            Trigger(
                id="idle-reminder",
                trigger_type=TriggerType.IDLE,
                name="Idle Notification Delivery",
                description="Deliver queued notifications when user goes idle",
                priority=TriggerPriority.LOW,
                params={"idle_seconds": 120},
                action_name="flush_notifications",
            ),
            Trigger(
                id="email-check",
                trigger_type=TriggerType.TIMED,
                name="Periodic Email Check",
                description="Check for new email every 15 minutes",
                priority=TriggerPriority.NORMAL,
                params={"interval_minutes": 15},
                action_name="check_email",
            ),
            Trigger(
                id="system-health",
                trigger_type=TriggerType.TIMED,
                name="System Health Check",
                description="Check battery, disk space, and system status hourly",
                priority=TriggerPriority.LOW,
                params={"interval_minutes": 60},
                action_name="system_health",
            ),
            Trigger(
                id="backup-reminder",
                trigger_type=TriggerType.CRON,
                name="Weekly Backup Reminder",
                description="Remind to back up important files every Sunday evening",
                priority=TriggerPriority.LOW,
                params={"cron": "0 20 * * 0", "time": "Sunday 20:00"},
                action_name="backup_reminder",
            ),
        ]

        for t in builtins:
            if t.id not in self._triggers:
                self._triggers[t.id] = t

    # ====================================================================
    # PUBLIC API
    # ====================================================================

    def register_trigger(self, trigger: Trigger) -> str:
        """Register a new trigger."""
        with self._lock:
            self._triggers[trigger.id] = trigger
            self._save_state()
        return f"Trigger '{trigger.name}' registered."

    def remove_trigger(self, trigger_id: str) -> str:
        """Remove a trigger by ID."""
        with self._lock:
            if trigger_id in self._triggers:
                name = self._triggers[trigger_id].name
                del self._triggers[trigger_id]
                self._save_state()
                return f"Trigger '{name}' removed."
        return f"Trigger '{trigger_id}' not found."

    def enable_trigger(self, trigger_id: str) -> str:
        """Enable a trigger."""
        with self._lock:
            if trigger_id in self._triggers:
                self._triggers[trigger_id].enabled = True
                self._save_state()
                return f"Trigger '{self._triggers[trigger_id].name}' enabled."
        return f"Trigger '{trigger_id}' not found."

    def disable_trigger(self, trigger_id: str) -> str:
        """Disable a trigger."""
        with self._lock:
            if trigger_id in self._triggers:
                self._triggers[trigger_id].enabled = False
                self._save_state()
                return f"Trigger '{self._triggers[trigger_id].name}' disabled."
        return f"Trigger '{trigger_id}' not found."

    def list_triggers(self) -> str:
        """List all registered triggers with status."""
        if not self._triggers:
            return "No triggers registered."

        speech = f"System has {len(self._triggers)} triggers. "
        for t in self._triggers.values():
            status = "active" if t.enabled else "disabled"
            last = ""
            if t.last_fired:
                ago = int((time.time() - t.last_fired) / 60)
                last = f", last fired {ago} minutes ago"
            speech += f"{t.name} ({status}{last}). "
        return speech

    def on_event(self, callback: Callable[[TriggerEvent], None]):
        """Register a callback for trigger events."""
        self._event_callbacks.append(callback)

    def fire(self, trigger_id: str, data: Optional[Dict[str, Any]] = None) -> Optional[TriggerEvent]:
        """Manually fire a trigger."""
        with self._lock:
            trigger = self._triggers.get(trigger_id)
            if not trigger or not trigger.enabled:
                return None

            # Check cooldown
            if trigger.last_fired and trigger.cooldown_seconds > 0:
                elapsed = time.time() - trigger.last_fired
                if elapsed < trigger.cooldown_seconds:
                    return None

            trigger.last_fired = time.time()
            trigger.fire_count += 1

            event = TriggerEvent(
                trigger_id=trigger.id,
                trigger_name=trigger.name,
                trigger_type=trigger.trigger_type,
                timestamp=time.time(),
                data=data or {},
            )

            if trigger.one_shot:
                trigger.enabled = False

            self._save_state()

        # Notify callbacks
        for cb in self._event_callbacks:
            try:
                cb(event)
            except Exception:
                pass

        return event

    # ====================================================================
    # FIRST RUN DETECTION
    # ====================================================================

    def check_first_run(self) -> bool:
        """Check if this is the first run. Returns True if onboarding should start."""
        if not self._first_run_detected:
            self._first_run_detected = True
            self._save_state()
            return True
        return False

    def complete_onboarding(self):
        """Mark onboarding as complete."""
        self._onboarding_complete = True
        self._save_state()

    def is_onboarding_complete(self) -> bool:
        return self._onboarding_complete

    # ====================================================================
    # TIMED / CRON ENGINE
    # ====================================================================

    def start(self):
        """Start the trigger engine background threads."""
        if self._running:
            return
        self._running = True
        self._timed_thread = threading.Thread(target=self._timed_loop, daemon=True)
        self._timed_thread.start()
        # Check first run
        if self.check_first_run():
            self.fire("first-run-onboarding", {"reason": "first_launch"})

    def stop(self):
        """Stop the trigger engine."""
        self._running = False

    def _timed_loop(self):
        """Background loop that checks timed and cron triggers."""
        while self._running:
            try:
                now = time.time()
                now_dt = datetime.now()

                with self._lock:
                    for trigger in list(self._triggers.values()):
                        if not trigger.enabled:
                            continue

                        should_fire = False
                        data = {}

                        if trigger.trigger_type == TriggerType.TIMED:
                            interval = trigger.params.get("interval_minutes", 60) * 60
                            if trigger.last_fired is None:
                                should_fire = True
                            elif (now - trigger.last_fired) >= interval:
                                should_fire = True

                        elif trigger.trigger_type == TriggerType.CRON:
                            cron_expr = trigger.params.get("cron", "")
                            if self._match_cron(cron_expr, now_dt):
                                if trigger.last_fired is None or \
                                   (now - trigger.last_fired) > 60:  # prevent double-fire
                                    should_fire = True

                        elif trigger.trigger_type == TriggerType.IDLE:
                            # Check if user has been idle (no recent input)
                            # This is checked by the main loop, not here
                            pass

                        if should_fire:
                            trigger.last_fired = now
                            trigger.fire_count += 1
                            if trigger.one_shot:
                                trigger.enabled = False
                            event = TriggerEvent(
                                trigger_id=trigger.id,
                                trigger_name=trigger.name,
                                trigger_type=trigger.trigger_type,
                                timestamp=now,
                                data=data,
                            )
                            # Fire in a separate thread to not block the loop
                            threading.Thread(
                                target=self._dispatch_event,
                                args=(event,),
                                daemon=True,
                            ).start()

                self._save_state()
                time.sleep(15)  # Check every 15 seconds

            except Exception:
                time.sleep(15)

    def _match_cron(self, cron_expr: str, dt: datetime) -> bool:
        """Simple cron expression matcher (minute hour day-of-month month day-of-week)."""
        try:
            parts = cron_expr.strip().split()
            if len(parts) != 5:
                return False

            minute, hour, dom, month, dow = parts

            def match_field(field: str, value: int) -> bool:
                if field == "*":
                    return True
                if "/" in field:
                    base, step = field.split("/")
                    if value % int(step) == 0:
                        return True
                    return False
                if "," in field:
                    return str(value) in field.split(",")
                return str(value) == field

            # Python weekday(): Mon=0..Sun=6; cron dow: Sun=0..Sat=6
            cron_dow = (dt.weekday() + 1) % 7
            return (match_field(minute, dt.minute) and
                    match_field(hour, dt.hour) and
                    match_field(dom, dt.day) and
                    match_field(month, dt.month) and
                    match_field(dow, cron_dow))
        except Exception:
            return False

    def _dispatch_event(self, event: TriggerEvent):
        """Dispatch a trigger event to all callbacks."""
        for cb in self._event_callbacks:
            try:
                cb(event)
            except Exception:
                pass

    # ====================================================================
    # FILE WATCH TRIGGERS
    # ====================================================================

    def watch_file(self, path: str, trigger_id: str = "file-changed"):
        """Watch a file or directory for changes using polling."""
        path = os.path.expanduser(path)
        if not os.path.exists(path):
            return f"Path '{path}' does not exist."

        trigger = Trigger(
            id=trigger_id,
            trigger_type=TriggerType.FILE_WATCH,
            name=f"File Watch: {os.path.basename(path)}",
            description=f"Watch for changes to {path}",
            params={"path": path},
            action_name="file_changed",
        )
        self._triggers[trigger.id] = trigger
        self._save_state()

        # Start watch thread
        thread = threading.Thread(
            target=self._file_watch_loop,
            args=(path, trigger.id),
            daemon=True,
        )
        self._file_watch_threads[trigger.id] = thread
        thread.start()

        return f"Watching '{path}' for changes."

    def _file_watch_loop(self, path: str, trigger_id: str):
        """Poll a file/directory for changes."""
        try:
            if os.path.isfile(path):
                last_mtime = os.path.getmtime(path)
                while self._running:
                    time.sleep(5)
                    if os.path.exists(path):
                        mtime = os.path.getmtime(path)
                        if mtime != last_mtime:
                            last_mtime = mtime
                            self.fire(trigger_id, {"path": path, "change": "modified"})
            elif os.path.isdir(path):
                last_files = set(os.listdir(path))
                while self._running:
                    time.sleep(5)
                    if os.path.exists(path):
                        current_files = set(os.listdir(path))
                        if current_files != last_files:
                            new_files = current_files - last_files
                            last_files = current_files
                            data = {"path": path, "new_files": list(new_files)}
                            self.fire(trigger_id, data)
        except Exception:
            pass

    # ====================================================================
    # ONBOARDING FLOW
    # ====================================================================

    def get_onboarding_steps(self) -> List[Dict[str, str]]:
        """Get the onboarding wizard steps."""
        return [
            {
                "id": "welcome",
                "title": "Welcome to Artume OS",
                "text": "Welcome to Artume, your audio-first operating system. "
                        "I'll guide you through setup. Say 'next' to continue, or 'skip' to skip a step.",
            },
            {
                "id": "credential_vault",
                "title": "Create Your Voice PIN",
                "text": "First, let's set up your credential vault. "
                        "This stores your email and WiFi passwords securely. "
                        "Choose a PIN you'll remember — you'll say it to unlock the vault. "
                        "Say your PIN now, or say 'skip' to do this later.",
            },
            {
                "id": "tts_preference",
                "title": "Voice Preference",
                "text": "I can speak at different speeds. "
                        "Say 'normal speed', 'slow', or 'fast' to set your preference. "
                        "Or say 'skip' to keep the default.",
            },
            {
                "id": "email_setup",
                "title": "Email Setup",
                "text": "Would you like to set up your email? "
                        "I'll need your email address, IMAP server, and SMTP server. "
                        "Say 'yes' to set up email, or 'skip' to do this later.",
            },
            {
                "id": "wifi_setup",
                "title": "WiFi Setup",
                "text": "Would you like to connect to a WiFi network? "
                        "Say 'scan' to see available networks, or 'skip' to do this later.",
            },
            {
                "id": "commands_tutorial",
                "title": "Learning Commands",
                "text": "Here are some things you can say: "
                        "'what's my status' for system info. "
                        "'open browser' to search the web. "
                        "'check my email' to read messages. "
                        "'open files' to browse your documents. "
                        "'what can I say' for a full list of commands. "
                        "Say 'next' to continue.",
            },
            {
                "id": "complete",
                "title": "Setup Complete",
                "text": "You're all set! Artume is ready to use. "
                        "Remember, you can always say 'help' or 'what can I say' "
                        "to discover new commands. Welcome aboard.",
            },
        ]


# ====================================================================
# GLOBAL SINGLETON
# ====================================================================

_trigger_engine: Optional[TriggerEngine] = None


def get_trigger_engine() -> TriggerEngine:
    global _trigger_engine
    if _trigger_engine is None:
        _trigger_engine = TriggerEngine()
    return _trigger_engine
