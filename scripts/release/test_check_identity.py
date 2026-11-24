import io
import json
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

import check_identity
import resource_manifest

TAG, SHA, VERSION = "v0.5.0", "a" * 40, "0.5.0"


def status(build_tag=TAG, build_sha=SHA, state="match", m_tag=TAG, m_sha=SHA, version=VERSION):
    return {
        "build": {"version": version, "tag": build_tag, "sha": build_sha},
        "resource_manifest": {
            "path": "/x/share/fission/fission-resources.json",
            "state": state,
            "detail": "bundle and binary come from the same release commit",
            "version": version,
            "tag": m_tag,
            "sha": m_sha,
        },
    }


def run(argv, stdin=None):
    err = io.StringIO()
    with patch("sys.stdin", io.StringIO(stdin or "")), redirect_stderr(err), redirect_stdout(io.StringIO()):
        code = check_identity.main(argv)
    return code, err.getvalue()


class Repo:
    def __init__(self, version):
        self.dir = tempfile.TemporaryDirectory()
        root = Path(self.dir.name)
        (root / "crates" / "fission-cli").mkdir(parents=True)
        (root / "crates" / "fission-cli" / "Cargo.toml").write_text(
            f'[package]\nname = "fission-cli"\nversion = "{version}"\n\n[dependencies]\nx = {{ version = "1.0" }}\n'
        )
        for d in resource_manifest.RESOURCE_DIRS:
            (root / "bundle" / d).mkdir(parents=True)
            (root / "bundle" / d / "a.txt").write_text(d)
        self.root = root


class SourceTests(unittest.TestCase):
    def test_the_tag_must_be_the_cli_version(self):
        repo = Repo("0.5.0")
        self.assertEqual(run(["source", "--tag", "v0.5.0", "--root", str(repo.root)])[0], 0)
        # Positive control: the same check on a tag Cargo.toml does not carry.
        code, err = run(["source", "--tag", "v0.5.1", "--root", str(repo.root)])
        self.assertEqual(code, 1)
        self.assertIn("v0.5.1 is not crates/fission-cli/Cargo.toml version 0.5.0", err)

    def test_a_prerelease_tag_needs_the_prerelease_version(self):
        final, rc = Repo("0.5.0"), Repo("0.5.0-rc.1")
        self.assertEqual(run(["source", "--tag", "v0.5.0-rc.1", "--root", str(final.root)])[0], 1)
        self.assertEqual(run(["source", "--tag", "v0.5.0-rc.1", "--root", str(rc.root)])[0], 0)


class StatusTests(unittest.TestCase):
    args = ["status", "--tag", TAG, "--sha", SHA, "--version", VERSION]

    def test_an_agreeing_release_passes(self):
        self.assertEqual(run(self.args, json.dumps(status())), (0, ""))

    def test_every_disagreement_fails(self):
        cases = {
            "binary tag": status(build_tag="v0.4.0"),
            "binary commit": status(build_sha="b" * 40),
            "an unreleased binary": status(build_tag=None, build_sha=None),
            "resource manifest state": status(state="mismatch"),
            "resource manifest commit": status(m_sha="c" * 40),
            "resource manifest tag": status(m_tag="v0.4.9"),
            "binary version": status(version="0.4.0"),
        }
        for name, payload in cases.items():
            with self.subTest(name):
                code, err = run(self.args, json.dumps(payload))
                self.assertEqual(code, 1, name)
                self.assertTrue(err, name)

    def test_a_missing_manifest_fails(self):
        payload = status(state="missing", m_tag=None, m_sha=None)
        payload["resource_manifest"]["detail"] = "/x has no fission-resources.json"
        code, err = run(self.args, json.dumps(payload))
        self.assertEqual(code, 1)
        self.assertIn("has no fission-resources.json", err)


class ManifestTests(unittest.TestCase):
    def test_a_written_manifest_checks_against_its_release(self):
        repo = Repo(VERSION)
        bundle = repo.root / "bundle"
        with redirect_stdout(io.StringIO()):
            resource_manifest.main([str(bundle), "--tag", TAG, "--sha", SHA, "--root", str(repo.root)])
        path = bundle / resource_manifest.MANIFEST
        manifest = json.loads(path.read_text())
        self.assertEqual(manifest["fission_version"], VERSION)
        self.assertEqual(manifest["resources"]["signatures"]["files"], 1)
        args = ["manifest", str(path), "--tag", TAG, "--sha", SHA, "--version", VERSION]
        self.assertEqual(run(args)[0], 0)
        self.assertEqual(run(["manifest", str(path), "--tag", "v0.5.1", "--sha", SHA])[0], 1)
        self.assertEqual(run(["manifest", str(path), "--tag", TAG, "--sha", "d" * 40])[0], 1)

    def test_the_digest_is_deterministic_and_sees_content(self):
        repo = Repo(VERSION)
        sig = repo.root / "bundle" / "signatures"
        first = resource_manifest.digest_dir(sig)
        self.assertEqual(resource_manifest.digest_dir(sig), first)
        (sig / "a.txt").write_text("changed")
        self.assertNotEqual(resource_manifest.digest_dir(sig)["sha256"], first["sha256"])


if __name__ == "__main__":
    unittest.main()
