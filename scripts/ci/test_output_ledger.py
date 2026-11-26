import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import output_diff
import output_ledger


def row(address, sha, project="p"):
    return {"arch": "x86_64", "compiler": "gcc", "config": "O2", "project": project, "binary": "b",
            "address": address, "code_sha256": sha}


def entry(commit, outputs, cpu="cpu", instructions=100, scorer="s", **summary):
    base = {"rows": 2, "rows_with_output": 2, "failed_calls": 0, "perfect": 1, "distance": 5.0, "types_perfect": 0}
    return {"commit": commit, "cpu": cpu, "scorer": scorer, "corpus_id": "c", "outputs": outputs,
            "summary": {**base, **summary}, "speed": {"instructions": instructions}}


class EntryTests(unittest.TestCase):
    def test_an_entry_keys_each_output_by_function(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)
            (run / "meta.json").write_text(json.dumps(
                {"git": {"commit": "abc", "dirty": False}, "scorer": "s", "corpus_id": "c"}))
            (run / "rows.jsonl").write_text("".join(json.dumps(r) + "\n" for r in
                                                    (row(0x10, "aa" * 32), row(0x20, None))))
            report = {"overall": {"rows": 2, "perfect": 1, "distance": 3.0, "types_perfect": 0},
                      "timing": {"failed_calls": 0}, "failures": {"no_output": 1}}
            e = output_ledger.entry_from_run(run, report, None)
        self.assertEqual(e["commit"], "abc")
        self.assertEqual(e["outputs"], {"x86_64/gcc-O2/p/b@10": "a" * 16, "x86_64/gcc-O2/p/b@20": ""})
        self.assertEqual(e["summary"]["rows_with_output"], 1)


class CompareTests(unittest.TestCase):
    def test_compare_names_changed_functions_and_deltas(self):
        old = entry("a", {"f": "1", "g": "2", "gone": "3"})
        new = entry("b", {"f": "1", "g": "9", "new": "4"}, perfect=2, instructions=110)
        result = output_ledger.compare(old, new)
        self.assertEqual(result["changed"], ["g"])
        self.assertEqual(result["appeared"], ["new"])
        self.assertEqual(result["disappeared"], ["gone"])
        self.assertEqual(result["summary_delta"]["perfect"], 1)
        self.assertEqual(result["speed"], {"old": 100, "new": 110, "same_cpu": True})
        self.assertTrue(result["comparable"])

    def test_counts_from_different_cpus_are_marked(self):
        result = output_ledger.compare(entry("a", {}, cpu="x"), entry("b", {}, cpu="y"))
        self.assertFalse(result["speed"]["same_cpu"])

    def test_a_different_scorer_is_not_comparable(self):
        self.assertFalse(output_ledger.compare(entry("a", {}), entry("b", {}, scorer="t"))["comparable"])


class HistoryTests(unittest.TestCase):
    def test_history_names_each_commit_that_changed_the_output(self):
        entries = [entry("c1", {"f": "1"}), entry("c2", {"f": "1"}), entry("c3", {}),
                   entry("c4", {"f": "2"}), entry("c5", {"f": "2"}), entry("c6", {"f": "1"})]
        self.assertEqual(output_ledger.first_changes(entries, "f"),
                         [{"commit": "c4", "from": "1", "to": "2"}, {"commit": "c6", "from": "2", "to": "1"}])

    def test_an_unchanged_output_has_no_history(self):
        self.assertEqual(output_ledger.first_changes([entry("c1", {"f": "1"}), entry("c2", {"f": "1"})], "f"), [])


class ReuseTests(unittest.TestCase):
    def run_dir(self, root, name, commit="base", dirty=False, scorer="s", functions=3):
        path = Path(root) / name
        path.mkdir()
        (path / "meta.json").write_text(json.dumps({"git": {"commit": commit, "dirty": dirty}, "scorer": scorer,
                                                    "corpus_id": "c", "functions": functions}))
        return path

    def test_the_ledger_run_stands_in_only_for_the_same_commit_scorer_and_functions(self):
        with tempfile.TemporaryDirectory() as root:
            head = self.run_dir(root, "head", commit="head")
            self.assertTrue(output_diff.reusable(self.run_dir(root, "ok"), head, "base"))
            self.assertFalse(output_diff.reusable(self.run_dir(root, "other"), head, "elsewhere"))
            self.assertFalse(output_diff.reusable(self.run_dir(root, "dirty", dirty=True), head, "base"))
            self.assertFalse(output_diff.reusable(self.run_dir(root, "scorer", scorer="t"), head, "base"))
            self.assertFalse(output_diff.reusable(self.run_dir(root, "fewer", functions=2), head, "base"))
            self.assertFalse(output_diff.reusable(None, head, "base"))
            self.assertFalse(output_diff.reusable(Path(root) / "missing", head, "base"))


class LedgerCommitTests(unittest.TestCase):
    def git(self, repo, *args):
        return subprocess.run(["git", "-C", str(repo), "-c", "user.name=t", "-c", "user.email=t@t", *args],
                              check=True, capture_output=True, text=True).stdout.strip()

    def commit(self, repo, path, text):
        (repo / path).parent.mkdir(parents=True, exist_ok=True)
        (repo / path).write_text(text)
        self.git(repo, "add", "-A")
        self.git(repo, "commit", "-q", "-m", path)
        return self.git(repo, "rev-parse", "HEAD")

    def test_the_ledger_commit_is_the_last_main_commit_that_changed_a_decompiler_input(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            self.git(repo, "init", "-q", "-b", "main")
            self.commit(repo, "crates/a.rs", "1")
            self.git(repo, "switch", "-q", "-c", "feature")
            self.commit(repo, "crates/a.rs", "2")
            self.git(repo, "switch", "-q", "main")
            self.git(repo, "merge", "-q", "--no-ff", "-m", "merge", "feature")
            merge = self.git(repo, "rev-parse", "HEAD")
            self.commit(repo, "docs/x.md", "docs only")
            # The merge, not the branch commit: main records the merge's push.
            self.assertEqual(output_diff.ledger_commit(repo), merge)
            changed = self.commit(repo, "Cargo.lock", "lock")
            self.assertEqual(output_diff.ledger_commit(repo), changed)


class WorkflowTests(unittest.TestCase):
    def test_the_ledger_records_every_push_that_changes_a_decompiler_input(self):
        text = (Path(__file__).resolve().parents[2] / ".github/workflows/output-ledger.yml").read_text()
        for path in output_diff.DECOMPILER_INPUTS:
            pattern = f'"{path}/**"' if not Path(path).suffix else f'"{path}"'
            self.assertIn(f"- {pattern}", text, path)


if __name__ == "__main__":
    unittest.main()
