#!/usr/bin/env python3
"""Artome Audio-Only File Browser Engine - Conversational file manager."""

import os
import shutil
import glob

class AudioFileBrowser:
    """Conversational voice file browser and manager for Artome DE."""

    def __init__(self, initial_dir=None):
        self.cwd = initial_dir or os.path.dirname(os.path.abspath(__file__))

    def get_location_audio(self):
        """Spoken summary of current directory location."""
        folder_name = os.path.basename(self.cwd) or self.cwd
        return f"Currently in directory {folder_name}."

    def list_contents_audio(self):
        """Spoken list of subfolders and files in current working directory."""
        try:
            entries = os.listdir(self.cwd)
            dirs = [e for e in entries if os.path.isdir(os.path.join(self.cwd, e))]
            files = [e for e in entries if os.path.isfile(os.path.join(self.cwd, e))]

            speech = f"Directory contains {len(dirs)} folders and {len(files)} files. "
            if dirs:
                speech += f"Folders: {', '.join(dirs[:6])}. "
            if files:
                speech += f"Files: {', '.join(files[:8])}. "
            return speech
        except Exception as e:
            return f"Error reading directory: {str(e)[:50]}"

    def change_dir(self, target):
        """Navigate to a subdirectory or parent directory."""
        if target == ".." or "up" in target.lower() or "parent" in target.lower():
            parent = os.path.dirname(self.cwd)
            if parent and os.path.exists(parent):
                self.cwd = parent
                return f"Moved up to {os.path.basename(self.cwd) or self.cwd}."
            return "Already at root directory."

        target_path = os.path.join(self.cwd, target)
        if os.path.exists(target_path) and os.path.isdir(target_path):
            self.cwd = target_path
            return f"Entered folder {os.path.basename(target_path)}. " + self.list_contents_audio()

        # Case-insensitive match check
        for entry in os.listdir(self.cwd):
            if entry.lower() == target.lower() and os.path.isdir(os.path.join(self.cwd, entry)):
                self.cwd = os.path.join(self.cwd, entry)
                return f"Entered folder {entry}. " + self.list_contents_audio()

        return f"Folder {target} not found."

    def read_file_audio(self, filename):
        """Read content or metadata of a file out loud."""
        filepath = os.path.join(self.cwd, filename)
        if not os.path.exists(filepath):
            # Try fuzzy match
            for entry in os.listdir(self.cwd):
                if entry.lower() == filename.lower():
                    filepath = os.path.join(self.cwd, entry)
                    filename = entry
                    break

        if not os.path.exists(filepath):
            return f"File {filename} not found."

        try:
            stat_info = os.stat(filepath)
            size_kb = round(stat_info.st_size / 1024, 1)

            if filename.endswith(('.wav', '.mp3', '.ogg', '.flac', '.onnx')):
                return f"Media file {filename}, size {size_kb} kilobytes."

            with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                content = f.read(600)

            speech = f"File {filename}, size {size_kb} KB. Content preview: {content}"
            return speech
        except Exception as e:
            return f"Error reading file {filename}: {str(e)[:50]}"

    def search_files_audio(self, query):
        """Search for files by pattern or query string."""
        matches = []
        for root, dirs, files in os.walk(self.cwd):
            for file in files:
                if query.lower() in file.lower():
                    rel_path = os.path.relpath(os.path.join(root, file), self.cwd)
                    matches.append(rel_path)
            if len(matches) >= 5:
                break

        if not matches:
            return f"No files matching {query} found."

        speech = f"Found {len(matches)} matching files. "
        speech += ", ".join(matches[:5])
        return speech

    def create_folder(self, folder_name):
        """Create a new folder in current directory."""
        path = os.path.join(self.cwd, folder_name)
        try:
            os.makedirs(path, exist_ok=True)
            return f"Created folder {folder_name}."
        except Exception as e:
            return f"Failed to create folder: {str(e)[:50]}"

if __name__ == "__main__":
    fb = AudioFileBrowser()
    print(fb.get_location_audio())
    print(fb.list_contents_audio())
    print("\nTesting file search for 'py':")
    print(fb.search_files_audio("py"))
