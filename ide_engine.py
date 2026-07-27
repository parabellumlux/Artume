#!/usr/bin/env python3
"""Artome Audio-Only AI-Assisted IDE Engine - AST Code Navigator & Voice Debugger."""

import os
import ast
import traceback

class AudioIDE:
    """Voice-first code navigator, editor, and traceback summarizer for Artome OS."""

    def __init__(self):
        self.active_file = None

    def load_file(self, filepath):
        """Load code file and return an audio structural overview (classes, functions)."""
        if not os.path.isabs(filepath):
            filepath = os.path.abspath(filepath)

        if not os.path.exists(filepath):
            return f"File not found: {filepath}"

        self.active_file = filepath
        filename = os.path.basename(filepath)

        try:
            with open(filepath, "r", encoding="utf-8") as f:
                code_content = f.read()

            parsed_ast = ast.parse(code_content)
            classes = [node.name for node in ast.walk(parsed_ast) if isinstance(node, ast.ClassDef)]
            functions = [node.name for node in ast.walk(parsed_ast) if isinstance(node, ast.FunctionDef)]
            total_lines = len(code_content.splitlines())

            summary = f"Opened {filename}. Total {total_lines} lines. "
            if classes:
                summary += f"Contains {len(classes)} classes: {', '.join(classes)}. "
            if functions:
                summary += f"Contains {len(functions)} functions: {', '.join(functions[:8])}. "
            if not classes and not functions:
                summary += "No class or function definitions found. "

            return summary
        except Exception as e:
            return f"Loaded {filename}, but failed to parse syntax: {str(e)[:50]}"

    def read_function(self, function_name):
        """Read source code of a specific function out loud."""
        if not self.active_file or not os.path.exists(self.active_file):
            return "No active file loaded in IDE."

        try:
            with open(self.active_file, "r", encoding="utf-8") as f:
                lines = f.readlines()

            code_content = "".join(lines)
            parsed_ast = ast.parse(code_content)

            for node in ast.walk(parsed_ast):
                if isinstance(node, ast.FunctionDef) and node.name.lower() == function_name.lower():
                    start_line = node.lineno
                    end_line = node.end_lineno
                    func_code = "".join(lines[start_line - 1:end_line])
                    return f"Function {function_name} from line {start_line} to {end_line}: {func_code}"

            return f"Function {function_name} not found in {os.path.basename(self.active_file)}."
        except Exception as e:
            return f"Error reading function: {str(e)[:50]}"

    def read_lines(self, start_line=1, count=10):
        """Read line range from active file."""
        if not self.active_file or not os.path.exists(self.active_file):
            return "No active file loaded in IDE."

        try:
            with open(self.active_file, "r", encoding="utf-8") as f:
                lines = f.readlines()

            end_line = min(start_line + count - 1, len(lines))
            snippet = "".join(lines[start_line - 1:end_line])
            return f"Lines {start_line} to {end_line}: {snippet}"
        except Exception as e:
            return f"Error reading lines: {str(e)[:50]}"

    def summarize_traceback(self, traceback_str):
        """Summarize stack trace error into plain English speech."""
        lines = [l.strip() for l in traceback_str.splitlines() if l.strip()]
        if not lines:
            return "No error traceback recorded."

        last_line = lines[-1]
        summary = f"Execution error detected: {last_line}. "

        # Look for file and line number
        for line in reversed(lines):
            if "File " in line and ", line " in line:
                summary += f"Location: {line}. "
                break

        return summary

if __name__ == "__main__":
    ide = AudioIDE()
    print("Testing Audio IDE file loading...")
    print(ide.load_file("earcons.py"))
    print("\nTesting function inspection...")
    print(ide.read_function("play_earcon"))
