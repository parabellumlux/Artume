"""Tests for audio_engine.py — PiperTTS and VAD listener init."""

from unittest.mock import patch, MagicMock

import pytest

from audio_engine import PiperTTS, DynamicVADListener, SpatialAudioEngine


class TestPiperTTS:
    def test_init(self):
        tts = PiperTTS()
        assert tts.speech_process is None
        assert tts.aplay_process is None
        assert tts.is_speaking is False

    def test_stop(self):
        tts = PiperTTS()
        mock_speech = MagicMock()
        mock_speech.poll.return_value = None
        mock_aplay = MagicMock()
        mock_aplay.poll.return_value = None
        tts.speech_process = mock_speech
        tts.aplay_process = mock_aplay
        with patch("audio_engine.subprocess.run"):
            tts.stop()
        assert tts.is_speaking is False

    @patch("audio_engine.subprocess.Popen")
    @patch("audio_engine.subprocess.run")
    def test_speak_blocks(self, mock_run, mock_popen):
        tts = PiperTTS()
        mock_proc = MagicMock()
        mock_proc.communicate.return_value = (b"\x00" * 100, b"")
        mock_popen.return_value = mock_proc
        tts.speak("hello world", block=True)
        assert tts.is_speaking is False

    def test_speak_empty_text(self):
        tts = PiperTTS()
        tts.speak("")
        tts.speak("   ")
        assert tts.is_speaking is False


class TestDynamicVADListener:
    def test_init_defaults(self):
        listener = DynamicVADListener()
        assert listener.sample_rate == 16000
        assert listener.chunk_ms == 50
        assert listener.energy_threshold == 0.018

    def test_init_custom(self):
        listener = DynamicVADListener(
            sample_rate=22050, chunk_ms=100, energy_threshold=0.025
        )
        assert listener.sample_rate == 22050
        assert listener.chunk_ms == 100
        assert listener.energy_threshold == 0.025

    def test_init_computes_chunk_samples(self):
        listener = DynamicVADListener(sample_rate=16000, chunk_ms=50)
        assert listener.chunk_samples == 800

    def test_init_computes_silence_threshold(self):
        listener = DynamicVADListener(chunk_ms=50, silence_threshold_ms=800)
        assert listener.silence_threshold_chunks == 16


class TestSpatialAudioEngine:
    def test_pan_to_gains_center(self):
        left, right = SpatialAudioEngine._pan_to_gains(0.0)
        assert left == 0.5
        assert right == 0.5

    def test_pan_to_gains_left(self):
        left, right = SpatialAudioEngine._pan_to_gains(-1.0)
        assert left == 1.0
        assert right == 0.0

    def test_pan_to_gains_right(self):
        left, right = SpatialAudioEngine._pan_to_gains(1.0)
        assert left == 0.0
        assert right == 1.0

    def test_pan_to_gains_clamp(self):
        left, right = SpatialAudioEngine._pan_to_gains(5.0)
        assert left == 0.0
        assert right == 1.0

    def test_auto_pan_known(self):
        assert SpatialAudioEngine._auto_pan("scope_enter_function") == -0.3
        assert SpatialAudioEngine._auto_pan("conditional") == 0.6
        assert SpatialAudioEngine._auto_pan("listening") == 0.0

    def test_auto_pan_unknown(self):
        assert SpatialAudioEngine._auto_pan("unknown_earcon") == 0.0

    def test_auto_pan_prefix_match(self):
        pan = SpatialAudioEngine._auto_pan("scope_enter_function_extra")
        assert pan == -0.3
