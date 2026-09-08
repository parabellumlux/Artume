//! System command implementations (volume, bluetooth, wifi, audio, timer, status).
//!
//! All commands shell out to standard Linux CLI tools (amixer, bluetoothctl,
//! nmcli, pactl, etc.) and return human-readable strings for TTS.

use log::warn;
use std::process::Command;

/// Run a command and return trimmed stdout, or an error message.
fn run(cmd: &str, args: &[&str]) -> String {
    match Command::new(cmd).args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if output.status.success() {
                stdout
            } else if !stderr.is_empty() {
                stderr
            } else {
                format!("Command exited with status {}", output.status)
            }
        }
        Err(e) => {
            warn!("system_commands: failed to run {cmd}: {e}");
            format!("Could not run {cmd}: {e}")
        }
    }
}

// ---------------------------------------------------------------------------
// Volume
// ---------------------------------------------------------------------------

/// Get current volume percentage.
pub fn volume_get() -> String {
    let out = run("amixer", &["get", "Master"]);
    for line in out.lines() {
        if line.contains('[') && line.contains('%') {
            if let Some(start) = line.rfind('[') {
                if let Some(end) = line[start..].find('%') {
                    let pct = &line[start + 1..start + end];
                    return format!("Volume is at {pct} percent.");
                }
            }
        }
    }
    "Could not read volume.".to_string()
}

/// Increase volume by 5%.
pub fn volume_up() -> String {
    run("amixer", &["set", "Master", "5%+"]);
    volume_get()
}

/// Decrease volume by 5%.
pub fn volume_down() -> String {
    run("amixer", &["set", "Master", "5%-"]);
    volume_get()
}

/// Set volume to a specific percentage (0–100).
pub fn volume_set(percent: u32) -> String {
    let pct = percent.min(100);
    run("amixer", &["set", "Master", &format!("{pct}%")]);
    format!("Volume set to {pct} percent.")
}

/// Parse a volume command from user text and execute it.
pub fn handle_volume(text: &str) -> Option<String> {
    let low = text.to_lowercase();

    if low.contains("up") || low.contains("louder") || low.contains("increase") {
        return Some(volume_up());
    }
    if low.contains("down")
        || low.contains("quieter")
        || low.contains("decrease")
        || low.contains("softer")
    {
        return Some(volume_down());
    }
    if low.contains("mute") {
        run("amixer", &["set", "Master", "toggle"]);
        return Some("Toggled mute.".to_string());
    }
    // "set volume to 50" or "volume 75"
    if low.contains("set") || low.contains(" to ") || has_digit(&low) {
        if let Some(n) = extract_number(&low) {
            return Some(volume_set(n));
        }
    }
    // Default: report current volume
    Some(volume_get())
}

// ---------------------------------------------------------------------------
// Bluetooth
// ---------------------------------------------------------------------------

/// List paired bluetooth devices.
pub fn bt_list() -> String {
    let out = run("bluetoothctl", &["devices"]);
    if out.is_empty() {
        return "No paired bluetooth devices found.".to_string();
    }
    let mut lines: Vec<&str> = out.lines().collect();
    lines.truncate(10);
    format!("Bluetooth devices: {}", lines.join("; "))
}

/// Connect to a bluetooth device by MAC or name.
pub fn bt_connect(target: &str) -> String {
    if target.is_empty() {
        return "Which device? Say 'bluetooth connect' followed by device name.".to_string();
    }
    let mac = resolve_bt_device(target);
    run("bluetoothctl", &["connect", &mac]);
    format!("Connecting to {target}.")
}

/// Disconnect from a bluetooth device.
pub fn bt_disconnect(target: &str) -> String {
    if target.is_empty() {
        return "Which device? Say 'bluetooth disconnect' followed by device name.".to_string();
    }
    let mac = resolve_bt_device(target);
    run("bluetoothctl", &["disconnect", &mac]);
    format!("Disconnected from {target}.")
}

/// Scan for nearby bluetooth devices.
pub fn bt_scan() -> String {
    run("bluetoothctl", &["scan", "on"]);
    std::thread::sleep(std::time::Duration::from_secs(5));
    run("bluetoothctl", &["scan", "off"]);
    let out = run("bluetoothctl", &["devices"]);
    if out.is_empty() {
        "No devices found during scan.".to_string()
    } else {
        let lines: Vec<&str> = out.lines().take(10).collect();
        format!("Found devices: {}", lines.join("; "))
    }
}

/// Parse a bluetooth command from user text.
pub fn handle_bluetooth(text: &str) -> Option<String> {
    let low = text.to_lowercase();
    if !low.contains("bluetooth") && !low.contains("bt") {
        return None;
    }
    if low.contains("list") || low.contains("devices") {
        return Some(bt_list());
    }
    if low.contains("scan") {
        return Some(bt_scan());
    }
    if low.contains("connect") {
        let target = extract_after(&low, &["bluetooth connect", "bt connect", "connect"]);
        return Some(bt_connect(&target));
    }
    if low.contains("disconnect") {
        let target = extract_after(
            &low,
            &["bluetooth disconnect", "bt disconnect", "disconnect"],
        );
        return Some(bt_disconnect(&target));
    }
    // Default: list devices
    Some(bt_list())
}

/// Resolve a bluetooth device name to a MAC address via bluetoothctl.
fn resolve_bt_device(name: &str) -> String {
    // If it looks like a MAC already, use it directly.
    if name.len() >= 17 && name.chars().filter(|c| *c == ':').count() == 5 {
        return name.to_string();
    }
    let out = run("bluetoothctl", &["devices"]);
    for line in out.lines() {
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() >= 3 && parts[2].to_lowercase().contains(&name.to_lowercase()) {
            return parts[1].to_string();
        }
    }
    name.to_string()
}

// ---------------------------------------------------------------------------
// WiFi
// ---------------------------------------------------------------------------

/// List available WiFi networks.
pub fn wifi_list() -> String {
    let out = run(
        "nmcli",
        &["-t", "-f", "SSID,SIGNAL", "device", "wifi", "list"],
    );
    if out.is_empty() {
        return "Could not scan WiFi networks.".to_string();
    }
    let mut results: Vec<String> = Vec::new();
    for line in out.lines().skip(1).take(8) {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 2 && !parts[0].is_empty() {
            results.push(format!("{} ({}%)", parts[0], parts[1]));
        }
    }
    if results.is_empty() {
        "No WiFi networks found.".to_string()
    } else {
        format!("WiFi networks: {}", results.join(", "))
    }
}

/// Connect to a WiFi network.
pub fn wifi_connect(ssid: &str) -> String {
    if ssid.is_empty() {
        return "Which network? Say 'wifi connect' followed by network name.".to_string();
    }
    let out = run("nmcli", &["device", "wifi", "connect", ssid]);
    if out.contains("Error") || out.contains("error") {
        format!("Failed to connect to {ssid}. It may require a password.")
    } else {
        format!("Connected to {ssid}.")
    }
}

/// Disconnect from WiFi.
pub fn wifi_disconnect() -> String {
    let out = run("nmcli", &["device", "disconnect", "wlan0"]);
    if out.contains("Error") || out.contains("error") {
        "Failed to disconnect WiFi.".to_string()
    } else {
        "WiFi disconnected.".to_string()
    }
}

/// Get WiFi status.
pub fn wifi_status() -> String {
    let out = run(
        "nmcli",
        &["-t", "-f", "ACTIVE,SSID,SIGNAL", "device", "wifi", "list"],
    );
    for line in out.lines() {
        if line.starts_with("yes:") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                return format!("Connected to {} with {}% signal.", parts[1], parts[2]);
            }
        }
    }
    "Not connected to WiFi.".to_string()
}

/// Parse a WiFi command from user text.
pub fn handle_wifi(text: &str) -> Option<String> {
    let low = text.to_lowercase();
    if !low.contains("wifi") && !low.contains("wi-fi") && !low.contains("wireless") {
        return None;
    }
    if low.contains("list") || low.contains("networks") || low.contains("scan") {
        return Some(wifi_list());
    }
    if low.contains("connect") {
        let ssid = extract_after(
            &low,
            &["wifi connect", "wi-fi connect", "connect to", "connect"],
        );
        return Some(wifi_connect(&ssid));
    }
    if low.contains("disconnect") {
        return Some(wifi_disconnect());
    }
    if low.contains("status") {
        return Some(wifi_status());
    }
    Some(wifi_status())
}

// ---------------------------------------------------------------------------
// Audio output switching
// ---------------------------------------------------------------------------

/// List audio output sinks.
pub fn audio_list_sinks() -> String {
    let out = run("pactl", &["list", "short", "sinks"]);
    if out.is_empty() {
        return "No audio outputs found.".to_string();
    }
    let lines: Vec<&str> = out.lines().take(6).collect();
    format!("Audio outputs: {}", lines.join("; "))
}

/// Switch audio output by sink name or index.
pub fn audio_switch(target: &str) -> String {
    if target.is_empty() {
        return "Which speaker? Say 'switch speaker 1' or 'switch to headphones'.".to_string();
    }
    // Try pactl set-default-sink with the target directly
    let out = run("pactl", &["set-default-sink", target]);
    if out.contains("Error") || out.contains("error") {
        format!("Failed to switch audio output to {target}.")
    } else {
        format!("Switched audio output to {target}.")
    }
}

/// Parse an audio output command from user text.
pub fn handle_audio(text: &str) -> Option<String> {
    let low = text.to_lowercase();
    if !low.contains("speaker")
        && !low.contains("audio output")
        && !low.contains("headphone")
        && !low.contains("headset")
        && !low.contains("switch audio")
    {
        return None;
    }
    if low.contains("list") || low.contains("show") || low.contains("outputs") {
        return Some(audio_list_sinks());
    }
    if low.contains("switch") {
        let target = extract_after(
            &low,
            &["switch speaker", "switch audio", "switch to", "switch"],
        );
        return Some(audio_switch(&target));
    }
    Some(audio_list_sinks())
}

// ---------------------------------------------------------------------------
// Timer / countdown
// ---------------------------------------------------------------------------

/// Simple timer: sleeps in a background thread and returns when done.
/// Since this is called synchronously, we report the timer was set.
pub fn set_timer(text: &str) -> String {
    let minutes = extract_number(text).unwrap_or(5);
    let secs = minutes * 60;
    // Spawn a background thread that will notify when done.
    // In the voice-native context, the TTS callback will announce "Timer done".
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secs as u64));
        // Best-effort notification via notify-send if available
        let _ = Command::new("notify-send")
            .args([
                "-u",
                "critical",
                "Artume",
                &format!("Timer for {minutes} minutes is done!"),
            ])
            .output();
    });
    format!(
        "Timer set for {minutes} minute{}.",
        if minutes == 1 { "" } else { "s" }
    )
}

/// Parse a timer command from user text.
pub fn handle_timer(text: &str) -> Option<String> {
    let low = text.to_lowercase();
    if !low.contains("timer") && !low.contains("countdown") && !low.contains("remind") {
        return None;
    }
    if low.contains("cancel") {
        // Note: actual cancellation would need shared state.
        return Some(
            "Timer cancellation is not yet supported. Please wait for it to finish.".to_string(),
        );
    }
    Some(set_timer(text))
}

// ---------------------------------------------------------------------------
// System status
// ---------------------------------------------------------------------------

/// Get battery status.
pub fn battery_status() -> String {
    let out = run("cat", &["/sys/class/power_supply/BAT0/capacity"]);
    if let Ok(pct) = out.trim().parse::<u32>() {
        let charging = run("cat", &["/sys/class/power_supply/BAT0/status"]);
        let status = if charging.to_uppercase().contains("CHARGING") {
            "charging"
        } else {
            "on battery"
        };
        format!("Battery is at {pct} percent, {status}.")
    } else {
        // Try UPower
        let out = run(
            "upower",
            &["-i", "/org/freedesktop/UPower/devices/battery_BAT0"],
        );
        for line in out.lines() {
            if line.contains("percentage:") {
                return format!("Battery {}.", line.trim());
            }
        }
        "Could not read battery status.".to_string()
    }
}

/// Get system status (time, battery, uptime, load).
pub fn system_status() -> String {
    let time = run("date", &["+%I:%M %p"]);
    let battery = battery_status();
    let uptime = run("uptime", &["-p"]);
    let load = run("cat", &["/proc/loadavg"]);
    let load_short: String = load
        .split_whitespace()
        .take(3)
        .collect::<Vec<&str>>()
        .join(", ");

    format!(
        "Current time: {}. System load: {}. {}. Uptime: {}.",
        time.trim(),
        load_short,
        battery,
        uptime.trim().replace("up ", "")
    )
}

/// Parse a system status command from user text.
pub fn handle_status(text: &str) -> Option<String> {
    let low = text.to_lowercase();
    if low.contains("battery") || low.contains("power") {
        return Some(battery_status());
    }
    if low.contains("status")
        || low.contains("system")
        || low.contains("time")
        || low.contains("uptime")
        || low.contains("load")
    {
        return Some(system_status());
    }
    Some(system_status())
}

// ---------------------------------------------------------------------------
// Top-level dispatcher
// ---------------------------------------------------------------------------

/// Dispatch a system command based on user text.
/// Returns Some(response) if handled, None if not a system command.
pub fn dispatch(text: &str) -> Option<String> {
    let low = text.to_lowercase();

    // Order matters: check specific commands first.
    if let Some(r) = handle_volume(&low) {
        return Some(r);
    }
    if let Some(r) = handle_bluetooth(&low) {
        return Some(r);
    }
    if let Some(r) = handle_wifi(&low) {
        return Some(r);
    }
    if let Some(r) = handle_audio(&low) {
        return Some(r);
    }
    if let Some(r) = handle_timer(&low) {
        return Some(r);
    }
    if let Some(r) = handle_status(&low) {
        return Some(r);
    }

    None
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

/// Check if a string contains any digit.
fn has_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit())
}

/// Extract the first number from text.
fn extract_number(text: &str) -> Option<u32> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().ok()
}

/// Extract text after the first matching prefix.
fn extract_after(text: &str, prefixes: &[&str]) -> String {
    for prefix in prefixes {
        if let Some(pos) = text.find(prefix) {
            let rest = &text[pos + prefix.len()..];
            // Strip leading punctuation / whitespace
            let cleaned: String = rest
                .chars()
                .skip_while(|c| c.is_whitespace() || *c == ':')
                .collect();
            return cleaned.trim().to_string();
        }
    }
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_number() {
        assert_eq!(extract_number("set volume to 50"), Some(50));
        assert_eq!(extract_number("timer 10 minutes"), Some(10));
        assert_eq!(extract_number("no numbers here"), None);
    }

    #[test]
    fn test_extract_after() {
        assert_eq!(
            extract_after("bluetooth connect headphones", &["bluetooth connect"]),
            "headphones"
        );
        assert_eq!(
            extract_after("wifi connect MyNetwork", &["wifi connect"]),
            "MyNetwork"
        );
        assert_eq!(
            extract_after("switch to headphones", &["switch to", "switch"]),
            "headphones"
        );
    }

    #[test]
    fn test_has_digit() {
        assert!(has_digit("volume 50"));
        assert!(!has_digit("volume up"));
    }

    #[test]
    fn test_handle_volume_keywords() {
        // volume_up / volume_down are side-effecting but we test the parsing.
        let r = handle_volume("volume mute");
        assert!(r.unwrap().to_lowercase().contains("mute"));
    }
}
