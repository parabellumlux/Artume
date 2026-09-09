"""Tests for spoken file-browser navigation (next/previous entry, go up)."""

import os
import shutil
import tempfile
from unittest.mock import MagicMock, patch

from file_browser_engine import AudioFileBrowser


class TestEntryNavigation:
    def setup_method(self):
        self.test_dir = tempfile.mkdtemp()
        for name in ("alpha", "beta"):
            os.makedirs(os.path.join(self.test_dir, name))
        for name in ("readme.txt", "notes.md"):
            with open(os.path.join(self.test_dir, name), "w") as f:
                f.write("content")
        os.makedirs(os.path.join(self.test_dir, "alpha", "inner"))
        self.fb = AudioFileBrowser(initial_dir=self.test_dir)

    def teardown_method(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_next_entry_orders_dirs_first(self):
        first = self.fb.next_entry(1)
        assert first == "Folder alpha, 1 of 4."
        assert self.fb.next_entry(1) == "Folder beta, 2 of 4."
        assert self.fb.next_entry(1) == "File notes.md, 3 of 4."

    def test_previous_entry(self):
        self.fb.next_entry(1)
        self.fb.next_entry(1)
        self.fb.next_entry(1)
        assert self.fb.next_entry(-1) == "Folder beta, 2 of 4."

    def test_clamped_at_edges(self):
        assert self.fb.next_entry(-1) == "Already at the first entry."
        for _ in range(4):
            self.fb.next_entry(1)
        assert self.fb.next_entry(1) == "Already at the last entry."

    def test_empty_directory(self):
        empty = tempfile.mkdtemp()
        fb = AudioFileBrowser(initial_dir=empty)
        assert fb.next_entry(1) == "No entries in this directory."
        shutil.rmtree(empty, ignore_errors=True)

    def test_go_up_moves_to_parent(self):
        sub = os.path.join(self.test_dir, "alpha")
        fb = AudioFileBrowser(initial_dir=sub)
        result = fb.go_up()
        assert fb.cwd == self.test_dir
        assert "directory" in result.lower()

    def test_selection_resets_when_changing_dir(self):
        self.fb.next_entry(1)
        self.fb.change_dir("alpha")
        assert self.fb.next_entry(1).startswith("Folder ")


class TestFileBrowserModeNavigation:
    def _ctx(self):
        fb = MagicMock(wraps=AudioFileBrowser(self._tmp))
        return {
            'tts': MagicMock(),
            'file_browser': fb,
            'browser': MagicMock(),
            'mail_client': MagicMock(),
            'state_mgr': MagicMock(),
            'ebook_reader': MagicMock(),
            'doc_writer': MagicMock(),
            'sys_settings': MagicMock(),
            'navigator': MagicMock(),
            'ide': MagicMock(),
        }

    def test_next_file_command_routed(self):
        import tempfile, shutil, os
        from handlers.modes import handle as mode_handle
        self._tmp = tempfile.mkdtemp()
        os.makedirs(os.path.join(self._tmp, "sub"))
        ctx = self._ctx()
        with patch("earcons.play_earcon"):
            ok, mode = mode_handle("next file", "", "", "next file", "file_action", "FILES", ctx)
        assert ok is True
        ctx['file_browser'].next_entry.assert_called_once_with(1)
        shutil.rmtree(self._tmp, ignore_errors=True)

    def test_previous_entry_command_routed(self):
        import tempfile, shutil
        from handlers.modes import handle as mode_handle
        self._tmp = tempfile.mkdtemp()
        ctx = self._ctx()
        with patch("earcons.play_earcon"):
            ok, mode = mode_handle("previous entry", "", "", "previous entry", "file_action", "FILES", ctx)
        ctx['file_browser'].next_entry.assert_called_once_with(-1)
        shutil.rmtree(self._tmp, ignore_errors=True)

    def test_go_up_command_routed(self):
        import tempfile, shutil
        from handlers.modes import handle as mode_handle
        self._tmp = tempfile.mkdtemp()
        ctx = self._ctx()
        with patch("earcons.play_earcon"):
            ok, mode = mode_handle("go up", "", "", "go up", "file_action", "FILES", ctx)
        ctx['file_browser'].go_up.assert_called_once_with()
        shutil.rmtree(self._tmp, ignore_errors=True)


class TestRouterFileNavigation:
    def test_fm_files_nav_goes_to_file_action(self):
        from intent_router import ask_artome_ai
        assert ask_artome_ai("next file", "FILES")["action"] == "file_action"
        assert ask_artome_ai("previous entry", "FILES")["action"] == "file_action"
        assert ask_artome_ai("go up", "FILES")["target"] == "go_up"