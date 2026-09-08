"""Tests for earcons.py — sound generation and playback."""

import os
import wave
import struct
import subprocess
from unittest.mock import patch, MagicMock

import pytest

import earcons


@pytest.fixture(autouse=True)
def _reset_earcon_proc():
    """Reset global state before each test."""
    earcons._current_earcon_proc = None
    yield
    earcons._current_earcon_proc = None


class TestGenerateWave:
    def test_creates_wav_file(self, tmp_path):
        filepath = os.path.join(str(tmp_path), "test_sound.wav")
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            result = earcons.generate_wave("test_sound.wav", [440.0], duration_sec=0.05)
        assert os.path.exists(result)
        assert result.endswith("test_sound.wav")

    def test_wav_is_valid_mono_16bit(self, tmp_path):
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            filepath = earcons.generate_wave("valid.wav", [440.0], duration_sec=0.05)
        with wave.open(filepath, "r") as wf:
            assert wf.getnchannels() == 1
            assert wf.getsampwidth() == 2
            assert wf.getframerate() == earcons.SAMPLE_RATE
            assert wf.getnframes() > 0

    def test_returns_existing_file_without_overwriting(self, tmp_path):
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            first = earcons.generate_wave("cached.wav", [440.0], duration_sec=0.05)
            mtime1 = os.path.getmtime(first)
            second = earcons.generate_wave("cached.wav", [880.0], duration_sec=0.10)
            mtime2 = os.path.getmtime(second)
        assert first == second
        assert mtime1 == mtime2

    def test_multiple_frequencies(self, tmp_path):
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            filepath = earcons.generate_wave("chord.wav", [261.63, 329.63, 392.00], duration_sec=0.05)
        with wave.open(filepath, "r") as wf:
            assert wf.getnframes() == int(earcons.SAMPLE_RATE * 0.05)

    def test_fade_disabled(self, tmp_path):
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            filepath = earcons.generate_wave("nofade.wav", [440.0], duration_sec=0.05, fade=False)
        assert os.path.exists(filepath)


class TestInitEarcons:
    def test_generates_system_sounds(self, tmp_path):
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            earcons.init_earcons()
        expected = ["listening.wav", "thinking.wav", "success.wav", "error.wav"]
        for name in expected:
            assert os.path.exists(os.path.join(str(tmp_path), name))


class TestPlayEarcon:
    @patch("earcons.subprocess.Popen")
    def test_play_earcon_spawns_aplay(self, mock_popen, tmp_path):
        wav_path = os.path.join(str(tmp_path), "listening.wav")
        with open(wav_path, "wb") as f:
            f.write(b"RIFF" + b"\x00" * 40)
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            earcons.play_earcon("listening")
        mock_popen.assert_called_once()
        cmd = mock_popen.call_args[0][0]
        assert cmd[0] == "aplay"
        assert "-q" in cmd

    @patch("earcons.subprocess.Popen")
    def test_play_earcon_terminates_previous(self, mock_popen, tmp_path):
        wav_path = os.path.join(str(tmp_path), "success.wav")
        with open(wav_path, "wb") as f:
            f.write(b"RIFF" + b"\x00" * 40)
        mock_proc = MagicMock()
        mock_proc.poll.return_value = None
        earcons._current_earcon_proc = mock_proc
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            earcons.play_earcon("success")
        mock_proc.terminate.assert_called_once()

    @patch("earcons.subprocess.Popen")
    @patch("earcons.init_earcons")
    def test_play_earcon_inits_if_missing(self, mock_init, mock_popen, tmp_path):
        with patch.object(earcons, "SOUNDS_DIR", str(tmp_path)):
            earcons.play_earcon("nonexistent")
        mock_init.assert_called_once()
