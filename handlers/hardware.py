"""Hardware command handlers — Bluetooth, WiFi, Audio Output, Power."""
from earcons import play_earcon


def handle(low_speech, ctx):
    """Handle hardware commands. Returns True if handled."""
    tts = ctx['tts']
    bt_manager = ctx['bt_manager']
    wifi_manager = ctx['wifi_manager']
    audio_switcher = ctx['audio_switcher']
    power_mgr = ctx['power_mgr']
    confirm_dialog = ctx['confirm_dialog']

    # --- Power management (global, any mode) ---
    if "battery" in low_speech or "power status" in low_speech:
        play_earcon("info")
        tts.speak(power_mgr.get_battery_status())
        play_earcon("success")
        return True

    power_action = None
    if "shut down" in low_speech or "power off" in low_speech:
        power_action = ("shut down the system", power_mgr.shutdown)
    elif "reboot" in low_speech or "restart system" in low_speech:
        power_action = ("reboot the system", power_mgr.reboot)
    elif "suspend" in low_speech or "go to sleep" in low_speech:
        power_action = ("suspend the system", power_mgr.suspend)
    elif "hibernate" in low_speech:
        power_action = ("hibernate the system", power_mgr.hibernate)

    if power_action:
        action_name, action_fn = power_action
        if confirm_dialog.confirm_destructive(action_name):
            play_earcon("warning")
            tts.speak(action_fn())
        else:
            play_earcon("success")
            tts.speak(f"{action_name.capitalize()} cancelled.")
        return True

    if "lock screen" in low_speech:
        play_earcon("success")
        tts.speak(power_mgr.lock_screen())
        return True
    if "cancel" in low_speech and power_mgr._pending_timer:
        power_mgr.cancel()
        play_earcon("success")
        tts.speak("Action cancelled.")
        return True

    # --- Bluetooth (global, any mode) ---
    if "bluetooth" in low_speech:
        if bt_manager is None:
            tts.speak("Bluetooth manager is not available.")
            return True
        if "list" in low_speech or "devices" in low_speech:
            play_earcon("info")
            tts.speak(bt_manager.list_devices())
            play_earcon("success")
            return True
        if "connect" in low_speech:
            play_earcon("info")
            mac = low_speech.replace("bluetooth connect", "").replace("connect", "").strip()
            if mac:
                tts.speak(bt_manager.connect(mac))
            else:
                tts.speak("Which device? Say 'bluetooth connect' followed by device name.")
            play_earcon("success")
            return True
        if "disconnect" in low_speech:
            play_earcon("info")
            mac = low_speech.replace("bluetooth disconnect", "").replace("disconnect", "").strip()
            if mac:
                tts.speak(bt_manager.disconnect(mac))
            else:
                tts.speak("Which device? Say 'bluetooth disconnect' followed by device name.")
            play_earcon("success")
            return True
        if "scan" in low_speech:
            play_earcon("info")
            tts.speak(bt_manager.scan())
            play_earcon("success")
            return True

    # --- WiFi (global, any mode) ---
    if "wifi" in low_speech:
        if wifi_manager is None:
            tts.speak("WiFi manager is not available.")
            return True
        if "list" in low_speech or "networks" in low_speech:
            play_earcon("info")
            tts.speak(wifi_manager.list_networks())
            play_earcon("success")
            return True
        if "connect" in low_speech:
            play_earcon("info")
            ssid = low_speech.replace("wifi connect", "").replace("connect to", "").replace("connect", "").strip()
            if ssid:
                tts.speak(wifi_manager.connect(ssid))
            else:
                tts.speak("Which network? Say 'wifi connect' followed by network name.")
            play_earcon("success")
            return True
        if "disconnect" in low_speech:
            play_earcon("info")
            tts.speak(wifi_manager.disconnect())
            play_earcon("success")
            return True
        if "status" in low_speech:
            play_earcon("info")
            tts.speak(wifi_manager.status())
            play_earcon("success")
            return True

    # --- Audio Output Switcher (global, any mode) ---
    if "speaker" in low_speech or "audio output" in low_speech or "switch speaker" in low_speech:
        if audio_switcher is None:
            tts.speak("Audio output switcher is not available.")
            return True
        if "list" in low_speech or "show" in low_speech:
            play_earcon("info")
            tts.speak(audio_switcher.list_sinks())
            play_earcon("success")
            return True
        if "switch" in low_speech:
            play_earcon("info")
            nums = [int(s) for s in low_speech.split() if s.isdigit()]
            if nums:
                tts.speak(audio_switcher.switch_to(str(nums[0])))
            else:
                tts.speak("Which speaker? Say 'switch speaker 1'.")
            play_earcon("success")
            return True

    return False
