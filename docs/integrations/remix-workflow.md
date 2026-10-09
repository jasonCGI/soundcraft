# Song development and remix workflow

The Generate Music panel connects songwriting, controlled generation, listening review, section replacement and remix export. These features are a development preview.

For a visual end-to-end walkthrough with current interface captures and original song examples, see the [CardonaLab SoundCraft page](https://cardonalab.dev/soundcraft/).

## Take Lab

Choose Arrangement, Vocal delivery or Mix density and queue two or three controlled takes. SoundCraft keeps the prompt, lyrics, tempo and other production decisions fixed, changes the seed for each take, and adds a narrow instruction naming the selected dimension. Generation stays sequential so one cancellable local provider job owns the GPU at a time. The complete set must fit within the eight-take limit.

Enable Blind take names while rating to show neutral candidate labels. Each take has a one-to-five rating, listening notes, the controlled dimension and a Promote as project winner option. Saving the review writes a bounded project record into the session. Promoting a new winner clears the previous winner while keeping its score and notes.

Commands: UI musicforge.variants; engine remix.take_review and remix.take_reviews.

![Take Lab controlled variants and blind review](../images/take-lab.png)

## Vocal Director

Save reusable project profiles containing lead and backing direction, aggression, clarity and the intended number of vocal layers. Applying a profile restores all of those controls and marks the generation as sung vocals. The current profile settings are added to the generation prompt alongside the lead and backing roles.

These controls direct the generator. They do not guarantee singer identity, isolate singers into separate stems or provide pitch editing.

Commands: UI musicforge.vocal_profile_apply; engine vocal.profile_save and vocal.profiles.

![Vocal Director profile settings](../images/vocal-director.png)

## 1. Versioned lyrics

Enable Sung vocals and open Lyric versions and vocal roles. Write section tags such as `[Verse]`, `[Chorus]` and `[Breakdown]`, and label lead and backing parts in the lyric text. Save a uniquely named revision with lead and backing role descriptions. Revisions are immutable and saved in the `.scraft` session; editing the working text does not change a saved revision. Use a new name to save another version. Up to 32 revisions, each up to 3000 characters, are supported.

Load a saved revision, then use the inclusive line range to select a generation excerpt. Loading restores its vocal-role notes. Vocal roles are appended to the music prompt when sung vocals are enabled. Use role tags in the lyrics to describe where each voice enters. The combined prompt and roles must fit within 2000 characters. They do not guarantee a particular singer or voice identity.

Commands: `lyrics.save_version`, `lyrics.versions`, and UI command `musicforge.lyrics_load`.

![Versioned lyrics and vocal roles](../images/lyrics-workspace.png)

## 2. Originals and estimated stems

Every successful music generation archives its original WAV and immutable generation settings under the SoundCraft preferences folder's `Generations` directory. The status line shows its location or any archive failure. This archive is separate from the temporary take list and survives app closure. Insert and save a session for an editable timeline project.

Select a music take, open the Remix tab, and choose Separate selected take into 4 stems. The asynchronous CPU worker uses TorchAudio Hybrid Demucs, producing vocals, drums, bass and other. First use downloads approximately 319 MiB of model weights from PyTorch. Generation and separation share a cancellable job slot; separation runs outside the realtime audio callback. Configure:

- `SOUNDCRAFT_STEM_PYTHON`: Python executable with compatible Torch and TorchAudio.
- `SOUNDCRAFT_STEM_BRIDGE`: absolute path of `integrations/audioforge/stem_bridge.py`.

The existing local music runtime can be used. No Rust dependency was added. The worker accepts mono/stereo PCM16 WAV up to 25 MB and 60 seconds. It processes overlapping chunks on CPU, preserves alignment, writes 44.1 kHz stereo PCM16 WAV, and applies a shared gain only if needed to avoid clipping. Files are labeled estimated; guitar/synth separation and separate female/male vocal stems are not available.

Import the returned paths to create aligned tracks at sample zero. Import is one undoable command and rejects unequal lengths after resampling without retaining a partial import. Imports from the selected generated take retain generation provenance. Save or export the project before closing: worker files are temporary. Mute the original mixed track when auditioning the stems together to avoid doubling the song.

Commands: UI `musicforge.separate`; engine `remix.import_stems` with a `stems` array of `{path,name,estimated}` and optional `generation` provenance.

## 3. Sections and layers

Create a named section with start/end seconds. This uses the existing selection-marker system and saves with the session. Loop a section, then use transport Play Selection to audition it. Use the normal mixer to mute, solo, pan and balance each layer. Group selected tracks to reuse linked editing/mixer behavior; groups persist in the project.

For alternatives, enter a track name and duplicate its active playlist. Edit that alternate, then promote the requested range back into the main playlist using Replace section from alternate playlist. Playlist indices are zero-based. The existing engine command replaces only the given range and supports undo. It does not regenerate audio or align a new performance automatically.

Commands: `remix.section`, `remix.loop_section`, existing `track.group`, `track.playlist_duplicate`, `track.playlist_promote` and mixer commands.

### Section candidates

Enter a marked range, a focused replacement prompt, intensity, transition length and an optional saved vocal profile. Save the request in the project or generate it as a separate take. Candidate duration follows the marked range and is clamped to the provider's 10-to-60-second limit.

The current ACE-Step bridge does not patch an interval inside an existing mixed file. Insert a chosen candidate on an alternate playlist, align it against the original, and use Replace section from alternate playlist to promote only the requested range. This keeps the accepted surrounding timeline intact while making the transition reviewable.

Commands: UI musicforge.section_generate; engine remix.section_candidate and remix.section_candidates.

![Section candidate generation settings](../images/section-candidates.png)

![Remix sections, playlists and export](../images/remix-workspace.png)

## 4. Compare and export

Mark two generated takes as A and B, then switch between Hear A and Hear B. Each audition starts at sample zero, using one preview player; stop session transport before auditioning. Discarding a marked take clears its comparison slot. This is sequential audition, not loudness-matched or sample-synchronized switching.

Export project, mix and stems creates a Remix Pack 2.0 folder containing `project.scraft`, its Audio Files, `mix.wav`, a Stems directory, loop-ready section renders, and `manifest.json`. Exports cover sample zero through the session content end, up to ten minutes. Stems render active unmuted audio, instrument and MIDI tracks through the existing mix path. Stable track IDs prevent duplicate sanitized filenames. A mixed generated song still exports as a mixed track until separated.

The schema 2 manifest retains tempo, key signatures, lyric revisions, take reviews, vocal profiles, section candidates, markers, groups, source generation metadata, track mixer settings and a sample position for each bar. Every selection marker inside the export range becomes an aligned WAV in the `Loops` directory.

When one or more track names contain vocal, voice or singer, the pack also renders `vocals.wav` and `instrumental.wav`. Track naming is an explicit classification rule. If no track matches, those derived files are omitted instead of guessing from the audio.

Existing folders are rejected. A failed export leaves an explicitly reported incomplete folder for inspection; it does not overwrite or delete an earlier export. The open document and its save location are unchanged.

Commands: UI `musicforge.compare_mark`, `musicforge.compare_audition`; engine `remix.export`.

## Verification and limits

Workspace CI and Python bridge tests cover the local provider contracts. Native and engine tests exercise controlled generation state, project workflow records, A/B slots, separation, aligned imports, groups, section loops, playlist promotion, provenance, schema 2 export and reopening. Speaker playback still needs listening verification on each target machine.

The local 40-second Last Word demo has been separated into four aligned stems and exported as a native remix project. Technical signal and alignment checks do not verify separation quality, lyric diction, or male/female voice assignment. Those need listening review. Precise lyric timing, independently generated coordinated instruments, separate singers, and automatic level matching remain future work.

Model and method reference: [TorchAudio Hybrid Demucs tutorial](https://docs.pytorch.org/audio/2.7.0/tutorials/hybrid_demucs_tutorial.html). The model and runtime are downloaded separately; no third-party recordings or model weights are bundled in this repository.
