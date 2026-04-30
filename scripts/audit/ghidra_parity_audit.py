#!/usr/bin/env python3
"""Report Fission implementation gaps against Ghidra owner chains.

This is a reporting-only audit. It does not classify benchmark rows as success,
does not repair semantics, and does not execute Ghidra code.
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


STATUS_IMPLEMENTED = "implemented"
STATUS_PARTIAL = "partial"
STATUS_LEGACY_DEBT = "legacy_debt"
STATUS_TYPED_UNSUPPORTED = "typed_unsupported"
STATUS_NOT_STARTED = "not_started"


@dataclass(frozen=True)
class Probe:
    name: str
    owner_chain: str
    status: str
    evidence: list[str]
    next_action: str


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return path.read_text(encoding="utf-8", errors="replace")


def count_pattern(paths: Iterable[Path], pattern: str) -> int:
    compiled = re.compile(pattern)
    total = 0
    for path in paths:
        if not path.is_file():
            continue
        total += len(compiled.findall(read_text(path)))
    return total


def rust_files(root: Path, relative: str) -> list[Path]:
    base = root / relative
    if not base.exists():
        return []
    return sorted(base.rglob("*.rs"))


def exists_all(root: Path, paths: Iterable[str]) -> list[str]:
    return [path for path in paths if (root / path).exists()]


def sleigh_probes(repo: Path, ghidra: Path) -> list[Probe]:
    fission_files = rust_files(repo, "crates/fission-sleigh/src")
    ghidra_refs = exists_all(
        ghidra,
        [
            "Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/app/plugin/processors/sleigh/SleighLanguage.java",
            "Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/app/plugin/processors/sleigh/SleighParserContext.java",
            "Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/app/plugin/processors/sleigh/PcodeEmit.java",
            "Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/app/plugin/processors/sleigh/template/ConstructTpl.java",
            "Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/app/plugin/processors/sleigh/template/HandleTpl.java",
        ],
    )
    native_model = count_pattern(fission_files, r"\bSlaLanguage\b")
    construct_tpl = count_pattern(fission_files, r"\bConstructTpl\b")
    legacy_token = count_pattern(fission_files, r"\bCompiledTokenCursorPolicy\b|\bdecode_shared_token_fields\b")
    manual_handle = count_pattern(fission_files, r"\bfixed_handle_for_bound_operand\b|\bBoundOperand::Memory\b")
    no_export = count_pattern(fission_files, r"\bfallback_binding_for_no_export_subtable\b")
    compatibility = count_pattern(fission_files, r"\bCompatibilityLowered\b|\bNativeFission\b")
    mnemonic_kind = count_pattern(fission_files, r"\bclassify_display_construct_kind\b")

    return [
        Probe(
            "sleigh_native_model",
            "SleighLanguage -> SubtableSymbol -> DecisionNode -> Constructor",
            STATUS_PARTIAL if native_model else STATUS_NOT_STARTED,
            [
                f"Ghidra reference files found={len(ghidra_refs)}",
                f"Fission SlaLanguage mentions={native_model}",
                f"Fission ConstructTpl mentions={construct_tpl}",
            ],
            "Promote .sla native identity to generated artifact source of truth.",
        ),
        Probe(
            "sleigh_token_cursor",
            "ParserWalker token field traversal",
            STATUS_LEGACY_DEBT if legacy_token else STATUS_IMPLEMENTED,
            [f"legacy token/direct parser mentions={legacy_token}"],
            "Replace shared-token/direct parser debt with decoded .sla token and operand metadata; fail typed when absent.",
        ),
        Probe(
            "sleigh_handle_resolution",
            "HandleTpl.fix -> FixedHandle -> PcodeEmit",
            STATUS_LEGACY_DEBT if manual_handle or no_export else STATUS_IMPLEMENTED,
            [
                f"BoundOperand/manual handle mentions={manual_handle}",
                f"no-export fallback mentions={no_export}",
            ],
            "Remove BoundOperand-derived fixed handles from raw P-code success path after row-level audit shows exact exported handle coverage.",
        ),
        Probe(
            "sleigh_compatibility_sources",
            "ConstructTpl execution source",
            STATUS_LEGACY_DEBT if compatibility or mnemonic_kind else STATUS_IMPLEMENTED,
            [
                f"CompatibilityLowered/NativeFission mentions={compatibility}",
                f"mnemonic construct-kind classifier mentions={mnemonic_kind}",
            ],
            "Keep compatibility/display debt outside template execution and audit success rows for real .sla ConstructTpl source only.",
        ),
    ]


def loader_probes(repo: Path, ghidra: Path) -> list[Probe]:
    fission_files = rust_files(repo, "crates/fission-loader/src")
    ghidra_loader_paths = sorted(
        (ghidra / "Ghidra").rglob("*Loader.java")
    )
    pipeline = repo / "crates/fission-loader/src/loader/pipeline.rs"
    pipeline_text = read_text(pipeline) if pipeline.exists() else ""
    implemented = sorted(set(re.findall(r"DetectedFormat::([A-Za-z0-9_]+)", pipeline_text)))
    known_unsupported = sorted(set(re.findall(r"KnownUnsupportedLoaderFamily::([A-Za-z0-9_]+)", pipeline_text)))
    raw_loader = count_pattern(fission_files, r"\bBinaryLoader\b")
    analyzer_heuristics = count_pattern(fission_files, r"heuristic")

    return [
        Probe(
            "loader_family_matrix",
            "Loader.detect -> findSupportedLoadSpecs -> map -> symbols -> finalize",
            STATUS_PARTIAL,
            [
                f"Ghidra Loader.java files found={len(ghidra_loader_paths)}",
                f"Fission executable detected formats={','.join(implemented) or 'none'}",
                f"Fission known unsupported families={','.join(known_unsupported) or 'none'}",
            ],
            "Keep a documented implemented/known-unsupported matrix and route only executable formats to LoadedBinary.",
        ),
        Probe(
            "loader_raw_binary",
            "BinaryLoader raw blob",
            STATUS_TYPED_UNSUPPORTED if raw_loader else STATUS_NOT_STARTED,
            [f"BinaryLoader mentions={raw_loader}"],
            "Keep raw binary opt-in only; unknown bytes must remain UnsupportedFormat unless an explicit load hint is provided.",
        ),
        Probe(
            "loader_postload_analyzers",
            "Post-load enrichment outside format owner",
            STATUS_LEGACY_DEBT if analyzer_heuristics else STATUS_IMPLEMENTED,
            [f"post-load heuristic mentions={analyzer_heuristics}"],
            "Ensure Go/Rust/C++ enrichment output does not own format detection, load-spec selection, memory mapping, or default seeds.",
        ),
    ]


def fid_probes(repo: Path, ghidra: Path) -> list[Probe]:
    fission_files = rust_files(repo, "crates/fission-signatures/src")
    ghidra_refs = exists_all(
        ghidra,
        [
            "Ghidra/Framework/DB/src/main/java/db/DBHandle.java",
            "Ghidra/Framework/DB/src/main/java/db/PackedDBHandle.java",
            "Ghidra/Features/FunctionID/src/main/java/ghidra/feature/fid/db/FidDB.java",
            "Ghidra/Features/FunctionID/src/main/java/ghidra/feature/fid/service/FidService.java",
            "Ghidra/Features/FunctionID/src/main/java/ghidra/feature/fid/service/FidProgramSeeker.java",
            "Ghidra/Features/FunctionID/src/main/java/ghidra/feature/fid/hash/MessageDigestFidHasher.java",
        ],
    )
    raw_db = count_pattern(fission_files, r"\bRawDbHandle\b|\bread_table_records\b")
    packed_unsupported = count_pattern(fission_files, r"UnsupportedPackedFidDatabase")
    hash_unsupported = count_pattern(fission_files, r"UnsupportedFidHashInput")
    relation_mentions = count_pattern(fission_files, r"force_relation|relation")
    fid_program_seeker = count_pattern(fission_files, r"FidProgramSeeker")

    return [
        Probe(
            "fid_raw_dbhandle",
            "DBHandle -> Table -> DBRecord -> FidDB",
            STATUS_PARTIAL if raw_db else STATUS_NOT_STARTED,
            [
                f"Ghidra FID/DB reference files found={len(ghidra_refs)}",
                f"RawDbHandle/table reader mentions={raw_db}",
                f"UnsupportedPackedFidDatabase mentions={packed_unsupported}",
            ],
            "Extend raw DBHandle coverage only with exact record/page decoding; packed .fidb remains typed unsupported until implemented.",
        ),
        Probe(
            "fid_hash_and_match",
            "FidHasher -> FidMatcher -> FidProgramSeeker",
            STATUS_PARTIAL,
            [
                f"UnsupportedFidHashInput mentions={hash_unsupported}",
                f"relation metadata mentions={relation_mentions}",
                f"FidProgramSeeker mentions={fid_program_seeker}",
            ],
            "Integrate exact instruction-mask and relation context before promoting matches to StrongFid; missing inputs remain typed unsupported.",
        ),
    ]


def render_markdown(report: dict) -> str:
    lines = [
        "# Ghidra Parity Gap Audit",
        "",
        "This report is generated from static owner-chain probes. It is reporting-only and must not be used as semantic repair.",
        "",
        f"- Repo: `{report['repo_root']}`",
        f"- Ghidra reference: `{report['ghidra_root']}`",
        "",
    ]
    for group in ("sleigh", "loader", "fid"):
        lines.append(f"## {group.upper()}")
        lines.append("")
        lines.append("| Probe | Owner chain | Status | Evidence | Next action |")
        lines.append("|---|---|---|---|---|")
        for item in report[group]:
            evidence = "<br>".join(item["evidence"])
            lines.append(
                f"| `{item['name']}` | {item['owner_chain']} | `{item['status']}` | {evidence} | {item['next_action']} |"
            )
        lines.append("")
    return "\n".join(lines)


def build_report(repo: Path, ghidra: Path) -> dict:
    report = {
        "repo_root": str(repo),
        "ghidra_root": str(ghidra),
        "sleigh": [probe.__dict__ for probe in sleigh_probes(repo, ghidra)],
        "loader": [probe.__dict__ for probe in loader_probes(repo, ghidra)],
        "fid": [probe.__dict__ for probe in fid_probes(repo, ghidra)],
    }
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", default=".", help="Fission repository root")
    parser.add_argument(
        "--ghidra-source",
        default="vendor/ghidra/ghidra-Ghidra_12.0.4_build",
        help="Ghidra source/reference checkout",
    )
    parser.add_argument("--markdown", action="store_true", help="Emit Markdown instead of JSON")
    args = parser.parse_args()

    repo = Path(args.repo_root).resolve()
    ghidra = (repo / args.ghidra_source).resolve()
    report = build_report(repo, ghidra)
    if args.markdown:
        print(render_markdown(report))
    else:
        print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
