"""Tests for wakeword_engine — env-configurable activation phrases."""

import os

from wakeword_engine import DEFAULT_WAKE_WORDS, WakeWordDetector


class TestDefaultPhrases:
    def test_default_list_has_artome(self):
        det = WakeWordDetector()
        assert "artome" in det.wake_words
        assert det.wake_words == DEFAULT_WAKE_WORDS

    def test_activate_and_strip_command(self):
        det = WakeWordDetector()
        ok, cmd = det.check_wake_word("hey artome open firefox")
        assert ok is True
        assert cmd == "open firefox"

    def test_not_activated_without_phrase(self):
        det = WakeWordDetector()
        ok, cmd = det.check_wake_word("open firefox")
        assert ok is False

    def test_disabled_always_listens(self):
        det = WakeWordDetector()
        det.toggle_wake_word(False)
        ok, cmd = det.check_wake_word("open firefox")
        assert ok is True
        assert cmd == "open firefox"


class TestEnvOverride:
    def test_custom_list_from_env(self, monkeypatch):
        monkeypatch.setenv("ARTUME_WAKE_WORDS", "hey artume, jarvis")
        det = WakeWordDetector()
        assert det.wake_words == ["hey artume", "jarvis"]

    def test_env_ignored_when_explicit_list_given(self, monkeypatch):
        monkeypatch.setenv("ARTUME_WAKE_WORDS", "foo")
        det = WakeWordDetector(wake_words=["bar"])
        assert det.wake_words == ["bar"]

    def test_blank_env_uses_defaults(self, monkeypatch):
        monkeypatch.setenv("ARTUME_WAKE_WORDS", "   ")
        det = WakeWordDetector()
        assert det.wake_words == DEFAULT_WAKE_WORDS