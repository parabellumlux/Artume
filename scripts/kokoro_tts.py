#!/usr/bin/env python3
"""Kokoro TTS bridge — reads text from stdin, writes 24kHz WAV to stdout.

Usage: echo "Hello" | python3 kokoro_tts.py [--voice af_heart]

Uses the project venv's Python and the kokoro package.
"""
import os
os.environ['CUDA_VISIBLE_DEVICES'] = ''  # Force CPU
os.environ['HF_HUB_DISABLE_SYMLINKS_WARNING'] = '1'

import sys
import io
import wave
import numpy as np
import logging

# Suppress all logging/warnings to stdout
logging.basicConfig(level=logging.ERROR)

# Resolve the project venv
venv_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), '.venv', 'lib')
if os.path.isdir(venv_path):
    py_ver = sorted(os.listdir(venv_path))[-1]  # e.g. python3.12
    site_pkg = os.path.join(venv_path, py_ver, 'site-packages')
    if os.path.isdir(site_pkg):
        sys.path.insert(0, site_pkg)

# Suppress warnings from kokoro/huggingface
import warnings
warnings.filterwarnings('ignore')

from kokoro import KPipeline

def main():
    voice = 'af_heart'
    if len(sys.argv) > 1 and sys.argv[1] == '--voice':
        voice = sys.argv[2]

    text = sys.stdin.read().strip()
    if not text:
        sys.exit(0)

    pipeline = KPipeline(lang_code='a', device='cpu', repo_id='hexgrad/Kokoro-82M')

    all_audio = []
    for gs, ps, audio in pipeline(text, voice=voice):
        all_audio.append(audio)

    if not all_audio:
        sys.exit(0)

    combined = np.concatenate(all_audio)

    # Write WAV to stdout
    with io.BytesIO() as buf:
        with wave.open(buf, 'wb') as wf:
            wf.setnchannels(1)
            wf.setsampwidth(2)
            wf.setframerate(24000)
            # Convert f32 [-1,1] to s16
            s16 = (combined * 32767).astype(np.int16)
            wf.writeframes(s16.tobytes())
        sys.stdout.buffer.write(buf.getvalue())

if __name__ == '__main__':
    main()
