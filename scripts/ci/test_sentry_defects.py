# SPDX-License-Identifier: Apache-2.0
"""Tests for sentry_defects.py. No test sends anything: every send goes to a fake opener."""

import io
import json
import os
import shutil
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest import mock

import sentry_defects as sd

DATA = Path(__file__).resolve().parent / "testdata" / "sentry_defects"
SHA = "ae5a55db57c846d7dc3b74add407a36bb87c1b1d"
RUN_ID = "45596391767"
FUZZ_ARTIFACT = f"fuzz-crashes-fuzz_coff_object-{SHA}"
CRASH = "crash-48047094ce2ae7cb43102a7c8d4b608b12f47550"
# `.invalid` never resolves (RFC 2606): even a missed fake opener cannot reach a server.
DSN = "https://0123456789abcdef0123456789abcdef@sentry.invalid/4500000000000001"


def env_for(event="push", ref="refs/heads/main", repo="fission-systems/Fission", **extra):
    env = {
        "GITHUB_EVENT_NAME": event,
        "GITHUB_REPOSITORY": repo,
        "GITHUB_SHA": SHA,
        "GITHUB_RUN_ID": RUN_ID,
        "GITHUB_REF": ref,
        "GITHUB_REF_TYPE": "branch",
        "GITHUB_REF_NAME": ref.rsplit("/", 1)[-1],
    }
    env.update(extra)
    return env


def ctx_for(**kwargs):
    return sd.RunContext.from_env(env_for(**kwargs))


CTX = ctx_for()


def defect(**overrides):
    fields = dict(kind="fuzz_crash", component="fission-loader", arch="unknown", stage="loader",
                  invariant="fuzz.fuzz_coff_object", category="crash",
                  repro_id=f"fuzz:fuzz_coff_object/{CRASH}", source="test")
    fields.update(overrides)
    return sd.Defect(**fields)


class FakeResponse:
    status = 200

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class RecordingOpener:
    def __init__(self, fail_with=None):
        self.requests = []
        self.fail_with = fail_with

    def __call__(self, request, timeout):
        self.requests.append(request)
        if self.fail_with is not None:
            raise self.fail_with
        return FakeResponse()


def parse_envelope(body: bytes):
    header, item, payload, tail = body.split(b"\n")
    assert tail == b""
    item = json.loads(item)
    assert item["length"] == len(payload)
    return json.loads(header), item, json.loads(payload)


def _no_network(*args, **kwargs):
    raise AssertionError("a test reached urllib.request.urlopen")


def setUpModule():
    # Fail closed: any send that is not given a fake opener is a test bug.
    global _NETWORK_GUARD
    _NETWORK_GUARD = mock.patch.object(sd.urllib.request, "urlopen", _no_network)
    _NETWORK_GUARD.start()


def tearDownModule():
    _NETWORK_GUARD.stop()


class RunContextTests(unittest.TestCase):
    def test_trusted_events_map_to_environments(self):
        self.assertEqual(ctx_for(event="push").environment, "ci")
        self.assertEqual(ctx_for(event="workflow_dispatch").environment, "ci")
        self.assertEqual(ctx_for(event="schedule").environment, "nightly")
        merge = ctx_for(event="merge_group", ref=f"refs/heads/gh-readonly-queue/main/pr-1044-{SHA}")
        self.assertEqual(merge.environment, "ci")
        for ctx in (ctx_for(event="push"), ctx_for(event="schedule"), merge):
            self.assertIsNone(ctx.refusal())

    def test_pull_request_events_are_refused(self):
        for event in ("pull_request", "pull_request_target", "workflow_run", "issue_comment", ""):
            with self.subTest(event=event):
                self.assertIsNotNone(ctx_for(event=event).refusal())

    def test_forks_are_refused(self):
        self.assertIn("fork", ctx_for(repo="someone/Fission").refusal())

    def test_runs_off_main_are_refused(self):
        self.assertIsNotNone(ctx_for(ref="refs/heads/feature").refusal())
        self.assertIsNotNone(ctx_for(event="workflow_dispatch", ref="refs/heads/feature").refusal())
        self.assertIsNotNone(ctx_for(event="merge_group", ref="refs/heads/gh-readonly-queue/dev/pr-1").refusal())

    def test_bad_run_identity_is_refused(self):
        self.assertIsNotNone(ctx_for(GITHUB_SHA="abc123").refusal())
        self.assertIsNotNone(ctx_for(GITHUB_SHA=SHA.upper()).refusal())
        self.assertIsNotNone(ctx_for(GITHUB_RUN_ID="12; rm").refusal())

    def test_release_names(self):
        self.assertEqual(CTX.release, f"fission@{SHA[:12]}")
        tag = sd.RunContext.from_env(env_for(ref="refs/tags/v0.4.1", GITHUB_REF_TYPE="tag", GITHUB_REF_NAME="v0.4.1"))
        self.assertEqual(tag.release, "fission-cli@0.4.1")
        self.assertIsNone(tag.refusal())

    def test_run_url_and_commit_are_the_run_head(self):
        event = sd.build_event(defect(), CTX)
        self.assertEqual(event["tags"]["run_url"], f"https://github.com/fission-systems/Fission/actions/runs/{RUN_ID}")
        self.assertEqual(event["tags"]["commit"], SHA)


class SchemaTests(unittest.TestCase):
    def assertRejected(self, field, **overrides):
        with self.assertRaises(sd.Rejected) as caught:
            sd.build_event(defect(**overrides), CTX)
        self.assertEqual(caught.exception.field_name, field)

    def test_a_valid_event_has_exactly_the_contract_shape(self):
        event = sd.build_event(defect(), CTX, event_id="0" * 32, timestamp="2026-11-25T00:00:00Z")
        self.assertEqual(set(event), sd.EVENT_KEYS)
        self.assertEqual(set(event["tags"]), set(sd.TAG_KEYS))
        for absent in ("message", "logentry", "breadcrumbs", "extra", "contexts", "user", "request"):
            self.assertNotIn(absent, event)
        self.assertEqual(event["exception"], {"values": [{
            "type": "fuzz_crash", "value": "fuzz_crash in fission-loader/loader"}]})
        self.assertEqual(event["level"], "error")
        self.assertEqual(event["environment"], "ci")
        self.assertEqual(event["fingerprint"], [
            "fuzz_crash", "fission-loader", "loader", "fuzz.fuzz_coff_object", f"fuzz:fuzz_coff_object/{CRASH}"])

    def test_fingerprint_carries_repro_only_for_fuzz(self):
        flaky = defect(kind="flaky_test", category="flaky", stage="test", arch="unknown",
                       invariant="fission-cli::debug_session_cli::interactive_emulator_eof",
                       component="fission-cli", repro_id=f"run:{RUN_ID}/defect-evidence-junit-linux")
        event = sd.build_event(flaky, CTX)
        self.assertEqual(event["fingerprint"], [
            "flaky_test", "fission-cli", "test", "fission-cli::debug_session_cli::interactive_emulator_eof"])
        self.assertEqual(event["level"], "warning")

    def test_closed_sets(self):
        self.assertRejected("kind", kind="segfault")
        self.assertRejected("arch", arch="x64")
        self.assertRejected("stage", stage="decompile")
        self.assertRejected("category", category="bug")

    def test_component_must_be_a_fission_crate(self):
        for bad in ("serde", "fission_cli", "Fission-cli", "fission-", "crates/fission-cli", ""):
            with self.subTest(bad=bad):
                self.assertRejected("component", component=bad)
        sd.build_event(defect(component="xtask", stage="test", kind="test_failure"), CTX)

    def test_invariant_rejects_paths_free_text_and_length(self):
        for bad in ("crates/fission-loader/src/detector/mod.rs", "index out of bounds", "Ground_truth.x",
                    "a" * 65, "", "native.x86_64.add.af\n", "fcvtzu x0, d1", "name'quote"):
            with self.subTest(bad=bad):
                self.assertRejected("invariant", invariant=bad)

    def test_repro_id_rejects_paths_free_text_and_external_addresses(self):
        for bad in (
            "fixture:../outside.elf@0x401000",
            "fixture:crates/../../etc/passwd",
            "/home/runner/work/corpus/bin@0x401000",
            "fixture:/opt/decbench/binaries/O2/curl@0x401000",
            "fixture:crates/x.elf@0x401000 extra",
            "fixture:crates/x.elf@401000",
            "fuzz:fuzz_coff_object/crash-48047",
            "fuzz:fuzz_coff_object/../crash-48047094ce2ae7cb43102a7c8d4b608b12f47550",
            "run:abc/artifact",
            "run:123/",
            "https://example.com/x",
            'run:123/"x"',
            "x" * 129,
            "",
        ):
            with self.subTest(bad=bad):
                self.assertRejected("repro_id", repro_id=bad)
        for good in ("fixture:crates/fission-loader/testdata/x64_password.elf@0x401000",
                     "fixture:crates/fission-dir/testdata/ground_truth_fixture.elf",
                     f"run:{RUN_ID}/defect-evidence-native-diff-ubuntu-24.04"):
            sd.build_event(defect(repro_id=good, kind="verify_diverged", category="confirmed_semantic_defect"), CTX)

    def test_bad_commit_and_run_url_are_rejected(self):
        for sha in ("abc123", SHA.upper(), SHA + "0", "g" * 40):
            ctx = sd.RunContext(event_name="push", repository=sd.REPOSITORY, sha=sha, run_id=RUN_ID,
                                ref="refs/heads/main")
            with self.subTest(sha=sha), self.assertRaises(sd.Rejected) as caught:
                sd.build_event(defect(), ctx)
            self.assertEqual(caught.exception.field_name, "commit")
        ctx = sd.RunContext(event_name="push", repository=sd.REPOSITORY, sha=SHA, run_id="0",
                            ref="refs/heads/main")
        with self.assertRaises(sd.Rejected) as caught:
            sd.build_event(defect(), ctx)
        self.assertEqual(caught.exception.field_name, "run_url")

    def test_check_tags_requires_every_tag(self):
        tags = sd.tags_for(defect(), CTX)
        del tags["run_url"]
        with self.assertRaises(sd.Rejected):
            sd.check_tags(tags)
        tags = sd.tags_for(defect(), CTX)
        tags["note"] = "extra"
        with self.assertRaises(sd.Rejected):
            sd.check_tags(tags)

    def test_flaky_category_belongs_to_flaky_tests_only(self):
        self.assertRejected("category", category="flaky")
        self.assertRejected("category", kind="flaky_test", category="crash", stage="test")

    def test_a_refused_run_reports_against_the_run_not_the_defect(self):
        # Adapter-time checks cover only defect fields, so a pull_request run's
        # findings are listed as found, not as rejected values.
        pr = ctx_for(event="pull_request", ref="refs/pull/1/merge", GITHUB_SHA="")
        ledger = sd.scan_fuzz_dir(DATA / FUZZ_ARTIFACT, "fuzz_coff_object", pr)
        self.assertEqual(len(ledger.defects), 1)
        self.assertEqual(ledger.rejected, [])


class JunitTests(unittest.TestCase):
    def setUp(self):
        self.ledger = sd.parse_junit(DATA / "junit.xml", CTX, "defect-evidence-junit-linux")

    def test_flaky_and_aborted_tests_become_events(self):
        by_kind = {(d.kind, d.invariant): d for d in self.ledger.defects}
        flaky = by_kind[("flaky_test", "fission-junitprobe::probe::flaky_once")]
        self.assertEqual((flaky.category, flaky.stage, flaky.component, flaky.arch),
                         ("flaky", "test", "fission-junitprobe", "unknown"))
        aborted = by_kind[("test_failure", "fission-junitprobe::probe::aborts")]
        self.assertEqual(aborted.category, "crash")
        self.assertEqual(aborted.repro_id, f"run:{RUN_ID}/defect-evidence-junit-linux")
        self.assertEqual(len(self.ledger.defects), 2)

    def test_timeouts_and_plain_failures_are_not_defects(self):
        reasons = " ".join(reason for _, reason in self.ledger.not_defects)
        self.assertIn("test timeout", reasons)
        self.assertIn("deterministic test failure", reasons)
        names = {d.invariant for d in self.ledger.defects}
        self.assertNotIn("fission-junitprobe::probe::hangs", names)
        self.assertNotIn("fission-junitprobe::probe::always_fails", names)

    def test_passing_tests_are_ignored(self):
        names = {d.invariant for d in self.ledger.defects}
        self.assertFalse(any("passes" in n or "deep_module" in n for n in names))

    def test_long_test_ids_are_shortened_deterministically(self):
        long = sd.test_invariant("fission-midend-normalize", "cleanup::tests::casts_and_subvars::" + "x" * 60)
        self.assertEqual(len(long), 64)
        self.assertRegex(long, sd.INVARIANT_RE)
        self.assertEqual(long, sd.test_invariant("fission-midend-normalize",
                                                 "cleanup::tests::casts_and_subvars::" + "x" * 60))
        self.assertNotEqual(long, sd.test_invariant("fission-midend-normalize",
                                                    "cleanup::tests::casts_and_subvars::" + "y" * 60))
        self.assertEqual(sd.test_invariant("fission-cli::debug_session_cli", "interactive_emulator_eof"),
                         "fission-cli::debug_session_cli::interactive_emulator_eof")

    def test_non_fission_binaries_and_odd_names_are_rejected(self):
        xml = DATA.joinpath("junit.xml").read_text()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "junit.xml"
            path.write_text(xml.replace("fission-junitprobe::probe", "serde_probe::probe")
                            .replace('name="flaky_once"', 'name="case [1] with spaces"'))
            ledger = sd.parse_junit(path, CTX, "a")
        self.assertEqual(ledger.defects, [])
        self.assertEqual({f for _, f, _ in ledger.rejected}, {"component"})

    def test_unreadable_junit_is_not_a_defect(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "junit.xml"
            path.write_text("<testsuites><testsuite")
            ledger = sd.parse_junit(path, CTX, "a")
        self.assertEqual(ledger.defects, [])
        self.assertTrue(ledger.not_defects)


class NativeDifferentialTests(unittest.TestCase):
    def test_local_aarch64_run(self):
        ledger = sd.parse_native_log(DATA / "native-diff-aarch64.txt", CTX, "defect-evidence-native-diff-ubuntu-24.04-arm")
        self.assertEqual(sorted(d.invariant for d in ledger.defects),
                         ["native.aarch64.fcvtzu.x0", "native.aarch64.fmax.q0"])
        for d in ledger.defects:
            self.assertEqual((d.kind, d.component, d.stage, d.arch, d.category),
                             ("native_differential_mismatch", "fission-emulator", "emulator", "aarch64",
                              "confirmed_semantic_defect"))
            self.assertEqual(d.repro_id, f"run:{RUN_ID}/defect-evidence-native-diff-ubuntu-24.04-arm")

    def test_ci_log_with_github_prefixes(self):
        ledger = sd.parse_native_log(DATA / "native-diff-gh-log-new.txt", CTX, "native")
        self.assertEqual(sorted(d.invariant for d in ledger.defects), [
            "native.x86_64.psubsw.xmm0", "native.x86_64.psubsw.xmm5",
            "native.x86_64.rcpps.xmm0", "native.x86_64.rcpps.xmm5",
            "native.x86_64.rcpss.xmm15",
            "native.x86_64.rsqrtps.xmm0", "native.x86_64.rsqrtps.xmm7", "native.x86_64.rsqrtps.xmm8",
        ])
        self.assertTrue(all(d.arch == "x86_64" for d in ledger.defects))

    def test_stale_baseline_and_unsupported_instructions_are_not_defects(self):
        ledger = sd.parse_native_log(DATA / "native-diff-gh-log-stale.txt", CTX, "native")
        self.assertEqual(ledger.defects, [])
        self.assertEqual(len(ledger.not_defects), 1)
        self.assertIn("stale baseline", ledger.not_defects[0][1])

    def test_lines_outside_the_failure_block_are_ignored(self):
        text = "  ADD: new divergence in [\"af\"]\nnative differential (x86_64) failed:\nnote: end\n" \
               "  SUB: new divergence in [\"cf\"]\n"
        ledger = self._parse(text)
        self.assertEqual(ledger.defects, [])

    def test_colour_codes_are_ignored(self):
        ledger = self._parse('\x1b[1mnative differential (x86_64) failed:\x1b[0m\n'
                             '  ADD: new divergence in ["\x1b[31mAF\x1b[0m"]\n')
        self.assertEqual([d.invariant for d in ledger.defects], ["native.x86_64.add.af"])

    def test_panic_and_decode_fields(self):
        ledger = self._parse('native differential (aarch64) failed:\n  LDR: new divergence in ["panic", "decode"]\n')
        by_field = {d.invariant.rsplit(".", 1)[1]: d for d in ledger.defects}
        self.assertEqual(by_field["panic"].category, "crash")
        self.assertEqual(by_field["decode"].stage, "lift")

    def test_bad_names_are_rejected(self):
        ledger = self._parse('native differential (x86_64) failed:\n  ADD: new divergence in ["x y"]\n'
                             'native differential (riscv64) failed:\n  ADD: new divergence in ["a0"]\n')
        self.assertEqual(ledger.defects, [])
        self.assertEqual(len(ledger.rejected), 2)

    def _parse(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "native-diff.log"
            path.write_text(text)
            return sd.parse_native_log(path, CTX, "native")


class ReplayWitnessTests(unittest.TestCase):
    def test_lane_disagreeing_with_native_is_a_defect(self):
        ledger = sd.parse_replay_witness(DATA / "replay-witness.json", CTX, "defect-evidence-replay-witness")
        self.assertEqual(len(ledger.defects), 1)
        d = ledger.defects[0]
        self.assertEqual((d.kind, d.component, d.stage, d.arch, d.invariant, d.category), (
            "verify_diverged", "fission-dir", "verify", "x86_64", "replay.hir.buffer", "confirmed_semantic_defect"))
        self.assertEqual(d.repro_id, f"run:{RUN_ID}/defect-evidence-replay-witness")

    def test_unknown_abi_or_version_is_not_sent(self):
        witness = json.loads(DATA.joinpath("replay-witness.json").read_text())
        for change, bucket in ((("abi", "x86_64-windows"), "rejected"), (("version", 2), "not_defects")):
            data = dict(witness)
            data[change[0]] = change[1]
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / "replay-witness.json"
                path.write_text(json.dumps(data))
                ledger = sd.parse_replay_witness(path, CTX, "a")
            self.assertEqual(ledger.defects, [])
            self.assertTrue(getattr(ledger, bucket))


class FuzzTests(unittest.TestCase):
    def test_crash_file_from_the_ci_artifact(self):
        ledger = sd.scan_fuzz_dir(DATA / FUZZ_ARTIFACT, "fuzz_coff_object", CTX)
        self.assertEqual(len(ledger.defects), 1)
        d = ledger.defects[0]
        self.assertEqual((d.kind, d.component, d.stage, d.arch, d.invariant, d.category, d.repro_id), (
            "fuzz_crash", "fission-loader", "loader", "unknown", "fuzz.fuzz_coff_object", "crash",
            f"fuzz:fuzz_coff_object/{CRASH}"))

    def test_owner_comes_from_the_checkout(self):
        self.assertEqual(sd.fuzz_target_owner("fuzz_sleigh_decode"), "fission-sleigh")
        self.assertEqual(sd.fuzz_target_owner("fuzz_elf_parser"), "fission-loader")
        self.assertIsNone(sd.fuzz_target_owner("fuzz_not_a_target"))
        self.assertIsNone(sd.fuzz_target_owner("../fuzz_elf_parser"))

    def test_timeouts_ooms_and_leaks_are_not_crashes(self):
        with tempfile.TemporaryDirectory() as tmp:
            for prefix in ("timeout", "oom", "leak", "slow-unit"):
                Path(tmp, f"{prefix}-{'a' * 40}").write_bytes(b"")
            Path(tmp, "notes.txt").write_bytes(b"")
            ledger = sd.scan_fuzz_dir(Path(tmp), "fuzz_elf_parser", CTX)
        self.assertEqual(ledger.defects, [])
        self.assertEqual(len(ledger.not_defects), 5)

    def test_unknown_target_is_rejected(self):
        ledger = sd.scan_fuzz_dir(DATA / FUZZ_ARTIFACT, "fuzz_somewhere_else", CTX)
        self.assertEqual(ledger.defects, [])
        self.assertEqual(ledger.rejected[0][1], "component")

    def test_each_crash_is_its_own_fingerprint(self):
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp, f"crash-{'a' * 40}").write_bytes(b"")
            Path(tmp, f"crash-{'b' * 40}").write_bytes(b"")
            ledger = sd.scan_fuzz_dir(Path(tmp), "fuzz_elf_parser", CTX)
        kept, duplicates, _ = sd.apply_caps(ledger.defects)
        self.assertEqual((len(kept), len(duplicates)), (2, 0))


class VerifyTests(unittest.TestCase):
    FIXTURE = "crates/fission-dir/testdata/ground_truth_fixture.elf"

    def parse(self, text, binary=FIXTURE, arch="x86_64", address=None):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "out.verify.jsonl"
            path.write_text(text)
            return sd.parse_verify(path, CTX, binary, arch, address)

    def test_real_batch_output(self):
        # `fission_cli verify <fixture> --all --tier all` on this checkout's fixture:
        # 20 functions, two ground-truth divergences, the rest equivalent,
        # unsupported, suspect or inconclusive.
        ledger = sd.parse_verify(DATA / "verify-batch.verify.jsonl", CTX, self.FIXTURE, "x86_64")
        self.assertEqual(ledger.rejected, [])
        self.assertEqual(sorted(d.repro_id for d in ledger.defects), [
            f"fixture:{self.FIXTURE}@0x201560", f"fixture:{self.FIXTURE}@0x201590"])
        self.assertEqual({d.invariant for d in ledger.defects}, {"ground_truth.hir_eval"})
        reasons = [r for _, r in ledger.not_defects]
        self.assertEqual(sum("suspect" in r for r in reasons), 1)
        self.assertEqual(sum("inconclusive" in r for r in reasons), 3)
        self.assertGreater(sum("unsupported" in r for r in reasons), 10)
        for d in ledger.defects:
            self.assertEqual((d.kind, d.component, d.stage, d.category),
                             ("verify_diverged", "fission-dir", "verify", "confirmed_semantic_defect"))
            self.assertRegex(d.repro_id, r"^fixture:crates/fission-dir/testdata/ground_truth_fixture\.elf@0x[0-9a-f]+$")

    def test_diverged_tiers(self):
        line = {"address": "0x401136", "name": "anything at all", "seed": 1,
                "concrete": {"category": "diverged", "detail": "x", "reason": "diverged", "counterexamples": []},
                "ground_truth": {"category": "diverged", "detail": "Diverged(..)", "reason": "diverged",
                                 "counterexamples": [{"run": 0, "args": [1], "prehir_result": 2,
                                                      "hir_result": 2, "emulator_result": 3, "detail": None}]},
                "symbolic": {"category": "diverged", "detail": "x", "reason": "solver counterexample"}}
        ledger = self.parse(json.dumps(line))
        self.assertEqual(sorted(d.invariant for d in ledger.defects),
                         ["concrete.prehir_hir", "ground_truth.return_value", "symbolic.counterexample"])
        self.assertTrue(all(d.repro_id == f"fixture:{self.FIXTURE}@0x401136" for d in ledger.defects))

    def test_side_effect_divergence(self):
        line = {"address": "0x10", "ground_truth": {"category": "diverged", "counterexamples": [
            {"hir_result": 1, "emulator_result": 1, "detail": "a stored byte differs"}]}}
        self.assertEqual([d.invariant for d in self.parse(json.dumps(line)).defects], ["ground_truth.side_effect"])
        line["ground_truth"]["counterexamples"] = [{"hir_result": None, "emulator_result": 4, "detail": "x"}]
        self.assertEqual([d.invariant for d in self.parse(json.dumps(line)).defects], ["ground_truth.hir_eval"])

    def test_inconclusive_unknown_unsupported_and_timeouts_are_not_defects(self):
        lines = [
            {"address": "0x10", "symbolic": {"category": "inconclusive", "reason": "solver Unknown"}},
            {"address": "0x20", "symbolic": {"category": "inconclusive", "reason": "timeout"}},
            {"address": "0x30", "ground_truth": {"category": "unsupported", "reason": "unsupported construct"}},
            {"address": "0x40", "ground_truth": {"category": "harness", "reason": "emulator setup failed"}},
            {"address": "0x50", "ground_truth": {"category": "suspect", "counterexamples": [{}]}},
            {"address": "0x60", "decompile": {"category": "error", "reason": "analysis timeout"}},
            {"address": "0x70", "concrete": {"category": "unknown"}},
            {"address": "0x80", "concrete": {"category": "equivalent", "checked": 8}},
        ]
        ledger = self.parse("\n".join(json.dumps(x) for x in lines))
        self.assertEqual(ledger.defects, [])
        self.assertEqual(ledger.rejected, [])
        self.assertEqual(len(ledger.not_defects), 7)

    def test_single_function_output(self):
        single = {"concrete": "Diverged (1 divergent out of 8 requested samples)", "seed": 1,
                  "tiers": {"concrete": {"category": "diverged", "detail": "d", "reason": "r"}}}
        self.assertEqual(len(self.parse(json.dumps(single), address="0x401136").defects), 1)
        self.assertEqual(self.parse(json.dumps(single)).rejected[0][1], "repro_id")

    def test_external_binaries_are_rejected(self):
        line = json.dumps({"address": "0x401000", "ground_truth": {"category": "diverged", "counterexamples": []}})
        for binary in ("/opt/decbench/binaries/O2/curl", "../Fission-other/x.elf",
                       "crates/fission-dir/testdata/does_not_exist.elf", ""):
            with self.subTest(binary=binary):
                ledger = self.parse(line, binary=binary)
                self.assertEqual(ledger.defects, [])
                self.assertEqual(ledger.rejected[0][1], "repro_id")

    def test_bad_arch_is_rejected(self):
        line = json.dumps({"address": "0x10", "concrete": {"category": "diverged"}})
        ledger = self.parse(line, arch="amd64")
        self.assertEqual(ledger.defects, [])
        self.assertEqual(ledger.rejected[0][1], "arch")


class TriageAcceptsTests(unittest.TestCase):
    """The triage on main (sentry_triage.py) accepts what this reporter sends."""

    def test_every_kind_passes_the_triage_tag_checks(self):
        import sentry_triage

        ledger = sd.Ledger()
        for path, parse in ((DATA / "junit.xml", sd.parse_junit), (DATA / "native-diff-aarch64.txt", sd.parse_native_log),
                            (DATA / "replay-witness.json", sd.parse_replay_witness)):
            ledger.extend(parse(path, CTX, "defect-evidence-x"))
        ledger.extend(sd.scan_fuzz_dir(DATA / FUZZ_ARTIFACT, "fuzz_coff_object", CTX))
        ledger.extend(sd.parse_verify(DATA / "verify-batch.verify.jsonl", CTX,
                                      "crates/fission-dir/testdata/ground_truth_fixture.elf", "x86_64"))
        events = [sd.build_event(d, ctx) for d in ledger.defects
                  for ctx in (CTX, ctx_for(event="schedule"))]
        self.assertEqual({e["tags"]["kind"] for e in events}, set(sd.KINDS))
        for event in events:
            # Sentry stores the event's environment as a tag beside the event's own tags.
            stored = {"groupID": "7", "tags": [{"key": "environment", "value": event["environment"]}]
                      + [{"key": k, "value": v} for k, v in event["tags"].items()]}
            tags = sentry_triage.validate_tags(stored, "7", "fission-systems/Fission")
            self.assertEqual(tags["run_id"], RUN_ID)
            run = {"repository": {"full_name": "fission-systems/Fission"}, "head_sha": SHA,
                   "event": "schedule" if event["environment"] == "nightly" else "push"}
            sentry_triage.check_run(run, tags, "fission-systems/Fission")
            if tags["kind"] == "flaky_test":
                with self.assertRaises(sentry_triage.Rejected):
                    sentry_triage.check_eligible(tags)
            elif tags["category"] in ("confirmed_semantic_defect", "crash"):
                sentry_triage.check_eligible(tags)


class CapTests(unittest.TestCase):
    def test_run_cap_and_fingerprint_dedupe(self):
        defects = [defect(kind="native_differential_mismatch", component="fission-emulator", stage="emulator",
                          arch="x86_64", invariant=f"native.x86_64.add.r{i}", category="confirmed_semantic_defect",
                          repro_id=f"run:{RUN_ID}/n") for i in range(60)]
        defects += [defects[0], defects[1]]
        kept, duplicates, over = sd.apply_caps(defects)
        self.assertEqual((len(kept), len(duplicates), len(over)), (50, 2, 10))
        self.assertEqual(len({tuple(d.fingerprint()) for d in kept}), 50)

    def test_defects_outrank_flaky_tests_at_the_cap(self):
        flaky = [defect(kind="flaky_test", category="flaky", stage="test", component="fission-cli",
                        invariant=f"fission-cli::t{i}", repro_id=f"run:{RUN_ID}/j") for i in range(55)]
        kept, _, over = sd.apply_caps(flaky + [defect()])
        self.assertEqual(kept[0].kind, "fuzz_crash")
        self.assertTrue(all(d.kind == "flaky_test" for d in over))

    def test_order_is_deterministic(self):
        items = [defect(repro_id=f"fuzz:fuzz_coff_object/crash-{c * 40}") for c in "cab"]
        self.assertEqual(sd.apply_caps(items)[0], sd.apply_caps(list(reversed(items)))[0])


class ScanAndSendTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.evidence = root / "evidence"
        (self.evidence / "defect-evidence-junit-linux").mkdir(parents=True)
        shutil.copy(DATA / "junit.xml", self.evidence / "defect-evidence-junit-linux" / "junit.xml")
        native = self.evidence / "defect-evidence-native-diff-ubuntu-24.04-arm"
        native.mkdir()
        shutil.copy(DATA / "native-diff-aarch64.txt", native / "native-diff.log")
        replay = self.evidence / "defect-evidence-replay-witness"
        replay.mkdir()
        shutil.copy(DATA / "replay-witness.json", replay / "replay-witness.json")
        shutil.copytree(DATA / FUZZ_ARTIFACT, self.evidence / FUZZ_ARTIFACT)
        self.summary = root / "summary.md"

    def tearDown(self):
        self.tmp.cleanup()

    def run_main(self, env, *extra, opener=None):
        env = dict(env, GITHUB_STEP_SUMMARY=str(self.summary))
        opener = opener or RecordingOpener()
        out = io.StringIO()
        with mock.patch("sys.stdout", out):
            code = sd.main(["--scan", str(self.evidence), *extra], env=env, opener=opener)
        return code, opener, self.summary.read_text()

    def test_scan_finds_every_adapter(self):
        ledger = sd.scan(self.evidence, CTX)
        kinds = sorted(d.kind for d in ledger.defects)
        self.assertEqual(kinds, ["flaky_test", "fuzz_crash", "native_differential_mismatch",
                                 "native_differential_mismatch", "test_failure", "verify_diverged"])

    def test_dry_run_without_dsn_sends_nothing_and_summarises(self):
        code, opener, text = self.run_main(env_for())
        self.assertEqual(code, 0)
        self.assertEqual(opener.requests, [])
        self.assertIn("dry run: SENTRY_DSN is not set", text)
        self.assertIn("Would send: 6 event(s)", text)
        self.assertIn("deterministic test failure", text)

    def test_pull_request_never_sends_even_with_a_dsn(self):
        for event in ("pull_request", "pull_request_target"):
            code, opener, text = self.run_main(env_for(event=event, ref="refs/pull/7/merge", SENTRY_DSN=DSN))
            self.assertEqual(code, 0)
            self.assertEqual(opener.requests, [])
            self.assertIn("not sending", text)
            self.assertIn("Would send: 0 event(s)", text)

    def test_fork_never_sends(self):
        _, opener, text = self.run_main(env_for(repo="someone/Fission", SENTRY_DSN=DSN))
        self.assertEqual(opener.requests, [])
        self.assertIn("fork", text)

    def test_dry_run_flag_wins_over_a_dsn(self):
        _, opener, text = self.run_main(env_for(SENTRY_DSN=DSN), "--dry-run")
        self.assertEqual(opener.requests, [])
        self.assertIn("dry run (--dry-run)", text)

    def test_send_posts_one_envelope_per_event(self):
        code, opener, text = self.run_main(env_for(event="schedule", SENTRY_DSN=DSN))
        self.assertEqual(code, 0)
        self.assertEqual(len(opener.requests), 6)
        request = opener.requests[0]
        self.assertEqual(request.full_url,
                         "https://sentry.invalid/api/4500000000000001/envelope/")
        self.assertIn("sentry_key=0123456789abcdef0123456789abcdef", request.get_header("X-sentry-auth"))
        self.assertEqual(request.get_header("Content-type"), "application/x-sentry-envelope")
        _, item, event = parse_envelope(request.data)
        self.assertEqual(item["type"], "event")
        self.assertEqual(event["environment"], "nightly")
        self.assertEqual(set(event), sd.EVENT_KEYS)
        self.assertIn("Sent: 6 event(s)", text)
        self.assertIn("sent (HTTP 200)", text)

    def test_network_failure_does_not_fail_the_job(self):
        opener = RecordingOpener(fail_with=urllib.error.URLError("down"))
        code, opener, text = self.run_main(env_for(SENTRY_DSN=DSN), opener=opener)
        self.assertEqual(code, 0)
        self.assertIn("failed (URLError)", text)

    def test_rate_limit_stops_sending(self):
        opener = RecordingOpener(fail_with=urllib.error.HTTPError("u", 429, "slow down", {}, None))
        _, opener, text = self.run_main(env_for(SENTRY_DSN=DSN), opener=opener)
        self.assertEqual(len(opener.requests), 1)
        self.assertIn("not sent (rate limited)", text)

    def test_a_reporter_error_does_not_fail_the_job(self):
        with mock.patch.object(sd, "run", side_effect=RuntimeError("boom")):
            code, opener, text = self.run_main(env_for(SENTRY_DSN=DSN))
        self.assertEqual(code, 0)
        self.assertIn("Reporter error", text)
        self.assertEqual(opener.requests, [])

    def test_events_out_matches_the_summary(self):
        out = Path(self.tmp.name) / "events.jsonl"
        self.run_main(env_for(), "--events-out", str(out))
        events = [json.loads(line) for line in out.read_text().splitlines()]
        self.assertEqual(len(events), 6)
        for event in events:
            sd.check_tags(event["tags"])

    def test_empty_evidence_is_a_clean_skip(self):
        shutil.rmtree(self.evidence)
        code, opener, text = self.run_main(env_for(SENTRY_DSN=DSN))
        self.assertEqual((code, opener.requests), (0, []))
        self.assertIn("No evidence files were found", text)


class DsnTests(unittest.TestCase):
    def test_valid_dsn(self):
        url, auth = sd.envelope_target("https://0123456789abcdef0123456789abcdef@sentry.example.com/prefix/42")
        self.assertEqual(url, "https://sentry.example.com/prefix/api/42/envelope/")
        self.assertIn("sentry_version=7", auth)

    def test_invalid_dsns(self):
        for bad in ("http://0123456789abcdef0123456789abcdef@sentry.io/1", "https://sentry.io/1",
                    "https://key@sentry.io/1", "not a dsn", ""):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                sd.envelope_target(bad)

    def test_an_invalid_dsn_falls_back_to_a_dry_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            evidence = Path(tmp) / "e"
            shutil.copytree(DATA / FUZZ_ARTIFACT, evidence / FUZZ_ARTIFACT)
            opener = RecordingOpener()
            out = io.StringIO()
            with mock.patch("sys.stdout", out):
                sd.main(["--scan", str(evidence)], env=env_for(SENTRY_DSN="http://insecure@x/1"), opener=opener)
        self.assertEqual(opener.requests, [])
        self.assertIn("SENTRY_DSN is not an https Sentry DSN", out.getvalue())


class NoAnalyzedTextTests(unittest.TestCase):
    """Free text in every input must never reach an event or an envelope."""

    SENTINEL = "SENTINEL_program_text_7f3a"

    def test_no_event_field_contains_input_text(self):
        s = self.SENTINEL
        with tempfile.TemporaryDirectory() as tmp:
            evidence = Path(tmp)
            junit = evidence / "defect-evidence-junit"
            junit.mkdir()
            xml = DATA.joinpath("junit.xml").read_text()
            xml = xml.replace("first attempt fails", s).replace("process aborted with signal 6 (SIGABRT)", s) \
                .replace("running 1 test", s).replace("arithmetic", s)
            (junit / "junit.xml").write_text(xml)
            native = evidence / "defect-evidence-native-diff"
            native.mkdir()
            log = DATA.joinpath("native-diff-aarch64.txt").read_text().replace("fcvtzu x0, d1", s)
            (native / "native-diff.log").write_text(log)
            replay = evidence / "defect-evidence-replay-witness"
            replay.mkdir()
            witness = json.loads(DATA.joinpath("replay-witness.json").read_text())
            witness.update(function=s, compiler=s, compiler_command=[s])
            (replay / "replay-witness.json").write_text(json.dumps(witness))
            fuzz = evidence / FUZZ_ARTIFACT
            fuzz.mkdir()
            (fuzz / CRASH).write_text(s)
            verify = evidence / "defect-evidence-verify"
            verify.mkdir()
            (verify / "fixture.verify.jsonl").write_text(json.dumps({
                "address": "0x401136", "name": s, "seed": 1,
                "ground_truth": {"category": "diverged", "detail": s, "reason": s,
                                 "counterexamples": [{"hir_result": 1, "emulator_result": 2, "detail": s}]}}))
            (verify / "fixture.verify.meta.json").write_text(json.dumps({
                "binary": "crates/fission-dir/testdata/ground_truth_fixture.elf", "arch": "x86_64"}))

            ledger = sd.scan(evidence, CTX)
            kept, _, _ = sd.apply_caps(ledger.defects)
            events = [sd.build_event(d, CTX) for d in kept]
        self.assertEqual(sorted({e["tags"]["kind"] for e in events}), sorted(sd.KINDS))
        for event in events:
            blob = json.dumps(event) + sd.envelope(event).decode()
            for leak in (s, "panicked", "SIGABRT", "fcvtzu x0", "REPRO", "tests/probe.rs", "/home/runner"):
                self.assertNotIn(leak, blob)
            # Every string anywhere in the event is a contract value.
            for key, value in event["tags"].items():
                self.assertRegex(value, r"^[A-Za-z0-9_.:/@+-]+$" if key != "run_url" else sd.RUN_URL_RE)
            self.assertEqual(event["exception"]["values"][0]["value"],
                             f"{event['tags']['kind']} in {event['tags']['component']}/{event['tags']['stage']}")


if __name__ == "__main__":
    unittest.main()
