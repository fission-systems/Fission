import gzip
import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import freshbench_corpus as corpus


def entry(path, kind, project="p", arch="x86_64", data=b"x"):
    return {"path": path, "kind": kind, "project": project, "arch": arch,
            "size": len(data), "sha256": hashlib.sha256(data).hexdigest()}


class FreshbenchCorpusTests(unittest.TestCase):
    def test_scoring_files_are_the_units_binaries_targets_and_arch_sources(self):
        entries = [
            entry("stripped/gcc-O2/p/a", "stripped"),
            entry("binaries/gcc-O2/p/a", "binary"),
            entry("truth/gcc-O2/p/targets.json", "ground_truth"),
            entry("sources/x86_64/p/a.c", "source"),
            entry("sources/i686/p/a.c", "source", arch="i686"),
            entry("stripped/gcc-O0/p/a", "stripped"),
            entry("stripped/gcc-O2/q/a", "stripped", project="q"),
            entry("projects/p/LICENSE", "license"),
        ]
        chosen = {e["path"] for e in corpus.scoring_files(entries, {("gcc-O2", "p")})}
        self.assertEqual(chosen, {"stripped/gcc-O2/p/a", "binaries/gcc-O2/p/a",
                                  "truth/gcc-O2/p/targets.json", "sources/x86_64/p/a.c"})

    def test_a_manifest_with_another_hash_is_refused(self):
        with tempfile.TemporaryDirectory() as root:
            Path(root, "manifest.jsonl.gz").write_bytes(gzip.compress(b'{"path": "x"}\n'))
            with self.assertRaises(SystemExit):
                corpus.manifest(Path(root))
            self.assertFalse(Path(root, "manifest.jsonl.gz").exists(), "a bad cached manifest is dropped")

    def test_a_downloaded_file_that_differs_from_the_manifest_is_refused(self):
        wanted = entry("stripped/gcc-O2/p/a", "stripped", data=b"expected")

        def wrong(url, target):
            Path(target).parent.mkdir(parents=True, exist_ok=True)
            Path(target).write_bytes(b"tampered")

        with tempfile.TemporaryDirectory() as root, patch.object(corpus, "_download", wrong):
            with self.assertRaises(SystemExit):
                corpus.fetch(wanted, Path(root))

    def test_a_file_already_present_and_matching_is_not_downloaded_again(self):
        wanted = entry("stripped/gcc-O2/p/a", "stripped", data=b"ok")
        with tempfile.TemporaryDirectory() as root:
            target = Path(root, "m1", wanted["path"])
            target.parent.mkdir(parents=True)
            target.write_bytes(b"ok")
            with patch.object(corpus, "_download", side_effect=AssertionError("downloaded")):
                self.assertEqual(corpus.fetch(wanted, Path(root)), target)

    def test_measurement_env_has_no_error_reporting(self):
        with patch.dict("os.environ", {"SENTRY_DSN": "x", "FISSION_SENTRY": "1", "PATH": "/bin"}):
            env = corpus.telemetry_free_env()
        self.assertNotIn("SENTRY_DSN", env)
        self.assertNotIn("FISSION_SENTRY", env)
        self.assertEqual(env["PATH"], "/bin")


if __name__ == "__main__":
    unittest.main()
