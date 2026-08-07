"""Artome Audio-First IDE — Git integration.

Provides voice-driven Git operations through the IDE.
"""

import subprocess
from pathlib import Path
from typing import Optional


class GitEngine:
    """Voice-driven Git operations."""

    def __init__(self, project_root: Optional[str] = None):
        self.project_root = Path(project_root).resolve() if project_root else Path.cwd()

    def _git(self, *args: str) -> str:
        """Run a git command and return stdout."""
        try:
            result = subprocess.run(
                ["git", "-C", str(self.project_root), *args],
                capture_output=True, text=True, timeout=30,
            )
            return result.stdout.strip()
        except subprocess.TimeoutExpired:
            return "Git command timed out."
        except FileNotFoundError:
            return "Git not found. Is it installed?"
        except subprocess.CalledProcessError as e:
            return f"Git error: {e.stderr.strip()[:100]}"

    def status(self) -> str:
        """Read changed files with added/modified/deleted earcon cues."""
        output = self._git("status", "--short")
        if not output:
            return "No changes. Working tree is clean."
        lines = output.split("\n")
        parts = []
        for line in lines[:20]:
            if not line.strip():
                continue
            status = line[:2].strip()
            filepath = line[2:].strip()
            if status == "M":
                parts.append(f"Modified: {filepath}")
            elif status == "A":
                parts.append(f"Added: {filepath}")
            elif status == "D":
                parts.append(f"Deleted: {filepath}")
            elif status == "??":
                parts.append(f"Untracked: {filepath}")
            elif status == "R":
                parts.append(f"Renamed: {filepath}")
            else:
                parts.append(f"{status}: {filepath}")
        count = len(lines)
        return f"{count} change{'s' if count != 1 else ''}. " + ". ".join(parts[:10])

    def diff(self, filepath: Optional[str] = None) -> str:
        """Read diff with line-by-line changes."""
        if filepath:
            output = self._git("diff", filepath)
        else:
            output = self._git("diff")
        if not output:
            return "No changes to show."
        # Summarize: count added/removed lines
        added = output.count("\n+") - output.count("\n+++")
        removed = output.count("\n-") - output.count("\n---")
        files = output.count("diff --git")
        return f"{files} file{'s' if files != 1 else ''} changed. {added} insertions, {removed} deletions."

    def log(self, count: int = 5) -> str:
        """Read recent commits."""
        output = self._git("log", f"--oneline", f"-{count}")
        if not output:
            return "No commits found."
        lines = output.split("\n")
        parts = []
        for line in lines:
            if line.strip():
                parts.append(line.strip())
        return "Recent commits: " + ". ".join(parts)

    def branch(self) -> str:
        """Read current branch name."""
        current = self._git("rev-parse", "--abbrev-ref", "HEAD")
        branches = self._git("branch")
        count = len([b for b in branches.split("\n") if b.strip()])
        return f"On branch {current}. {count} branches total."

    def commit(self, message: str) -> str:
        """Stage all and commit."""
        self._git("add", "-A")
        output = self._git("commit", "-m", message)
        if "nothing to commit" in output:
            return "Nothing to commit."
        return f"Committed: {message}"

    def push(self) -> str:
        """Push to remote."""
        output = self._git("push")
        if "Everything up-to-date" in output:
            return "Already up to date."
        return "Pushed to remote."

    def pull(self) -> str:
        """Pull from remote."""
        output = self._git("pull")
        if "Already up to date" in output:
            return "Already up to date."
        return "Pulled from remote."

    def switch_branch(self, name: str) -> str:
        """Checkout branch."""
        output = self._git("checkout", name)
        if "error" in output:
            return f"Failed to switch to {name}: {output[:80]}"
        return f"Switched to branch {name}."

    def show_file_changes(self, filepath: str) -> str:
        """Read diff for specific file."""
        return self.diff(filepath)
