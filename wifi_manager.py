"""Artume WiFi Manager — List, connect, disconnect WiFi networks.

Uses nmcli for network management.
"""

import subprocess
from typing import List, Optional


class WiFiNetwork:
    def __init__(self, ssid: str, signal: str = "", security: str = "", active: bool = False):
        self.ssid = ssid
        self.signal = signal
        self.security = security
        self.active = active


class WiFiManager:
    """Voice-driven WiFi network management."""

    def __init__(self):
        self._networks: List[WiFiNetwork] = []

    def _run(self, cmd: list) -> str:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            return result.stdout.strip()
        except Exception:
            return ""

    def list_networks(self) -> str:
        """List available WiFi networks."""
        output = self._run(["nmcli", "-t", "-f", "SSID,SIGNAL,SECURITY", "dev", "wifi", "list"])
        if not output:
            return "WiFi not available or no networks found."

        self._networks = []
        for line in output.split("\n"):
            parts = line.split(":")
            if len(parts) >= 3 and parts[0]:
                ssid, signal, security = parts[0], parts[1], parts[2]
                self._networks.append(WiFiNetwork(ssid, signal, security))

        if not self._networks:
            return "No WiFi networks found."

        speech = f"Found {len(self._networks)} networks. "
        for n in self._networks[:8]:
            try:
                sig = int(n.signal)
            except (ValueError, TypeError):
                sig = 0
            bars = "strong" if sig > 70 else "medium" if sig > 40 else "weak"
            speech += f"{n.ssid} ({bars} signal). "
        return speech

    def connect(self, ssid: str, password: Optional[str] = None) -> str:
        """Connect to a WiFi network."""
        try:
            if password:
                result = subprocess.run(
                    ["nmcli", "dev", "wifi", "connect", ssid, "password", password],
                    capture_output=True, text=True, timeout=15
                )
            else:
                result = subprocess.run(
                    ["nmcli", "dev", "wifi", "connect", ssid],
                    capture_output=True, text=True, timeout=15
                )
            if "successfully" in result.stdout.lower():
                return f"Connected to {ssid}."
            return f"Failed to connect to {ssid}. {result.stderr[:100]}"
        except Exception as e:
            return f"WiFi error: {str(e)[:50]}"

    def disconnect(self) -> str:
        """Disconnect from current WiFi network."""
        try:
            subprocess.run(
                ["nmcli", "dev", "disconnect", "wifi"],
                capture_output=True, text=True, timeout=10
            )
            return "Disconnected from WiFi."
        except Exception as e:
            return f"WiFi error: {str(e)[:50]}"

    def status(self) -> str:
        """Get current WiFi connection status."""
        output = self._run(["nmcli", "-t", "-f", "ACTIVE,SSID,SIGNAL", "dev", "wifi"])
        for line in output.split("\n"):
            parts = line.split(":")
            if len(parts) >= 3 and parts[0] == "yes":
                return f"Connected to {parts[1]} with {parts[2]}% signal strength."
        return "Not connected to any WiFi network."

    def get_status(self) -> str:
        """Alias for status()."""
        return self.status()

    def turn_on(self) -> str:
        """Enable WiFi."""
        self._run(["nmcli", "radio", "wifi", "on"])
        return "WiFi turned on."

    def turn_off(self) -> str:
        """Disable WiFi."""
        self._run(["nmcli", "radio", "wifi", "off"])
        return "WiFi turned off."

    def save_credentials(self, ssid: str, password: str) -> str:
        """Save WiFi credentials for auto-connect."""
        import tempfile
        import os
        try:
            # Write password to temp file to avoid exposing in ps output
            with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
                f.write(password)
                pw_path = f.name
            try:
                result = subprocess.run(
                    ["nmcli", "connection", "add", "type", "wifi", "con-name", ssid,
                     "ssid", ssid, "wifi-sec.key-mgmt", "wpa-psk",
                     "wifi-sec.psk", f"file:{pw_path}"],
                    capture_output=True, text=True, timeout=10
                )
                if "successfully" in result.stdout.lower():
                    return f"Saved WiFi network {ssid}."
                return f"Failed to save: {result.stderr[:100]}"
            finally:
                os.unlink(pw_path)
        except Exception as e:
            return f"WiFi error: {str(e)[:50]}"


# Global singleton
_wifi: Optional[WiFiManager] = None


def get_wifi() -> WiFiManager:
    global _wifi
    if _wifi is None:
        _wifi = WiFiManager()
    return _wifi
