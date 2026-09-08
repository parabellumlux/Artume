"""Tests for artome_ide/ai_assistant.py — AI assistant with mock requests."""

from unittest.mock import patch, MagicMock

import pytest

from artome_ide.ai_assistant import AiAssistant


@pytest.fixture
def ai():
    return AiAssistant()


class TestQuery:
    @patch("artome_ide.ai_assistant.requests.post")
    def test_query_returns_response(self, mock_post, ai):
        mock_post.return_value = MagicMock(
            json=lambda: {"response": "Hello world"}
        )
        result = ai._query("test-model", "prompt")
        assert result == "Hello world"
        mock_post.assert_called_once()

    @patch("artome_ide.ai_assistant.requests.post", side_effect=Exception("timeout"))
    def test_query_returns_none_on_error(self, mock_post, ai):
        result = ai._query("test-model", "prompt")
        assert result is None


class TestFixCode:
    @patch.object(AiAssistant, "_query")
    def test_fix_extracts_code_block(self, mock_query, ai):
        mock_query.return_value = "```python\ndef foo(): pass\n```"
        result = ai.fix_code("def foo(): pass")
        assert "def foo" in result
        assert "```" not in result

    @patch.object(AiAssistant, "_query", return_value=None)
    def test_fix_fallback(self, mock_query, ai):
        result = ai.fix_code("broken code")
        assert "could not" in result.lower()


class TestExplainCode:
    @patch.object(AiAssistant, "_query")
    def test_explain(self, mock_query, ai):
        mock_query.return_value = "This function returns 42."
        result = ai.explain_code("def f(): return 42")
        assert "42" in result

    @patch.object(AiAssistant, "_query", return_value=None)
    def test_explain_fallback(self, mock_query, ai):
        result = ai.explain_code("code")
        assert "could not" in result.lower()


class TestAddDocstring:
    @patch.object(AiAssistant, "_query")
    def test_add_docstring(self, mock_query, ai):
        mock_query.return_value = "```python\ndef f():\n    \"\"\"Docstring.\"\"\"\n    pass\n```"
        result = ai.add_docstring("def f(): pass")
        assert "Docstring" in result
        assert "```" not in result

    @patch.object(AiAssistant, "_query", return_value=None)
    def test_add_docstring_fallback(self, mock_query, ai):
        code = "def f(): pass"
        result = ai.add_docstring(code)
        assert result == code


class TestReviewCode:
    @patch.object(AiAssistant, "_query")
    def test_review(self, mock_query, ai):
        mock_query.return_value = "No issues found."
        result = ai.review_code("code")
        assert "No issues" in result

    @patch.object(AiAssistant, "_query", return_value=None)
    def test_review_fallback(self, mock_query, ai):
        result = ai.review_code("code")
        assert "could not" in result.lower()


class TestWriteTest:
    @patch.object(AiAssistant, "_query")
    def test_write_test(self, mock_query, ai):
        mock_query.return_value = "```python\ndef test_foo(): assert True\n```"
        result = ai.write_test("def foo(): pass", "foo")
        assert "test_foo" in result or "assert" in result
