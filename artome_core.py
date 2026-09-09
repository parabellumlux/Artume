#!/usr/bin/env python3
"""Artume OS Core Desktop Environment Daemon — thin dispatcher with lazy imports."""

import os
import signal
import sys


def _init_engine(name, factory, default=None):
    try:
        return factory()
    except Exception as e:
        print(f"WARNING: {name} failed to init: {e}")
        return default


def _init_all():
    """Initialize all engines with graceful degradation."""
    from earcons import init_earcons
    from audio_engine import PiperTTS, DynamicVADListener
    from faster_whisper import WhisperModel

    print("Initializing Artome OS Core Engine...")
    init_earcons()

    print("Loading Whisper Speech Recognition...")
    whisper_model = WhisperModel("tiny.en", device="cpu", compute_type="int8")
    tts = PiperTTS()
    listener = DynamicVADListener(silence_threshold_ms=750, energy_threshold=0.018)

    from browser_engine import AudioWebBrowser
    from mail_engine import AudioMailClient
    from ide_engine import AudioIDE
    from file_browser_engine import AudioFileBrowser
    from doc_writer_engine import AudioDocWriter
    from system_settings_engine import AudioSystemSettings
    from ebook_engine import AudioEBookReader
    from command_navigator import AudioCommandNavigator
    from wakeword_engine import WakeWordDetector
    from screen_reader import AtspiScreenReader

    from artome_ide.ai_assistant import AiAssistant
    from artome_ide.git_engine import GitEngine
    from artome_ide.terminal_engine import TerminalEngine

    from credential_vault import get_vault
    from notification_center import get_notification_center
    from clipboard_manager import ClipboardManager
    from window_manager import WindowManager
    from app_launcher import AppLauncher
    from confirmation_dialog import get_dialog as get_confirmation_dialog
    from state_manager import get_state_manager

    from bluetooth_manager import BluetoothManager
    from wifi_manager import WiFiManager
    from audio_output_switcher import AudioOutputSwitcher
    from power_manager import get_power_manager

    from trigger_engine import get_trigger_engine

    from calculator import get_calculator
    from weather_service import get_weather
    from notes_manager import NotesManager

    browser = _init_engine("BrowserEngine", AudioWebBrowser)
    mail_client = _init_engine("MailEngine", AudioMailClient)
    ide = _init_engine("IDEEngine", AudioIDE)
    file_browser = _init_engine("FileBrowser", AudioFileBrowser)
    doc_writer = _init_engine("DocWriter", AudioDocWriter)
    sys_settings = _init_engine("SystemSettings", AudioSystemSettings)
    ebook_reader = _init_engine("EBookReader", AudioEBookReader)
    navigator = _init_engine("CommandNavigator", AudioCommandNavigator)
    wakeword = _init_engine("WakeWord", WakeWordDetector)
    screen_reader = _init_engine("ScreenReader", AtspiScreenReader)

    ai_assistant = _init_engine("AiAssistant", AiAssistant)
    git_engine = _init_engine("GitEngine", lambda: GitEngine(project_root=os.path.dirname(os.path.abspath(__file__))))
    terminal_engine = _init_engine("TerminalEngine", TerminalEngine)

    vault = get_vault()
    notification_center = get_notification_center()
    notification_center.set_speak_callback(lambda text: tts.speak(text))
    clipboard = _init_engine("ClipboardManager", ClipboardManager)
    window_mgr = _init_engine("WindowManager", WindowManager)
    app_launcher = _init_engine("AppLauncher", AppLauncher)
    confirm_dialog = get_confirmation_dialog(tts_engine=tts, whisper_model=whisper_model)
    state_mgr = get_state_manager()

    bt_manager = _init_engine("BluetoothManager", BluetoothManager)
    wifi_manager = _init_engine("WiFiManager", WiFiManager)
    audio_switcher = _init_engine("AudioOutputSwitcher", AudioOutputSwitcher)
    power_mgr = get_power_manager()

    trigger_engine = get_trigger_engine()

    calculator = get_calculator()
    weather = get_weather()
    notes = _init_engine("NotesManager", NotesManager)

    return {
        'tts': tts, 'listener': listener, 'whisper_model': whisper_model,
        'wakeword': wakeword, 'screen_reader': screen_reader, 'navigator': navigator,
        'browser': browser, 'mail_client': mail_client, 'ide': ide,
        'file_browser': file_browser, 'doc_writer': doc_writer, 'sys_settings': sys_settings,
        'ebook_reader': ebook_reader, 'ai_assistant': ai_assistant,
        'git_engine': git_engine, 'terminal_engine': terminal_engine,
        'vault': vault, 'notification_center': notification_center,
        'clipboard': clipboard, 'window_mgr': window_mgr, 'app_launcher': app_launcher,
        'confirm_dialog': confirm_dialog, 'state_mgr': state_mgr,
        'bt_manager': bt_manager, 'wifi_manager': wifi_manager,
        'audio_switcher': audio_switcher, 'power_mgr': power_mgr,
        'trigger_engine': trigger_engine,
        'calculator': calculator, 'weather': weather, 'notes': notes,
    }


def execute_action(intent, user_speech, current_mode, ctx):
    """Thin dispatcher — delegates to handler modules."""
    from earcons import play_earcon
    from handlers import ide as ide_handler
    from handlers import hardware as hw_handler
    from handlers import desktop as desktop_handler
    from handlers import modes as modes_handler

    tts = ctx['tts']
    action = intent.get("action", "speak")
    speech = intent.get("speech", "")
    target = intent.get("target", "")
    low_speech = user_speech.lower() if user_speech else speech.lower()

    # 0. Wake Word Mode Toggle
    if "wake word" in low_speech:
        wakeword = ctx['wakeword']
        if "enable" in low_speech or "on" in low_speech:
            msg = wakeword.toggle_wake_word(True)
        elif "disable" in low_speech or "off" in low_speech:
            msg = wakeword.toggle_wake_word(False)
        else:
            msg = wakeword.toggle_wake_word()
        play_earcon("success")
        tts.speak(msg)
        return current_mode

    # 1. Screen Summary
    if action == "screen_summary" or "screen" in low_speech or target == "COMMAND:SCREEN_SUMMARY":
        play_earcon("success")
        try:
            payload = str(ctx['screen_reader'].generate_screen_summary_payload())
        except Exception:
            payload = ""
        if payload:
            from intent_router import OLLAMA_URL, REASONING_MODEL
            import requests
            ai_summary = ""
            try:
                res = requests.post(
                    OLLAMA_URL,
                    json={
                        "model": REASONING_MODEL,
                        "prompt": (
                            "You are Artome OS voice assistant. Summarize what is on the "
                            "screen for a blind user in 2 concise sentences based on this "
                            "UI accessibility tree.\n\n"
                            f"Accessibility UI Elements:\n{payload[:4000]}\n\n"
                            "Spoken summary for blind user:"
                        ),
                        "stream": False,
                        "options": {"temperature": 0.3, "num_predict": 90},
                    },
                    timeout=12,
                )
                ai_summary = res.json().get("response", "").strip()
            except Exception:
                ai_summary = ""
            if ai_summary:
                tts.speak(f"Screen summary: {ai_summary[:300]}")
            else:
                tts.speak(f"Screen summary: {payload[:300]}")
        else:
            tts.speak(speech or "Screen summary is not available.")
        return current_mode

    # 1b. Calculator
    if action == "calculator":
        import re
        play_earcon("success")
        # Extract math expression from speech
        expr = re.sub(r'^(calculate|compute|what is|what\'s|how much is|math)\s*', '', low_speech)
        if not expr:
            expr = target
        result = ctx['calculator'].calculate(expr)
        tts.speak(f"The result is {result}")
        return current_mode

    # 1c. Weather
    if action == "weather":
        play_earcon("success")
        import re
        location = re.sub(r'^(weather|forecast|temperature|outside)\s*(in|for|at)?\s*', '', low_speech)
        location = location.strip() or None
        result = ctx['weather'].get_weather(location)
        tts.speak(result)
        return current_mode

    # 1d. Notes
    if action == "notes":
        import re
        play_earcon("success")
        if "clear notes" in low_speech:
            result = ctx['notes'].clear()
            tts.speak(result)
        elif "delete note" in low_speech:
            nums = re.findall(r'\d+', low_speech)
            if nums:
                result = ctx['notes'].delete(int(nums[0]))
                tts.speak(result)
            else:
                tts.speak("Which note? Say 'delete note 1'.")
        elif "list notes" in low_speech:
            result = ctx['notes'].list_notes()
            tts.speak(result)
        elif "read note" in low_speech:
            nums = re.findall(r'\d+', low_speech)
            if nums:
                result = ctx['notes'].read_note(int(nums[0]))
                tts.speak(result)
            else:
                tts.speak("Which note? Say 'read note 1'.")
        elif "search note" in low_speech:
            query = re.sub(r'^(search note)\s*', '', low_speech)
            result = ctx['notes'].search(query) if query else "What should I search for?"
            tts.speak(result)
        elif "take a note" in low_speech or "save note" in low_speech or "note" in low_speech:
            note_text = re.sub(r'^(take a note|save note|note)\s*', '', low_speech)
            if note_text:
                result = ctx['notes'].add(note_text)
                tts.speak(result)
            else:
                tts.speak("What would you like to note? Say 'take a note' followed by your note.")
        return current_mode

    # 2. Global Help & Navigation
    if any(h in low_speech for h in ["help", "what can i say", "commands", "what can i do"]):
        play_earcon("success")
        tts.speak(ctx['navigator'].get_audio_help(current_mode))
        return current_mode

    if any(m in low_speech for m in ["main menu", "menu", "navigation menu", "categories"]):
        play_earcon("success")
        tts.speak(ctx['navigator'].get_main_menu_audio())
        return current_mode

    new_mode, menu_speech = ctx['navigator'].select_menu_option(low_speech)
    if new_mode:
        play_earcon("success")
        tts.speak(menu_speech)
        return new_mode

    # --- Phase 999: Pre-recorded anticipatory response ---
    from clip_registry import lookup as clip_lookup
    _last_clip = None
    if speech:
        _last_clip = clip_lookup(action, target)
        if _last_clip:
            play_earcon(_last_clip["file"].replace(".wav", ""))
        else:
            tts.speak(speech)

    # Pass clip_played flag to handlers so they skip their own ack
    ctx['clip_played'] = _last_clip is not None

    try:
        target_lower = target.lower() if target else speech.lower()

        # --- Browser navigation (back, forward, bookmark) ---
        if action == "web_navigate" and target in ("go_back", "go_forward", "bookmark",
                                                     "list_bookmarks") or \
           action == "web_navigate" and target.startswith("open_bookmark:"):
            play_earcon("success")
            browser = ctx['browser']
            if browser is None:
                tts.speak("Browser is not available.")
            elif target == "go_back":
                tts.speak(browser.go_back())
            elif target == "go_forward":
                tts.speak(browser.go_forward())
            elif target == "bookmark":
                tts.speak(browser.bookmark_current())
            elif target == "list_bookmarks":
                tts.speak(browser.list_bookmarks())
            elif target.startswith("open_bookmark:"):
                idx = int(target.split(":")[1])
                tts.speak(browser.click_bookmark(idx))
            return current_mode

        # --- Email folder/search/attachment commands ---
        if action == "email_folder":
            play_earcon("success")
            mail_client = ctx['mail_client']
            if mail_client is None:
                tts.speak("Email client is not available.")
            elif target == "list_folders":
                tts.speak(mail_client.list_folders())
            else:
                tts.speak(mail_client.switch_folder(target))
            return current_mode

        if action == "email_search":
            play_earcon("success")
            mail_client = ctx['mail_client']
            if mail_client is None:
                tts.speak("Email client is not available.")
            else:
                tts.speak(mail_client.search_emails(target))
            return current_mode

        if action == "email_action":
            play_earcon("success")
            mail_client = ctx['mail_client']
            if mail_client is None:
                tts.speak("Email client is not available.")
            elif target in ("fetch_inbox", "inbox"):
                # Use vault credentials if available
                from credential_vault import get_vault
                vault = get_vault()
                creds = vault.get("email")
                if creds:
                    result = mail_client.fetch_inbox(
                        creds.get("imap_server", ""),
                        creds.get("username", ""),
                        creds.get("password", ""),
                    )
                    tts.speak(result)
                else:
                    tts.speak("No email credentials stored. Say 'store credential' to set up.")
            elif target == "compose":
                dialog = ctx['confirm_dialog']
                recipient = dialog.capture_phrase("Who should I send the email to?")
                if not recipient:
                    tts.speak("I could not hear the recipient. Try again.")
                    return current_mode
                subject = dialog.capture_phrase("What is the subject?")
                body = dialog.capture_phrase("What should the message say?")
                if not body:
                    tts.speak("I could not hear the message. Try again.")
                    return current_mode
                result = mail_client.prepare_draft(recipient, subject or "(no subject)", body)
                tts.speak(result)
            elif target == "send_draft":
                from credential_vault import get_vault
                vault = get_vault()
                creds = vault.get("email")
                draft = mail_client.active_draft
                if not draft:
                    tts.speak("No draft to send. Say 'compose email' first.")
                elif not creds:
                    tts.speak("No email credentials stored. Say 'store credential' to set up.")
                else:
                    confirm = ctx['confirm_dialog'].ask(
                        f"Send the email to {draft.get('to', '?')}?")
                    if confirm:
                        smtp_server = creds.get("smtp_server") or creds.get("imap_server", "")
                        smtp_port = int(creds.get("smtp_port") or 465)
                        result = mail_client.send_draft(
                            smtp_server, smtp_port,
                            creds.get("username", ""), creds.get("password", ""))
                        tts.speak(result)
                    else:
                        tts.speak("Email send cancelled.")
            elif target == "cancel_draft":
                mail_client.active_draft = None
                play_earcon("warning")
                tts.speak("Email draft cancelled.")
            elif target.startswith("read_email:") or target == "read":
                if target == "read":
                    idx = 1
                else:
                    idx = int(target.split(":")[1]) or 1
                tts.speak(mail_client.read_email_audio(idx))
            elif target.startswith("attachments:"):
                idx = int(target.split(":")[1]) or 1
                tts.speak(mail_client.read_attachments(idx))
            return current_mode

        # --- File operations (copy, move, rename, delete, create, sort) ---
        if action in ("file_action", "file_op"):
            play_earcon("success")
            file_browser = ctx['file_browser']
            if file_browser is None:
                tts.speak("File browser is not available.")
            elif target.startswith("search_file:"):
                query = target.split(":", 1)[1]
                tts.speak(file_browser.search_files_audio(query))
            elif target.startswith("copy:"):
                parts = target.split(":", 2)
                if len(parts) == 3:
                    tts.speak(file_browser.copy_file(parts[1], parts[2]))
                else:
                    tts.speak("Say 'copy [file] to [destination]'.")
            elif target.startswith("move:"):
                parts = target.split(":", 2)
                if len(parts) == 3:
                    tts.speak(file_browser.move_file(parts[1], parts[2]))
                else:
                    tts.speak("Say 'move [file] to [destination]'.")
            elif target.startswith("rename:"):
                parts = target.split(":", 2)
                if len(parts) == 3:
                    tts.speak(file_browser.rename_file(parts[1], parts[2]))
                else:
                    tts.speak("Say 'rename [old name] to [new name]'.")
            elif target.startswith("delete:"):
                name = target.split(":", 1)[1]
                if ctx['confirm_dialog'].confirm_destructive(f"delete file {name}"):
                    tts.speak(file_browser.delete_file(name))
                else:
                    play_earcon("success")
                    tts.speak(f"File {name} not deleted.")
            elif target.startswith("create_file:"):
                name = target.split(":", 1)[1]
                tts.speak(file_browser.create_file(name))
            elif target.startswith("create_folder:"):
                name = target.split(":", 1)[1]
                tts.speak(file_browser.create_folder(name))
            elif target.startswith("sort:"):
                by = target.split(":", 1)[1]
                tts.speak(file_browser.sort_directory(by))
            return current_mode

        # Hardware commands (global, any mode)
        if hw_handler.handle(low_speech, ctx):
            return current_mode

        # Desktop commands (global, any mode)
        if desktop_handler.handle(low_speech, ctx):
            return current_mode

        # Mode-specific commands
        handled, new_mode = modes_handler.handle(
            low_speech, target_lower, target, speech, action, current_mode, ctx)
        if handled:
            return new_mode

        # IDE mode commands (checked last — most specific)
        if current_mode == "IDE" or action in ("ide_action", "dap_action"):
            if ide_handler.handle(low_speech, target_lower, target, ctx):
                return current_mode

        # Fallback
        play_earcon("success")

    except Exception as e:
        print(f"Execution Error: {e}")
        play_earcon("error")
        tts.speak(f"Action error: {str(e)[:40]}")

    return current_mode


def handle_trigger_event(event, ctx):
    """Handle trigger engine events."""
    from notification_center import get_notification_center, NotificationCategory, NotificationPriority
    nc = get_notification_center()
    tts = ctx['tts']
    listener = ctx['listener']
    sys_settings = ctx['sys_settings']
    trigger_engine = ctx['trigger_engine']

    if listener.is_listening:
        return

    if event.trigger_id == "first-run-onboarding":
        steps = trigger_engine.get_onboarding_steps()
        for step in steps:
            tts.speak(step["text"])
            break

    elif event.trigger_id == "daily-briefing":
        status = sys_settings.get_system_status_audio()
        nc.notify(NotificationCategory.SYSTEM, "Daily Briefing", status)

    elif event.trigger_id == "email-check":
        try:
            mail_client = ctx.get('mail_client')
            if mail_client:
                from credential_vault import get_vault
                vault = get_vault()
                creds = vault.get("email")
                if creds:
                    count = mail_client.fetch_inbox(
                        creds.get("imap_server", ""),
                        creds.get("username", ""),
                        creds.get("password", ""),
                        limit=5
                    )
                    if count:
                        nc.notify(NotificationCategory.SYSTEM, "New Email",
                                  f"You have {len(count)} new emails.")
                else:
                    nc.notify(NotificationCategory.SYSTEM, "Email Check",
                              "No email credentials stored. Say 'store credential' to set up email.")
        except Exception as e:
            nc.notify(NotificationCategory.SYSTEM, "Email Error",
                      f"Could not check email: {str(e)[:50]}")

    elif event.trigger_id == "system-health":
        try:
            if os.path.exists("/sys/class/power_supply/BAT0/capacity"):
                with open("/sys/class/power_supply/BAT0/capacity") as f:
                    bat = int(f.read().strip())
                if bat < 20:
                    nc.notify(NotificationCategory.SYSTEM, "Low Battery",
                              f"Battery is at {bat} percent. Please charge.",
                              priority=NotificationPriority.HIGH)
        except Exception:
            pass

    elif event.trigger_id == "backup-reminder":
        nc.notify(NotificationCategory.REMINDER, "Backup Reminder",
                  "Time to back up your important files.")

    elif event.trigger_id == "idle-reminder":
        nc.flush()

    elif event.trigger_type.value == "file_watch":
        data = event.data
        path = data.get("path", "")
        new_files = data.get("new_files", [])
        if new_files:
            nc.notify(NotificationCategory.SYSTEM, "New Files",
                      f"New files in {os.path.basename(path)}: {', '.join(new_files[:3])}")


def run_artome():
    """Artome OS Main Daemon Loop."""
    from earcons import play_earcon
    from intent_router import ask_artome_ai
    from artome_ide import start_daemon, stop_daemon

    ctx = _init_all()

    current_mode = "DESKTOP"

    ctx['trigger_engine'].on_event(lambda event: handle_trigger_event(event, ctx))
    ctx['trigger_engine'].start()
    if not ctx['trigger_engine'].is_onboarding_complete():
        print("First run detected — onboarding will start on first interaction.")

    if start_daemon():
        print("aether-ide-daemon started and socket ready.")
    else:
        print("aether-ide-daemon not started (binary missing or socket timeout).")

    def _shutdown_handler(signum, frame):
        print("\nShutdown signal received — stopping IDE daemon...")
        ctx['power_mgr'].cancel()
        ctx['state_mgr'].save()
        stop_daemon()
        sys.exit(0)

    signal.signal(signal.SIGTERM, _shutdown_handler)
    signal.signal(signal.SIGINT, _shutdown_handler)

    saved_mode = ctx['state_mgr'].get_mode()
    if saved_mode and saved_mode != "DESKTOP":
        current_mode = saved_mode
        print(f"Restored mode: {current_mode}")

    play_earcon("success")
    ctx['tts'].speak("Artome desktop environment ready with TTS barge-in and AT-SPI2 screen reader.")

    fallback_mode = False
    while True:
        try:
            if not fallback_mode:
                try:
                    audio_data = ctx['listener'].listen(max_duration_sec=12, tts_engine=ctx['tts'])
                    if audio_data is None:
                        continue

                    segments, _ = ctx['whisper_model'].transcribe(audio_data, beam_size=5)
                    raw_user_text = " ".join([seg.text for seg in segments]).strip()
                except Exception as e:
                    print(f"ALSA/Sound device initialization failed: {e}")
                    print("Switching to interactive Console Fallback Mode...")
                    fallback_mode = True
                    continue
            else:
                try:
                    print(f"\n[{current_mode}] Enter command (or 'exit' to quit): ", end="", flush=True)
                    raw_user_text = sys.stdin.readline().strip()
                except KeyboardInterrupt:
                    break
                if not raw_user_text:
                    continue

            if not raw_user_text:
                continue

            if fallback_mode:
                active_command = raw_user_text
                is_activated = True
            else:
                is_activated, active_command = ctx['wakeword'].check_wake_word(raw_user_text)

            if not is_activated:
                continue

            play_earcon("listening")

            if not active_command:
                ctx['tts'].speak("Yes?")
                continue

            print(f"\n[{current_mode}] COMMAND: {active_command}")

            cmd_lower = active_command.lower().strip()
            exit_phrases = {"exit", "quit", "stop", "shut down", "power off", "log out", "goodbye"}
            is_exit = cmd_lower in exit_phrases or any(cmd_lower.startswith(p) for p in exit_phrases)
            if is_exit:
                ctx['power_mgr'].cancel()
                play_earcon("success")
                ctx['tts'].speak("Shutting down Artome OS. Goodbye.")
                break

            intent = ask_artome_ai(active_command, mode=current_mode)
            print(f"[{current_mode}] INTENT: {intent}")

            current_mode = execute_action(intent, active_command, current_mode, ctx)

        except KeyboardInterrupt:
            ctx['power_mgr'].cancel()
            play_earcon("error")
            ctx['tts'].speak("Artome stopped.")
            break
        except Exception as e:
            print(f"Core Loop Error: {e}")
            play_earcon("error")


if __name__ == "__main__":
    run_artome()
