# Cardona Pipeline Tools: SoundCraft voice preview

Development fork of [SoundCraft](https://github.com/storytold/soundcraft) by the ArtCraft team. This preview adds local AudioForge text-to-speech generation to the native Rust audio editor. It is an independent modified version.

## Generate Voice

Enter a script and voice ID, generate a WAV take through a local AudioForge-compatible service, audition it, then insert it at the playhead. Insertions use the normal undo boundary. Source text, voice and speed are saved with the audio source.

See [setup and current limits](docs/integrations/audioforge.md). The small Python bridge uses only the standard library. The TTS model runs outside SoundCraft's realtime audio engine. No model is automatically downloaded or started.

## Development

The upstream Rust workspace and command architecture remain intact. Run `cargo xtask ci` for repository checks. The AudioForge preview workflow builds and checks the Windows version. Native verification is in progress; this is not a production release.

The next preview adds voice presets, take selection/rename/export/discard, and a local ACE-Step instrumental adapter. Its build and native verification are in progress. Sentence-level regeneration remains planned.

## Attribution and licenses

Based on SoundCraft by the ArtCraft team and its contributors. Code remains licensed under MIT or Apache-2.0. See LICENSE-MIT, LICENSE-APACHE, NOTICE and ATTRIBUTION.md for the applicable notices. ArtCraft trademark artwork has been removed from this modified fork; its license text is retained in docs/brand/LICENSE-brand.txt.
