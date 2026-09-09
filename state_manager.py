"""Persistent state manager for Artume OS - saves/loads application state across restarts."""
import copy, json, os, threading, time
from pathlib import Path

_DEFAULT_STATE = {"mode": "DESKTOP", "active_file": None, "recent_files": [], "bookmarks": {}, "preferences": {}}

class StateManager:
    _instance = None
    _lock_class = threading.Lock()

    def __new__(cls, *args, **kwargs):
        with cls._lock_class:
            if cls._instance is None:
                cls._instance = super().__new__(cls)
            return cls._instance

    def __init__(self, state_dir: str = None):
        if hasattr(self, "_initialized"):
            return
        self._initialized = True
        self._lock = threading.RLock()
        self._state_dir = Path(state_dir or os.path.expanduser("~/.config/artume/state/"))
        self._state_file = self._state_dir / "app_state.json"
        self._state = copy.deepcopy(_DEFAULT_STATE)
        self._last_save = 0.0
        self._save_timer = None
        self._state_dir.mkdir(parents=True, exist_ok=True)

    def _serialize(self, obj):
        return str(obj)

    def _save_now(self):
        with self._lock:
            self._last_save = time.time()
            try:
                self._state_file.write_text(json.dumps(self._state, indent=2, default=self._serialize))
            except OSError:
                pass

    def _schedule_save(self):
        with self._lock:
            elapsed = time.time() - self._last_save
            if elapsed >= 5.0:
                self._save_now()
            elif self._save_timer is None or not self._save_timer.is_alive():
                self._save_timer = threading.Timer(5.0, self._save_now)
                self._save_timer.daemon = True
                self._save_timer.start()

    def save(self):
        if self._save_timer and self._save_timer.is_alive():
            self._save_timer.cancel()
        self._save_now()

    def load(self) -> bool:
        if not self._state_file.exists():
            return False
        try:
            with self._lock:
                self._state = json.loads(self._state_file.read_text())
            return True
        except (json.JSONDecodeError, OSError):
            return False

    def get(self, key: str, default=None):
        with self._lock:
            return self._state.get(key, default)

    def set(self, key: str, value):
        with self._lock:
            self._state[key] = value
        self._schedule_save()

    def get_mode(self) -> str:
        return self.get("mode", "DESKTOP")

    def set_mode(self, mode: str):
        self.set("mode", mode)

    def get_active_file(self) -> str:
        return self.get("active_file")

    def set_active_file(self, path: str):
        self.set("active_file", path)
        self.add_recent_file(path)

    def add_recent_file(self, path: str):
        with self._lock:
            recent = [f for f in self._state.get("recent_files", []) if f != path]
            recent.insert(0, path)
            self._state["recent_files"] = recent[:20]
        self._schedule_save()

    def get_recent_files(self) -> list:
        return self.get("recent_files", [])

    def add_bookmark(self, name: str, path: str, line: int = 0):
        with self._lock:
            self._state.setdefault("bookmarks", {})[name] = {"path": path, "line": line}
        self._schedule_save()

    def get_bookmarks(self) -> dict:
        return self.get("bookmarks", {})

    def remove_bookmark(self, name: str):
        with self._lock:
            self._state.get("bookmarks", {}).pop(name, None)
        self._schedule_save()

    def get_preference(self, key: str, default=None):
        return self.get("preferences", {}).get(key, default)

    def set_preference(self, key: str, value):
        with self._lock:
            self._state.setdefault("preferences", {})[key] = value
        self._schedule_save()

_state_manager = None
_sm_lock = threading.Lock()

def get_state_manager(state_dir: str = None) -> StateManager:
    global _state_manager
    with _sm_lock:
        if _state_manager is None:
            _state_manager = StateManager(state_dir)
        return _state_manager
