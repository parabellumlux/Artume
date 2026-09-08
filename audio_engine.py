#!/usr/bin/env python3
"""Audio engine for Artome DE: Piper TTS with Barge-In capability and Dynamic VAD speech listener."""

import os
import time
import subprocess
import threading
import numpy as np
import sounddevice as sd
from earcons import play_earcon, SOUNDS_DIR, IDE_SOUNDS_DIR

PIPER_DIR = os.path.dirname(os.path.abspath(__file__))
PIPER_BIN = os.path.join(PIPER_DIR, "piper", "piper")
MODEL_FILE = os.path.join(PIPER_DIR, "en_US-lessac-medium.onnx")

class PiperTTS:
    """Non-blocking Piper Text-to-Speech manager with speech interruption & Barge-In support."""

    def __init__(self):
        self.speech_process = None
        self.aplay_process = None
        self.is_speaking = False
        self._lock = threading.Lock()

    def stop(self):
        """Immediately interrupt any ongoing speech synthesis or playback."""
        with self._lock:
            self.is_speaking = False
            if self.speech_process and self.speech_process.poll() is None:
                try:
                    self.speech_process.kill()
                except Exception:
                    pass
            if self.aplay_process and self.aplay_process.poll() is None:
                try:
                    self.aplay_process.kill()
                except Exception:
                    pass
            subprocess.run(["killall", "-9", "piper", "aplay"], 
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def speak(self, text, interrupt=True, block=True, listener_callback=None):
        """Synthesize and speak text. Supports Barge-In speech interruption.

        :param text: Text string to speak.
        :param interrupt: If True, stop current speech before starting.
        :param block: If True, wait until speech finishes before returning.
        :param listener_callback: Optional listener to check for barge-in voice interruption.
        """
        if not text or not text.strip():
            return

        print(f"\n[TTS Spoken Summary]: {text}\n", flush=True)

        if interrupt:
            self.stop()

        with self._lock:
            self.is_speaking = True

        def _run_speech():
            try:
                self.speech_process = subprocess.Popen(
                    [PIPER_BIN, "-m", MODEL_FILE, "--output-raw"],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.DEVNULL
                )
                raw_pcm, _ = self.speech_process.communicate(input=text.encode('utf-8'))
                if not raw_pcm or not self.is_speaking:
                    return

                self.aplay_process = subprocess.Popen(
                    ["aplay", "-q", "-r", "22050", "-f", "S16_LE", "-t", "raw"],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL
                )
                self.aplay_process.communicate(input=raw_pcm)
            except Exception as e:
                print(f"TTS Error: {e}")
            finally:
                with self._lock:
                    self.is_speaking = False

        if block:
            _run_speech()
        else:
            t = threading.Thread(target=_run_speech, daemon=True)
            t.start()


class DynamicVADListener:
    """Continuous VAD speech listener with dynamic start/stop detection and TTS Barge-In support."""

    def __init__(self, sample_rate=16000, chunk_ms=50, silence_threshold_ms=800, energy_threshold=0.018):
        self.sample_rate = sample_rate
        self.chunk_ms = chunk_ms
        self.chunk_samples = int(sample_rate * (chunk_ms / 1000.0))
        self.silence_threshold_chunks = int(silence_threshold_ms / chunk_ms)
        self.energy_threshold = energy_threshold
        self.is_listening = False

    def listen(self, max_duration_sec=15, tts_engine=None):
        """Listen dynamically. If tts_engine is speaking and user speaks, triggers Barge-In!

        Returns numpy array of float32 audio or None.
        """
        speech_buffer = []
        start_time = time.time()
        self.is_listening = True

        try:
            return self._listen_loop(max_duration_sec, tts_engine, speech_buffer, start_time)
        finally:
            self.is_listening = False

    def _listen_loop(self, max_duration_sec, tts_engine, speech_buffer, start_time):
        """Internal listen loop — extracted for clean finally-block exit."""
        is_speaking = False
        silent_chunks = 0

        with sd.InputStream(samplerate=self.sample_rate, channels=1, dtype='float32') as stream:
            while True:
                if time.time() - start_time > max_duration_sec:
                    break

                chunk, overflow = stream.read(self.chunk_samples)
                if overflow:
                    import logging
                    logging.warning("Audio buffer overflow detected — some samples were dropped")

                audio_data = chunk.flatten()
                rms = np.sqrt(np.mean(audio_data ** 2))

                # User Speech Detection
                if rms >= self.energy_threshold:
                    # BARGE-IN: If TTS is playing, stop TTS immediately!
                    if tts_engine and tts_engine.is_speaking:
                        print("BARGE-IN DETECTED! Interrupting TTS playback...")
                        tts_engine.stop()

                    if not is_speaking:
                        is_speaking = True
                        play_earcon("listening")
                        print("Speech detected...")
                    speech_buffer.append(audio_data)
                    silent_chunks = 0
                else:
                    if is_speaking:
                        speech_buffer.append(audio_data)
                        silent_chunks += 1
                        if silent_chunks >= self.silence_threshold_chunks:
                            print("End of speech detected.")
                            play_earcon("thinking")
                            break

        if speech_buffer:
            return np.concatenate(speech_buffer)
        return None


class SpatialAudioEngine:
    """Spatial earcon playback engine using sounddevice stereo panning.

    Maps earcon names to spatial positions in the stereo field, so functions,
    classes, loops, and conditionals each land in a distinct location.
    """

    SAMPLE_RATE = 22050

    _PAN_MAP = {
        "scope_enter_function": -0.3,
        "scope_exit_function": -0.3,
        "scope_enter_class": 0.3,
        "scope_exit_class": 0.3,
        "loop_iteration": -0.6,
        "conditional": 0.6,
        "git_added": -0.1,
        "git_modified": -0.1,
        "git_deleted": -0.1,
        "git_untracked": -0.1,
        "listening": 0.0,
        "thinking": 0.0,
        "success": 0.0,
        "error": 0.0,
        "warning": 0.0,
        "info": 0.0,
        "breakpoint_hit": 0.0,
        "step_complete": 0.0,
        "variable_changed": 0.0,
        "exception": 0.6,
        "scope_enter_try": 0.6,
        "scope_enter_catch": 0.6,
        "boundary": 0.0,
        "long_line": 0.0,
        "blank_line": 0.0,
        "comment": 0.0,
        "found_match": 0.2,
        "no_match": -0.2,
        "watch_triggered": 0.0,
    }

    def __init__(self):
        import wave as _wave
        self._wave = _wave

    @classmethod
    def _pan_to_gains(cls, pan):
        """Convert a pan value (-1.0 left .. 1.0 right) to (left_gain, right_gain)."""
        pan = max(-1.0, min(1.0, pan))
        right = (pan + 1.0) / 2.0
        left = 1.0 - right
        return left, right

    def _resolve_path(self, earcon_name):
        """Find the WAV file for an earcon, checking both sound directories."""
        base = f"{earcon_name}.wav"
        ide_path = os.path.join(IDE_SOUNDS_DIR, base)
        if os.path.exists(ide_path):
            return ide_path
        sounds_path = os.path.join(SOUNDS_DIR, base)
        if os.path.exists(sounds_path):
            return sounds_path
        return None

    @classmethod
    def _auto_pan(cls, earcon_name):
        """Determine stereo pan from earcon name. Falls back to center (0.0)."""
        if earcon_name in cls._PAN_MAP:
            return cls._PAN_MAP[earcon_name]
        for prefix, pan in cls._PAN_MAP.items():
            if earcon_name.startswith(prefix):
                return pan
        return 0.0

    def play_spatial(self, earcon_name, pan_position=None):
        """Play an earcon with spatial stereo panning (non-blocking).

        :param earcon_name: Name of the earcon (without .wav).
        :param pan_position: Float -1.0 (left) to 1.0 (right), or None for
            auto-detection based on the earcon name.
        """
        filepath = self._resolve_path(earcon_name)
        if not os.path.exists(os.path.join(SOUNDS_DIR, f"{earcon_name}.wav")):
            from earcons import init_earcons
            init_earcons()
            filepath = self._resolve_path(earcon_name)
        if filepath is None:
            print(f"SpatialAudioEngine: earcon not found: {earcon_name}")
            return

        if pan_position is None:
            pan_position = self._auto_pan(earcon_name)

        left_gain, right_gain = self._pan_to_gains(pan_position)

        def _play():
            try:
                with self._wave.open(filepath, "rb") as wf:
                    raw = wf.readframes(wf.getnframes())
                    n_channels = wf.getnchannels()
                    sampwidth = wf.getsampwidth()

                if sampwidth == 2:
                    samples = np.frombuffer(raw, dtype=np.int16).astype(np.float32)
                elif sampwidth == 1:
                    samples = np.frombuffer(raw, dtype=np.uint8).astype(np.float32)
                    samples = (samples - 128.0) * 256.0
                else:
                    samples = np.frombuffer(raw, dtype=np.int16).astype(np.float32)

                if n_channels == 1:
                    stereo = np.column_stack((samples * left_gain, samples * right_gain))
                else:
                    stereo = samples.reshape(-1, 2).copy()
                    stereo[:, 0] *= left_gain
                    stereo[:, 1] *= right_gain

                sd.play(stereo, samplerate=self.SAMPLE_RATE, blocking=False)
            except Exception as e:
                print(f"SpatialAudioEngine play error ({earcon_name}): {e}")

        t = threading.Thread(target=_play, daemon=True)
        t.start()


if __name__ == "__main__":
    tts = PiperTTS()
    listener = DynamicVADListener()
    print("Testing PiperTTS with Barge-In capability...")
    tts.speak("Artome audio engine online with barge-in support.", block=False)
