"""Tests for audio_output_switcher.py — PulseAudio sink switching."""

from unittest.mock import patch, MagicMock

import pytest

from audio_output_switcher import AudioOutputSwitcher, AudioSink


@pytest.fixture
def aos():
    return AudioOutputSwitcher()


MOCK_PACTL_SHORT = "0\talsa_output.pci-0000_00_1f.3.analog\tanalog-output-speaker\n1\talsa_output.usb-headset\theadphone-output"
MOCK_PACTL_LONG = "Sink #0\n\tState: RUNNING\n\tDescription: Built-in Audio\nSink #1\n\tDescription: USB Headphone"


class TestListSinks:
    @patch.object(AudioOutputSwitcher, "_run")
    def test_list_sinks(self, mock_run, aos):
        mock_run.side_effect = [MOCK_PACTL_SHORT, MOCK_PACTL_LONG]
        result = aos.list_sinks()
        assert "2 audio outputs" in result or "Built-in" in result

    @patch.object(AudioOutputSwitcher, "_run")
    def test_list_empty(self, mock_run, aos):
        mock_run.return_value = ""
        result = aos.list_sinks()
        assert "not available" in result.lower()

    @patch.object(AudioOutputSwitcher, "_run")
    def test_list_no_sinks(self, mock_run, aos):
        mock_run.return_value = "bad data\n"
        result = aos.list_sinks()
        assert "no audio outputs" in result.lower()


class TestSwitchTo:
    @patch.object(AudioOutputSwitcher, "_run")
    @patch("audio_output_switcher.subprocess.run")
    def test_switch_by_name(self, mock_run_sub, mock_run, aos):
        mock_run.side_effect = [MOCK_PACTL_SHORT, ""]
        result = aos.switch_to("usb")
        assert "switched" in result.lower() or "usb" in result.lower()

    @patch.object(AudioOutputSwitcher, "_run")
    def test_switch_not_found(self, mock_run, aos):
        mock_run.side_effect = [MOCK_PACTL_SHORT, ""]
        result = aos.switch_to("nonexistent")
        assert "not found" in result.lower()


class TestCurrent:
    @patch.object(AudioOutputSwitcher, "_run")
    def test_current(self, mock_run, aos):
        mock_run.return_value = "Default Sink: alsa_output.pci.analog"
        result = aos.current()
        assert "alsa_output.pci.analog" in result

    @patch.object(AudioOutputSwitcher, "_run")
    def test_current_unknown(self, mock_run, aos):
        mock_run.return_value = ""
        result = aos.current()
        assert "could not" in result.lower()
