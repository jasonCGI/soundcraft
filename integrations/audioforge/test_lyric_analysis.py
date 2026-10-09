import unittest

from lyric_analysis import analyze, rhyme_key, syllables


class LyricAnalysisTests(unittest.TestCase):
    def test_sections_cues_repetition_and_rhyme(self):
        report = analyze("""[Verse 1]
Key beside the door
Falling to the floor

[Chorus]
Blackout bloom, the lights go down
[Male backing shout]
Turn it up
Blackout bloom, the lights go down""")
        self.assertEqual(report["summary"]["lyric_lines"], 5)
        self.assertEqual(report["summary"]["sections"], 2)
        self.assertEqual(report["summary"]["repeated_lines"], 1)
        self.assertEqual(report["lines"][0]["rhyme"], report["lines"][1]["rhyme"])
        self.assertEqual(report["cues"][0]["cue"], "Male backing shout")

    def test_estimates_are_bounded_and_common_words_are_stable(self):
        self.assertEqual(syllables("rhythm"), 2)
        self.assertEqual(syllables("beautiful"), 3)
        self.assertEqual(rhyme_key("sound"), rhyme_key("down"))
        self.assertGreater(analyze("One original line")["summary"]["average_syllables"], 0)

    def test_rejects_empty_and_oversized_input(self):
        with self.assertRaises(ValueError):
            analyze("   ")
        with self.assertRaises(ValueError):
            analyze("x" * 3001)


if __name__ == "__main__":
    unittest.main()
