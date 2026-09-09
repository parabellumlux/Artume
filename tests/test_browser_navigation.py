"""Tests for spoken page navigation (next/previous heading and link)."""

from unittest.mock import MagicMock, patch

from browser_engine import AudioWebBrowser
from handlers.modes import handle as mode_handle


def _browser():
    b = AudioWebBrowser()
    b.headings = [{"tag": "h1", "text": "Intro"}, {"tag": "h2", "text": "Details"}]
    b.links = [{"text": "Docs", "url": "https://d"}, {"text": "API", "url": "https://a"}]
    return b


def test_next_heading_clamped():
    b = _browser()
    assert b.next_heading(1) == "Heading 1 of 2: Intro."
    assert b.next_heading(1) == "Heading 2 of 2: Details."
    assert b.next_heading(1) == "Already at the last heading."
    assert b.next_heading(-1) == "Heading 1 of 2: Intro."


def test_previous_heading_at_start():
    b = _browser()
    assert b.next_heading(-1) == "Already at the first heading."


def test_next_link():
    b = _browser()
    assert b.next_link(1) == "Link 1 of 2: Docs."
    assert b.next_link(1) == "Link 2 of 2: API."


def test_cursors_are_independent():
    b = _browser()
    b.next_link(1)
    b.next_heading(1)
    assert b._link_index == 0
    assert b._heading_index == 0
    assert b.next_heading(-1) == "Already at the first heading."


def test_no_links():
    b = _browser()
    b.links = []
    assert b.next_link(1) == "No links found on this page."


class TestBrowserModeNavigation:
    def _ctx(self, browser=None):
        return {
            'tts': MagicMock(),
            'file_browser': MagicMock(),
            'browser': browser if browser is not None else MagicMock(),
            'mail_client': MagicMock(),
            'state_mgr': MagicMock(),
            'ebook_reader': MagicMock(),
            'doc_writer': MagicMock(),
            'sys_settings': MagicMock(),
            'navigator': MagicMock(),
            'ide': MagicMock(),
        }

    def test_next_heading_routed(self):
        browser = MagicMock()
        browser.next_heading.return_value = "Heading 1 of 2: Intro."
        ctx = self._ctx(browser)
        with patch("earcons.play_earcon"):
            ok, mode = mode_handle("next heading", "", "", "next heading",
                                   "web_navigate", "BROWSER", ctx)
        assert ok is True
        browser.next_heading.assert_called_once_with(1)

    def test_previous_link_routed(self):
        browser = MagicMock()
        browser.next_link.return_value = "Link 1 of 2: Docs."
        ctx = self._ctx(browser)
        with patch("earcons.play_earcon"):
            mode_handle("previous link", "", "", "previous link",
                        "web_navigate", "BROWSER", ctx)
        browser.next_link.assert_called_once_with(-1)

    def test_target_driven_heading_nav(self):
        browser = MagicMock()
        ctx = self._ctx(browser)
        with patch("earcons.play_earcon"):
            mode_handle("next heading", "", "navigate_headings",
                        "Navigating headings", "web_navigate", "BROWSER", ctx)
        browser.next_heading.assert_called_once()

    def test_link_listing_unaffected(self):
        browser = MagicMock()
        browser.get_links_audio.return_value = "Top 7 links"
        ctx = self._ctx(browser)
        with patch("earcons.play_earcon"):
            mode_handle("list links", "", "", "list links",
                        "web_navigate", "BROWSER", ctx)
        browser.get_links_audio.assert_called_once()


class TestRouterPageNavigation:
    def test_page_nav_phrases(self):
        from intent_router import ask_artome_ai
        assert ask_artome_ai("next heading", "BROWSER")["action"] == "web_navigate"
        assert ask_artome_ai("previous link")["target"] == "navigate_links"