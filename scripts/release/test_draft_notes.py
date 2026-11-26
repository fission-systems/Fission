import copy
import json
import re
import unittest
from pathlib import Path

import draft_notes

HERE = Path(__file__).resolve().parent
DATA = json.loads((HERE / "testdata" / "prs.json").read_text(encoding="utf-8"))
GOLDEN = HERE / "testdata" / "v1.2.0.md"


def section(notes: str, heading: str) -> str:
    m = re.search(rf"^## {re.escape(heading)}\n(.*?)(?=^## |\Z)", notes, re.DOTALL | re.MULTILINE)
    return m.group(1) if m else ""


class RenderTests(unittest.TestCase):
    def setUp(self):
        self.notes = draft_notes.render(copy.deepcopy(DATA), "v1.2.0")

    def test_matches_the_golden_file(self):
        self.assertEqual(self.notes, GOLDEN.read_text(encoding="utf-8"))

    def test_same_input_renders_byte_identical_output(self):
        shuffled = copy.deepcopy(DATA)
        shuffled["pull_requests"].reverse()
        shuffled["open_issues"].reverse()
        self.assertEqual(draft_notes.render(shuffled, "v1.2.0"), self.notes)
        self.assertEqual(draft_notes.render(copy.deepcopy(DATA), "1.2.0"), self.notes)

    def test_a_pr_with_no_labels_is_classified_by_its_title_prefix(self):
        self.assertIn("- perf: reuse the worker stack (#101)", section(self.notes, "Performance"))
        self.assertIn("(#110)", section(self.notes, "Internal"))
        self.assertIn("(#107)", section(self.notes, "Testing & Infrastructure"))
        self.assertIn("(#109)", section(self.notes, "Documentation"))
        # `type!:` marks a breaking change.
        self.assertIn("(#99)", section(self.notes, "Breaking Changes"))

    def test_an_unclassifiable_pr_is_listed_under_other_and_named(self):
        other = section(self.notes, "Other")
        self.assertIn("- emulator: answer each guest in its own ABI (#103)", other)
        # A label outside the managed sections does not classify it.
        self.assertIn("- Add a languages command (#108)", other)
        self.assertIn("Needs a label (no managed label and no conventional title prefix): #103, #108.", other)
        # A direct push has no PR and is listed, not dropped.
        self.assertIn("(95e052259, no pull request)", other)

    def test_the_first_matching_label_wins_and_sections_are_ordered(self):
        breaking = section(self.notes, "Breaking Changes")
        self.assertEqual(
            [line for line in breaking.splitlines() if line.startswith("- ")],
            ["- fix(loader)!: refuse truncated headers (#99)", "- cli: rename --json-out to --json (#106)"],
        )
        self.assertNotIn("(#106)", section(self.notes, "New Capabilities"))
        headings = re.findall(r"^## (.+)$", self.notes, re.MULTILINE)
        order = [h for _, h, _ in draft_notes.SECTIONS]
        listed = [h for h in headings if h in order]
        self.assertEqual(listed, sorted(listed, key=order.index))

    def test_bots_and_skip_changelog_are_excluded_but_recorded(self):
        self.assertNotIn("bump serde", self.notes)
        self.assertNotIn("typo in a comment", self.notes)
        self.assertIn("<!-- excluded: #104 (bot author), #105 (skip-changelog) -->", self.notes)

    def test_known_limitations_come_from_confirmed_open_issues(self):
        limits = section(self.notes, "Known limitations")
        self.assertIn("- One jump table lost in an O2 build (#120)", limits)
        self.assertNotIn(draft_notes.TODO_MARKER, limits)
        empty = draft_notes.render(dict(copy.deepcopy(DATA), open_issues=[]), "v1.2.0")
        self.assertIn(draft_notes.TODO_MARKER, section(empty, "Known limitations"))

    def test_the_quality_block_is_left_for_the_maintainer(self):
        quality = section(self.notes, "Quality (full FreshBench sweep)")
        self.assertIn(draft_notes.TODO_MARKER, quality)
        self.assertIn("ADR 0019", quality)


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.notes = draft_notes.render(copy.deepcopy(DATA), "v1.2.0")
        self.evidence = section(self.notes, "Measured evidence")

    def test_numbers_in_an_evidence_section_are_quoted_with_their_pr(self):
        self.assertIn("From #102 (Correctness Evidence):", self.evidence)
        self.assertIn("verify --tier ground-truth on gzip -O0: diverged 14 -> 3", self.evidence)
        self.assertIn("From #102 (Benchmark Impact):", self.evidence)
        self.assertIn("  GED perfect:        13,260 -> 13,465", self.evidence)
        # A table keeps its header with its numbers.
        self.assertIn("| config | before | after |\n| --- | --- | --- |\n| O0 | 1,605 | 1,544 |", self.evidence)

    def test_numbers_outside_an_evidence_section_are_not_quoted(self):
        # Positive control: these numbers are in the PR bodies, just not under
        # an evidence heading (or inside a comment, or a fence that looks like one).
        bodies = "".join(pr["body"] for pr in DATA["pull_requests"])
        for text in ("42%", "7 of 9", "12 times", "99%", "3 numbers 4 5", "5 examples"):
            self.assertIn(text, bodies)
            self.assertNotIn(text, self.notes)
        self.assertNotIn("From #101", self.evidence)

    def test_unfilled_template_placeholders_are_not_quoted(self):
        self.assertNotIn("<before>", self.notes)
        self.assertNotIn("No numbers on this line", self.notes)

    def test_a_section_with_only_placeholders_quotes_nothing(self):
        body = "## Benchmark Impact\n\n```text\nGED perfect: <before> -> <after>\n```\n"
        self.assertEqual(draft_notes.evidence_blocks(body), [])

    def test_a_shell_comment_in_a_fence_is_not_a_heading(self):
        body = "## Validation\n\n```bash\n# Benchmark 50% faster\n```\n"
        self.assertEqual(draft_notes.evidence_blocks(body), [])

    def test_a_top_level_measurement_section_is_evidence(self):
        # Speed PRs put their numbers under `## Measurement (...)` / `## Measured`.
        body = (
            "Preamble: 128.9 -> 79.3 s\n\n"
            "## Measurement (fixed subset: 81 binaries; interleaved runs)\n\n"
            "| step | CPU |\n| --- | --- |\n| total | 1217.1 -> 1077.3 s (-11.5%) |\n\n"
            "## Measured\n\nwall 653.2 -> 644.2 s\n\n"
            "## Validation\n\n### Measured locally\n\n12 tests, 3 runs\n\n"
            "## Tests\n\n17 new tests\n"
        )
        self.assertEqual(
            draft_notes.evidence_blocks(body),
            [
                (
                    "Measurement (fixed subset: 81 binaries; interleaved runs)",
                    ["| step | CPU |", "| --- | --- |", "| total | 1217.1 -> 1077.3 s (-11.5%) |"],
                ),
                ("Measured", ["wall 653.2 -> 644.2 s"]),
            ],
        )
        # Positive control: the numbers outside those two sections (the
        # preamble, a `### Measured` subsection of another section, Tests)
        # are in the body and are not quoted.
        quoted = "\n".join(line for _, lines in draft_notes.evidence_blocks(body) for line in lines)
        for text in ("128.9 -> 79.3", "12 tests, 3 runs", "17 new tests"):
            self.assertIn(text, body)
            self.assertNotIn(text, quoted)

    def test_a_measurement_section_is_quoted_in_the_notes(self):
        data = copy.deepcopy(DATA)
        data["pull_requests"].append(
            {
                "author": "m",
                "body": "## Measurement (81 binaries)\n\nCPU 695 -> 425 s (-39%)\n",
                "labels": [],
                "number": 112,
                "title": "perf: one fewer pass",
            }
        )
        evidence = section(draft_notes.render(data, "v1.2.0"), "Measured evidence")
        self.assertIn("From #112 (Measurement (81 binaries)):", evidence)
        self.assertIn("CPU 695 -> 425 s (-39%)", evidence)

    def test_a_release_pr_is_not_quoted_again(self):
        # A Release PR's body carries that release's notes, whose own
        # "## Measured evidence" section would otherwise be re-quoted.
        notes = draft_notes.render(copy.deepcopy(DATA), "v1.2.0")
        body = draft_notes.pr_body(copy.deepcopy(DATA), "v1.2.0", notes)
        self.assertTrue(draft_notes.evidence_blocks(body))
        later = dict(copy.deepcopy(DATA), previous_tag="v1.2.0", open_issues=[])
        later["pull_requests"] = [
            {"author": "m", "body": body, "labels": [], "number": 130, "title": "release: v1.2.0"}
        ]
        again = draft_notes.render(later, "v1.3.0")
        self.assertNotIn("Measured evidence", again)
        self.assertNotIn("(#130)", again.split("<!-- excluded:")[0])
        self.assertIn("<!-- excluded: #130 (release PR) -->", again)

    def test_an_evidence_section_ends_at_the_next_heading_of_its_level(self):
        body = "## Benchmark Impact\n\nGED 1 -> 2\n\n### Detail\n\nrow 3\n\n## Risk\n\nrow 4\n"
        self.assertEqual(
            draft_notes.evidence_blocks(body), [("Benchmark Impact", ["GED 1 -> 2", "row 3"])]
        )


class RerunTests(unittest.TestCase):
    """prepare-release.yml regenerates the Release PR in place."""

    def test_pr_body_generation_is_byte_identical_on_the_same_input(self):
        notes1 = draft_notes.render(copy.deepcopy(DATA), "v1.2.0")
        body1 = draft_notes.pr_body(copy.deepcopy(DATA), "v1.2.0", notes1)
        notes2 = draft_notes.render(copy.deepcopy(DATA), "v1.2.0")
        body2 = draft_notes.pr_body(copy.deepcopy(DATA), "v1.2.0", notes2)
        self.assertEqual(body1, body2)
        self.assertIn(f"Candidate `{DATA['target_sha']}`", body1)
        self.assertIn("<!-- release-pr: v1.2.0 -->", body1)
        self.assertTrue(body1.endswith(notes1))

    def test_rendering_over_its_own_output_is_a_fixed_point(self):
        notes = draft_notes.render(copy.deepcopy(DATA), "v1.2.0")
        self.assertEqual(draft_notes.render(copy.deepcopy(DATA), "v1.2.0", notes), notes)

    def test_a_rerun_keeps_what_the_maintainer_wrote(self):
        notes = draft_notes.render(copy.deepcopy(DATA), "v1.2.0")
        filled = re.sub(
            r"(<!-- maintainer:quality -->\n).*?(<!-- /maintainer:quality -->)",
            r"\1| GED exact | 31,982 | 32,560 |\n\2",
            notes,
            flags=re.DOTALL,
        )
        newer = copy.deepcopy(DATA)
        newer["pull_requests"].append(
            {"author": "m", "body": "", "labels": [], "number": 111, "title": "fix: one more"}
        )
        again = draft_notes.render(newer, "v1.2.0", filled)
        self.assertIn("| GED exact | 31,982 | 32,560 |", section(again, "Quality (full FreshBench sweep)"))
        self.assertNotIn(draft_notes.TODO_MARKER, section(again, "Quality (full FreshBench sweep)"))
        self.assertIn("- fix: one more (#111)", section(again, "Correctness Fixes"))

    def test_candidate_record_names_the_pinned_sha(self):
        record = json.loads(draft_notes.candidate_record(DATA, "1.2.0"))
        self.assertEqual(
            record,
            {
                "candidate_sha": DATA["target_sha"],
                "notes": "docs/changelog/v1.2.0.md",
                "previous_tag": "v1.1.0",
                "schema": 1,
                "tag": "v1.2.0",
                "version": "1.2.0",
            },
        )


class ReleaseYmlTests(unittest.TestCase):
    def test_github_categories_match_the_drafter(self):
        text = (HERE.parents[1] / ".github" / "release.yml").read_text(encoding="utf-8")
        categories = []
        for line in text.split("\n"):
            title = re.match(r"^    - title: (.+)$", line)
            label = re.match(r"^        - (.+)$", line)
            if title:
                categories.append((title.group(1).strip(), []))
            elif label and categories:
                categories[-1][1].append(label.group(1).strip().strip('"'))
        categories = [(t, tuple(labels)) for t, labels in categories]
        expected = [(h, labels or ("*",)) for _, h, labels in draft_notes.SECTIONS]
        self.assertEqual(categories, expected)
        excluded = re.search(r"exclude:\n    labels:\n((?:      - .+\n)+)", text).group(1)
        self.assertEqual(set(re.findall(r"- (.+)", excluded)), set(draft_notes.SKIP_LABELS))


if __name__ == "__main__":
    unittest.main()
