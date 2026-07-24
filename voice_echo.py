#!/usr/bin/env python3
"""Simple voice echo - listens, transcribes, speaks back."""
import sounddevice as sd
import numpy as np
from faster_whisper import WhisperModel
import subprocess
import sys

# Initialize Whisper (tiny model for speed, upgrade later)
print("Loading speech recognition model... (one-time download)")
model = WhisperModel("tiny.en", device="cpu", compute_type="int8")

# Audio settings
SAMPLE_RATE = 16000
DURATION = 5  # seconds of listening

def speak(text):
    """Speak using Piper TTS."""
    process = subprocess.Popen(
        ["./piper/piper", "-m", "en_US-lessac-medium.onnx", "--output-raw"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE
    )
    output, _ = process.communicate(input=text.encode())
    subprocess.run(["aplay", "-r", "22050", "-f", "S16_LE", "-t", "raw"],
                   input=output)

def listen():
    """Record audio and return transcription."""
    print("Listening... (speak now)")
    audio = sd.rec(int(DURATION * SAMPLE_RATE), 
                   samplerate=SAMPLE_RATE, 
                   channels=1, 
                   dtype='float32')
    sd.wait()
    audio = audio.flatten()
    
    # Transcribe
    segments, _ = model.transcribe(audio, beam_size=5)
    text = " ".join([seg.text for seg in segments])
    
    if text.strip():
        print(f"You said: {text}")
        return text.strip()
    return None

# Main loop
speak("Voice echo ready. Say something.")
while True:
    try:
        text = listen()
        if text:
            if "exit" in text.lower() or "quit" in text.lower():
                speak("Goodbye.")
                break
            speak(f"You said: {text}")
    except KeyboardInterrupt:
        speak("Stopping.")
        break
