# Cardona Pipeline Tools: SoundCraft audio preview

Independent development fork of [SoundCraft](https://github.com/storytold/soundcraft) by the ArtCraft team, with local AudioForge speech and ACE-Step music generation.

## Generate Voice

Enter a script, choose a voice preset or save your own, and generate a WAV take. Keep up to eight takes, compare their original settings, rename, audition, export, or insert at the playhead. Insertions are undoable. Saved sessions retain source text, voice, speed, and audio.

## Generate Music

Choose an editable genre preset, set duration, tempo and seed, and generate an instrumental. Enable Sung vocals and enter original English lyrics for a mixed song. Spoken TTS vocals can be arranged on a separate track. Source metadata retains prompt, genre, model configuration, lyrics, and generation settings.

See [setup, screenshots, verification, and limits](docs/integrations/audioforge.md). Both Python bridges use only the standard library. Models run in separate local services; the app does not download or start them automatically.

## Development

The Windows preview build and complete repository gates passed at a1982fb. Native tests passed presets, take management, real speech/music/singing generation, undo, save/reopen, metadata persistence, and mix export. This remains a development preview. Native speaker audition playback and independent vocal stems remain unverified or unimplemented.

Run `cargo xtask ci` for repository checks and `python -m unittest discover -s integrations/audioforge -v` for bridge tests. The local music adapter follows the [official ACE-Step API](https://ace-step.github.io/ACE-Step-1.5/en/API).

## Attribution and licenses

Based on SoundCraft by the ArtCraft team and its contributors. Code remains MIT or Apache-2.0. See LICENSE-MIT, LICENSE-APACHE, NOTICE and ATTRIBUTION.md. ArtCraft trademark artwork was removed from this modified fork; its license text is retained in docs/brand/LICENSE-brand.txt. ACE-Step is a separately installed MIT-licensed engine.
