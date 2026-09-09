#!/usr/bin/env python3
"""Artome Audio-Only File Browser Engine - Conversational file manager."""

import os
import shutil

# Configurable socket path — override with AETHERFS_SOCKET env var
SOCKET_PATH = os.environ.get("AETHERFS_SOCKET", "/tmp/aetherfs.sock")

class AudioFileBrowser:
    """Conversational voice file browser and manager for Artome DE."""

    def __init__(self, initial_dir=None):
        self.cwd = initial_dir or os.path.dirname(os.path.abspath(__file__))
        self._entries = []
        self._entries_dir = None
        self._selected_index = -1

    def _refresh_entries(self):
        """Cache (kind, name) entries for the current directory, dirs first."""
        if self._entries_dir != self.cwd or not self._entries:
            try:
                entries = os.listdir(self.cwd)
                dirs = sorted(e for e in entries
                              if os.path.isdir(os.path.join(self.cwd, e)))
                files = sorted(e for e in entries
                               if os.path.isfile(os.path.join(self.cwd, e)))
                self._entries = [("folder", d) for d in dirs] + \
                                [("file", f) for f in files]
            except Exception:
                self._entries = []
            self._entries_dir = self.cwd
            if self._selected_index >= 0:
                self._selected_index = min(self._selected_index,
                                           max(0, len(self._entries) - 1))

    def next_entry(self, step=1):
        """Move the selection cursor to the next/previous entry and speak it."""
        self._refresh_entries()
        if not self._entries:
            return "No entries in this directory."
        total = len(self._entries)
        current = self._selected_index
        if current == -1:
            if step < 0:
                return "Already at the first entry."
            target = 0
        else:
            target = max(0, min(total - 1, current + step))
            if target == current:
                edge = "last" if step > 0 else "first"
                return f"Already at the {edge} entry."
        self._selected_index = target
        kind, name = self._entries[target]
        return (f"{kind.capitalize()} {name}, "
                f"{target + 1} of {total}.")

    def go_up(self):
        """Move to the parent directory and speak the new location."""
        before = self.cwd
        result = self.change_dir("..")
        if self.cwd == before:
            return result
        return f"{result} {self.get_location_audio()}"

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

    def _reset_entries(self):
        """Clear the entry cache and selection after a directory change."""
        self._entries = []
        self._entries_dir = None
        self._selected_index = -1

    def change_dir(self, target):
        """Navigate to a subdirectory or parent directory."""
        if target == ".." or "up" in target.lower() or "parent" in target.lower():
            parent = os.path.dirname(self.cwd)
            if parent and os.path.exists(parent):
                self.cwd = parent
                self._reset_entries()
                return f"Moved up to {os.path.basename(self.cwd) or self.cwd}."
            return "Already at root directory."

        target_path = os.path.join(self.cwd, target)
        if os.path.exists(target_path) and os.path.isdir(target_path):
            self.cwd = target_path
            self._reset_entries()
            return f"Entered folder {os.path.basename(target_path)}. " + self.list_contents_audio()

        # Case-insensitive match check
        for entry in os.listdir(self.cwd):
            if entry.lower() == target.lower() and os.path.isdir(os.path.join(self.cwd, entry)):
                self.cwd = os.path.join(self.cwd, entry)
                self._reset_entries()
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
        socket_path = SOCKET_PATH
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
        socket_path = SOCKET_PATH
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

        socket_path = SOCKET_PATH
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

    def create_file(self, filename: str) -> str:
        """Create a new empty file in current directory."""
        path = os.path.join(self.cwd, filename)
        try:
            if os.path.exists(path):
                return f"File {filename} already exists."
            with open(path, "w"):
                pass
            return f"Created file {filename}."
        except Exception as e:
            return f"Failed to create file: {str(e)[:50]}"

    def copy_file(self, source: str, destination: str) -> str:
        """Copy a file within or across directories."""
        src = os.path.join(self.cwd, source)
        if not os.path.exists(src):
            # Try fuzzy match
            for entry in os.listdir(self.cwd):
                if entry.lower() == source.lower():
                    src = os.path.join(self.cwd, entry)
                    break
        if not os.path.exists(src):
            return f"Source file {source} not found."
        dst = os.path.join(self.cwd, destination)
        try:
            if os.path.isdir(dst):
                dst = os.path.join(dst, os.path.basename(src))
            shutil.copy2(src, dst)
            return f"Copied {os.path.basename(src)} to {os.path.basename(dst)}."
        except Exception as e:
            return f"Copy failed: {str(e)[:50]}"

    def move_file(self, source: str, destination: str) -> str:
        """Move a file within or across directories."""
        src = os.path.join(self.cwd, source)
        if not os.path.exists(src):
            for entry in os.listdir(self.cwd):
                if entry.lower() == source.lower():
                    src = os.path.join(self.cwd, entry)
                    break
        if not os.path.exists(src):
            return f"Source file {source} not found."
        dst = os.path.join(self.cwd, destination)
        try:
            if os.path.isdir(dst):
                dst = os.path.join(dst, os.path.basename(src))
            shutil.move(src, dst)
            return f"Moved {os.path.basename(src)} to {os.path.basename(dst)}."
        except Exception as e:
            return f"Move failed: {str(e)[:50]}"

    def rename_file(self, old_name: str, new_name: str) -> str:
        """Rename a file."""
        src = os.path.join(self.cwd, old_name)
        if not os.path.exists(src):
            for entry in os.listdir(self.cwd):
                if entry.lower() == old_name.lower():
                    src = os.path.join(self.cwd, entry)
                    old_name = entry
                    break
        if not os.path.exists(src):
            return f"File {old_name} not found."
        dst = os.path.join(self.cwd, new_name)
        try:
            os.rename(src, dst)
            return f"Renamed {old_name} to {new_name}."
        except Exception as e:
            return f"Rename failed: {str(e)[:50]}"

    def delete_file(self, filename: str) -> str:
        """Delete a file (with confirmation expected from caller)."""
        path = os.path.join(self.cwd, filename)
        if not os.path.exists(path):
            for entry in os.listdir(self.cwd):
                if entry.lower() == filename.lower():
                    path = os.path.join(self.cwd, entry)
                    filename = entry
                    break
        if not os.path.exists(path):
            return f"File {filename} not found."
        try:
            if os.path.isdir(path):
                shutil.rmtree(path)
                return f"Deleted folder {filename}."
            os.remove(path)
            return f"Deleted file {filename}."
        except Exception as e:
            return f"Delete failed: {str(e)[:50]}"

    def sort_directory(self, by: str = "name") -> str:
        """List directory contents sorted by name, date, or size."""
        try:
            entries = os.listdir(self.cwd)
            items = []
            for e in entries:
                full = os.path.join(self.cwd, e)
                stat = os.stat(full)
                items.append({
                    "name": e,
                    "is_dir": os.path.isdir(full),
                    "size": stat.st_size,
                    "mtime": stat.st_mtime,
                })
            if by == "date":
                items.sort(key=lambda x: x["mtime"], reverse=True)
            elif by == "size":
                items.sort(key=lambda x: x["size"], reverse=True)
            else:
                items.sort(key=lambda x: x["name"].lower())

            dirs = [i for i in items if i["is_dir"]]
            files = [i for i in items if not i["is_dir"]]
            speech = f"Sorted by {by}. {len(dirs)} folders, {len(files)} files. "
            for i, item in enumerate(items[:10], 1):
                kind = "folder" if item["is_dir"] else f"{round(item['size']/1024, 1)}KB"
                speech += f"{i}. {item['name']} ({kind}). "
            return speech
        except Exception as e:
            return f"Sort failed: {str(e)[:50]}"

if __name__ == "__main__":
    fb = AudioFileBrowser()
    print(fb.get_location_audio())
    print(fb.list_contents_audio())
    print("\nTesting file search for 'py':")
    print(fb.search_files_audio("py"))
