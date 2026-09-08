"""Tests for state_manager.py — persistent state save/load."""

import json
import os
import threading
from unittest.mock import patch

import pytest

from state_manager import StateManager, get_state_manager


@pytest.fixture
def sm(tmp_path):
    StateManager._instance = None
    sm = StateManager(state_dir=str(tmp_path))
    yield sm
    StateManager._instance = None


class TestGetSet:
    def test_get_set(self, sm):
        sm.set("key1", "value1")
        assert sm.get("key1") == "value1"

    def test_get_default(self, sm):
        assert sm.get("nonexistent", "default") == "default"

    def test_set_overwrites(self, sm):
        sm.set("key", "old")
        sm.set("key", "new")
        assert sm.get("key") == "new"


class TestMode:
    def test_get_mode_default(self, sm):
        assert sm.get_mode() == "DESKTOP"

    def test_set_mode(self, sm):
        sm.set_mode("IDE")
        assert sm.get_mode() == "IDE"


class TestActiveFile:
    def test_set_active_file(self, sm):
        sm.set_active_file("/tmp/test.py")
        assert sm.get_active_file() == "/tmp/test.py"

    def test_active_file_adds_to_recent(self, sm):
        sm.set_active_file("/tmp/a.py")
        sm.set_active_file("/tmp/b.py")
        recent = sm.get_recent_files()
        assert recent[0] == "/tmp/b.py"


class TestRecentFiles:
    def test_recent_files_limit(self, sm):
        for i in range(25):
            sm.add_recent_file(f"/tmp/file{i}.py")
        assert len(sm.get_recent_files()) == 20

    def test_recent_files_dedup(self, sm):
        sm.add_recent_file("/tmp/a.py")
        sm.add_recent_file("/tmp/a.py")
        assert sm.get_recent_files().count("/tmp/a.py") == 1


class TestBookmarks:
    def test_add_get_bookmark(self, sm):
        sm.add_bookmark("my_func", "/tmp/a.py", line=10)
        bm = sm.get_bookmarks()
        assert "my_func" in bm
        assert bm["my_func"]["path"] == "/tmp/a.py"
        assert bm["my_func"]["line"] == 10

    def test_remove_bookmark(self, sm):
        sm.add_bookmark("temp", "/tmp/b.py")
        sm.remove_bookmark("temp")
        assert "temp" not in sm.get_bookmarks()

    def test_remove_nonexistent_bookmark(self, sm):
        sm.remove_bookmark("nonexistent")
        assert sm.get_bookmarks() == {}


class TestPreferences:
    def test_set_get_preference(self, sm):
        sm.set_preference("theme", "dark")
        assert sm.get_preference("theme") == "dark"

    def test_preference_default(self, sm):
        assert sm.get_preference("missing", "light") == "light"


class TestPersistence:
    def test_save_and_load(self, tmp_path):
        StateManager._instance = None
        sm1 = StateManager(state_dir=str(tmp_path))
        sm1.set("key", "value")
        sm1.save()

        StateManager._instance = None
        sm2 = StateManager(state_dir=str(tmp_path))
        assert sm2.load()
        assert sm2.get("key") == "value"

    def test_load_nonexistent(self, sm):
        assert not sm.load()


class TestSingleton:
    def test_singleton(self, tmp_path):
        StateManager._instance = None
        sm1 = get_state_manager(str(tmp_path))
        sm2 = get_state_manager(str(tmp_path))
        assert sm1 is sm2
        StateManager._instance = None
