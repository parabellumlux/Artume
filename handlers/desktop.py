"""Desktop command handlers — Clipboard, Window Manager, App Launcher, Notifications, Vault."""
from earcons import play_earcon


def handle(low_speech, ctx):
    """Handle desktop infrastructure commands. Returns True if handled."""
    tts = ctx['tts']
    notification_center = ctx['notification_center']
    clipboard = ctx['clipboard']
    window_mgr = ctx['window_mgr']
    app_launcher = ctx['app_launcher']
    vault = ctx['vault']

    # --- Notification center (global, any mode) ---
    if "notifications" in low_speech or "unread notifications" in low_speech:
        play_earcon("info")
        history = notification_center.get_history()
        tts.speak(history)
        play_earcon("success")
        return True
    if "clear notifications" in low_speech or "dismiss notifications" in low_speech:
        play_earcon("info")
        notification_center.clear_history()
        tts.speak("Notifications cleared.")
        play_earcon("success")
        return True
    if "focus mode on" in low_speech or "enable focus" in low_speech:
        notification_center.set_focus_mode(True)
        play_earcon("success")
        tts.speak("Focus mode enabled. Notifications will be queued.")
        return True
    if "focus mode off" in low_speech or "disable focus" in low_speech:
        notification_center.set_focus_mode(False)
        play_earcon("success")
        tts.speak("Focus mode disabled.")
        return True

    # --- Credential Vault (global, any mode) ---
    if "unlock vault" in low_speech or "open vault" in low_speech:
        play_earcon("info")
        tts.speak("Please say your PIN.")
        play_earcon("listening")
        tts.speak("Vault unlock requires voice PIN entry. Use 'unlock vault' in the next prompt with your PIN number.")
        play_earcon("success")
        return True
    if "create vault" in low_speech:
        play_earcon("info")
        tts.speak("Creating credential vault. Say 'set vault PIN' followed by your 4 to 8 digit PIN.")
        play_earcon("success")
        return True
    if "set vault pin" in low_speech:
        import re
        pin_match = re.search(r'(\d{4,8})', low_speech)
        if pin_match:
            pin = pin_match.group(1)
            play_earcon("info")
            result = vault.create_vault(pin)
            tts.speak(result)
            play_earcon("success")
        else:
            play_earcon("warning")
            tts.speak("Please say a 4 to 8 digit PIN. For example: set vault PIN 1234")
        return True
    if "store credential" in low_speech:
        play_earcon("info")
        tts.speak("Credential storage requires interactive setup. Use the vault command to store credentials securely.")
        play_earcon("success")
        return True
    if "list credentials" in low_speech or "vault list" in low_speech:
        play_earcon("info")
        if vault._unlocked:
            result = vault.list_services()
            tts.speak(result)
        else:
            tts.speak("Vault is locked. Say 'unlock vault' first.")
        play_earcon("success")
        return True
    if "lock vault" in low_speech:
        play_earcon("info")
        result = vault.lock()
        tts.speak(result)
        play_earcon("success")
        return True

    # --- Clipboard Manager (global, any mode) ---
    if clipboard is not None:
        if "copy" in low_speech and "clipboard" not in low_speech:
            play_earcon("info")
            tts.speak(clipboard.copy_selection())
            play_earcon("success")
            return True
        if "paste" in low_speech:
            play_earcon("info")
            tts.speak(clipboard.paste())
            play_earcon("success")
            return True
        if "clipboard history" in low_speech or "show clipboard" in low_speech:
            play_earcon("info")
            tts.speak(clipboard.history())
            play_earcon("success")
            return True
        if "clear clipboard" in low_speech:
            play_earcon("info")
            clipboard.clear_history()
            tts.speak("Clipboard history cleared.")
            play_earcon("success")
            return True
        if "select all" in low_speech:
            play_earcon("info")
            tts.speak(clipboard.select_all())
            play_earcon("success")
            return True

    # --- Window Manager (global, any mode) ---
    if window_mgr is not None:
        if "list windows" in low_speech or "show windows" in low_speech:
            play_earcon("info")
            tts.speak(window_mgr.list_windows())
            play_earcon("success")
            return True
        if "focus window" in low_speech:
            play_earcon("info")
            nums = [int(s) for s in low_speech.split() if s.isdigit()]
            if nums:
                tts.speak(window_mgr.focus_window(str(nums[0])))
            else:
                tts.speak("Which window? Say 'focus window 1'.")
            play_earcon("success")
            return True
        if "minimize window" in low_speech or "minimize" in low_speech:
            play_earcon("info")
            tts.speak(window_mgr.minimize_window())
            play_earcon("success")
            return True
        if "maximize window" in low_speech or "maximize" in low_speech:
            play_earcon("info")
            tts.speak(window_mgr.maximize_window())
            play_earcon("success")
            return True
        if "close window" in low_speech or "close this" in low_speech:
            play_earcon("warning")
            tts.speak(window_mgr.close_window())
            play_earcon("success")
            return True
        if "switch desktop" in low_speech:
            play_earcon("info")
            nums = [int(s) for s in low_speech.split() if s.isdigit()]
            if nums:
                tts.speak(window_mgr.switch_desktop(nums[0]))
            else:
                tts.speak("Which desktop? Say 'switch desktop 1'.")
            play_earcon("success")
            return True

    # --- App Launcher (global, any mode) ---
    if app_launcher is not None:
        if "launch" in low_speech or ("open app" in low_speech):
            play_earcon("info")
            app = low_speech.replace("launch", "").replace("open app", "").strip()
            if app:
                tts.speak(app_launcher.launch(app))
            else:
                tts.speak("Which app? Say 'launch firefox' for example.")
            play_earcon("success")
            return True
        if "list apps" in low_speech or "show apps" in low_speech:
            play_earcon("info")
            tts.speak(app_launcher.list_apps())
            play_earcon("success")
            return True
        if "quit app" in low_speech or "kill app" in low_speech:
            play_earcon("warning")
            app = low_speech.replace("quit app", "").replace("kill app", "").strip()
            if app:
                tts.speak(app_launcher.quit_app(app))
            else:
                tts.speak("Which app to quit?")
            play_earcon("success")
            return True

    return False
