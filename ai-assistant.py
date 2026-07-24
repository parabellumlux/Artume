#!/usr/bin/env python3
"""AI-powered voice assistant for blind users."""
import sounddevice as sd
import numpy as np
from faster_whisper import WhisperModel
import subprocess
import requests
import json
import re
import os

print("Loading AI brain...")
whisper_model = WhisperModel("tiny.en", device="cpu", compute_type="int8")

SAMPLE_RATE = 16000
LISTEN_DURATION = 4

def speak(text, interrupt=True):
    """Speak text. If interrupt, stop any current speech first."""
    print(f"ASSISTANT: {text}")
    if interrupt:
        subprocess.run(["killall", "piper"], stderr=subprocess.DEVNULL)
        subprocess.run(["killall", "aplay"], stderr=subprocess.DEVNULL)
    process = subprocess.Popen(
        ["./piper/piper", "-m", "en_US-lessac-medium.onnx", "--output-raw"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE
    )
    output, _ = process.communicate(input=text.encode())
    subprocess.run(["aplay", "-r", "22050", "-f", "S16_LE", "-t", "raw"],
                   input=output)

def listen_for_command():
    """Listen for a voice command."""
    print("Listening...")
    audio = sd.rec(int(LISTEN_DURATION * SAMPLE_RATE),
                   samplerate=SAMPLE_RATE,
                   channels=1,
                   dtype='float32')
    sd.wait()
    audio = audio.flatten()
    segments, _ = whisper_model.transcribe(audio, beam_size=5)
    text = " ".join([seg.text for seg in segments]).strip()
    if text:
        print(f"YOU: {text}")
        return text
    return None

def ask_ai(user_input):
    """Send to Ollama with a system prompt optimized for blind users."""
    system_prompt = """You are a voice assistant for a blind person.
    Be EXTREMELY concise. No pleasantries. No explanations unless asked.

    You can suggest these actions by replying with commands:
    COMMAND:OPEN firefox
    COMMAND:TYPE cats
    COMMAND:PRESS enter
    COMMAND:READ <text to read>
    COMMAND:SYSTEM <shell command>
    COMMAND:TELL <information to speak>

    Example: User says 'search for weather'
    You reply:
    COMMAND:TELL Opening browser to search for weather
    COMMAND:OPEN firefox
    COMMAND:TYPE weather today
    COMMAND:PRESS enter

    If user just wants information, just use COMMAND:TELL.
    Keep all responses under 20 words unless absolutely necessary."""

    try:
        response = requests.post(
            "http://localhost:11434/api/generate",
            json={
                "model": "tinyllama",
                "prompt": f"{system_prompt}\n\nUser: {user_input}\nAssistant:",
                "stream": False,
                "options": {"temperature": 0.7, "num_predict": 100}
            },
            timeout=15
        )
        return response.json()["response"].strip()
    except Exception as e:
        return f"COMMAND:TELL Error connecting to AI: {str(e)[:50]}"

def execute_command(command_str):
    """Parse and execute a COMMAND: from the AI."""
    match = re.match(r"COMMAND:(\w+)\s+(.+)", command_str)
    if not match:
        return
    cmd_type = match.group(1).upper()
    args = match.group(2)
    if cmd_type == "TELL":
        speak(args)
    elif cmd_type == "OPEN":
        subprocess.Popen([args], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        speak(f"Opened {args}")
    elif cmd_type == "TYPE":
        subprocess.run(["xdotool", "type", args])
    elif cmd_type == "PRESS":
        subprocess.run(["xdotool", "key", args])
    elif cmd_type == "SYSTEM":
        try:
            result = subprocess.check_output(args, shell=True, stderr=subprocess.STDOUT, text=True)
            if result.strip():
                speak(result[:200])
        except Exception as e:
            speak(f"Command failed: {str(e)[:50]}")
    elif cmd_type == "READ":
        speak(args)

speak("AI assistant ready. How can I help?")
while True:
    try:
        user_input = listen_for_command()
        if not user_input:
            continue
        if any(word in user_input.lower() for word in ["exit", "quit", "goodbye", "stop"]):
            speak("Goodbye.")
            break
        print("Thinking...")
        ai_response = ask_ai(user_input)
        print(f"AI: {ai_response}")
        for line in ai_response.split('\n'):
            if line.strip().startswith("COMMAND:"):
                execute_command(line.strip())
    except KeyboardInterrupt:
        speak("Stopping.")
        break
    except Exception as e:
        speak(f"Error: {str(e)[:50]}")
