"""Artume Bluetooth Manager — List, connect, disconnect Bluetooth devices.

Uses bluetoothctl for Bluetooth device management.
"""

import subprocess
from typing import List, Optional


class BluetoothDevice:
    def __init__(self, mac: str, name: str, connected: bool = False):
        self.mac = mac
        self.name = name
        self.connected = connected


class BluetoothManager:
    """Voice-driven Bluetooth device management."""

    def __init__(self):
        self._devices: List[BluetoothDevice] = []

    def _run(self, cmd: list) -> str:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            return result.stdout.strip()
        except Exception:
            return ""

    def list_devices(self) -> str:
        """List paired Bluetooth devices."""
        output = self._run(["bluetoothctl", "devices"])
        if not output:
            return "No Bluetooth devices found or Bluetooth is unavailable."

        self._devices = []
        for line in output.split("\n"):
            parts = line.split(None, 2)
            if len(parts) >= 3 and parts[0] == "Device":
                mac, name = parts[1], parts[2]
                self._devices.append(BluetoothDevice(mac, name))

        # Check connected status for each device
        for d in self._devices:
            info_output = self._run(["bluetoothctl", "info", d.mac])
            if "Connected: yes" in info_output:
                d.connected = True

        if not self._devices:
            return "No paired Bluetooth devices found."

        speech = f"Found {len(self._devices)} Bluetooth devices. "
        for d in self._devices[:8]:
            status = "connected" if d.connected else "disconnected"
            speech += f"{d.name} ({status}). "
        return speech

    def connect(self, name: str) -> str:
        """Connect to a Bluetooth device by name."""
        if not self._devices:
            self.list_devices()

        # Find device
        target = None
        for d in self._devices:
            if name.lower() in d.name.lower():
                target = d
                break

        if not target:
            return f"Bluetooth device '{name}' not found."

        try:
            result = subprocess.run(
                ["bluetoothctl", "connect", target.mac],
                capture_output=True, text=True, timeout=15
            )
            if "Connection successful" in result.stdout:
                target.connected = True
                return f"Connected to {target.name}."
            return f"Failed to connect to {target.name}."
        except subprocess.TimeoutExpired:
            return f"Connection to {target.name} timed out."
        except Exception as e:
            return f"Bluetooth error: {str(e)[:50]}"

    def disconnect(self, name: str) -> str:
        """Disconnect from a Bluetooth device by name."""
        if not self._devices:
            self.list_devices()

        target = None
        for d in self._devices:
            if name.lower() in d.name.lower():
                target = d
                break

        if not target:
            return f"Bluetooth device '{name}' not found."

        try:
            subprocess.run(
                ["bluetoothctl", "disconnect", target.mac],
                capture_output=True, text=True, timeout=10
            )
            target.connected = False
            return f"Disconnected from {target.name}."
        except Exception as e:
            return f"Bluetooth error: {str(e)[:50]}"

    def scan(self) -> str:
        """Scan for new Bluetooth devices."""
        try:
            result = subprocess.run(
                ["bluetoothctl", "scan", "on"],
                capture_output=True, text=True, timeout=8
            )
            output = result.stdout
            devices = set()
            for line in output.split("\n"):
                if "Device" in line:
                    parts = line.split(None, 2)
                    if len(parts) >= 3:
                        devices.add(parts[2])
            if devices:
                return f"Found {len(devices)} devices: {', '.join(list(devices)[:5])}."
            return "No new devices found."
        except Exception as e:
            return f"Bluetooth scan error: {str(e)[:50]}"

    def pair(self, name: str) -> str:
        """Pair with a new Bluetooth device."""
        # First scan
        self._run(["bluetoothctl", "scan", "on"])
        # Then try to pair by name
        try:
            result = subprocess.run(
                ["bluetoothctl", "pair", name],
                capture_output=True, text=True, timeout=15
            )
            if "Pairing successful" in result.stdout:
                return f"Paired with {name}."
            return f"Pairing with {name} failed."
        except Exception as e:
            return f"Bluetooth pairing error: {str(e)[:50]}"


# Global singleton
_bluetooth: Optional[BluetoothManager] = None


def get_bluetooth() -> BluetoothManager:
    global _bluetooth
    if _bluetooth is None:
        _bluetooth = BluetoothManager()
    return _bluetooth
