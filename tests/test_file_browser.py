"""Tests for file_browser_engine — file operations, sorting, and creation."""

import os
import sys
import shutil
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from file_browser_engine import AudioFileBrowser


class TestFileOperations(unittest.TestCase):
    """Test file copy, move, rename, delete, create operations."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.fb = AudioFileBrowser(initial_dir=self.test_dir)
        # Create test files
        with open(os.path.join(self.test_dir, "test.txt"), "w") as f:
            f.write("hello world")
        with open(os.path.join(self.test_dir, "data.csv"), "w") as f:
            f.write("a,b,c")
        os.makedirs(os.path.join(self.test_dir, "subdir"))

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_create_file(self):
        result = self.fb.create_file("new.py")
        self.assertIn("Created", result)
        self.assertTrue(os.path.exists(os.path.join(self.test_dir, "new.py")))

    def test_create_file_exists(self):
        result = self.fb.create_file("test.txt")
        self.assertIn("already exists", result)

    def test_create_folder(self):
        result = self.fb.create_folder("newdir")
        self.assertIn("Created", result)
        self.assertTrue(os.path.isdir(os.path.join(self.test_dir, "newdir")))

    def test_copy_file(self):
        result = self.fb.copy_file("test.txt", "copy.txt")
        self.assertIn("Copied", result)
        self.assertTrue(os.path.exists(os.path.join(self.test_dir, "copy.txt")))

    def test_copy_file_to_dir(self):
        result = self.fb.copy_file("test.txt", "subdir")
        self.assertIn("Copied", result)
        self.assertTrue(os.path.exists(os.path.join(self.test_dir, "subdir", "test.txt")))

    def test_copy_file_not_found(self):
        result = self.fb.copy_file("nonexistent.txt", "copy.txt")
        self.assertIn("not found", result)

    def test_move_file(self):
        result = self.fb.move_file("test.txt", "moved.txt")
        self.assertIn("Moved", result)
        self.assertTrue(os.path.exists(os.path.join(self.test_dir, "moved.txt")))
        self.assertFalse(os.path.exists(os.path.join(self.test_dir, "test.txt")))

    def test_rename_file(self):
        result = self.fb.rename_file("test.txt", "renamed.txt")
        self.assertIn("Renamed", result)
        self.assertTrue(os.path.exists(os.path.join(self.test_dir, "renamed.txt")))

    def test_rename_file_not_found(self):
        result = self.fb.rename_file("nonexistent.txt", "new.txt")
        self.assertIn("not found", result)

    def test_delete_file(self):
        result = self.fb.delete_file("test.txt")
        self.assertIn("Deleted", result)
        self.assertFalse(os.path.exists(os.path.join(self.test_dir, "test.txt")))

    def test_delete_folder(self):
        result = self.fb.delete_file("subdir")
        self.assertIn("Deleted", result)
        self.assertFalse(os.path.exists(os.path.join(self.test_dir, "subdir")))

    def test_delete_file_not_found(self):
        result = self.fb.delete_file("nonexistent.txt")
        self.assertIn("not found", result)


class TestSortDirectory(unittest.TestCase):
    """Test directory sorting by name, date, and size."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.fb = AudioFileBrowser(initial_dir=self.test_dir)
        # Create files with different sizes
        with open(os.path.join(self.test_dir, "small.txt"), "w") as f:
            f.write("a")
        with open(os.path.join(self.test_dir, "medium.txt"), "w") as f:
            f.write("a" * 1000)
        with open(os.path.join(self.test_dir, "large.txt"), "w") as f:
            f.write("a" * 10000)
        os.makedirs(os.path.join(self.test_dir, "adir"))

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_sort_by_name(self):
        result = self.fb.sort_directory("name")
        self.assertIn("Sorted by name", result)
        self.assertIn("adir", result)

    def test_sort_by_size(self):
        result = self.fb.sort_directory("size")
        self.assertIn("Sorted by size", result)
        self.assertIn("large.txt", result)

    def test_sort_by_date(self):
        result = self.fb.sort_directory("date")
        self.assertIn("Sorted by date", result)


class TestChangeDir(unittest.TestCase):
    """Test directory navigation."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.fb = AudioFileBrowser(initial_dir=self.test_dir)
        os.makedirs(os.path.join(self.test_dir, "child"))
        os.makedirs(os.path.join(self.test_dir, "parent_test"))

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_change_to_child(self):
        result = self.fb.change_dir("child")
        self.assertIn("Entered folder", result)
        self.assertEqual(self.fb.cwd, os.path.join(self.test_dir, "child"))

    def test_change_to_parent(self):
        result = self.fb.change_dir("..")
        self.assertIn("Moved up", result)

    def test_change_to_nonexistent(self):
        result = self.fb.change_dir("nope")
        self.assertIn("not found", result)


if __name__ == "__main__":
    unittest.main()
