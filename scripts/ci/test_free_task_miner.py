import json
import os
import tempfile
import unittest
from unittest import mock

import free_model_draft as fmd
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

    def test_cross_crate_non_markdown_match_is_refused(self):
        files = {"crates/fission-decompiler/src/README.md": "[p](./pcode.rs)\n",
                 "crates/fission-static/src/analysis/pcode.rs": ""}
        self.assertEqual(mine(files), [])

    def test_same_crate_non_markdown_match_is_accepted(self):
        files = {"crates/c/src/README.md": "[p](./pcode.rs)\n", "crates/c/src/x/pcode.rs": ""}
        self.assertEqual(mine(files)[0].after, "x/pcode.rs")

    def test_markdown_match_in_another_crate_is_accepted(self):
        files = {"crates/c/README.md": "[g](GUIDE.md)\n", "crates/d/GUIDE.md": ""}
        self.assertEqual(mine(files)[0].after, "../d/GUIDE.md")

    def test_non_crate_source_needs_same_top_level_dir_for_non_markdown(self):
        files = {"docs/a.md": "[s](x.py) [t](y.py)\n", "docs/q/x.py": "", "scripts/y.py": ""}
        self.assertEqual([x.after for x in mine(files)], ["q/x.py"])

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
        for x in group:
            self.assertIn(f"<!-- free-task-item:{ftm.item_key(x)} -->", body)
        self.assertNotIn("```", body)

    def test_title_counts(self):
        self.assertIn("(2 in 1 file)", ftm.render_title([f(1), f(2)]))


class DedupTests(unittest.TestCase):
    def issues_for(self, findings):
        return [{"number": 1, "body": ftm.render_body(findings)}, {"number": 2, "body": None}]

    def test_item_key_ignores_the_line_number(self):
        a = ftm.Finding("home-path", "docs/a.md", 3, "b", "c")
        self.assertEqual(ftm.item_key(a), ftm.item_key(a._replace(line=99)))
        self.assertNotEqual(ftm.item_key(a), ftm.item_key(a._replace(file="docs/z.md")))

    def test_shifted_line_numbers_do_not_refile(self):
        old = [f(n) for n in range(3)]
        shifted = [x._replace(line=x.line + 10) for x in old]
        markers = ftm.known_markers(self.issues_for(old))
        self.assertEqual(ftm.plan(shifted, markers, 3), [])

    def test_partly_filed_group_files_only_the_new_findings(self):
        markers = ftm.known_markers(self.issues_for([f(0), f(1)]))
        groups = ftm.plan([f(n) for n in range(4)], markers, 3)
        self.assertEqual(groups, [[f(2), f(3)]])

    def test_cap_applies_after_dedup(self):
        findings = [f(n, file=f"docs/f{n}.md") for n in range(500)]
        self.assertEqual(len(ftm.plan(findings, set(), 3)), 3)

    def test_create_files_labels_and_starts_the_draft_once_per_issue(self):
        root = make_repo({"docs/a.md": "/Users/b/fission-benchmark/x\n"})
        with mock.patch.object(ftm, "gh_existing_issues", return_value=[]), \
                mock.patch.object(ftm, "gh_create", return_value="9") as create, \
                mock.patch.object(ftm, "gh_start_draft") as start:
            ftm.main(["--root", root, "--create", "--no-rust-docs"])
        self.assertEqual(create.call_count, 1)
        start.assert_called_once_with("9")

    def test_existing_marker_means_nothing_is_created(self):
        root = make_repo({"docs/a.md": "/Users/b/fission-benchmark/x\n"})
        group = ftm.group_findings(ftm.mine(root))[0]
        issues = [{"number": 3, "body": ftm.render_body(group)}]
        with mock.patch.object(ftm, "gh_existing_issues", return_value=issues), \
                mock.patch.object(ftm, "gh_create") as create:
            ftm.main(["--root", root, "--create", "--no-rust-docs"])
        create.assert_not_called()

    def test_dry_run_creates_nothing(self):
        root = make_repo({"docs/a.md": "/Users/b/fission-benchmark/x\n"})
        with mock.patch.object(ftm, "gh_existing_issues", return_value=[]), \
                mock.patch.object(ftm, "gh_create") as create:
            ftm.main(["--root", root, "--no-rust-docs"])
        create.assert_not_called()


def cargo_msg(code, text, file="crates/c/src/lib.rs", line=7, src=None, message="missing documentation",
              primary=True):
    return json.dumps({
        "reason": "compiler-message",
        "target": {"src_path": "/work/" + (src or file)},
        "message": {"message": message, "code": {"code": code},
                    "spans": [{"file_name": file, "line_start": line, "is_primary": primary,
                               "text": [{"text": text}]}]}})


class RustDocTests(unittest.TestCase):
    def test_missing_docs_keeps_file_line_and_item_header(self):
        out = cargo_msg("missing_docs", "    pub fn new() -> Self {", line=9)
        self.assertEqual(ftm.parse_cargo_messages(out, "c", "missing-docs"),
                         [ftm.Finding("missing-docs", "crates/c/src/lib.rs", 9, "pub fn new() -> Self {", "")])

    def test_only_the_crates_own_diagnostics_count(self):
        out = "\n".join([
            cargo_msg("missing_docs", "pub fn a()", file="crates/c/src/a.rs"),
            cargo_msg("missing_docs", "pub fn dep()", file="crates/d/src/lib.rs"),  # dependency
            cargo_msg("missing_docs", "pub fn mac()", file="crates/d/src/m.rs", src="crates/c/src/lib.rs"),  # expansion
            cargo_msg("missing_docs", "pub fn other()", file="crates/cc/src/lib.rs"),  # prefix lookalike
            cargo_msg("unused_variables", "let x", file="crates/c/src/a.rs"),
            cargo_msg("missing_docs", "pub fn np()", primary=False),
            "not json", json.dumps({"reason": "build-finished"})])
        found = ftm.parse_cargo_messages(out, "c", "missing-docs")
        self.assertEqual([x.before for x in found], ["pub fn a()"])

    def test_identical_headers_keep_distinct_keys_that_ignore_lines(self):
        out = "\n".join(cargo_msg("missing_docs", "pub fn new() -> Self {", line=n) for n in (5, 50))
        a, b = ftm.parse_cargo_messages(out, "c", "missing-docs")
        self.assertNotEqual(ftm.item_key(a), ftm.item_key(b))
        shifted = "\n".join(cargo_msg("missing_docs", "pub fn new() -> Self {", line=n) for n in (9, 80))
        c, d = ftm.parse_cargo_messages(shifted, "c", "missing-docs")
        self.assertEqual((ftm.item_key(a), ftm.item_key(b)), (ftm.item_key(c), ftm.item_key(d)))

    def test_rustdoc_keeps_only_rustdoc_lints_with_message_and_line(self):
        out = "\n".join([
            cargo_msg("rustdoc::broken_intra_doc_links", "/// see [`Gone`]", message="unresolved link to `Gone`"),
            cargo_msg("rustdoc::bare_urls", "/// http://x.y", line=3, message="this URL is not a hyperlink"),
            cargo_msg("missing_docs", "pub fn a()")])
        found = ftm.parse_cargo_messages(out, "c", "rustdoc")
        self.assertEqual([(x.line, x.before) for x in found],
                         [(3, "this URL is not a hyperlink"), (7, "unresolved link to `Gone`")])
        self.assertEqual({x.kind for x in found}, {"rustdoc"})

    def test_rotation_is_by_day_and_wraps(self):
        n = len(ftm.DOC_CRATES)
        self.assertEqual(ftm.pick_doc_crate(0), ftm.DOC_CRATES[0])
        self.assertEqual(ftm.pick_doc_crate(n + 1), ftm.DOC_CRATES[1])
        self.assertEqual(len({ftm.pick_doc_crate(d) for d in range(1, 367)}), n)

    def test_every_doc_crate_exists_in_the_workspace(self):
        root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        for crate in ftm.DOC_CRATES:
            self.assertTrue(os.path.isfile(os.path.join(root, "crates", crate, "Cargo.toml")), crate)

    def test_groups_hold_at_most_8_files_and_15_items(self):
        many = [ftm.Finding("missing-docs", "crates/c/src/a.rs", n, f"pub fn f{n}()", "") for n in range(40)]
        self.assertEqual([len(g) for g in ftm.group_findings(many)], [15, 15, 10])
        spread = [ftm.Finding("rustdoc", f"crates/c/src/f{i}.rs", 1, "m", "t") for i in range(20)]
        self.assertEqual([len({x.file for x in g}) for g in ftm.group_findings(spread)], [8, 8, 4])

    def test_rust_bodies_carry_the_comment_only_marker_and_markdown_ones_do_not(self):
        rust = [ftm.Finding("missing-docs", "crates/c/src/a.rs", 3, "pub fn f()", "")]
        body = ftm.render_body(rust)
        self.assertIn(ftm.COMMENT_ONLY_MARKER, body)
        self.assertIn("- `crates/c/src/a.rs:3`: `pub fn f()`", body)
        self.assertIn("docs(c): document public items (1 in 1 file)", ftm.render_title(rust))
        self.assertNotIn(ftm.COMMENT_ONLY_MARKER, ftm.render_body([f(1)]))

    def test_source_text_with_backticks_stays_valid_markdown(self):
        item = ftm.Finding("rustdoc", "crates/c/a.rs", 1, "unresolved link", "/// see [`X`]")
        self.assertIn("`` /// see [`X`] ``", ftm.render_body([item]))

    def test_the_marker_is_the_one_the_draft_script_looks_for(self):
        self.assertEqual(ftm.COMMENT_ONLY_MARKER, fmd.COMMENT_ONLY_MARKER)
        body = ftm.render_body([ftm.Finding("rustdoc", "crates/c/a.rs", 1, "m", "t")])
        self.assertTrue(fmd.is_comment_only({"body": body}))

    def test_the_cap_is_shared_and_kinds_take_turns(self):
        docs = [ftm.Finding("missing-docs", f"crates/c/src/f{n}.rs", 1, "pub fn a()", "") for n in range(100)]
        lint = [ftm.Finding("rustdoc", "crates/c/src/z.rs", 1, "m", "t")]
        chosen = ftm.plan(docs + lint + [f(1)], set(), 3)
        self.assertEqual([g[0].kind for g in chosen], ["home-path", "missing-docs", "rustdoc"])
        self.assertEqual(len(ftm.plan(docs, set(), 3)), 3)

    def test_main_runs_the_rust_detectors_for_the_chosen_crate_without_creating(self):
        root = make_repo({"docs/a.md": "ok\n"})
        item = ftm.Finding("missing-docs", "crates/c/src/a.rs", 3, "pub fn f()", "")
        with mock.patch.object(ftm, "gh_existing_issues", return_value=[]), \
                mock.patch.object(ftm, "gh_create") as create, \
                mock.patch.object(ftm, "detect_missing_docs", return_value=[item]) as md, \
                mock.patch.object(ftm, "detect_rustdoc_lints", return_value=[]) as rd:
            ftm.main(["--root", root, "--doc-crate", "c"])
        md.assert_called_once_with(root, "c")
        rd.assert_called_once_with(root, "c")
        create.assert_not_called()

    def test_cargo_commands_are_scoped_to_one_crate_and_get_no_token(self):
        calls = []

        def fake_run(cmd, **kw):
            calls.append((cmd, kw))
            return mock.Mock(stdout="", stderr="", returncode=0)

        with mock.patch.dict(os.environ, {"GH_TOKEN": "t", "GITHUB_TOKEN": "t"}), \
                mock.patch.object(ftm.subprocess, "run", fake_run):
            ftm.detect_missing_docs(".", "c")
            ftm.detect_rustdoc_lints(".", "c")
        self.assertEqual(calls[0][0][:6], ["cargo", "rustc", "--locked", "-p", "c", "--lib"])
        self.assertEqual(calls[0][0][-3:], ["--", "-W", "missing_docs"])
        self.assertEqual(calls[1][0][:5], ["cargo", "doc", "--locked", "--no-deps", "-p"])
        for _cmd, kw in calls:
            self.assertNotIn("GH_TOKEN", kw["env"])
            self.assertNotIn("GITHUB_TOKEN", kw["env"])

    def test_a_failed_cargo_run_yields_nothing_instead_of_raising(self):
        with mock.patch.object(ftm.subprocess, "run", side_effect=OSError("no cargo")):
            self.assertEqual(ftm.detect_missing_docs(".", "c"), [])


if __name__ == "__main__":
    unittest.main()
