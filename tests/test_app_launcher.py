"""Tests for app_launcher.py — XDG app launcher with mock filesystem."""

import os
from unittest.mock import patch, MagicMock
from pathlib import Path

import pytest

from app_launcher import AppLauncher, AppInfo


@pytest.fixture
def launcher():
    return AppLauncher()


@pytest.fixture
def fake_desktop_files(tmp_path):
    app_dir = tmp_path / "applications"
    app_dir.mkdir()
    (app_dir / "firefox.desktop").write_text(
        "[Desktop Entry]\n"
        "Name=Firefox\n"
        "Exec=/usr/bin/firefox %u\n"
        "Categories=Network;WebBrowser;\n"
        "Icon=firefox\n"
    )
    (app_dir / "terminal.desktop").write_text(
        "[Desktop Entry]\n"
        "Name=Terminal\n"
        "Exec=/usr/bin/terminal\n"
        "Categories=System;TerminalEmulator;\n"
        "Icon=terminal\n"
    )
    return app_dir


class TestScanApps:
    def test_scan_parses_desktop_files(self, launcher, fake_desktop_files):
        launcher._scan_dirs = [fake_desktop_files]
        launcher._scan_apps()
        assert "firefox" in launcher._apps
        assert "terminal" in launcher._apps
        assert launcher._apps["firefox"].name == "Firefox"
        assert "firefox" in launcher._apps["firefox"].exec_cmd.lower()

    def test_scan_strips_field_codes(self, launcher, fake_desktop_files):
        launcher._scan_dirs = [fake_desktop_files]
        launcher._scan_apps()
        assert "%u" not in launcher._apps["firefox"].exec_cmd

    def test_scan_skips_invalid_files(self, launcher, tmp_path):
        bad_dir = tmp_path / "apps"
        bad_dir.mkdir()
        (bad_dir / "bad.desktop").write_text("no name or exec")
        launcher._scan_dirs = [bad_dir]
        launcher._scan_apps()
        assert len(launcher._apps) == 0


class TestListApps:
    def test_list_apps(self, launcher, fake_desktop_files):
        launcher._scan_dirs = [fake_desktop_files]
        result = launcher.list_apps()
        assert "2 applications" in result or "Firefox" in result

    def test_list_by_category(self, launcher, fake_desktop_files):
        launcher._scan_dirs = [fake_desktop_files]
        result = launcher.list_apps(category="Network")
        assert "Firefox" in result
        assert "Terminal" not in result

    def test_list_empty(self, launcher, tmp_path):
        launcher._scan_dirs = [tmp_path / "empty"]
        result = launcher.list_apps()
        assert "no applications" in result.lower()


class TestLaunch:
    @patch("app_launcher.subprocess.Popen")
    def test_launch_exact(self, mock_popen, launcher, fake_desktop_files):
        launcher._scan_dirs = [fake_desktop_files]
        launcher._scan_apps()
        result = launcher.launch("Firefox")
        assert "launched" in result.lower()
        mock_popen.assert_called_once()

    @patch("app_launcher.subprocess.Popen")
    def test_launch_fuzzy(self, mock_popen, launcher, fake_desktop_files):
        launcher._scan_dirs = [fake_desktop_files]
        launcher._scan_apps()
        result = launcher.launch("fire")
        assert "launched" in result.lower()

    @patch("app_launcher.subprocess.Popen")
    def test_launch_xdg_fallback(self, mock_popen, launcher, tmp_path):
        launcher._scan_dirs = [tmp_path / "empty"]
        launcher._scan_apps()
        result = launcher.launch("https://example.com")
        assert "opened" in result.lower()


class TestQuitApp:
    @patch("app_launcher.subprocess.run")
    def test_quit(self, mock_run, launcher):
        result = launcher.quit_app("firefox")
        assert "quit" in result.lower()


class TestSearchApps:
    def test_search(self, launcher, fake_desktop_files):
        launcher._scan_dirs = [fake_desktop_files]
        launcher._scan_apps()
        result = launcher.search_apps("fire")
        assert "Firefox" in result

    def test_search_no_match(self, launcher, fake_desktop_files):
        launcher._scan_dirs = [fake_desktop_files]
        launcher._scan_apps()
        result = launcher.search_apps("notepad")
        assert "no applications" in result.lower()
