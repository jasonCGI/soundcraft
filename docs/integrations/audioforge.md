# AudioForge voice preview

This fork adds a native Generate Voice panel backed by the existing local AudioForge/Kokoro speech API. It works independently of CourseForge. The Python bridge uses only the standard library; SoundCraft remains a Rust application.

## Setup

Run an existing local AudioForge-compatible service. The default endpoint is http://127.0.0.1:8800. No remote services or system proxies are used by the bridge.

Set SOUNDCRAFT_AUDIOFORGE_PYTHON to a Python 3 executable and SOUNDCRAFT_AUDIOFORGE_BRIDGE to the absolute path of integrations/audioforge/bridge.py. Open Generate Voice, enter a script and voice ID, generate a take, audition it, and insert it at the playhead. Every insertion creates a new track and can be undone. Save the session to retain audio and generation metadata.

The first adapter targets Kokoro. Speech is requested as WAV with a 5000-character limit and a 25 MB response limit. Generation runs in a background process. Cancellation stops the local client; the provider may finish its outstanding request. No automatic model download or engine startup occurs.

## Automation

- window.generate_voice: open or close the panel
- audioforge.generate: input, voice, speed, endpoint
- audioforge.inspect: job state and completed take
- audioforge.cancel: cancel the local generation client
- audioforge.insert: import a WAV take with input, voice, speed, at, and optional target track

Inserted sources store the provider, source text, voice, and speed. Provider model revision is not currently exposed by AudioForge's speech response and is not invented.

## Current limits

Native desktop only. No music generation, pronunciation dictionary, sentence replacement, take browser, automatic startup, or approval workflow yet. Audition uses a separate playback instance and is disabled while the session transport is running. Check the build and listening results before using the preview for production.

## Verification

Windows CI passed the native app build and complete `cargo xtask ci` gates, including formatting, strict workspace Clippy, workspace tests, asset and layer checks, and WASM checks. Three bridge tests and three engine regression tests cover input rejection, speech requests, insertion, undo, metadata round trips, and separate saved takes.

The real local Kokoro endpoint produced an 8.45-second mono WAV at 24 kHz through the bridge. The official v0.3.0 CLI also passed import, save/reopen, and bounce checks. The built Windows preview passed real TTS through its native control channel, insertion, undo, metadata save, reopen, mix export, and screenshot capture. The screenshot was visually checked. Speaker playback and subjective voice quality still require a listening check.

![Generate Voice panel with an inserted take](../images/audioforge-voice.png)

