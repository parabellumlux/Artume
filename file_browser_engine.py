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

    def _get_cli_path(self):
        base_dir = os.path.dirname(os.path.abspath(__file__))
        path = os.path.join(base_dir, "target", "debug", "aetherfs-cli")
        if os.path.exists(path):
            return path
        return "aetherfs-cli"

    def search_files_audio(self, query):
        """Search for files using semantic AetherFS daemon or fall back to local walk."""
        socket_path = "/tmp/aetherfs.sock"
        cli_path = self._get_cli_path()
        
        if os.path.exists(socket_path) and (cli_path != "aetherfs-cli" or shutil.which("aetherfs-cli")):
            import subprocess
            try:
                result = subprocess.run(
                    [cli_path, "search", query],
                    capture_output=True,
                    text=True,
                    timeout=5
                )
                if result.returncode == 0:
                    output = result.stdout
                    lines = output.split('\n')
                    matches_section = False
                    matches = []
                    for line in lines:
                        if "Matching Files Found:" in line:
                            matches_section = True
                            continue
                        if matches_section:
                            line_stripped = line.strip()
                            if line_stripped.startswith("Path:"):
                                path = line_stripped.replace("Path:", "").strip()
                                if path.startswith(self.cwd):
                                    rel = os.path.relpath(path, self.cwd)
                                    matches.append(rel)
                                else:
                                    matches.append(os.path.basename(path))
                    if matches:
                        speech = f"AetherFS background engine found {len(matches)} matching files. "
                        speech += ", ".join(matches[:5])
                        return speech
            except Exception:
                pass

        # Fallback local walk search
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

    def get_duplicates_audio(self):
        """Query the AetherFS deduplication engine for duplicate groups."""
        socket_path = "/tmp/aetherfs.sock"
        cli_path = self._get_cli_path()
        
        if not os.path.exists(socket_path):
            return "AetherFS background daemon is not running. Please start the core service to check for duplicates."
            
        if cli_path == "aetherfs-cli" and not shutil.which("aetherfs-cli"):
            return "AetherFS CLI utility not found. Please compile the workspace."

        import subprocess
        try:
            result = subprocess.run(
                [cli_path, "dups"],
                capture_output=True,
                text=True,
                timeout=5
            )
            if result.returncode == 0:
                output = result.stdout.strip()
                if "No duplicate files found." in output:
                    return "No duplicate files found in the system."
                
                lines = output.split('\n')
                cleaned_lines = []
                for line in lines:
                    line_stripped = line.strip()
                    if line_stripped.startswith("Group ") or line_stripped.startswith("Canonical Path:") or line_stripped.startswith("Duplicate Copies:"):
                        cleaned_lines.append(line_stripped)
                    elif line_stripped.startswith("- "):
                        cleaned_lines.append("duplicate copy: " + line_stripped[2:])
                
                if cleaned_lines:
                    summary = ". ".join(cleaned_lines[:15])
                    return f"Deduplication registry: {summary}"
                return output
            return "Failed to query duplicates registry from background daemon."
        except Exception as e:
            return f"Error querying duplicates: {str(e)[:50]}"

    def index_directory_audio(self, path):
        """Request the background daemon to index a directory."""
        target_path = os.path.abspath(os.path.expanduser(path))
        if not os.path.exists(target_path):
            return f"Directory {path} does not exist."

        socket_path = "/tmp/aetherfs.sock"
        cli_path = self._get_cli_path()
        
        if not os.path.exists(socket_path):
            return "AetherFS background daemon is not running. Cannot index directory."

        import subprocess
        try:
            result = subprocess.run(
                [cli_path, "index", target_path],
                capture_output=True,
                text=True,
                timeout=5
            )
            if result.returncode == 0:
                output = result.stdout.strip()
                if "Success" in output:
                    return f"Successfully queued indexing for {os.path.basename(target_path)}."
                return output
            return f"Failed to request indexing: {result.stderr.strip()[:60]}"
        except Exception as e:
            return f"Error requesting indexing: {str(e)[:50]}"

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
