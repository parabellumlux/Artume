"""Tests for intent_router.py — keyword classification without LLM calls."""

from unittest.mock import patch, MagicMock

import pytest

from intent_router import (
    _normalize_label,
    _extract_after,
    _dispatch,
    _classify_prompt,
    INTENT_LABELS,
)


class TestNormalizeLabel:
    def test_exact_match(self):
        assert _normalize_label("conversation") == "conversation"
        assert _normalize_label("system_command") == "system_command"

    def test_case_insensitive(self):
        assert _normalize_label("Web_Fetch") == "web_fetch"

    def test_strips_punctuation(self):
        assert _normalize_label("file_search.") == "file_search"

    def test_fuzzy_embedded(self):
        assert _normalize_label("the label is execute_action") == "execute_action"

    def test_unknown(self):
        assert _normalize_label("totally random garbage xyz") == "unknown"


class TestExtractAfter:
    def test_basic(self):
        assert _extract_after("open firefox", ["open"]) == "firefox"

    def test_multiple_prefixes(self):
        result = _extract_after("read me the page", ["read me", "read", "fetch"])
        assert result == "the page"

    def test_no_match(self):
        assert _extract_after("hello world", ["open", "close"]) == "hello world"


class TestDispatch:
    def test_switch_mode(self):
        result = _dispatch("switch_mode", "switch to browser", "DESKTOP")
        assert result["action"] == "switch_mode"
        assert "BROWSER" in result["target"]

    def test_entity_lookup(self):
        result = _dispatch("entity_lookup", "copy that tracking number", "DESKTOP")
        assert result["action"] == "speak"
        assert "entity" in result["target"]

    def test_web_fetch(self):
        result = _dispatch("web_fetch", "read me https://example.com", "DESKTOP")
        assert result["action"] == "web_navigate"

    def test_file_search(self):
        result = _dispatch("file_search", "find my notes", "DESKTOP")
        assert result["action"] == "file_action"
        assert "notes" in result["target"]

    def test_volume_up(self):
        result = _dispatch("system_command", "volume up", "DESKTOP")
        assert result["action"] == "setting_action"
        assert "volume_up" in result["target"]

    def test_open_app(self):
        result = _dispatch("execute_action", "open firefox", "DESKTOP")
        assert result["action"] == "open_app"
        assert "firefox" in result["target"]

    def test_conversation(self):
        result = _dispatch("conversation", "hello there", "DESKTOP")
        assert result["action"] == "speak"
        assert "hello" in result["speech"].lower()

    def test_help(self):
        result = _dispatch("", "help", "DESKTOP")
        assert result["action"] == "speak"
        assert "help" in result["target"]

    def test_default_fallback(self):
        result = _dispatch("", "asdfghjkl", "DESKTOP")
        assert result["action"] == "speak"
        assert "not sure" in result["speech"].lower() or "heard" in result["speech"].lower()


class TestModeKeywords:
    def test_browser_search(self):
        from intent_router import _mode_keywords
        result = _mode_keywords("search for cats", "BROWSER")
        assert result.get("action") == "web_navigate"

    def test_email_inbox(self):
        from intent_router import _mode_keywords
        result = _mode_keywords("check inbox", "EMAIL")
        assert result.get("action") == "email_action"

    def test_ide_open_file(self):
        from intent_router import _mode_keywords
        result = _mode_keywords("open file main.py", "IDE")
        assert result.get("action") == "ide_action"

    def test_files_list(self):
        from intent_router import _mode_keywords
        result = _mode_keywords("list files", "FILES")
        assert result.get("action") == "file_action"

    def test_docs_new(self):
        from intent_router import _mode_keywords
        result = _mode_keywords("new document", "DOCS")
        assert result.get("action") == "doc_action"


class TestClassifyPrompt:
    def test_contains_all_labels(self):
        prompt = _classify_prompt("test utterance")
        for label in INTENT_LABELS:
            assert label in prompt


class TestIntentLabels:
    def test_labels_are_strings(self):
        for label in INTENT_LABELS:
            assert isinstance(label, str)
            assert label == label.lower()
