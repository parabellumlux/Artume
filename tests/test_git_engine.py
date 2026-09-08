"""Tests for artome_ide/git_engine.py — Git operations with mock subprocess."""

from unittest.mock import patch, MagicMock

import pytest

from artome_ide.git_engine import GitEngine


@pytest.fixture
def ge(tmp_path):
    return GitEngine(project_root=str(tmp_path))


class TestStatus:
    @patch.object(GitEngine, "_git")
    def test_status_clean(self, mock_git, ge):
        mock_git.return_value = ""
        result = ge.status()
        assert "clean" in result.lower() or "no changes" in result.lower()

    @patch.object(GitEngine, "_git")
    def test_status_modified(self, mock_git, ge):
        mock_git.return_value = "M  src/main.py\nA  src/new.py\n D src/old.py\n?? untracked.txt"
        result = ge.status()
        assert "Modified" in result
        assert "Added" in result
        assert "Deleted" in result
        assert "Untracked" in result


class TestDiff:
    @patch.object(GitEngine, "_git")
    def test_diff_no_changes(self, mock_git, ge):
        mock_git.return_value = ""
        result = ge.diff()
        assert "no changes" in result.lower()

    @patch.object(GitEngine, "_git")
    def test_diff_with_changes(self, mock_git, ge):
        mock_git.return_value = "diff --git a/f.py b/f.py\n+new line\n-old line"
        result = ge.diff()
        assert "file" in result.lower()


class TestLog:
    @patch.object(GitEngine, "_git")
    def test_log(self, mock_git, ge):
        mock_git.return_value = "abc1234 first commit\ndef5678 second commit"
        result = ge.log()
        assert "abc1234" in result
        assert "Recent commits" in result

    @patch.object(GitEngine, "_git")
    def test_log_empty(self, mock_git, ge):
        mock_git.return_value = ""
        result = ge.log()
        assert "no commits" in result.lower()


class TestBranch:
    @patch.object(GitEngine, "_git")
    def test_branch(self, mock_git, ge):
        mock_git.side_effect = ["main", "* main\n  dev\n  feature"]
        result = ge.branch()
        assert "main" in result
        assert "3 branches" in result


class TestCommit:
    @patch.object(GitEngine, "_git")
    def test_commit(self, mock_git, ge):
        mock_git.side_effect = ["", "commit message text"]
        result = ge.commit("my message")
        assert "committed" in result.lower() or "my message" in result


class TestPushPull:
    @patch.object(GitEngine, "_git")
    def test_push(self, mock_git, ge):
        mock_git.return_value = ""
        result = ge.push()
        assert "pushed" in result.lower()

    @patch.object(GitEngine, "_git")
    def test_pull(self, mock_git, ge):
        mock_git.return_value = ""
        result = ge.pull()
        assert "pulled" in result.lower()
