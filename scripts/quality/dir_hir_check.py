#!/usr/bin/env python3
"""DIR-vs-HIR structural check: does structuring look like it changed behavior?

DIR is the flattened, goto/label-based body `fission-pcode`'s structuring
stage receives as input (`fission_pcode::take_last_dir_snapshot`); HIR is
its final structured output (if/while/for). Both are exposed by
`fission_cli decomp --dir --json` (`code_dir`/`code_hir` fields, added
alongside the existing NIR/HIR layer output).

This does NOT interpret either side -- there is no P-code/AST evaluator
here, deliberately (an earlier attempt at that lived in a since-deleted
`fission-dir` Rust crate; the comparison approach here is intentionally the
much cheaper "does the rendered text look structurally consistent" kind of
check `golden_corpus_check.py` already uses for NIR/HIR, not a semantic
equivalence proof). It extracts a lightweight structural "signal" from each
side's rendered C-like text -- control-flow keywords, comparison/logic
operators, and integer constants, with identifier names stripped out (DIR
and HIR routinely use different names for the same value: raw scaffold
names like `param_10`/`rax` vs. promoted `param_1`) -- and flags functions
where HIR's signal is missing something DIR's had. A missing `if`/`return`/
comparison operator is a strong hint structuring dropped or collapsed a
real branch (the exact bug class this session found once already, by hand,
in an AArch64 function); missing arithmetic operators/constants are weaker
signals (normalize's own constant folding/dead-code elimination can
legitimately explain those) and are reported at lower severity.

This is a heuristic, not a proof -- a clean report is not a correctness
guarantee, and a flagged function is a candidate for a human to look at
(e.g. via `fission_cli decomp --addr <addr> --dir <binary>`), not an
automatic failure.

Confirmed (by hand, against real corpus output) recurring HIGH-severity
false positives from *legitimate* presentation transforms, not bugs --
expect these, don't chase them:
  - `while (1) { if (!cond) break; ... }` in DIR folded to
    `while (cond) { ... }` in HIR (loses the `if`/`break` tokens) --
    e.g. `count_bits` in the `control_flow` corpus family.
  - `if (cond) { return a; } return b;` in DIR folded to
    `return cond ? a : b;` in HIR (loses the `if`/`return` tokens) --
    e.g. `clamp` in the `control_flow` corpus family.
  - `if (k < x) ...` in DIR canonicalized to `if (x > k) ...` in HIR
    (comparison operand order flipped for readability, same operator
    family swapped `<` for `>`) -- e.g. `rc4_init` in the `crypto`
    corpus family. Only the operator token changes, not what's compared.
A real finding looks different from these: a *comparison operator or
constant* silently missing (not just a keyword folded into different C
surface syntax for the same condition, or an operator swapped for its
mirror while comparing the same two values), or a branch's `return`
value changing identity, not just its keyword disappearing into a
ternary.

This script's first real run (2026-07-19) caught exactly this class of
bug for real: `pre_c_init` (mingw CRT startup, present in every corpus
binary) had a `goto` into a labeled block that structuring's HIR
*printer*-facing presentation pass (`prune_unreachable_after_total_
return` in `fission-pcode/src/render/presentation/mod.rs`) discarded
because the block was laid out textually after an unconditional
`return` -- correct for genuinely dead code, wrong when a `goto`
elsewhere still targets it. Fixed by making that pass keep any trailing
label a function-wide `goto` still references. See PROJECT.md for the
full root-cause trace.

Workflow
--------
  cargo build -p fission-cli --profile quick-release
  python3 scripts/quality/dir_hir_check.py check
  python3 scripts/quality/dir_hir_check.py check --binaries control_flow_gcc_O0.exe
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_BENCHMARK_ROOT = REPO_ROOT.parent / "fission-benchmark"
CORPUS_SUBDIR = "corpus/dev/binaries/c"

# Same family list as golden_corpus_check.py's default, so the two scripts
# cover matching ground -- see that script's own comment for the rationale.
DEFAULT_BINARIES = [
    f"{family}_{variant}.exe"
    for family in (
        "advanced_patterns",
        "control_flow",
        "crypto",
        "data_structures",
        "math",
        "memory_layouts",
        "semantic_stress",
        "string_utils",
    )
    for variant in ("gcc_O0", "gcc_O2")
]

DEFAULT_LIMIT = 10
DEFAULT_TIMEOUT_MS = 20000

CONTROL_KEYWORDS = {
    "if", "else", "while", "for", "do", "return", "goto", "switch", "case",
    "default", "break", "continue",
}
COMPARISON_OPERATORS = {"==", "!=", "<=", ">=", "<", ">"}
_IDENT_RE = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
_OPERATOR_RE = re.compile(r"==|!=|<=|>=|<<|>>|&&|\|\||[+\-*/%&|^~!<>=]")
_CONST_RE = re.compile(r"\b0[xX][0-9a-fA-F]+\b|\b\d+\b")


def default_cli() -> Path:
    quick = REPO_ROOT / "target" / "quick-release" / "fission_cli"
    if quick.exists():
        return quick
    return REPO_ROOT / "target" / "release" / "fission_cli"


def resolve_cli(cli_arg: str | None) -> Path:
    path = Path(cli_arg) if cli_arg else default_cli()
    if not path.exists():
        print(f"error: fission_cli not found at {path}", file=sys.stderr)
        print(
            "  build it first: cargo build -p fission-cli --profile quick-release",
            file=sys.stderr,
        )
        sys.exit(1)
    return path


def _strip_comment_lines(code: str) -> str:
    """Drop `// ...` lines (the "Function: ..." header and the "DIR
    (pre-structuring)" section marker `decomp --dir` inserts) before
    extracting a structural signal -- comments aren't part of either side's
    actual computed logic."""
    return "\n".join(
        line for line in code.splitlines() if not line.strip().startswith("//")
    )


def structural_signal(code: str) -> Counter[str]:
    """A `Counter` of control-flow keywords, comparison/logic/arithmetic
    operators, and integer constants in `code`, with identifier names
    (variable/function names -- which legitimately differ between DIR and
    HIR) stripped out. See this module's own doc comment for the full
    rationale."""
    code = _strip_comment_lines(code)
    idents = _IDENT_RE.findall(code)
    keywords = [w for w in idents if w in CONTROL_KEYWORDS]
    operators = _OPERATOR_RE.findall(code)
    constants = _CONST_RE.findall(code)
    return Counter(keywords) + Counter(operators) + Counter(constants)


def diff_signals(dir_code: str, hir_code: str) -> tuple[Counter[str], Counter[str]]:
    """`(missing_in_hir, extra_in_hir)` -- tokens present in one side's
    signal but not (or fewer times) in the other's. `Counter` subtraction
    already drops non-positive results, so both are "strictly more of this
    token on the other side"."""
    dir_sig = structural_signal(dir_code)
    hir_sig = structural_signal(hir_code)
    return dir_sig - hir_sig, hir_sig - dir_sig


def severity_of(missing_in_hir: Counter[str]) -> str:
    """"high" if a control-flow keyword or comparison operator was lost --
    the strongest structural hint of an actually-dropped branch. "low" for
    everything else (arithmetic operator/constant count drift, which
    normalize's own legitimate constant-folding/dead-code passes can
    explain)."""
    for token in missing_in_hir:
        if token in CONTROL_KEYWORDS or token in COMPARISON_OPERATORS:
            return "high"
    return "low" if missing_in_hir else "none"


def decompile_all_with_dir(
    cli: Path, binary: Path, limit: int, timeout_ms: int
) -> list[dict[str, Any]]:
    proc = subprocess.run(
        [
            str(cli),
            "decomp",
            "--all",
            "--limit",
            str(limit),
            "--timeout-ms",
            str(timeout_ms),
            "--json",
            "--layer",
            "both",
            "--dir",
            str(binary),
        ],
        capture_output=True,
        text=True,
        cwd=str(REPO_ROOT),
    )
    if proc.returncode != 0:
        print(f"error: decomp --all --dir failed for {binary.name}", file=sys.stderr)
        print(proc.stderr[-2000:], file=sys.stderr)
        sys.exit(1)
    # `--json --all` without `--benchmark` returns a bare JSON array (no
    # envelope) -- unlike golden_corpus_check.py's `decompile_all`, this
    # script doesn't need the `--benchmark` timing wrapper.
    return json.loads(proc.stdout)


def cmd_check(args: argparse.Namespace) -> int:
    cli = resolve_cli(args.cli)
    benchmark_root = Path(args.benchmark_root)
    binaries = args.binaries or DEFAULT_BINARIES
    corpus_dir = benchmark_root / CORPUS_SUBDIR

    print(f"using CLI: {cli}", file=sys.stderr)
    print(f"checking {len(binaries)} binaries...", file=sys.stderr)

    high: list[str] = []
    low: list[str] = []
    unsupported = 0
    checked = 0

    for name in binaries:
        binary_path = corpus_dir / name
        if not binary_path.exists():
            print(f"warning: binary not found, skipping: {binary_path}", file=sys.stderr)
            continue
        functions = decompile_all_with_dir(cli, binary_path, args.limit, args.timeout_ms)
        for fn in functions:
            dir_code = fn.get("code_dir")
            hir_code = fn.get("code_hir")
            if not dir_code or not hir_code:
                unsupported += 1
                continue
            checked += 1
            missing_in_hir, extra_in_hir = diff_signals(dir_code, hir_code)
            sev = severity_of(missing_in_hir)
            if sev == "none":
                continue
            key = f"{name}::{fn['name']}@{fn['address']}"
            detail = (
                f"missing_in_hir={dict(missing_in_hir)} extra_in_hir={dict(extra_in_hir)}"
            )
            if sev == "high":
                high.append(f"{key} -- {detail}")
            else:
                low.append(f"{key} -- {detail}")

    print(
        f"checked {checked} functions ({unsupported} skipped: no code_dir/code_hir)",
        file=sys.stderr,
    )

    if high:
        print(f"\nHIGH severity ({len(high)}) -- control-flow/comparison token lost, worth a human look:\n", file=sys.stderr)
        for h in high:
            print(f"  - {h}", file=sys.stderr)
    if low and args.show_low:
        print(f"\nLOW severity ({len(low)}) -- arithmetic/constant drift, often legitimate:\n", file=sys.stderr)
        for l in low:
            print(f"  - {l}", file=sys.stderr)
    elif low:
        print(f"\n({len(low)} low-severity diffs suppressed -- pass --show-low to see them)", file=sys.stderr)

    if high:
        print(
            f"\nFAIL: {len(high)} function(s) with a high-severity DIR/HIR structural "
            "diff -- re-run the specific address with `decomp --addr <addr> --dir` "
            "and read both sides before concluding it's a real bug (this is a "
            "heuristic, not a proof).",
            file=sys.stderr,
        )
        return 1

    print("\nPASS: no high-severity DIR/HIR structural diffs.")
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="command", required=True)

    p_check = sub.add_parser("check", help="Compare DIR vs HIR structural signal across the corpus")
    p_check.add_argument("--cli", default=None, help="Path to fission_cli (default: quick-release, falling back to release)")
    p_check.add_argument(
        "--benchmark-root",
        default=str(DEFAULT_BENCHMARK_ROOT),
        help=f"fission-benchmark repo root (default: {DEFAULT_BENCHMARK_ROOT})",
    )
    p_check.add_argument("--binaries", nargs="*", default=None, help="Override the binary list")
    p_check.add_argument("--limit", type=int, default=DEFAULT_LIMIT, help="Max functions per binary")
    p_check.add_argument("--timeout-ms", type=int, default=DEFAULT_TIMEOUT_MS, help="Per-function decomp timeout")
    p_check.add_argument("--show-low", action="store_true", help="Also print low-severity (arithmetic/constant) diffs")
    p_check.set_defaults(func=cmd_check)

    return p


def main() -> int:
    args = build_parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
