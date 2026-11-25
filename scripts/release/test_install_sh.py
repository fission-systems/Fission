#!/usr/bin/env python3
"""Tests for transactional, rollback-safe Unix installation in install.sh (#1207)."""

from __future__ import annotations

import hashlib
import io
import json
import os
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
INSTALL_SH = REPO_ROOT / "install.sh"
RESOURCE_DIRS = ("sleigh-specs", "signatures", "ghidra-data")


def _digest_tree(path: Path) -> dict:
    lines = []
    for p in sorted(x for x in path.rglob("*") if x.is_file()):
        rel = p.relative_to(path).as_posix()
        digest = hashlib.sha256(p.read_bytes()).hexdigest()
        lines.append(f"{rel}\0{digest}\n")
    return {
        "files": len(lines),
        "sha256": hashlib.sha256("".join(lines).encode("utf-8")).hexdigest(),
    }


def _mock_cli_script(
    version: str,
    *,
    fail_pre_health: bool = False,
    fail_post_health: bool = False,
) -> str:
    return f"""#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

VERSION = {version!r}
FAIL_PRE = {fail_pre_health!r}
FAIL_POST = {fail_post_health!r}

invoked = sys.argv[0]
resolved_exe = Path(__file__).resolve()
bundle = resolved_exe.parent.parent / "share" / "fission"

if FAIL_PRE and ".stage." in invoked:
    sys.stderr.write("injected pre-activation health check failure\\n")
    sys.exit(1)
if FAIL_POST and ".stage." not in invoked:
    sys.stderr.write("injected post-activation health check failure\\n")
    sys.exit(1)

if sys.argv[1:2] == ["--version"]:
    print(f"fission_cli {{VERSION}} (v{{VERSION}})")
    sys.exit(0)

if sys.argv[1:4] == ["resources", "status", "--json"]:
    spec_ver_file = bundle / "sleigh-specs" / "version.txt"
    spec_ver = spec_ver_file.read_text(encoding="utf-8").strip() if spec_ver_file.is_file() else None
    manifest_file = bundle / "fission-resources.json"
    state = "missing"
    man_ver = None
    if manifest_file.is_file():
        try:
            man = json.loads(manifest_file.read_text(encoding="utf-8"))
            man_ver = man.get("fission_version")
            state = "match" if man_ver == VERSION else "mismatch"
        except Exception:
            state = "invalid"
    payload = {{
        "build": {{"version": VERSION, "tag": f"v{{VERSION}}", "sha": None}},
        "resource_manifest": {{
            "path": str(manifest_file) if manifest_file.is_file() else None,
            "state": state,
            "version": man_ver,
        }},
        "resources": {{
            "signatures_base": str(bundle / "signatures") if (bundle / "signatures").is_dir() else None,
            "bundle_version": spec_ver,
        }},
        "resolved_bundle": str(bundle),
    }}
    print(json.dumps(payload))
    sys.exit(0)

sys.stderr.write(f"unexpected args: {{sys.argv[1:]}}\\n")
sys.exit(2)
"""


def _add_file_to_tar(tf: tarfile.TarFile, arcname: str, data: bytes, mode: int = 0o644) -> None:
    ti = tarfile.TarInfo(name=arcname)
    ti.size = len(data)
    ti.mode = mode
    tf.addfile(ti, io.BytesIO(data))


def build_release_archive(
    dest_dir: Path,
    version: str,
    *,
    filename: str | None = None,
    missing_dirs: set[str] | None = None,
    manifest_version: str | None = None,
    include_manifest: bool = True,
    fail_pre_health: bool = False,
    fail_post_health: bool = False,
    write_sha256: bool = True,
) -> Path:
    missing = missing_dirs or set()
    archive_path = dest_dir / (filename or f"fission-{version}.tar.gz")
    with tempfile.TemporaryDirectory() as stage_tmp:
        root_name = f"fission-{version}-test"
        pkg_root = Path(stage_tmp) / root_name
        (pkg_root / "bin").mkdir(parents=True, exist_ok=True)
        share = pkg_root / "share" / "fission"
        share.mkdir(parents=True, exist_ok=True)

        cli_path = pkg_root / "bin" / "fission_cli"
        cli_path.write_text(
            _mock_cli_script(
                version,
                fail_pre_health=fail_pre_health,
                fail_post_health=fail_post_health,
            ),
            encoding="utf-8",
        )
        cli_path.chmod(0o755)

        present_dirs = [d for d in RESOURCE_DIRS if d not in missing]
        for d in present_dirs:
            d_path = share / d
            d_path.mkdir(parents=True, exist_ok=True)
            (d_path / "version.txt").write_text(f"{version}\n", encoding="utf-8")
            (d_path / f"{d}.dat").write_text(f"{d}-{version}\n", encoding="utf-8")

        if include_manifest and not missing:
            man_ver = manifest_version or version
            manifest = {
                "schema": 1,
                "fission_version": man_ver,
                "tag": f"v{man_ver}",
                "sha": None,
                "resources": {d: _digest_tree(share / d) for d in RESOURCE_DIRS},
            }
            (share / "fission-resources.json").write_text(
                json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
            )

        with tarfile.open(archive_path, "w:gz") as tf:
            tf.add(pkg_root, arcname=root_name)

    if write_sha256:
        digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
        (archive_path.with_name(archive_path.name + ".sha256")).write_text(
            f"{digest}  {archive_path.name}\n", encoding="utf-8"
        )
    return archive_path


class InstallShTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.fixtures = self.root / "fixtures"
        self.fixtures.mkdir()
        self.prefix = self.root / "prefix"

    def tearDown(self) -> None:
        # Restore permissions in case a failure-injection test chmod'd a directory.
        for p in self.root.rglob("*"):
            try:
                if p.is_dir():
                    p.chmod(0o755)
            except OSError:
                pass
        self.tmp.cleanup()

    def run_install(
        self,
        *args: str,
        env_extra: dict[str, str] | None = None,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.pop("FISSION_ROOT", None)
        env.pop("FISSION_RESOURCE_ROOT", None)
        env.pop("FISSION_ARCHIVE", None)
        env.pop("FISSION_CHECKSUM_FILE", None)
        env["FISSION_PREFIX"] = str(self.prefix)
        env["FISSION_LOCK_TIMEOUT"] = "1"
        if env_extra:
            env.update(env_extra)
        proc = subprocess.run(
            ["sh", str(INSTALL_SH), *args],
            cwd=str(self.root),
            env=env,
            text=True,
            capture_output=True,
        )
        if check and proc.returncode != 0:
            self.fail(
                f"install.sh {args} failed (rc={proc.returncode}):\n"
                f"STDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
            )
        return proc

    def query_cli_status(self, cli_path: Path | None = None) -> dict:
        exe = cli_path or (self.prefix / "bin" / "fission_cli")
        proc = subprocess.run(
            [str(exe), "resources", "status", "--json"],
            text=True,
            capture_output=True,
            check=True,
        )
        return json.loads(proc.stdout)

    def assert_active_pair(self, expected_version: str) -> None:
        ver_proc = subprocess.run(
            [str(self.prefix / "bin" / "fission_cli"), "--version"],
            text=True,
            capture_output=True,
            check=True,
        )
        self.assertIn(expected_version, ver_proc.stdout)
        status = self.query_cli_status()
        self.assertEqual(status["build"]["version"], expected_version)
        self.assertEqual(status["resources"]["bundle_version"], expected_version)
        self.assertEqual(status["resource_manifest"]["state"], "match")
        # Compatibility symlink / path resolves identically.
        self.assertEqual(
            (self.prefix / "share" / "fission" / "sleigh-specs" / "version.txt")
            .read_text(encoding="utf-8")
            .strip(),
            expected_version,
        )

    def test_fresh_install_upgrade_concurrent_reader_and_rollback(self) -> None:
        v3 = build_release_archive(self.fixtures, "0.3.0")
        v4 = build_release_archive(self.fixtures, "0.4.0")

        self.run_install("--archive", str(v3), "--require-checksum")
        self.assert_active_pair("0.3.0")

        # Record the pinned canonical v0.3.0 executable (models an already-running process).
        pinned_v3_exe = (self.prefix / "bin" / "fission_cli").resolve()
        self.assertIn("releases", str(pinned_v3_exe))

        # Upgrade 0.3.0 -> 0.4.0 cleanly.
        self.run_install("--archive", str(v4), "--require-checksum")
        self.assert_active_pair("0.4.0")

        # Previously running v0.3.0 reader still resolves its own immutable v0.3.0 bundle.
        pinned_status = self.query_cli_status(pinned_v3_exe)
        self.assertEqual(pinned_status["build"]["version"], "0.3.0")
        self.assertEqual(pinned_status["resources"]["bundle_version"], "0.3.0")

        # Rollback restores BOTH binary and resources to 0.3.0.
        self.run_install("--rollback")
        self.assert_active_pair("0.3.0")

        # Rolling back again flips back to 0.4.0.
        self.run_install("--rollback")
        self.assert_active_pair("0.4.0")

    def test_downgrade_blocked_by_default_and_allowed_explicitly(self) -> None:
        v3 = build_release_archive(self.fixtures, "0.3.0")
        v4 = build_release_archive(self.fixtures, "0.4.0")

        self.run_install("--archive", str(v4))
        self.assert_active_pair("0.4.0")

        res = self.run_install("--archive", str(v3), check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("downgrade", res.stderr.lower())
        self.assert_active_pair("0.4.0")

        self.run_install("--archive", str(v3), "--allow-downgrade")
        self.assert_active_pair("0.3.0")

    def test_injected_failures_preserve_prior_complete_install(self) -> None:
        v3 = build_release_archive(self.fixtures, "0.3.0")
        self.run_install("--archive", str(v3))
        self.assert_active_pair("0.3.0")

        # 1. Missing required checksum when --require-checksum is set.
        v4_nosum = build_release_archive(
            self.fixtures, "0.4.0", filename="nosum-0.4.0.tar.gz", write_sha256=False
        )
        res = self.run_install("--archive", str(v4_nosum), "--require-checksum", check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assert_active_pair("0.3.0")

        # 2. Checksum mismatch.
        bad_sum = self.fixtures / "bad.sha256"
        bad_sum.write_text("0" * 64 + "  nosum-0.4.0.tar.gz\n", encoding="utf-8")
        res = self.run_install(
            "--archive", str(v4_nosum), "--checksum", str(bad_sum), check=False
        )
        self.assertNotEqual(res.returncode, 0)
        self.assert_active_pair("0.3.0")

        # 3. Corrupt / truncated archive.
        corrupt = self.fixtures / "corrupt.tar.gz"
        corrupt.write_bytes(b"not a gzip tar archive")
        res = self.run_install("--archive", str(corrupt), check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assert_active_pair("0.3.0")

        # 4. Path-traversal archive entry (`..`).
        traversal = self.fixtures / "traversal.tar.gz"
        with tarfile.open(traversal, "w:gz") as tf:
            _add_file_to_tar(tf, "fission-0.4.0/../evil.txt", b"pwned")
        res = self.run_install("--archive", str(traversal), check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assertFalse((self.prefix / "evil.txt").exists())
        self.assert_active_pair("0.3.0")

        # 5. Escaping symlink inside archive.
        symlink_tar = self.fixtures / "symlink.tar.gz"
        with tarfile.open(symlink_tar, "w:gz") as tf:
            ti = tarfile.TarInfo(name="fission-0.4.0/bin/fission_cli")
            ti.type = tarfile.SYMTYPE
            ti.linkname = "/etc/passwd"
            tf.addfile(ti)
        res = self.run_install("--archive", str(symlink_tar), check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assert_active_pair("0.3.0")

        # 6. Missing resource directory (`ghidra-data`).
        missing_res = build_release_archive(
            self.fixtures,
            "0.4.0",
            filename="missing-res-0.4.0.tar.gz",
            missing_dirs={"ghidra-data"},
        )
        res = self.run_install("--archive", str(missing_res), check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assert_active_pair("0.3.0")

        # 7. Binary vs manifest version mismatch.
        mismatched = build_release_archive(
            self.fixtures,
            "0.4.0",
            filename="mismatch-0.4.0.tar.gz",
            manifest_version="0.9.9",
        )
        res = self.run_install("--archive", str(mismatched), check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assert_active_pair("0.3.0")

        # 8. Failing pre-activation health check.
        bad_pre = build_release_archive(
            self.fixtures,
            "0.4.0",
            filename="bad-pre-0.4.0.tar.gz",
            fail_pre_health=True,
        )
        res = self.run_install("--archive", str(bad_pre), check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assert_active_pair("0.3.0")

        # 9. Failing post-activation health check automatically rolls back to 0.3.0.
        bad_post = build_release_archive(
            self.fixtures,
            "0.4.0",
            filename="bad-post-0.4.0.tar.gz",
            fail_post_health=True,
        )
        res = self.run_install("--archive", str(bad_post), check=False)
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("rolled back", res.stderr.lower())
        self.assert_active_pair("0.3.0")

        # 10. Read-only releases directory (copy/commit failure) keeps 0.3.0 runnable.
        v4 = build_release_archive(self.fixtures, "0.4.0")
        releases_dir = self.prefix / "lib" / "fission" / "releases"
        releases_dir.chmod(0o555)
        try:
            res = self.run_install("--archive", str(v4), check=False)
            self.assertNotEqual(res.returncode, 0)
        finally:
            releases_dir.chmod(0o755)
        self.assert_active_pair("0.3.0")
        self.assertEqual(list((self.prefix / "lib" / "fission").glob(".stage.*")), [])

    def test_concurrent_lock_blocks_live_holder_and_reaps_stale_lock(self) -> None:
        v3 = build_release_archive(self.fixtures, "0.3.0")
        v4 = build_release_archive(self.fixtures, "0.4.0")
        self.run_install("--archive", str(v3))

        store = self.prefix / "lib" / "fission"
        lock_dir = store / ".install.lock"
        lock_dir.mkdir()
        # Hold lock with an active child process.
        sleeper = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(10)"])
        try:
            (lock_dir / "pid").write_text(f"{sleeper.pid}\n", encoding="utf-8")
            res = self.run_install(
                "--archive",
                str(v4),
                env_extra={"FISSION_LOCK_TIMEOUT": "0"},
                check=False,
            )
            self.assertNotEqual(res.returncode, 0)
            self.assertIn("lock", res.stderr.lower())
            self.assert_active_pair("0.3.0")
        finally:
            sleeper.terminate()
            sleeper.wait(timeout=5)

        # Now `sleeper.pid` is dead; installer must reap the stale lock and succeed.
        self.run_install("--archive", str(v4), env_extra={"FISSION_LOCK_TIMEOUT": "0"})
        self.assert_active_pair("0.4.0")

    def test_legacy_migration_preserves_custom_user_data_and_uninstall_keeps_user_files(self) -> None:
        # Seed a legacy flat prefix matching pre-transactional install.sh layout.
        (self.prefix / "bin").mkdir(parents=True, exist_ok=True)
        (self.prefix / "bin" / "other_tool").write_text("#!/bin/sh\necho keep\n", encoding="utf-8")
        legacy_bin = self.prefix / "bin" / "fission_cli"
        legacy_bin.write_text(_mock_cli_script("0.3.0"), encoding="utf-8")
        legacy_bin.chmod(0o755)

        share = self.prefix / "share" / "fission"
        for d in RESOURCE_DIRS:
            d_path = share / d
            d_path.mkdir(parents=True, exist_ok=True)
            (d_path / "version.txt").write_text("0.3.0\n", encoding="utf-8")

        custom_file = share / "my_custom.fid"
        custom_file.write_text("user-signature-data\n", encoding="utf-8")
        custom_dir = share / "user_notes"
        custom_dir.mkdir()
        (custom_dir / "readme.txt").write_text("do not delete\n", encoding="utf-8")

        # Pre-seed an abandoned staging directory to verify cleanup.
        abandoned = self.prefix / "lib" / "fission" / ".stage.999999.abandoned"
        abandoned.mkdir(parents=True, exist_ok=True)
        (abandoned / "junk").write_text("leftover", encoding="utf-8")

        v4 = build_release_archive(self.fixtures, "0.4.0")
        self.run_install("--archive", str(v4))
        self.assert_active_pair("0.4.0")
        self.assertFalse(abandoned.exists())

        # Custom user files preserved untouched.
        self.assertEqual(custom_file.read_text(encoding="utf-8"), "user-signature-data\n")
        self.assertEqual((custom_dir / "readme.txt").read_text(encoding="utf-8"), "do not delete\n")

        # Rollback restores the snapshotted legacy 0.3.0 binary and resources!
        self.run_install("--rollback")
        ver_proc = subprocess.run(
            [str(self.prefix / "bin" / "fission_cli"), "--version"],
            text=True,
            capture_output=True,
            check=True,
        )
        self.assertIn("0.3.0", ver_proc.stdout)
        self.assertEqual(
            (self.prefix / "share" / "fission" / "sleigh-specs" / "version.txt")
            .read_text(encoding="utf-8")
            .strip(),
            "0.3.0",
        )

        # Uninstall cleans Fission-managed links/store while keeping custom data & unrelated tools.
        self.run_install("--uninstall")
        self.assertFalse((self.prefix / "bin" / "fission_cli").exists())
        self.assertFalse((self.prefix / "lib" / "fission").exists())
        self.assertTrue((self.prefix / "bin" / "other_tool").is_file())
        self.assertEqual(custom_file.read_text(encoding="utf-8"), "user-signature-data\n")
        self.assertEqual((custom_dir / "readme.txt").read_text(encoding="utf-8"), "do not delete\n")

    def test_repeated_install_and_prune_keep_current_and_previous(self) -> None:
        v3 = build_release_archive(self.fixtures, "0.3.0")
        v4 = build_release_archive(self.fixtures, "0.4.0")
        v5 = build_release_archive(self.fixtures, "0.5.0")

        self.run_install("--archive", str(v3))
        self.run_install("--archive", str(v4))
        self.run_install("--archive", str(v4))  # repeated install of same version
        self.assert_active_pair("0.4.0")

        self.run_install("--archive", str(v5), "--keep", "2")
        self.assert_active_pair("0.5.0")

        releases_dir = self.prefix / "lib" / "fission" / "releases"
        kept_dirs = [p.name for p in releases_dir.iterdir() if p.is_dir()]
        self.assertEqual(len(kept_dirs), 2, f"expected 2 retained generations, got {kept_dirs}")
        self.run_install("--prune", "--keep", "2")
        kept_after_prune = [p.name for p in releases_dir.iterdir() if p.is_dir()]
        self.assertEqual(len(kept_after_prune), 2)


if __name__ == "__main__":
    unittest.main()
