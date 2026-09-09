"""Mode-specific command handlers — EBook, Docs, Settings, Files, Browser, Email."""
import subprocess
import shlex
from earcons import play_earcon

ALLOWED_CMDS = {"ls", "pwd", "date", "uptime", "df", "du", "free", "top",
                "ps", "whoami", "hostname", "uname", "cat", "head", "tail",
                "wc", "grep", "find", "echo", "env", "which", "file",
                "stat", "id", "groups", "cargo", "python", "python3",
                "pip", "pytest", "make", "npm", "node", "git", "rustc"}


def handle(low_speech, target_lower, target, speech, action, current_mode, ctx):
    """Handle mode-specific commands. Returns True if handled."""
    tts = ctx['tts']
    navigator = ctx['navigator']
    ebook_reader = ctx['ebook_reader']
    doc_writer = ctx['doc_writer']
    sys_settings = ctx['sys_settings']
    file_browser = ctx['file_browser']
    browser = ctx['browser']
    mail_client = ctx['mail_client']
    state_mgr = ctx['state_mgr']

    # Mode Switching
    if action == "switch_mode" or "switch to" in target_lower:
        new_mode = current_mode
        if "browser" in target_lower or "web" in target_lower:
            new_mode = "BROWSER"
        elif "email" in target_lower or "mail" in target_lower:
            new_mode = "EMAIL"
        elif "ide" in target_lower or "code" in target_lower:
            new_mode = "IDE"
        elif "file" in target_lower or "folder" in target_lower:
            new_mode = "FILES"
        elif "doc" in target_lower or "writer" in target_lower:
            new_mode = "DOCS"
        elif "book" in target_lower or "ebook" in target_lower:
            new_mode = "EBOOK"
        elif "setting" in target_lower or "status" in target_lower:
            new_mode = "SETTINGS"
        else:
            new_mode = "DESKTOP"

        play_earcon("success")
        tts.speak(navigator.get_audio_help(new_mode))
        state_mgr.set_mode(new_mode)
        return True, new_mode

    # EBook Reader
    if current_mode == "EBOOK" or action == "ebook_action":
        if ebook_reader is None:
            tts.speak("EBook Reader is not available.")
            return True, current_mode
        if "open" in target_lower or "load" in target_lower:
            bname = target.replace("open book", "").replace("open", "").replace("load", "").strip() or "/tmp/sample_book.txt"
            tts.speak(ebook_reader.load_book(bname))
        elif "chapter" in target_lower and any(char.isdigit() for char in target):
            nums = [int(s) for s in target.split() if s.isdigit()]
            if nums:
                tts.speak(ebook_reader.read_chapter_audio(nums[0]))
        elif "list" in target_lower or "table of contents" in target_lower:
            tts.speak(ebook_reader.list_chapters_audio())
        elif "next" in target_lower:
            tts.speak(ebook_reader.next_chapter_audio())
        elif "previous" in target_lower or "prev" in target_lower:
            tts.speak(ebook_reader.previous_chapter_audio())
        elif "bookmark" in target_lower:
            if "set" in target_lower:
                tts.speak(ebook_reader.set_bookmark())
            else:
                tts.speak(ebook_reader.read_bookmark())
        elif "search" in target_lower:
            q = target.replace("search for", "").replace("search", "").strip()
            tts.speak(ebook_reader.search_book_audio(q))
        else:
            tts.speak(ebook_reader.list_chapters_audio())
        play_earcon("success")
        return True, current_mode

    # Document Writer
    if current_mode == "DOCS" or action == "doc_action":
        if doc_writer is None:
            tts.speak("Document Writer is not available.")
            return True, current_mode
        if "new" in target_lower or "create" in target_lower:
            doc_title = target.replace("new document", "").replace("new doc", "").replace("create", "").strip() or "Untitled"
            tts.speak(doc_writer.start_new_doc(doc_title))
        elif "heading" in target_lower:
            h_text = target.replace("add heading", "").replace("heading", "").strip()
            tts.speak(doc_writer.add_heading(h_text))
        elif "paragraph" in target_lower or "text" in target_lower:
            p_text = target.replace("add paragraph", "").replace("paragraph", "").replace("text", "").strip()
            tts.speak(doc_writer.add_paragraph(p_text))
        elif "read" in target_lower or "draft" in target_lower:
            tts.speak(doc_writer.read_draft_audio())
        elif "export" in target_lower or "save" in target_lower:
            tts.speak(doc_writer.export_all_formats())
        elif "dropbox" in target_lower or "cloud" in target_lower or "share" in target_lower:
            tts.speak(doc_writer.share_to_dropbox_or_cloud())
        else:
            tts.speak(doc_writer.read_draft_audio())
        play_earcon("success")
        return True, current_mode

    # System Settings
    if current_mode == "SETTINGS" or action == "setting_action":
        if sys_settings is None:
            tts.speak("System Settings is not available.")
            return True, current_mode
        if "status" in target_lower or "battery" in target_lower or "time" in target_lower:
            tts.speak(sys_settings.get_system_status_audio())
        elif "volume up" in target_lower or "louder" in target_lower:
            tts.speak(sys_settings.volume_up())
        elif "volume down" in target_lower or "quieter" in target_lower:
            tts.speak(sys_settings.volume_down())
        elif "timer" in target_lower:
            nums = [int(s) for s in target.split() if s.isdigit()]
            mins = nums[0] if nums else 5
            tts.speak(sys_settings.set_timer(mins, callback_tts=tts))
        else:
            tts.speak(sys_settings.get_system_status_audio())
        play_earcon("success")
        return True, current_mode

    # File Browser
    if current_mode == "FILES" or action == "file_action":
        if file_browser is None:
            tts.speak("File Browser is not available.")
            return True, current_mode
        low_user_speech = low_speech
        if "search" in low_user_speech or "find" in low_user_speech:
            q = low_user_speech.replace("search for", "").replace("search file", "").replace("search", "").replace("find file", "").replace("find", "").strip()
            tts.speak(file_browser.search_files_audio(q))
        elif "duplicate" in low_user_speech or "dups" in low_user_speech:
            tts.speak(file_browser.get_duplicates_audio())
        elif "index" in low_user_speech or "scan" in low_user_speech:
            folder = low_user_speech.replace("index folder", "").replace("index directory", "").replace("index", "").replace("scan folder", "").replace("scan directory", "").replace("scan", "").strip() or file_browser.cwd
            tts.speak(file_browser.index_directory_audio(folder))
        elif "where" in target_lower or "location" in target_lower or "where am i" in low_user_speech:
            tts.speak(file_browser.get_location_audio())
        elif "list" in target_lower or "show" in target_lower or "list" in low_user_speech:
            tts.speak(file_browser.list_contents_audio())
        elif "read" in target_lower or "view" in target_lower or "read file" in low_user_speech:
            fname = target.replace("read file", "").replace("read", "").strip() if target else low_user_speech.replace("read file", "").replace("read", "").strip()
            tts.speak(file_browser.read_file_audio(fname))
        elif any(w in low_user_speech for w in ["next file", "next folder", "next entry",
                                                 "next", "previous file", "previous folder",
                                                 "previous entry", "previous", "prev file",
                                                 "prev folder", "prev entry", "prev"]):
            step = -1 if any(w in low_user_speech for w in ["previous", "prev"]) else 1
            tts.speak(file_browser.next_entry(step))
        elif any(w in low_user_speech for w in ["go up", "parent folder", "parent directory",
                                                 "up one level", "go back", "one level up"]):
            tts.speak(file_browser.go_up())
        elif "cd" in target_lower or "go to" in target_lower or "enter" in target_lower or any(w in low_user_speech for w in ["go to", "enter", "cd"]):
            folder = target.replace("go to", "").replace("enter", "").replace("cd", "").strip() if target else low_user_speech.replace("go to", "").replace("enter", "").replace("cd", "").strip()
            tts.speak(file_browser.change_dir(folder))
        else:
            tts.speak(file_browser.list_contents_audio())
        play_earcon("success")
        return True, current_mode

    # Browser
    if current_mode == "BROWSER" or action == "web_navigate":
        if browser is None:
            tts.speak("Browser is not available.")
            return True, current_mode
        if "search" in target_lower or "search" in speech.lower():
            query = target.replace("search for", "").replace("search", "").strip() or speech
            tts.speak(browser.search(query))
        elif target.startswith("http") or "." in target:
            tts.speak(browser.load_url(target))
        elif "heading" in target_lower or "headings" in speech.lower():
            tts.speak(browser.get_headings_audio())
        elif "link" in target_lower or "links" in speech.lower():
            tts.speak(browser.get_links_audio())
        elif "click" in target_lower or "click" in speech.lower():
            nums = [int(s) for s in target.split() if s.isdigit()]
            if nums:
                tts.speak(browser.click_link_by_index(nums[0]))
        play_earcon("success")
        return True, current_mode

    # Email
    if current_mode == "EMAIL" or action == "email_action":
        if mail_client is None:
            tts.speak("Mail Client is not available.")
            return True, current_mode
        import re
        nums = [int(n) for n in re.findall(r'\d+', low_speech)]
        if "check" in target_lower or "inbox" in target_lower:
            from credential_vault import get_vault
            creds = get_vault().get("email")
            if creds:
                result = mail_client.fetch_inbox(
                    creds.get("imap_server", ""),
                    creds.get("username", ""),
                    creds.get("password", ""))
            else:
                result = "No email credentials stored. Say 'store credential' to set up."
            tts.speak(result)
        elif "read" in target_lower:
            idx = nums[0] if nums else 1
            tts.speak(mail_client.read_email_audio(idx))
        play_earcon("success")
        return True, current_mode

    # Desktop System Actions (open_app, type_text, press_key, run_cmd)
    if action == "open_app" and target:
        subprocess.Popen([target], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        play_earcon("success")
        return True, current_mode
    if action == "type_text" and target:
        subprocess.run(["xdotool", "type", target])
        play_earcon("success")
        return True, current_mode
    if action == "press_key" and target:
        subprocess.run(["xdotool", "key", target])
        play_earcon("success")
        return True, current_mode
    if action == "run_cmd" and target:
        try:
            args = shlex.split(target)
        except ValueError:
            args = target.split()
        if args and args[0] in ALLOWED_CMDS:
            res = subprocess.check_output(args, stderr=subprocess.STDOUT, text=True, timeout=30)
            if res.strip():
                tts.speak(res.strip()[:250])
        else:
            tts.speak(f"Command not allowed: {args[0] if args else 'empty'}")
        play_earcon("success")
        return True, current_mode

    return False, current_mode
