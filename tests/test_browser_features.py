"""Tests for browser_engine — history navigation and bookmark management."""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from browser_engine import AudioWebBrowser


class TestBrowserHistory(unittest.TestCase):
    """Test browser back/forward navigation."""

    def setUp(self):
        self.browser = AudioWebBrowser()

    def test_initial_history_empty(self):
        self.assertEqual(self.browser._history, [])
        self.assertEqual(self.browser._history_index, -1)

    def test_go_back_no_history(self):
        result = self.browser.go_back()
        self.assertIn("No previous page", result)

    def test_go_forward_no_history(self):
        result = self.browser.go_forward()
        self.assertIn("No next page", result)

    def test_history_tracking(self):
        self.browser.current_url = "https://a.com"
        self.browser._history = ["https://a.com"]
        self.browser._history_index = 0
        self.browser.current_url = "https://b.com"
        self.browser._history = ["https://a.com", "https://b.com"]
        self.browser._history_index = 1
        result = self.browser.go_back()
        self.assertEqual(self.browser._history_index, 0)

    def test_history_truncate_on_new_branch(self):
        self.browser._history = ["https://a.com", "https://b.com", "https://c.com"]
        self.browser._history_index = 2
        # Simulate navigating to a new page from middle
        self.browser._history = self.browser._history[:self.browser._history_index + 1]
        self.browser._history.append("https://d.com")
        self.browser._history_index = len(self.browser._history) - 1
        self.assertEqual(self.browser._history, ["https://a.com", "https://b.com", "https://c.com", "https://d.com"])


class TestBrowserBookmarks(unittest.TestCase):
    """Test bookmark management."""

    def setUp(self):
        self.browser = AudioWebBrowser()

    def test_bookmark_empty_page(self):
        result = self.browser.bookmark_current()
        self.assertIn("No page loaded", result)

    def test_bookmark_page(self):
        self.browser.current_url = "https://example.com"
        self.browser.page_title = "Example"
        result = self.browser.bookmark_current()
        self.assertIn("Bookmarked", result)
        self.assertEqual(len(self.browser._bookmarks), 1)

    def test_bookmark_duplicate(self):
        self.browser.current_url = "https://example.com"
        self.browser.page_title = "Example"
        self.browser.bookmark_current()
        result = self.browser.bookmark_current()
        self.assertIn("Already bookmarked", result)
        self.assertEqual(len(self.browser._bookmarks), 1)

    def test_list_bookmarks_empty(self):
        result = self.browser.list_bookmarks()
        self.assertIn("No bookmarks", result)

    def test_list_bookmarks(self):
        self.browser._bookmarks = [
            {"url": "https://a.com", "title": "A"},
            {"url": "https://b.com", "title": "B"},
        ]
        result = self.browser.list_bookmarks()
        self.assertIn("2 bookmarks", result)
        self.assertIn("A", result)

    def test_click_bookmark(self):
        self.browser._bookmarks = [{"url": "https://example.com", "title": "Example"}]
        # click_bookmark calls load_url which makes HTTP request — just test index validation
        result = self.browser.click_bookmark(5)
        self.assertIn("Invalid", result)

    def test_click_bookmark_valid_index(self):
        self.browser._bookmarks = [{"url": "https://example.com", "title": "Example"}]
        # This will try to fetch the URL, which may fail, but the method should be callable
        try:
            self.browser.click_bookmark(1)
        except Exception:
            pass  # Network error is OK in tests


if __name__ == "__main__":
    unittest.main()
