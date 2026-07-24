#!/usr/bin/env python3
"""Audio engine for Artome DE: Piper TTS with Barge-In capability and Dynamic VAD speech listener."""

import os
import sys
import time
import subprocess
import threading
import numpy as np
import sounddevice as sd
from earcons import play_earcon

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

    def listen(self, max_duration_sec=15, tts_engine=None):
        """Listen dynamically. If tts_engine is speaking and user speaks, triggers Barge-In!

        Returns numpy array of float32 audio or None.
        """
        speech_buffer = []
        is_speaking = False
        silent_chunks = 0
        start_time = time.time()

        with sd.InputStream(samplerate=self.sample_rate, channels=1, dtype='float32') as stream:
            while True:
                if time.time() - start_time > max_duration_sec:
                    break

                chunk, overflow = stream.read(self.chunk_samples)
                if overflow:
                    pass

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


if __name__ == "__main__":
    tts = PiperTTS()
    listener = DynamicVADListener()
    print("Testing PiperTTS with Barge-In capability...")
    tts.speak("Artome audio engine online with barge-in support.", block=False)
