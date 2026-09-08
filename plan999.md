# Artume OS — Phase 999: Pre-Recorded Anticipatory Responses

> Reducing perceived response time by playing pre-recorded acknowledgments while the LLM generates the actual response.

---

## The Problem

The current latency pipeline has a critical bottleneck: **every spoken response, including short acknowledgments, goes through the full TTS synthesis path** (piper spawn → model load → batch synthesis → aplay). Even a 3-word ack like "Searching for files" costs ~1.5s of synthesis time.

### Current Latency Breakdown (warm system)

| Stage | Best (keyword) | Worst (LLM) |
|-------|---------------|-------------|
| End-of-speech detect | 750ms (fixed) | 750ms |
| STT (tiny.en int8) | ~300ms | ~1s |
| Classification | ~0ms | 350ms–18s |
| Dispatch | <1ms | <1ms |
| **Ack TTS (piper spawn+synth)** | **~1.5s** | **~1.5s** |
| Response generation (handler) | 50–500ms | 5–30s (IDE AI) |
| Result TTS | ~1.5s | scales with length |

**The ack TTS is the bottleneck for perceived responsiveness.** A keyword-classified command returns in <1ms, then the user waits 1.5s just to hear "Increasing volume" before the 50ms amixer call runs.

### The Opportunity

The architecture already has acknowledge-then-act: `_dispatch()` returns a `speech` field like "Loading {url}" which is spoken **before** the handler runs. But this ack is synthesized live. Playing a pre-recorded WAV instead cuts ack latency from **~1.5s to ~100ms** (aplay startup).

For the longest-running commands (IDE AI at 5–30s), a pre-recorded "Looking into that..." followed by a streaming response would feel dramatically more responsive.

---

## Design: How It Works

### 1. Clip Registry

A mapping from `(action, target_prefix)` → pre-recorded WAV file. The registry knows:
- Which clip to play for which command
- The text the clip says (for repetition avoidance)
- Whether to play the clip immediately or wait for classification

```
clip_registry = {
    ("setting_action", "volume_up"): {
        "file": "clips/volume_up.wav",
        "text": "Volume up.",
        "play": "immediate",  # play as soon as intent is classified
    },
    ("web_navigate", "url:"): {
        "file": "clips/loading_page.wav",
        "text": "Loading that page for you now.",
        "play": "immediate",
    },
    ("ide_action", "fix"): {
        "file": "clips/looking_into_it.wav",
        "text": "Looking into that.",
        "play": "immediate",
    },
    # ... 30+ clips
}
```

### 2. Injection Point: `execute_action()`

Currently (artome_core.py:164):
```python
if speech:
    tts.speak(speech)  # 1.5s piper synthesis
```

Proposed:
```python
if speech:
    clip = clip_registry.lookup(action, target)
    if clip:
        play_earcon(clip.file)  # ~100ms, non-blocking
        last_clip_text = clip.text  # for repetition avoidance
    else:
        tts.speak(speech)  # fallback to live TTS
```

### 3. Repetition Avoidance

Three mechanisms work together:

**A. LLM prompt injection** — When a clip was played, the conversation context includes:
```
[System note: The user already heard "Looking into that." 
Do not repeat this acknowledgment. Jump straight to the result.]
```

**B. Response prefix stripping** — If the handler's response starts with the same words as the clip, strip the duplicate:
```python
if clip and response.lower().startswith(clip.text.lower()[:20]):
    response = response[len(clip.text):].lstrip()
```

**C. Handler-level awareness** — Handlers receive a `clip_played` flag and can skip their own acknowledgment:
```python
def handle_fix_code(target, clip_played=False):
    if not clip_played:
        tts.speak("Looking into that.")
    # ... actual work
```

### 4. Pre-Generation Pipeline

A script (`scripts/generate_clips.py`) that:
1. Takes a list of `(phrase, filename)` pairs
2. Runs each through PiperTTS or Kokoro
3. Saves WAVs to `clips/` directory
4. Generates `clip_registry.py` with the mappings

This runs once during setup, not at runtime. The WAVs are committed to the repo.

---

## Where Pre-Recorded Responses Help Most

### Tier 1: Immediate Acknowledgment (before any LLM/classification)

These play the instant end-of-speech is detected, **before STT even starts**:

| Clip | When | Latency Saved |
|------|------|---------------|
| `thinking.wav` (exists) | End-of-speech, always | Already plays — just needs to be a spoken phrase instead of a beep |
| `on_it.wav` | End-of-speech, always | 750ms + 300ms STT + 0ms classification = user hears ack in ~100ms vs ~1.1s |

### Tier 2: Intent-Classified Acknowledgments (after classification, before handler)

These play the instant the intent is classified, **before the handler runs**:

| Action | Current Ack | Pre-Recorded Clip | Latency Saved |
|--------|-------------|-------------------|---------------|
| `setting_action` volume up/down | "Increasing/Decreasing volume" | `volume_up.wav` / `volume_down.wav` | ~1.3s |
| `setting_action` status | "Checking system status" | `checking_status.wav` | ~1.3s |
| `switch_mode` | "Switching to {m} mode" | `switching_mode.wav` | ~1.3s |
| `file_search` | "Searching for {q}" | `searching_files.wav` | ~1.3s |
| `open_app` | "Opening {app}" | `opening_app.wav` | ~1.3s |
| `web_navigate` | "Loading {url}" | `loading_page.wav` | ~1.3s |
| `email_action` inbox | "Checking your inbox" | `checking_email.wav` | ~1.3s |
| `entity_lookup` | "Looking up that entity" | `looking_it_up.wav` | ~1.3s |

### Tier 3: Long-Running Command Acknowledgments (before 5–30s work)

These have the **highest impact** because the handler takes seconds:

| Action | Current Ack | Pre-Recorded Clip | Latency Saved |
|--------|-------------|-------------------|---------------|
| IDE AI (fix/explain/test/review) | "AI: {text}" | `looking_into_it.wav` | ~1.3s |
| IDE Git (commit/push/pull) | "Git: {text}" | `working_on_it.wav` | ~1.3s |
| Terminal (run tests/make) | "Terminal: {text}" | `running_that_now.wav` | ~1.3s |
| Screen summary | "Reading screen" | `reading_screen.wav` | ~1.3s |
| BT scan | "Scanning..." | `scanning.wav` | ~1.3s |
| WiFi connect | "Connected to {s}" | `connecting_wifi.wav` | ~1.3s |

### Tier 4: Confirmation Responses (after user confirms)

| Action | Current Flow | Pre-Recorded Clip |
|--------|-------------|-------------------|
| Destructive confirm | "Are you sure?" → User: "Yes" → TTS: "Shutting down..." | `confirming_action.wav` |
| Timer set | TTS: "Timer set for X minutes" | `timer_set.wav` |
| Volume mute toggle | TTS: "Toggled mute" | `muted.wav` / `unmuted.wav` |

---

## The Don't-Repeat Problem: Detailed Solution

### Problem Scenario

1. User says: "Fix this code"
2. Classification: `ide_action` with target "fix"
3. Pre-recorded clip plays: *"Looking into that."* (~100ms)
4. LLM generates response: *"I'll look into that code for you. Here's what I found..."*
5. User hears: "Looking into that. I'll look into that code for you..." — **redundant**

### Solution: Three-Layer Prevention

**Layer 1: Clip text in system prompt**

When a clip was played before the LLM call, inject into the conversation context:
```
[System: The user just heard a pre-recorded message: "Looking into that."
Do NOT repeat this acknowledgment. Respond with the actual result only.
Start your response with the finding, not the preamble.]
```

**Layer 2: Response prefix detection**

After the handler returns, check if the response starts with the clip text:
```python
def _strip_clip_duplicate(clip_text, response):
    """Remove redundant ack prefix from response if it duplicates the clip."""
    if not clip_text:
        return response
    clip_lower = clip_text.lower().rstrip('. ')
    resp_lower = response.lower().lstrip()
    # Check if response starts with same words as clip
    if resp_lower.startswith(clip_lower):
        return response[len(clip_text):].lstrip(' .,')
    # Check fuzzy overlap (first 3+ words match)
    clip_words = clip_lower.split()[:3]
    resp_words = resp_lower.split()[:len(clip_words)]
    if clip_words == resp_words:
        return response[len(' '.join(clip_words)):].lstrip(' .,')
    return response
```

**Layer 3: Handler flag**

Pass `clip_played=True` to handlers that do their own acknowledgment:
```python
# In handlers/ide.py
def handle_ide_action(target, low_speech, ctx, clip_played=False):
    if not clip_played:
        play_earcon("info")
        tts.speak(f"AI: {target}")
    # ... actual work
```

---

## Audio Pipeline for Clips

### Option A: Use existing `play_earcon()` (simplest)

```python
# In execute_action(), after dispatch returns:
clip = clip_registry.lookup(action, target)
if clip:
    play_earcon(clip["file"])  # plays from sounds/ or clips/ dir
```

**Pros:** Zero new infrastructure. `play_earcon()` already handles non-blocking WAV playback via `aplay`.
**Cons:** Only one clip plays at a time (new clip kills previous). No mixing with earcons.

### Option B: Use `SpatialAudioEngine` (richer)

```python
# SpatialAudioEngine.play_spatial() uses sounddevice + numpy stereo panning
spatial_engine.play_spatial("clips/looking_into_it.wav", pan=0.0)
```

**Pros:** Stereo panning, can mix with earcons, no process spawn overhead.
**Cons:** Currently only used in tests; needs production wiring.

### Option C: Dedicated clip player (most control)

A `ClipPlayer` class that:
- Pre-loads frequently-used clips into memory on startup
- Plays via `sounddevice` (no subprocess spawn)
- Supports interrupt (barge-in kills current clip)
- Reports playback state (is_playing, current_clip)

**Pros:** Sub-50ms startup, memory-mapped, barge-in aware.
**Cons:** More code, memory usage for 30+ WAV files.

### Recommendation: Option A for Phase 999, Option C as future optimization

The existing `play_earcon()` infrastructure is battle-tested and handles all the edge cases. Starting with it means Phase 999 can be implemented in ~200 lines of Python. Option C is worth it only if the ~100ms aplay startup latency becomes noticeable.

---

## Pre-Generated Clip List

### System Clips (10 files, ~30s total audio)

| Filename | Phrase | Use Case |
|----------|--------|----------|
| `on_it.wav` | "On it." | Generic immediate ack |
| `working_on_it.wav` | "Working on that." | Long-running commands |
| `checking_status.wav` | "Checking your system status." | Status/battery/time |
| `searching_files.wav` | "Searching your files now." | File search |
| `loading_page.wav` | "Loading that page for you." | Web fetch |
| `switching_mode.wav` | "Switching modes." | Mode change |
| `looking_into_it.wav` | "Looking into that." | IDE AI commands |
| `running_that_now.wav` | "Running that now." | Terminal commands |
| `checking_email.wav` | "Checking your email." | Email inbox |
| `connecting_wifi.wav` | "Connecting to WiFi." | WiFi connect |

### Hardware Clips (8 files, ~20s total audio)

| Filename | Phrase | Use Case |
|----------|--------|----------|
| `volume_up.wav` | "Volume up." | Volume increase |
| `volume_down.wav` | "Volume down." | Volume decrease |
| `muted.wav` | "Muted." | Mute on |
| `unmuted.wav` | "Unmuted." | Mute off |
| `bluetooth_on.wav` | "Bluetooth on." | BT enable |
| `bluetooth_off.wav` | "Bluetooth off." | BT disable |
| `scanning.wav` | "Scanning for devices." | BT scan |
| `timer_set.wav` | "Timer set." | Timer |

### Confirmation Clips (4 files, ~10s total audio)

| Filename | Phrase | Use Case |
|----------|--------|----------|
| `confirming.wav` | "Confirmed." | Post-confirmation |
| `cancelled.wav` | "Cancelled." | Action cancelled |
| `opening_app.wav` | "Opening that for you." | App launch |
| `not_sure.wav` | "I'm not sure what you meant." | Unknown intent |

**Total: ~22 clips, ~60s of audio, ~500KB of WAV files.**

---

## Implementation Plan

| # | Task | Description | Files | Effort |
|---|------|-------------|-------|--------|
| 999.1 | Create `clips/` directory structure | `clips/system/`, `clips/hardware/`, `clips/confirm/` | New dirs | Trivial |
| 999.2 | Write `scripts/generate_clips.py` | Takes phrase list → runs PiperTTS → saves WAVs to `clips/` | New script | Medium |
| 999.3 | Generate all 22 clips | Run the script, commit WAVs | `clips/*.wav` | Small |
| 999.4 | Create `clip_registry.py` | Maps `(action, target_prefix)` → clip metadata | New module | Small |
| 999.5 | Modify `execute_action()` | Check registry before `tts.speak()`, play clip if found | `artome_core.py` | Small |
| 999.6 | Implement `_strip_clip_duplicate()` | Remove redundant ack prefix from responses | `artome_core.py` | Small |
| 999.7 | Add clip context to LLM prompt | Inject "user already heard X" when clip was played | `intent_router.py` | Small |
| 999.8 | Add `clip_played` flag to handlers | Modify handlers to skip ack when clip already played | `handlers/*.py` | Medium |
| 999.9 | Add immediate ack on end-of-speech | Play `on_it.wav` before STT completes | `audio_engine.py` | Small |
| 999.10 | Tests | Unit test clip registry, prefix stripping, integration test | `tests/` | Medium |

**Estimated effort: 1–2 days**

---

## Future Optimizations (Phase 999+)

| # | Task | Description | Effort |
|---|------|-------------|--------|
| 999.11 | Pre-load clips into memory | Avoid aplay subprocess spawn; use sounddevice for sub-50ms playback | Medium |
| 999.12 | Streaming TTS for long responses | Sentence-level streaming so first sentence plays while rest generates | Large |
| 999.13 | Adaptive clip selection | Choose clip based on time-of-day, user preference, conversation context | Medium |
| 999.14 | Clip volume mixing | Play clips at lower volume when user is in "listening" mode | Small |
| 999.15 | User-recorded clips | Let user record custom ack phrases via voice | Large |
| 999.16 | Emotion-aware clips | Choose clip tone based on command type (serious for shutdown, upbeat for success) | Medium |

---

*Generated: 2026-08-20 by opencode (Phase 999 research & plan)*
