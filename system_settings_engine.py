#!/usr/bin/env python3
"""Artome Audio System Settings, Notifications & Status Center Engine."""

import os
import time
import subprocess
import threading

class AudioSystemSettings:
    """Voice settings manager: Volume controls, battery %, WiFi, and Timers."""

    def __init__(self):
        self.speech_rate = 1.0

    def set_volume(self, level_pct):
        """Set system master volume percentage (0 - 100)."""
        try:
            subprocess.run(["amixer", "set", "Master", f"{level_pct}%"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return f"Master volume set to {level_pct} percent."
        except Exception as e:
            return f"Failed to set volume: {str(e)[:40]}"

    def volume_up(self):
        """Increase master volume by 10%."""
        try:
            subprocess.run(["amixer", "set", "Master", "10%+"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return "Volume increased by 10 percent."
        except Exception:
            return "Volume adjustment failed."

    def volume_down(self):
        """Decrease master volume by 10%."""
        try:
            subprocess.run(["amixer", "set", "Master", "10%-"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return "Volume decreased by 10 percent."
        except Exception:
            return "Volume adjustment failed."

    def get_system_status_audio(self):
        """Spoken summary of time, battery, volume, and WiFi network."""
        status_items = []

        # Current Time
        time_str = time.strftime("%I:%M %p, %A %B %d")
        status_items.append(f"Time is {time_str}")

        # Battery % check
        try:
            if os.path.exists("/sys/class/power_supply/BAT0/capacity"):
                with open("/sys/class/power_supply/BAT0/capacity", "r") as f:
                    bat = f.read().strip()
                status_items.append(f"Battery is at {bat} percent")
        except Exception:
            pass

        # WiFi network check
        try:
            wifi_out = subprocess.check_output("nmcli -t -f active,ssid dev wifi | grep '^yes'",
                                               shell=True, text=True, stderr=subprocess.STDOUT)
            ssid = wifi_out.split(":")[-1].strip()
            if ssid:
                status_items.append(f"Connected to WiFi network {ssid}")
        except Exception:
            status_items.append("WiFi status unavailable")

        return ". ".join(status_items) + "."

    def set_timer(self, minutes, callback_tts=None):
        """Set an audio timer in minutes."""
        def _timer_thread():
            time.sleep(minutes * 60)
            if callback_tts:
                callback_tts.speak(f"Timer alert: {minutes} minutes have elapsed!")

        t = threading.Thread(target=_timer_thread, daemon=True)
        t.start()
        return f"Timer set for {minutes} minutes."

if __name__ == "__main__":
    sys_settings = AudioSystemSettings()
    print("Testing System Status Audio...")
    print(sys_settings.get_system_status_audio())
