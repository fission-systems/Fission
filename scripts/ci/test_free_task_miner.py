import os
import tempfile
import unittest
from unittest import mock

import free_task_miner as ftm


def make_repo(files):
    root = tempfile.mkdtemp()
    for path, text in files.items():
        full = os.path.join(root, path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w", encoding="utf-8") as fh:
            fh.write(text)
    return root


def mine(files):
    return ftm.mine(make_repo(files))


def f(n, kind="home-path", file="docs/a.md"):
    return ftm.Finding(kind, file, n, f"/Users/x/fission-benchmark/{n}", f"~/fission-benchmark/{n}")


class HomePathTests(unittest.TestCase):
    def test_benchmark_path_becomes_tilde(self):
        found = mine({"docs/a.md": "ok\nRun /Users/bob/fission-benchmark/corpus/scale now.\n"})
        self.assertEqual(found, [ftm.Finding("home-path", "docs/a.md", 2,
                                             "/Users/bob/fission-benchmark/corpus/scale",
                                             "~/fission-benchmark/corpus/scale")])

    def test_linux_home_and_trailing_punctuation(self):
        found = mine({"README.md": "see /home/al/fission-benchmark/x.\n"})
        self.assertEqual((found[0].before, found[0].after),
                         ("/home/al/fission-benchmark/x", "~/fission-benchmark/x"))

    def test_checkout_path_becomes_repo_relative_only_if_it_exists(self):
        found = mine({"docs/a.md": "/Users/bob/Fission/crates/c/x.rs and /Users/bob/Fission/nope.rs\n",
                      "crates/c/x.rs": ""})
        self.assertEqual([(x.before, x.after) for x in found],
                         [("/Users/bob/Fission/crates/c/x.rs", "crates/c/x.rs")])

    def test_checkout_path_in_a_link_is_relative_to_the_file(self):
        found = mine({"docs/a.md": "[x](/Users/bob/Fission/crates/c/x.rs)\n", "crates/c/x.rs": ""})
        self.assertEqual(found[0].after, "../crates/c/x.rs")

    def test_other_home_paths_are_left_alone(self):
        self.assertEqual(mine({"docs/a.md": "/Users/bob/Downloads/foo\n"}), [])

    def test_included_in_proposals_and_plans(self):
        text = "/Users/b/fission-benchmark/x\n"
        found = mine({"docs/proposals/p.md": text, "docs/plans/q.md": text})
        self.assertEqual(len(found), 2)


class ExclusionTests(unittest.TestCase):
    def test_excluded_and_out_of_scope_files_yield_nothing(self):
        text = "/Users/b/fission-benchmark/x\n"
        files = {p: text for p in (
            "docs/reports/r.md", "docs/validation/v.md", "docs/audits/a.md",
            "docs/changelog/c.md", "vendor/ghidra/README.md", "benchmark/b.md",
            "target-x/t.md", ".claude/worktrees/w/AGENTS.md", "scripts/s.md", "docs/notes.txt")}
        self.assertEqual(mine(files), [])

    def test_links_in_proposals_and_plans_are_not_fixed(self):
        link = "[x](missing.md)\n"
        files = {"docs/proposals/p.md": link, "docs/plans/q.md": link, "docs/sub/missing.md": ""}
        self.assertEqual(mine(files), [])


class LinkTests(unittest.TestCase):
    def test_unique_basename_is_fixed_with_anchor_kept(self):
        found = mine({"docs/a.md": "line\n[t](old/b.md#sec)\n", "docs/deep/b.md": ""})
        self.assertEqual(found, [ftm.Finding("broken-link", "docs/a.md", 2, "old/b.md#sec", "deep/b.md#sec")])

    def test_valid_links_and_non_file_targets_are_ignored(self):
        files = {"docs/a.md": "[a](b.md) [h](http://x/y.md) [m](mailto:a@b) [#](#top) [i](c.md)\n",
                 "docs/b.md": ""}
        self.assertEqual(mine(files), [])

    def test_ambiguous_and_missing_targets_are_skipped(self):
        files = {"docs/a.md": "[a](x/b.md) [c](gone.md)\n", "docs/p/b.md": "", "docs/q/b.md": ""}
        self.assertEqual(mine(files), [])

    def test_links_in_code_fences_and_vendor_targets_are_skipped(self):
        files = {"docs/a.md": "```\n[a](x/b.md)\n```\n[v](v.md)\n",
                 "docs/deep/b.md": "", "vendor/v.md": ""}
        self.assertEqual(mine(files), [])

    def test_root_file_links_into_docs(self):
        found = mine({"README.md": "[g](guide.md)\n", "docs/guide.md": ""})
        self.assertEqual(found[0].after, "docs/guide.md")


class GroupingTests(unittest.TestCase):
    def test_limits_of_forty_replacements_and_twelve_files(self):
        many = [f(n) for n in range(100)]
        groups = ftm.group_findings(many)
        self.assertEqual([len(g) for g in groups], [40, 40, 20])
        spread = [f(1, file=f"docs/f{i:02}.md") for i in range(30)]
        self.assertEqual([len({x.file for x in g}) for g in ftm.group_findings(spread)], [12, 12, 6])

    def test_kinds_are_never_mixed(self):
        groups = ftm.group_findings([f(1), f(2, kind="broken-link")])
        self.assertEqual([{x.kind for x in g} for g in groups], [{"broken-link"}, {"home-path"}])


class FingerprintAndBodyTests(unittest.TestCase):
    def test_fingerprint_is_stable_order_independent_and_16_hex(self):
        a, b = [f(1), f(2)], [f(2), f(1)]
        self.assertEqual(ftm.fingerprint(a), ftm.fingerprint(b))
        self.assertRegex(ftm.fingerprint(a), r"^[0-9a-f]{16}$")
        self.assertNotEqual(ftm.fingerprint(a), ftm.fingerprint([f(1)]))
        self.assertEqual(ftm.fingerprint([f(1)]), ftm.fingerprint([f(1)]))

    def test_body_lists_every_replacement_with_marker_and_no_code_fence(self):
        group = [f(n) for n in range(40)]
        body = ftm.render_body(group)
        self.assertEqual(body.count("- `docs/a.md:"), 40)
        self.assertIn("- `docs/a.md:3`: `/Users/x/fission-benchmark/3` → `~/fission-benchmark/3`", body)
        self.assertIn(f"<!-- free-task:{ftm.fingerprint(group)} -->", body)
        self.assertNotIn("```", body)

    def test_title_counts(self):
        self.assertIn("(2 in 1 file)", ftm.render_title([f(1), f(2)]))


class DedupTests(unittest.TestCase):
    def test_groups_with_a_known_marker_are_skipped_and_cap_applies(self):
        groups = [[f(n)] for n in range(5)]
        issues = [{"number": 1, "body": f"x\n<!-- free-task:{ftm.fingerprint(groups[0])} -->"},
                  {"number": 2, "body": None}]
        markers = ftm.known_markers(issues)
        self.assertEqual(ftm.select_groups(groups, markers, 3), groups[1:4])

    def test_create_files_labels_and_starts_the_draft_once_per_issue(self):
        root = make_repo({"docs/a.md": "/Users/b/fission-benchmark/x\n"})
        with mock.patch.object(ftm, "gh_existing_issues", return_value=[]), \
                mock.patch.object(ftm, "gh_create", return_value="9") as create, \
                mock.patch.object(ftm, "gh_start_draft") as start:
            ftm.main(["--root", root, "--create"])
        self.assertEqual(create.call_count, 1)
        start.assert_called_once_with("9")

    def test_existing_marker_means_nothing_is_created(self):
        root = make_repo({"docs/a.md": "/Users/b/fission-benchmark/x\n"})
        group = ftm.group_findings(ftm.mine(root))[0]
        issues = [{"number": 3, "body": ftm.render_body(group)}]
        with mock.patch.object(ftm, "gh_existing_issues", return_value=issues), \
                mock.patch.object(ftm, "gh_create") as create:
            ftm.main(["--root", root, "--create"])
        create.assert_not_called()

    def test_dry_run_creates_nothing(self):
        root = make_repo({"docs/a.md": "/Users/b/fission-benchmark/x\n"})
        with mock.patch.object(ftm, "gh_existing_issues", return_value=[]), \
                mock.patch.object(ftm, "gh_create") as create:
            ftm.main(["--root", root])
        create.assert_not_called()


if __name__ == "__main__":
    unittest.main()
