# Song package

Use this structure for a complete song or generator-ready package. Omit fields that would be invented without value.

```yaml
title: ""
promise: "The one thing the listener should remember"
speaker: ""
listener: ""
moment: ""
central_image: ""
hook: ""
tempo_bpm: 120
tonal_center: "optional"
genre: ""
emotional_arc:
  start: ""
  turn: ""
  finish: ""
vocal_roles:
  lead: ""
  backing: ""
section_map:
  - section: "Verse 1"
    job: ""
  - section: "Chorus"
    job: ""
production_prompt: ""
```

Follow the spec with section-tagged lyrics. The spec directs the draft; it is not part of the sung text.

## Section guidance

- **Verse:** new evidence, scene, action, or consequence. Avoid repeating the chorus in longer sentences.
- **Pre-chorus:** narrow the language and raise expectation. It should make the chorus feel inevitable.
- **Chorus:** deliver the title or hook early, then reveal why it matters.
- **Bridge or breakdown:** change perspective, time, cost, or intensity. Do not add one only to satisfy a template.
- **Outro:** resolve, destabilize, or leave one deliberate image. Avoid a generic summary.

## Vocal roles

Describe register, intensity, articulation, and the relationship between parts. Keep identities fictional and generator-agnostic. For example: `close, low female lead that opens into a full belt` and `brief lower male responses on the final phrase`.

Use bracketed role cues only where the singer changes. Excess cues compete with the lyric and may be sung aloud by some generators.
