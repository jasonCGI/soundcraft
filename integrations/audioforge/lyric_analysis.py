"""Dependency-free, estimated lyric structure and flow analysis."""

import argparse
import json
import re
import sys
from collections import Counter

MAX_LYRICS = 3000
SECTION_NAMES = (
    "verse", "pre-chorus", "prechorus", "chorus", "hook", "refrain",
    "bridge", "breakdown", "drop", "intro", "outro", "interlude",
)
WORD_RE = re.compile(r"[A-Za-z]+(?:'[A-Za-z]+)?")
TAG_RE = re.compile(r"^\s*\[([^\]]{1,80})\]\s*$")


def words(text):
    return WORD_RE.findall(text)


def syllables(word):
    """Estimate English syllables without a pronunciation dictionary."""
    value = re.sub(r"[^a-z]", "", word.lower())
    if not value:
        return 0
    exceptions = {
        "every": 2, "different": 2, "business": 2, "camera": 3,
        "family": 3, "fire": 1, "hour": 1, "our": 1, "quiet": 2,
        "rhythm": 2, "people": 2, "beautiful": 3, "chocolate": 3,
    }
    if value in exceptions:
        return exceptions[value]
    groups = len(re.findall(r"[aeiouy]+", value))
    if len(value) > 2 and value.endswith("e") and not value.endswith(("le", "ye")):
        groups -= 1
    if len(value) > 3 and value.endswith("ed") and not value.endswith(("ted", "ded")):
        groups -= 1
    if len(value) > 3 and value.endswith("es") and not value.endswith(("ses", "xes", "zes", "ches", "shes")):
        groups -= 1
    return max(1, groups)


def normalize_line(text):
    return " ".join(word.lower() for word in words(text))


def rhyme_key(word):
    """Return a conservative spelling-based end-rhyme key."""
    value = re.sub(r"[^a-z]", "", word.lower())
    if not value:
        return ""
    value = re.sub(r"ou(?=nd$)", "ow", value)
    value = re.sub(r"ownd$", "own", value)
    matches = list(re.finditer(r"[aeiouy]+", value))
    if not matches:
        return value[-3:]
    start = matches[-1].start()
    if len(matches) > 1 and len(value) - start < 3:
        start = matches[-2].start()
    return value[start:]


def is_section_tag(label):
    lowered = label.strip().lower()
    return any(
        lowered == name or lowered.startswith(name + " ") or lowered.startswith(name + " -")
        for name in SECTION_NAMES
    )


def assign_rhyme_labels(keys):
    labels = {}
    result = []
    for key in keys:
        if not key:
            result.append("-")
            continue
        if key not in labels:
            index = len(labels)
            labels[key] = chr(ord("A") + index) if index < 26 else f"R{index + 1}"
        result.append(labels[key])
    return result


def analyze(text):
    if not isinstance(text, str) or not text.strip() or len(text) > MAX_LYRICS:
        raise ValueError("Enter between 1 and 3000 lyric characters")
    current_section = "Unsectioned"
    parsed = []
    section_order = []
    cues = []
    for source_line, raw in enumerate(text.splitlines(), start=1):
        stripped = raw.strip()
        if not stripped:
            continue
        tag = TAG_RE.match(stripped)
        if tag:
            label = tag.group(1).strip()
            if is_section_tag(label):
                current_section = label
                if label not in section_order:
                    section_order.append(label)
            else:
                cues.append({"source_line": source_line, "cue": label})
            continue
        line_words = words(stripped)
        if not line_words:
            continue
        parsed.append({
            "number": len(parsed) + 1,
            "source_line": source_line,
            "section": current_section,
            "text": stripped,
            "words": len(line_words),
            "syllables": sum(syllables(word) for word in line_words),
            "end_word": line_words[-1].lower(),
            "rhyme_key": rhyme_key(line_words[-1]),
        })
        if current_section not in section_order:
            section_order.append(current_section)
    if not parsed:
        raise ValueError("No lyric lines found")

    rhyme_labels = assign_rhyme_labels([line["rhyme_key"] for line in parsed])
    counts = Counter(normalize_line(line["text"]) for line in parsed)
    syllable_values = sorted(line["syllables"] for line in parsed)
    middle = len(syllable_values) // 2
    median = (
        float(syllable_values[middle])
        if len(syllable_values) % 2
        else sum(syllable_values[middle - 1:middle + 1]) / 2
    )
    for line, label in zip(parsed, rhyme_labels):
        line["rhyme"] = label
        flags = []
        if line["syllables"] >= median + 4:
            flags.append("longer than the song median")
        if line["syllables"] <= max(1, median - 4):
            flags.append("shorter than the song median")
        if counts[normalize_line(line["text"])] > 1:
            flags.append("repeated line")
        line["flags"] = flags

    sections = []
    for name in section_order:
        members = [line for line in parsed if line["section"] == name]
        if members:
            sections.append({
                "name": name,
                "lines": len(members),
                "average_syllables": round(sum(line["syllables"] for line in members) / len(members), 1),
                "rhyme_scheme": "".join(line["rhyme"] for line in members),
            })
    repeated = [
        {"text": line, "count": count}
        for line, count in counts.most_common()
        if count > 1
    ]
    return {
        "schema": 1,
        "method": "estimated-english-spelling-v1",
        "estimated": True,
        "summary": {
            "lyric_lines": len(parsed),
            "sections": len(sections),
            "words": sum(line["words"] for line in parsed),
            "average_syllables": round(sum(line["syllables"] for line in parsed) / len(parsed), 1),
            "median_syllables": median,
            "rhyme_scheme": "".join(rhyme_labels),
            "repeated_lines": len(repeated),
        },
        "sections": sections,
        "lines": parsed,
        "cues": cues,
        "repetitions": repeated,
        "note": "Syllables and rhymes are estimates; names, dialect, contractions, and sung vowels need listening review.",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()
    try:
        payload = json.loads(sys.stdin.read(8192))
        if not isinstance(payload, dict):
            raise ValueError("Expected a JSON object")
        result = analyze(payload.get("lyrics"))
        print(json.dumps(result, indent=2 if args.pretty else None, ensure_ascii=False))
    except (ValueError, TypeError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
