"""Artome Audio-First IDE — AI-assisted editing via Ollama.

Uses Llama 3.1 8B on GTX 1080 for code reasoning tasks.
"""

import json
import os
import re
import requests
from typing import Optional

OLLAMA_URL = "http://localhost:11434/api/generate"
REASONING_MODEL = "llama3.1:8b"
ROUTER_MODEL = "nemotron-3-nano:4b"


class AiAssistant:
    """AI-assisted code editing via Ollama."""

    def _query(self, model: str, prompt: str, temperature: float = 0.3, max_tokens: int = 512) -> Optional[str]:
        """Send a prompt to Ollama and return the response."""
        try:
            res = requests.post(
                OLLAMA_URL,
                json={
                    "model": model,
                    "prompt": prompt,
                    "stream": False,
                    "options": {
                        "temperature": temperature,
                        "num_predict": max_tokens,
                    },
                },
                timeout=30,
            )
            return res.json().get("response", "").strip()
        except Exception as e:
            return None

    def fix_code(self, code: str, error: str = "") -> str:
        """Fix code with optional error context."""
        prompt = f"""Fix this code. Return ONLY the corrected code, no explanation.

{code}

Error: {error}

Corrected code:"""
        result = self._query(REASONING_MODEL, prompt, temperature=0.2, max_tokens=1024)
        if result:
            # Extract code block if present
            code_match = re.search(r"```(?:\w+)?\n(.*?)\n```", result, re.DOTALL)
            return code_match.group(1) if code_match else result
        return "Could not fix the code."

    def explain_code(self, code: str) -> str:
        """Explain what code does in 2-3 sentences."""
        prompt = f"""Explain this code in 2-3 concise sentences for a blind programmer:

```python
{code}
```

Explanation:"""
        result = self._query(REASONING_MODEL, prompt, temperature=0.3, max_tokens=256)
        return result or "Could not explain the code."

    def add_docstring(self, code: str) -> str:
        """Add a docstring to a function."""
        prompt = f"""Add a docstring to this function. Return the complete function with docstring:

{code}

Complete function with docstring:"""
        result = self._query(REASONING_MODEL, prompt, temperature=0.3, max_tokens=1024)
        if result:
            code_match = re.search(r"```(?:\w+)?\n(.*?)\n```", result, re.DOTALL)
            return code_match.group(1) if code_match else result
        return code

    def add_type_hints(self, code: str) -> str:
        """Add type annotations to a function."""
        prompt = f"""Add type hints to this Python function. Return the complete function with type hints:

{code}

Complete function with type hints:"""
        result = self._query(REASONING_MODEL, prompt, temperature=0.2, max_tokens=1024)
        if result:
            code_match = re.search(r"```(?:\w+)?\n(.*?)\n```", result, re.DOTALL)
            return code_match.group(1) if code_match else result
        return code

    def write_test(self, code: str, function_name: str) -> str:
        """Generate a test for a function."""
        prompt = f"""Write a pytest test for this function. Return ONLY the test code:

```python
{code}
```

Test for {function_name}:"""
        result = self._query(REASONING_MODEL, prompt, temperature=0.4, max_tokens=1024)
        if result:
            code_match = re.search(r"```(?:\w+)?\n(.*?)\n```", result, re.DOTALL)
            return code_match.group(1) if code_match else result
        return "Could not generate test."

    def refactor_code(self, code: str, target: str) -> str:
        """Refactor code to use a different pattern."""
        prompt = f"""Refactor this code to {target}. Return ONLY the refactored code:

```python
{code}
```

Refactored code:"""
        result = self._query(REASONING_MODEL, prompt, temperature=0.3, max_tokens=1024)
        if result:
            code_match = re.search(r"```(?:\w+)?\n(.*?)\n```", result, re.DOTALL)
            return code_match.group(1) if code_match else result
        return code

    def review_code(self, code: str) -> str:
        """Review code for issues."""
        prompt = f"""Review this code for bugs, performance issues, and style problems. Be concise (2-3 sentences):

```python
{code}
```

Review:"""
        result = self._query(REASONING_MODEL, prompt, temperature=0.3, max_tokens=256)
        return result or "Could not review the code."

    def optimize_code(self, code: str) -> str:
        """Suggest performance improvements."""
        prompt = f"""Optimize this code for performance. Return ONLY the optimized code:

```python
{code}
```

Optimized code:"""
        result = self._query(REASONING_MODEL, prompt, temperature=0.3, max_tokens=1024)
        if result:
            code_match = re.search(r"```(?:\w+)?\n(.*?)\n```", result, re.DOTALL)
            return code_match.group(1) if code_match else result
        return code
