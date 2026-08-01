#!/usr/bin/env python3
"""Artume Skill System — pluggable capability discovery and dispatch.

Skills are self-contained Python modules in ~/.config/artume/skills/ that
declare what intents they handle and provide an execute() function.

A skill is a directory with a skill.toml manifest and optionally a
script.py or prompt template:

    ~/.config/artume/skills/
    ├── weather/
    │   ├── skill.toml
    │   └── script.py
    └── timer/
        ├── skill.toml
        └── script.py

skill.toml format:
    [skill]
    name = "weather"
    description = "Check weather forecasts"
    version = "1.0.0"
    intents = ["web_fetch", "conversation"]
    script = "script.py"          # optional
    prompt_template = ""          # optional, {input} placeholder
"""

import importlib.util
import json
import os
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any, Callable, Optional


def skills_dir() -> Path:
    """Get the skills directory (XDG config dir)."""
    xdg_config = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
    return Path(xdg_config) / "artume" / "skills"


# ---------------------------------------------------------------------------
# Skill manifest
# ---------------------------------------------------------------------------

class SkillMeta:
    """Metadata loaded from a skill.toml manifest."""

    def __init__(self, data: dict):
        self.name: str = data["name"]
        self.description: str = data.get("description", "")
        self.version: str = data.get("version", "0.1.0")
        self.intents: list[str] = data.get("intents", [])
        self.script: Optional[str] = data.get("script")
        self.prompt_template: Optional[str] = data.get("prompt_template")

    @classmethod
    def load(cls, path: Path) -> Optional["SkillMeta"]:
        manifest_path = path / "skill.toml"
        if not manifest_path.exists():
            return None
        with open(manifest_path, "rb") as f:
            data = tomllib.load(f)
        return cls(data.get("skill", data))


# ---------------------------------------------------------------------------
# Skill registry
# ---------------------------------------------------------------------------

class SkillRegistry:
    """Registry of all available skills — built-in + file-based."""

    def __init__(self):
        self._builtin: dict[str, Callable] = {}
        self._file_skills: dict[str, SkillMeta] = {}
        self._intent_index: dict[str, list[str]] = {}
        self._loaded: bool = False

    def register_builtin(self, name: str, handler: Callable, intents: list[str], description: str = ""):
        """Register a built-in skill handler."""
        self._builtin[name] = handler
        for intent in intents:
            self._intent_index.setdefault(intent, []).append(name)

    def load_file_skills(self):
        """Scan the skills directory and load all skill.toml manifests."""
        sdir = skills_dir()
        if not sdir.exists():
            return

        for entry in sorted(sdir.iterdir()):
            if entry.is_dir():
                meta = SkillMeta.load(entry)
                if meta:
                    self._file_skills[meta.name] = meta
                    for intent in meta.intents:
                        self._intent_index.setdefault(intent, []).append(meta.name)

        self._loaded = True

    def skills_for_intent(self, intent: str) -> list[str]:
        """Find skill names that handle a given intent."""
        if not self._loaded:
            self.load_file_skills()
        return self._intent_index.get(intent, [])

    def execute(self, name: str, user_text: str, context: dict[str, Any] = None) -> Optional[str]:
        """Execute a skill by name. Tries built-in first, then file-based."""
        # Built-in
        if name in self._builtin:
            return self._builtin[name](user_text, context or {})

        # File-based
        meta = self._file_skills.get(name)
        if not meta:
            return None

        # Script execution
        if meta.script:
            sdir = skills_dir() / name
            script_path = sdir / meta.script
            if script_path.exists():
                try:
                    result = subprocess.run(
                        [sys.executable, str(script_path), user_text],
                        capture_output=True, text=True, timeout=30,
                    )
                    if result.returncode == 0 and result.stdout.strip():
                        return result.stdout.strip()
                except (subprocess.TimeoutExpired, OSError) as e:
                    print(f"Skill '{name}': script failed: {e}")

        # Prompt template (LLM-based)
        if meta.prompt_template:
            prompt = meta.prompt_template.replace("{input}", user_text)
            return f"[Skill '{name}' would process: {user_text}]"

        return None

    def list_skills(self) -> list[dict]:
        """List all registered skills."""
        skills = []
        for name, handler in self._builtin.items():
            skills.append({
                "name": name,
                "description": getattr(handler, "__doc__", "") or "",
                "source": "builtin",
            })
        for name, meta in self._file_skills.items():
            skills.append({
                "name": meta.name,
                "description": meta.description,
                "source": "file",
            })
        return skills


# ---------------------------------------------------------------------------
# Global registry singleton
# ---------------------------------------------------------------------------

_registry: Optional[SkillRegistry] = None


def get_registry() -> SkillRegistry:
    """Get or create the global skill registry."""
    global _registry
    if _registry is None:
        _registry = SkillRegistry()
        _register_defaults(_registry)
        _registry.load_file_skills()
    return _registry


def _register_defaults(registry: SkillRegistry):
    """Register built-in skills."""

    def web_fetch(user_text: str, ctx: dict) -> str:
        """Fetch and summarize web pages from URLs."""
        return f"[WebFetch] Would fetch: {user_text}"

    def file_search(user_text: str, ctx: dict) -> str:
        """Search indexed files on the local filesystem."""
        return f"[FileSearch] Would search: {user_text}"

    def entity_lookup(user_text: str, ctx: dict) -> str:
        """Look up entities from recent conversation context."""
        return f"[EntityLookup] Would look up: {user_text}"

    def system_command(user_text: str, ctx: dict) -> str:
        """Execute system commands (volume, settings, help)."""
        return f"[SystemCommand] Would execute: {user_text}"

    registry.register_builtin("web_fetch", web_fetch, ["web_fetch", "web", "browse"])
    registry.register_builtin("file_search", file_search, ["file_search", "file", "search"])
    registry.register_builtin("entity_lookup", entity_lookup, ["entity_lookup", "lookup"])
    registry.register_builtin("system_command", system_command, ["system_command", "command"])


# ---------------------------------------------------------------------------
# Convenience
# ---------------------------------------------------------------------------

def execute_skill(name: str, user_text: str, context: dict = None) -> Optional[str]:
    """Execute a skill by name."""
    return get_registry().execute(name, user_text, context)


def skills_for_intent(intent: str) -> list[str]:
    """Find skills that handle a given intent."""
    return get_registry().skills_for_intent(intent)


def list_skills() -> list[dict]:
    """List all registered skills."""
    return get_registry().list_skills()
