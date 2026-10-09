---
name: cardona-songwriting
description: Write, revise, or audit original song lyrics and generator-ready song briefs for SoundCraft. Use for lyrics, hooks, sections, vocal roles, song concepts, alternate lines, and AI music prompts; keep mixing and audio engineering work in the normal SoundCraft workflow.
---

# SoundCraft Songwriting

Turn a musical idea into an original, singable song package that can be saved as a SoundCraft lyric revision and sent to a local generator.

## Choose the working mode

- **New song:** establish the song promise, speaker, listener, moment, recurring concrete image, hook, emotional turn, and section map before drafting full lyrics.
- **Revision:** preserve the user's intended meaning and strongest lines, then address the highest-impact weakness first. Do not rewrite everything unless requested.
- **Audit:** identify specific evidence in the lyric, rank the three most consequential issues, and propose targeted repairs.
- **Generator package:** separate lyrics, vocal roles, tempo, structure, and production direction so the model does not have to infer them from one dense prompt.

Read [references/song-spec.md](references/song-spec.md) when creating a complete song package. Read [references/revision.md](references/revision.md) when revising or auditing existing lyrics. Read [references/sources.md](references/sources.md) only when provenance or the open-source influences matter.

## Core decisions

1. Write one sentence stating what the song promises the listener.
2. Identify who is speaking, to whom, and at what exact moment.
3. Choose a title or hook phrase that can survive repetition.
4. Give each section a job. Verses reveal; pre-choruses increase pressure; choruses deliver the promise; bridges reframe or raise the cost.
5. Use at least one concrete object, place, action, or sensory detail that changes meaning across the song.
6. Treat rhyme and meter as support for emotional movement. Avoid forcing syntax solely to complete a rhyme.
7. Keep the chorus easier to remember than the verses.

## Originality and references

When the user names an artist or song, extract high-level traits such as tempo range, line length, rhyme density, vocal contrast, arrangement arc, and production texture. Write a fresh subject, hook, imagery, and lyric structure. Do not reproduce or closely paraphrase distinctive lyrics, hooks, or melodies.

Use the user's own lyrics as editable source material. Preserve requested lines or concepts and clearly label substantial rewrites.

## SoundCraft output

For a complete vocal song, return the title and promise; tempo and tonal center when useful; genre and emotional arc; lead and backing roles; a concise production prompt with no artist names; section-tagged lyrics; and a short quality check.

SoundCraft accepts up to 3000 lyric characters and a combined production prompt plus vocal-role notes of up to 2000 characters. Prefer a strong short song over filling every possible section.

When working in this repository, use `integrations/audioforge/lyric_analysis.py` for a deterministic first pass. Treat its syllable and rhyme results as estimates, especially for names, dialect, contractions, and sung vowel changes. Use the report to direct judgment, not replace it.

## Final pass

Before presenting lyrics, read them aloud at the intended tempo. Remove explanations already conveyed by an image or action. Replace the most predictable rhyme if it weakens the voice. Check that repetitions earn their place and that lead and backing cues are performable. Keep generation directions out of lines meant to be sung.
