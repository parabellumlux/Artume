"""Tests for clip_registry — pre-recorded response clip lookup and duplicate stripping."""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from clip_registry import lookup, strip_clip_duplicate, get_clip_text, list_clips, CLIPS_DIR


class TestClipLookup(unittest.TestCase):
    """Test clip registry lookup functionality."""

    def test_exact_match_volume_up(self):
        clip = lookup("setting_action", "volume_up")
        self.assertIsNotNone(clip)
        self.assertIn("volume_up", clip["file"])

    def test_exact_match_volume_down(self):
        clip = lookup("setting_action", "volume_down")
        self.assertIsNotNone(clip)
        self.assertIn("volume_down", clip["file"])

    def test_exact_match_status(self):
        clip = lookup("setting_action", "status")
        self.assertIsNotNone(clip)
        self.assertIn("checking_status", clip["file"])

    def test_exact_match_timer(self):
        clip = lookup("setting_action", "timer")
        self.assertIsNotNone(clip)
        self.assertIn("timer_set", clip["file"])

    def test_exact_match_mute(self):
        clip = lookup("setting_action", "mute")
        self.assertIsNotNone(clip)
        self.assertIn("muted", clip["file"])

    def test_exact_match_switch_mode(self):
        clip = lookup("switch_mode", "")
        self.assertIsNotNone(clip)
        self.assertIn("switching_mode", clip["file"])

    def test_exact_match_file_search(self):
        clip = lookup("file_search", "")
        self.assertIsNotNone(clip)
        self.assertIn("searching_files", clip["file"])

    def test_prefix_match_web_navigate_url(self):
        clip = lookup("web_navigate", "url:https://example.com")
        self.assertIsNotNone(clip)
        self.assertIn("loading_page", clip["file"])

    def test_prefix_match_web_navigate_search(self):
        clip = lookup("web_navigate", "search:python docs")
        self.assertIsNotNone(clip)
        self.assertIn("searching_files", clip["file"])

    def test_prefix_match_ide_fix(self):
        clip = lookup("ide_action", "fix this code")
        self.assertIsNotNone(clip)
        self.assertIn("looking_into_it", clip["file"])

    def test_prefix_match_ide_explain(self):
        clip = lookup("ide_action", "explain this function")
        self.assertIsNotNone(clip)
        self.assertIn("looking_into_it", clip["file"])

    def test_prefix_match_ide_git(self):
        clip = lookup("ide_action", "git_status")
        self.assertIsNotNone(clip)
        self.assertIn("working_on_it", clip["file"])

    def test_prefix_match_ide_run(self):
        clip = lookup("ide_action", "run tests")
        self.assertIsNotNone(clip)
        self.assertIn("running_that_now", clip["file"])

    def test_prefix_match_ide_build(self):
        clip = lookup("ide_action", "build project")
        self.assertIsNotNone(clip)
        self.assertIn("running_that_now", clip["file"])

    def test_no_match_unknown_action(self):
        clip = lookup("unknown_action", "something")
        self.assertIsNone(clip)

    def test_no_match_empty_action(self):
        clip = lookup("", "")
        self.assertIsNone(clip)

    def test_open_app(self):
        clip = lookup("open_app", "firefox")
        self.assertIsNotNone(clip)
        self.assertIn("opening_app", clip["file"])

    def test_email_inbox(self):
        clip = lookup("email_action", "inbox")
        self.assertIsNotNone(clip)
        self.assertIn("checking_email", clip["file"])

    def test_entity_lookup(self):
        clip = lookup("entity_lookup", "")
        self.assertIsNotNone(clip)
        self.assertIn("on_it", clip["file"])

    def test_screen_summary(self):
        clip = lookup("screen_summary", "")
        self.assertIsNotNone(clip)
        self.assertIn("reading_screen", clip["file"])

    def test_clip_has_text(self):
        clip = lookup("setting_action", "volume_up")
        self.assertIn("text", clip)
        self.assertTrue(len(clip["text"]) > 0)

    def test_clip_has_file(self):
        clip = lookup("setting_action", "volume_up")
        self.assertIn("file", clip)
        self.assertTrue(clip["file"].endswith(".wav"))


class TestStripClipDuplicate(unittest.TestCase):
    """Test response prefix stripping for duplicate avoidance."""

    def test_exact_duplicate_stripped(self):
        clip = {"text": "Looking into that."}
        response = "Looking into that. Here's what I found..."
        result = strip_clip_duplicate(clip, response)
        self.assertEqual(result, "Here's what I found...")

    def test_no_duplicate_kept(self):
        clip = {"text": "Looking into that."}
        response = "The function has a bug on line 42."
        result = strip_clip_duplicate(clip, response)
        self.assertEqual(result, "The function has a bug on line 42.")

    def test_partial_overlap_kept(self):
        clip = {"text": "Volume up."}
        response = "Volume is now at 75 percent."
        result = strip_clip_duplicate(clip, response)
        # "Volume" overlaps but "up" != "is" so no strip
        self.assertEqual(result, "Volume is now at 75 percent.")

    def test_none_clip(self):
        result = strip_clip_duplicate(None, "Some response")
        self.assertEqual(result, "Some response")

    def test_none_response(self):
        clip = {"text": "On it."}
        result = strip_clip_duplicate(clip, None)
        self.assertIsNone(result)

    def test_empty_clip_text(self):
        clip = {"text": ""}
        response = "Some response"
        result = strip_clip_duplicate(clip, response)
        self.assertEqual(result, "Some response")

    def test_fuzzy_three_word_match(self):
        clip = {"text": "Checking your system status."}
        response = "Checking your system status now. Battery at 80%."
        result = strip_clip_duplicate(clip, response)
        self.assertEqual(result, "now. Battery at 80%.")

    def test_empty_response_after_strip(self):
        clip = {"text": "On it."}
        response = "On it."
        result = strip_clip_duplicate(clip, response)
        # After stripping, nothing left — return original
        self.assertEqual(result, "On it.")


class TestClipFilesExist(unittest.TestCase):
    """Test that all registered clip files actually exist on disk."""

    def test_all_clip_files_exist(self):
        clips = list_clips()
        missing = [c for c in clips if not c["exists"]]
        self.assertEqual(missing, [], f"Missing clip files: {missing}")

    def test_clips_dir_exists(self):
        self.assertTrue(os.path.isdir(CLIPS_DIR))

    def test_system_clips_dir(self):
        system_dir = os.path.join(CLIPS_DIR, "system")
        self.assertTrue(os.path.isdir(system_dir))

    def test_hardware_clips_dir(self):
        hw_dir = os.path.join(CLIPS_DIR, "hardware")
        self.assertTrue(os.path.isdir(hw_dir))

    def test_confirm_clips_dir(self):
        confirm_dir = os.path.join(CLIPS_DIR, "confirm")
        self.assertTrue(os.path.isdir(confirm_dir))


class TestGetClipText(unittest.TestCase):
    """Test get_clip_text helper."""

    def test_returns_text(self):
        clip = {"text": "Hello world"}
        self.assertEqual(get_clip_text(clip), "Hello world")

    def test_none_clip(self):
        self.assertEqual(get_clip_text(None), "")

    def test_empty_clip(self):
        self.assertEqual(get_clip_text({}), "")


if __name__ == "__main__":
    unittest.main()
