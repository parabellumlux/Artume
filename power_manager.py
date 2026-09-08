import subprocess
import threading

_instance = None
_lock = threading.Lock()


class PowerManager:
    def __init__(self):
        self._pending_timer = None
        self._action_pending = False

    def get_battery_status(self) -> str:
        try:
            with open("/sys/class/power_supply/BAT0/capacity") as f:
                cap = f.read().strip()
            with open("/sys/class/power_supply/BAT0/status") as f:
                st = f.read().strip().lower()
            sm = {"charging": "charging", "discharging": "discharging",
                  "full": "fully charged", "not charging": "not charging"}
            return f"Battery at {cap} percent, {sm.get(st, st)}"
        except FileNotFoundError:
            return "No battery detected."

    def suspend(self) -> str:
        return self._schedule("systemctl suspend", "Suspend")

    def hibernate(self) -> str:
        return self._schedule("systemctl hibernate", "Hibernate")

    def shutdown(self) -> str:
        return self._schedule("systemctl poweroff", "Shutdown")

    def reboot(self) -> str:
        return self._schedule("systemctl reboot", "Reboot")

    def lock_screen(self) -> str:
        for cmd in ["loginctl lock-session", "xdg-screensaver lock"]:
            try:
                r = subprocess.run(cmd.split(), timeout=10, capture_output=True)
                if r.returncode == 0:
                    return "Screen locked."
            except (FileNotFoundError, subprocess.TimeoutExpired):
                continue
        return "Could not lock screen."

    def cancel(self) -> str:
        with _lock:
            if self._action_pending:
                if self._pending_timer and self._pending_timer.is_alive():
                    self._pending_timer.cancel()
                self._pending_timer = None
                self._action_pending = False
                return "Action cancelled."
        return "Nothing to cancel."

    def get_power_info(self) -> str:
        parts = [self.get_battery_status()]
        try:
            with open("/sys/class/power_supply/AC/online") as f:
                ac = f.read().strip()
            parts.append("AC adapter " + ("connected" if ac == "1" else "disconnected"))
        except FileNotFoundError:
            pass
        try:
            with open("/sys/class/backlight/acpi_video0/brightness") as f:
                bl = f.read().strip()
            parts.append(f"Brightness: {bl}")
        except FileNotFoundError:
            pass
        return ", ".join(parts) + "."

    def _schedule(self, cmd: str, label: str) -> str:
        with _lock:
            if self._action_pending:
                return "Another power action is already pending. Say 'cancel' first."
            self._action_pending = True

            def _run():
                try:
                    subprocess.run(cmd.split(), timeout=10)
                finally:
                    with _lock:
                        self._pending_timer = None
                        self._action_pending = False

            self._pending_timer = threading.Timer(5.0, _run)
            self._pending_timer.daemon = True
            self._pending_timer.start()
        return f"{label}ing in 5 seconds. Say 'cancel' to abort."


def get_power_manager() -> PowerManager:
    global _instance
    if _instance is None:
        _instance = PowerManager()
    return _instance
