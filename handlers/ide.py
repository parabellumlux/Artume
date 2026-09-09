"""IDE mode command handlers — AI Assistant, Git, Terminal, IDE daemon."""
import os
from earcons import play_earcon


def _cursor_position():
    """Return (line, character) for LSP (0-indexed) from the IDE daemon cursor."""
    try:
        from artome_ide import get_client
        cursor = get_client().get_cursor()
        line = int(cursor.get("line", 1))
        return (max(0, line - 1), int(cursor.get("column", 0) or 0))
    except Exception:
        return (0, 0)


def _lsp_command(kind, file_path, line, column, new_name=None):
    """Run an LSP command via the daemon's persistent client, falling back
    to a per-command pylsp process. Returns the spoken result string."""
    for source in ("daemon", "python"):
        try:
            if source == "daemon":
                from artome_ide import (lsp_go_to_definition, lsp_hover,
                                        lsp_references, lsp_rename)
                if kind == "definition":
                    return lsp_go_to_definition(file_path, line, column)
                if kind == "references":
                    return lsp_references(file_path, line, column)
                if kind == "hover":
                    return lsp_hover(file_path, line, column)
                return lsp_rename(file_path, line, column, new_name)
            from lsp_client import LSPClient
            lsp = LSPClient(["pylsp"], root_uri=os.path.dirname(file_path))
            if not lsp.start():
                return "LSP server not available. Install pylsp."
            try:
                if kind == "definition":
                    return lsp.go_to_definition(file_path, line, column)
                if kind == "references":
                    return lsp.find_references(file_path, line, column)
                if kind == "hover":
                    return lsp.hover(file_path, line, column)
                return lsp.rename(file_path, line, column, new_name)
            finally:
                lsp.stop()
        except Exception:
            if source == "python":
                return "LSP server not available."


def _check_file_errors(filepath):
    """Compile and statically lint a Python file, returning a spoken summary."""
    import io
    import py_compile

    try:
        py_compile.compile(filepath, doraise=True)
    except py_compile.PyCompileError as e:
        return f"Syntax error: {e}"

    try:
        from pyflakes.api import checkPath
        from pyflakes.reporter import Reporter
        stream = io.StringIO()
        reporter = Reporter(stream, stream)
        try:
            checkPath(filepath, reporter, encoding="utf-8")
        except TypeError:
            checkPath(filepath, reporter)
        issues = stream.getvalue().strip().splitlines()
    except ImportError:
        return "No syntax errors."

    if not issues:
        return "No errors found."
    head = issues[:5]
    summary = ". ".join(i.split(":", 2)[-1].strip() for i in head)
    if len(issues) > 5:
        summary += f". And {len(issues) - 5} more issues."
    return f"{len(issues)} issues found. {summary}"


def handle(low_speech, target_lower, target, ctx):
    """Handle IDE mode commands. Returns True if handled."""
    tts = ctx['tts']
    ide = ctx['ide']
    ai_assistant = ctx['ai_assistant']
    git_engine = ctx['git_engine']
    terminal_engine = ctx['terminal_engine']
    confirm_dialog = ctx['confirm_dialog']
    state_mgr = ctx['state_mgr']

    if ide is None:
        tts.speak("IDE Engine is not available.")
        return True

    # --- AI Assistant commands ---
    if "fix this" in low_speech or "fix code" in low_speech or "fix error" in low_speech:
        if ai_assistant is None:
            tts.speak("AI Assistant is not available.")
            return True
        play_earcon("warning")
        if ide.active_file and os.path.exists(ide.active_file):
            with open(ide.active_file, "r") as f:
                code = f.read()
            error_text = low_speech.replace("fix this", "").replace("fix code", "").replace("fix error", "").strip()
            result = ai_assistant.fix_code(code, error=error_text)
            tts.speak("Code fix attempted. " + result[:200])
        else:
            tts.speak("No active file loaded. Say 'open code filename' first.")
        play_earcon("success")
        return True

    if "explain this" in low_speech or "explain code" in low_speech or "what does this do" in low_speech:
        if ai_assistant is None:
            tts.speak("AI Assistant is not available.")
            return True
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            with open(ide.active_file, "r") as f:
                code = f.read()
            result = ai_assistant.explain_code(code)
            tts.speak(result)
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if "generate tests" in low_speech or "write tests" in low_speech or "add tests" in low_speech:
        if ai_assistant is None:
            tts.speak("AI Assistant is not available.")
            return True
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            with open(ide.active_file, "r") as f:
                code = f.read()
            func_name = low_speech.replace("generate tests for", "").replace("write tests for", "").replace("generate tests", "").replace("write tests", "").strip() or "main"
            result = ai_assistant.write_test(code, func_name)
            tts.speak("Tests generated. " + result[:200])
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if "review code" in low_speech or "review this" in low_speech:
        if ai_assistant is None:
            tts.speak("AI Assistant is not available.")
            return True
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            with open(ide.active_file, "r") as f:
                code = f.read()
            result = ai_assistant.review_code(code)
            tts.speak(result)
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if "optimize code" in low_speech or "optimize this" in low_speech:
        if ai_assistant is None:
            tts.speak("AI Assistant is not available.")
            return True
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            with open(ide.active_file, "r") as f:
                code = f.read()
            result = ai_assistant.optimize_code(code)
            tts.speak("Optimization attempted. " + result[:200])
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if "add docstring" in low_speech or "generate docstring" in low_speech:
        if ai_assistant is None:
            tts.speak("AI Assistant is not available.")
            return True
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            with open(ide.active_file, "r") as f:
                code = f.read()
            result = ai_assistant.add_docstring(code)
            tts.speak("Docstring added. " + result[:200])
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if "add type hints" in low_speech or "add type annotations" in low_speech:
        if ai_assistant is None:
            tts.speak("AI Assistant is not available.")
            return True
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            with open(ide.active_file, "r") as f:
                code = f.read()
            result = ai_assistant.add_type_hints(code)
            tts.speak("Type hints added. " + result[:200])
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if "refactor code" in low_speech or "refactor this" in low_speech:
        if ai_assistant is None:
            tts.speak("AI Assistant is not available.")
            return True
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            with open(ide.active_file, "r") as f:
                code = f.read()
            refactor_target = low_speech.replace("refactor code to", "").replace("refactor code", "").replace("refactor this to", "").replace("refactor this", "").strip() or "improve structure"
            result = ai_assistant.refactor_code(code, refactor_target)
            tts.speak("Refactoring attempted. " + result[:200])
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    # --- Git commands ---
    if git_engine is None:
        git_handled = any(kw in low_speech for kw in ["git status", "git changes", "git diff", "git log", "git history", "git branch", "current branch", "git commit", "git push", "git pull", "git switch", "checkout branch"])
        if git_handled:
            tts.speak("Git Engine is not available.")
            return True
    else:
        if "git status" in low_speech or "git changes" in low_speech:
            play_earcon("git_modified")
            result = git_engine.status()
            tts.speak(result)
            play_earcon("success")
            return True

        if "git diff" in low_speech:
            play_earcon("git_modified")
            filepath = low_speech.replace("git diff", "").strip()
            result = git_engine.diff(filepath if filepath else None)
            tts.speak(result)
            play_earcon("success")
            return True

        if "git log" in low_speech or "git history" in low_speech:
            play_earcon("info")
            result = git_engine.log()
            tts.speak(result)
            play_earcon("success")
            return True

        if "git branch" in low_speech or "current branch" in low_speech:
            play_earcon("info")
            result = git_engine.branch()
            tts.speak(result)
            play_earcon("success")
            return True

        if "git commit" in low_speech:
            play_earcon("git_added")
            message = low_speech.replace("git commit", "").replace("commit", "").strip()
            if not message:
                message = "Voice commit from Artome IDE"
            if confirm_dialog.confirm_action("commit", message):
                result = git_engine.commit(message)
                tts.speak(result)
            else:
                tts.speak("Commit cancelled.")
            play_earcon("success")
            return True

        if "git push" in low_speech:
            play_earcon("warning")
            if confirm_dialog.confirm_destructive("push to remote"):
                result = git_engine.push()
                tts.speak(result)
            else:
                tts.speak("Push cancelled.")
            play_earcon("success")
            return True

        if "git pull" in low_speech:
            play_earcon("info")
            result = git_engine.pull()
            tts.speak(result)
            play_earcon("success")
            return True

        if "git switch" in low_speech or "checkout branch" in low_speech:
            play_earcon("info")
            branch = low_speech.replace("git switch to", "").replace("git switch", "").replace("checkout branch", "").replace("switch to branch", "").strip()
            if branch:
                result = git_engine.switch_branch(branch)
                tts.speak(result)
            else:
                tts.speak("Which branch? Say 'git switch to branch-name'.")
            play_earcon("success")
            return True

    # --- Terminal commands ---
    if terminal_engine is None:
        term_handled = any(kw in low_speech for kw in ["run tests", "run test", "run file", "run this file", "run make", "run build", "make build", "run command", "run ", "show terminal", "terminal output", "clear terminal"])
        if term_handled:
            tts.speak("Terminal is not available.")
            return True
    else:
        if "run tests" in low_speech or "run test" in low_speech:
            play_earcon("info")
            test_path = low_speech.replace("run tests in", "").replace("run tests", "").replace("run test in", "").replace("run test", "").strip()
            result = terminal_engine.run_tests(test_path if test_path else None)
            tts.speak(result[:300])
            play_earcon("success")
            return True

        if "run file" in low_speech or "run this file" in low_speech:
            play_earcon("info")
            if ide.active_file and os.path.exists(ide.active_file):
                result = terminal_engine.run_file(ide.active_file)
                tts.speak(result[:300])
            else:
                tts.speak("No active file loaded. Say 'open code filename' first.")
            play_earcon("success")
            return True

        if "run make" in low_speech or "run build" in low_speech or "make build" in low_speech:
            play_earcon("info")
            cmd = low_speech.replace("run make", "make").replace("run build", "make build").strip()
            result = terminal_engine.run(cmd, timeout=120)
            tts.speak(result[:300])
            play_earcon("success")
            return True

        if "run command" in low_speech or "run" in low_speech:
            cmd = low_speech.replace("run command", "").replace("run", "").strip()
            if cmd and len(cmd) > 2:
                play_earcon("info")
                result = terminal_engine.run(cmd)
                tts.speak(result[:300])
                play_earcon("success")
                return True

        if "stop" in low_speech or "cancel" in low_speech or "interrupt" in low_speech:
            play_earcon("warning")
            result = terminal_engine.stop()
            tts.speak(result)
            play_earcon("success")
            return True

        if "show terminal" in low_speech or "terminal output" in low_speech:
            play_earcon("info")
            result = terminal_engine.show_terminal()
            tts.speak(result[:300])
            play_earcon("success")
            return True

        if "clear terminal" in low_speech:
            play_earcon("info")
            result = terminal_engine.clear_terminal()
            tts.speak(result)
            play_earcon("success")
            return True

    # --- LSP commands (go to definition, find references, rename, hover) ---
    if any(kw in low_speech for kw in ["go to definition", "jump to definition", "find definition",
                                        "where is this defined", "where defined"]):
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            lsp_line, lsp_col = _cursor_position()
            tts.speak(_lsp_command("definition", ide.active_file, lsp_line, lsp_col))
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if any(kw in low_speech for kw in ["find references", "show references", "who uses this",
                                        "where is this used", "all references"]):
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            lsp_line, lsp_col = _cursor_position()
            tts.speak(_lsp_command("references", ide.active_file, lsp_line, lsp_col))
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if any(kw in low_speech for kw in ["hover", "what is this", "type info", "type of",
                                        "what type", "what kind"]):
        play_earcon("info")
        if ide.active_file and os.path.exists(ide.active_file):
            lsp_line, lsp_col = _cursor_position()
            tts.speak(_lsp_command("hover", ide.active_file, lsp_line, lsp_col))
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if any(kw in low_speech for kw in ["rename symbol", "rename this", "rename to",
                                        "rename function", "rename variable", "rename class"]):
        import re
        new_name = re.sub(r'^(rename (symbol|this|function|variable|class)?)\s*', '', low_speech)
        if not new_name:
            new_name = re.sub(r'^rename\s*', '', low_speech)
        if new_name:
            play_earcon("info")
            if ide.active_file and os.path.exists(ide.active_file):
                lsp_line, lsp_col = _cursor_position()
                tts.speak(_lsp_command("rename", ide.active_file, lsp_line, lsp_col, new_name))
            else:
                tts.speak("No active file loaded.")
        else:
            tts.speak("Rename to what? Say 'rename to' followed by the new name.")
        play_earcon("success")
        return True

    # --- DAP commands (debug, step, continue, breakpoint) ---
    if any(kw in low_speech for kw in ["start debugging", "debug this", "debug file",
                                        "start debug session"]):
        play_earcon("warning")
        if ide.active_file and os.path.exists(ide.active_file):
            from debug_adapter import get_debug_adapter
            result = get_debug_adapter().start(ide.active_file)
            tts.speak(result)
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if any(kw in low_speech for kw in ["set breakpoint", "add breakpoint", "break here"]):
        import re
        nums = re.findall(r'\d+', low_speech)
        line = int(nums[0]) if nums else 1
        play_earcon("info")
        from debug_adapter import get_debug_adapter
        client = get_debug_adapter().session_client()
        if client is None:
            tts.speak("No active debug session. Say debug file first.")
        elif ide.active_file and os.path.exists(ide.active_file):
            result = client.set_breakpoint(ide.active_file, line)
            tts.speak(result)
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if any(kw in low_speech for kw in ["continue", "resume", "keep running"]):
        play_earcon("info")
        from debug_adapter import get_debug_adapter
        result = get_debug_adapter().run()
        tts.speak(result)
        play_earcon("success")
        return True

    if any(kw in low_speech for kw in ["step over", "next line", "step"]):
        play_earcon("info")
        from debug_adapter import get_debug_adapter
        client = get_debug_adapter().session_client()
        if client:
            result = client.step_over()
            tts.speak(result)
            play_earcon("success")
            return True
        if "step" in low_speech:
            tts.speak("No active debug session.")
            play_earcon("success")
            return True
        # "next line" without a live session → falls through to spoken navigation

    if any(kw in low_speech for kw in ["step into", "go into", "enter function"]):
        play_earcon("info")
        from debug_adapter import get_debug_adapter
        client = get_debug_adapter().session_client()
        if client:
            result = client.step_into()
            tts.speak(result)
        else:
            tts.speak("No active debug session.")
        play_earcon("success")
        return True

    if any(kw in low_speech for kw in ["step out", "exit function", "return from"]):
        play_earcon("info")
        from debug_adapter import get_debug_adapter
        client = get_debug_adapter().session_client()
        if client:
            result = client.step_out()
            tts.speak(result)
        else:
            tts.speak("No active debug session.")
        play_earcon("success")
        return True

    if low_speech.startswith("evaluate") or "check variable" in low_speech:
        expr = low_speech.replace("evaluate", "").replace("check variable", "").strip()
        if expr:
            play_earcon("info")
            from debug_adapter import get_debug_adapter
            client = get_debug_adapter().session_client()
            if client:
                result = client.evaluate(expr)
                tts.speak(result)
            else:
                tts.speak("No active debug session.")
            play_earcon("success")
        return True

    # --- IDE daemon commands ---
    if "ide structure" in low_speech or "code structure" in low_speech:
        play_earcon("scope_enter_class")
        try:
            from artome_ide import get_structure
            result = get_structure()
            tts.speak(result[:300])
        except Exception as e:
            tts.speak(f"IDE daemon error: {str(e)[:40]}")
        play_earcon("scope_exit_class")
        return True

    if "ide summary" in low_speech or "code summary" in low_speech:
        play_earcon("info")
        try:
            from artome_ide import get_summary
            result = get_summary()
            tts.speak(result[:300])
        except Exception as e:
            tts.speak(f"IDE daemon error: {str(e)[:40]}")
        play_earcon("success")
        return True

    if any(kw in low_speech for kw in ["show errors", "show diagnostics", "check errors",
                                        "any errors", "find errors", "list errors"]):
        play_earcon("warning")
        if ide.active_file and os.path.exists(ide.active_file):
            try:
                result = _check_file_errors(ide.active_file)
            except Exception as e:
                result = f"Error check failed: {str(e)[:40]}"
            tts.speak(result)
        else:
            tts.speak("No active file loaded.")
        play_earcon("success")
        return True

    if "go to function" in low_speech or "jump to function" in low_speech or \
            ("function" in low_speech and any(w in low_speech for w in ["go to", "jump to", "find"])):
        import re
        name = re.sub(r'^(go to|jump to|find)\s+function\s*', '', low_speech).strip()
        if name:
            play_earcon("info")
            try:
                from artome_ide import go_to_function
                tts.speak(go_to_function(name))
            except Exception as e:
                tts.speak(f"IDE daemon error: {str(e)[:40]}")
            play_earcon("found_match")
            return True

    # --- Spoken navigation (cursor movement) ---
    if any(kw in low_speech for kw in
           ["next function", "next class", "next method", "next symbol",
            "previous function", "previous class", "previous method",
            "previous symbol", "prev function", "prev class", "prev method",
            "prev symbol"]):
        play_earcon("found_match")
        direction = -1 if ("previous" in low_speech or "prev" in low_speech) else 1
        if "method" in low_speech:
            kinds = ("function", "method")
        elif "class" in low_speech:
            kinds = ("class", "function")
        else:
            kinds = ("function", "class")
        try:
            from artome_ide import next_symbol
            result = next_symbol(direction, kinds)
            tts.speak(result["speech"])
            if result["line"]:
                tts.speak(ide.read_lines(start_line=result["line"], count=1))
        except Exception as e:
            tts.speak(f"IDE daemon error: {str(e)[:40]}")
        play_earcon("scope_enter_function")
        return True

    if "go to line" in low_speech or "jump to line" in low_speech:
        import re
        nums = [int(n) for n in re.findall(r'\d+', low_speech)]
        if nums:
            play_earcon("found_match")
            try:
                from artome_ide import go_to_line
                target_line = nums[0]
                tts.speak(go_to_line(target_line))
                tts.speak(ide.read_lines(start_line=target_line, count=1))
            except Exception as e:
                tts.speak(f"IDE daemon error: {str(e)[:40]}")
            play_earcon("scope_enter_function")
            return True

    if any(kw in low_speech for kw in ["next line", "previous line", "go up",
                                       "go down", "move up", "move down",
                                       "jump down", "jump up", "line down",
                                       "line up"]):
        play_earcon("found_match")
        import re
        nums = [int(n) for n in re.findall(r'\d+', low_speech)]
        amount = nums[0] if nums else 10
        down = any(k in low_speech for k in ["next line", "down", "jump down"])
        delta = amount if down else -amount
        try:
            from artome_ide import move_lines
            moved = move_lines(delta)
            tts.speak(moved["speech"])
            if moved["line"]:
                tts.speak(ide.read_lines(start_line=moved["line"], count=1))
        except Exception as e:
            tts.speak(f"IDE daemon error: {str(e)[:40]}")
        play_earcon("scope_enter_function")
        return True

    if any(kw in low_speech for kw in ["start of file", "top of file", "beginning of file",
                                       "end of file", "bottom of file", "end of the file",
                                       "top of the file"]):
        play_earcon("found_match")
        direction = -1 if any(k in low_speech for k in ["start of", "top of", "beginning of"]) else 1
        try:
            from artome_ide import go_to_edge
            moved = go_to_edge(direction)
            tts.speak(moved["speech"])
            if moved["line"]:
                tts.speak(ide.read_lines(start_line=moved["line"], count=1))
        except Exception as e:
            tts.speak(f"IDE daemon error: {str(e)[:40]}")
        play_earcon("scope_enter_function")
        return True

    if "where am i" in low_speech or "cursor position" in low_speech:
        play_earcon("info")
        try:
            from artome_ide import where_am_i
            result = where_am_i()
            tts.speak(result)
        except Exception as e:
            tts.speak(f"IDE daemon error: {str(e)[:40]}")
        play_earcon("found_match")
        return True

    # --- Original IDE commands ---
    if "open" in target_lower or "load" in target_lower:
        filename = target.replace("open code", "").replace("open", "").replace("load", "").strip() or "artome_core.py"
        play_earcon("scope_enter_function")
        result = ide.load_file(filename)
        if ide.active_file:
            state_mgr.set_active_file(ide.active_file)
            state_mgr.add_recent_file(ide.active_file)
        tts.speak(result)
    elif "function" in target_lower or "def" in target_lower:
        func_name = target.replace("read function", "").replace("function", "").strip()
        play_earcon("scope_enter_function")
        tts.speak(ide.read_function(func_name))
        play_earcon("scope_exit_function")
    elif "line" in target_lower or "lines" in target_lower:
        play_earcon("info")
        import re
        nums = [int(n) for n in re.findall(r'\d+', low_speech)]
        if len(nums) >= 2:
            start, end = nums[0], nums[1]
            if end < start:
                start, end = end, start
            tts.speak(ide.read_lines(start_line=start, count=end - start + 1))
        elif len(nums) == 1:
            tts.speak(ide.read_lines(start_line=nums[0], count=1))
        else:
            line, _col = _cursor_position()
            tts.speak(ide.read_lines(start_line=line + 1, count=10))
    elif "next" in low_speech:
        play_earcon("scope_enter_function")
        tts.speak(ide.read_function(""))
    elif "previous" in low_speech or "prev" in low_speech:
        play_earcon("scope_exit_function")
        tts.speak(ide.read_function(""))
    else:
        play_earcon("scope_enter_function")
        tts.speak(ide.load_file("artome_core.py"))
    play_earcon("success")
    return True
