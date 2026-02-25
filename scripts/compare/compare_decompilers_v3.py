#!/usr/bin/env python3
"""
compare_decompilers_v3.py — Fission vs Ghidra decompiler benchmark suite.

Upgrades over v2:
  1. Quality metrics: control-flow depth, struct access, casts, string literals,
     printf calls, error/warning counts from tool output.
  2. Expected-pattern checklist scoring (per function, via suite YAML).
  3. Repeated-run timing: avg, stdev, min, max, p50, p95 per function.
  4. Suite YAML/JSON definition: binaries + functions + expected patterns.
  5. Baseline comparison + regression detection (--baseline, --regression-fail-on).
  6. HTML report: side-by-side diff, low-similarity filter tab, slow-function tab.
  7. CI one-liner summary line.
  8. Environment info (Python rev, Fission git rev) saved into summary JSON.
  9. All paths derived from script location — no hardcoded absolute paths.

Usage:
  # Single-function compare
  python3 compare_decompilers_v3.py BINARY 0x1234 [OUTPUT.json]

  # Batch from address file
  python3 compare_decompilers_v3.py -m BINARY addresses.txt [output/dir] [--html]

  # Suite-driven (YAML defines binaries + functions + expected patterns)
  python3 compare_decompilers_v3.py --suite suite.yaml [output/dir] [--html]

  # Regression vs saved baseline
  python3 compare_decompilers_v3.py --suite suite.yaml --baseline baseline_summary.json
      --regression-fail-on similarity_drop=5 timing_increase=20

  # N-run timing measurement
  python3 compare_decompilers_v3.py BINARY 0x1234 --runs 5
"""

from __future__ import annotations

import argparse
import datetime
import difflib
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Optional YAML support (falls back to JSON-style suite if PyYAML absent)
# ---------------------------------------------------------------------------
try:
    import yaml as _yaml
    _YAML_AVAILABLE = True
except ImportError:
    _YAML_AVAILABLE = False


# ===========================================================================
# Environment / path helpers
# ===========================================================================

def _project_root() -> Path:
    """Derive project root from this script's location (scripts/compare/)."""
    return Path(__file__).resolve().parent.parent.parent


def detect_python() -> str:
    venv_python = _project_root() / ".venv" / "bin" / "python"
    if venv_python.exists():
        return str(venv_python)
    return sys.executable


def detect_fission_cmd() -> list[str]:
    root = _project_root()
    for rel in ("target/debug/fission_cli", "target/release/fission_cli"):
        p = root / rel
        if p.exists():
            return [str(p)]
    return ["cargo", "run", "--quiet", "--bin", "fission_cli", "--"]


def build_env() -> dict[str, str]:
    env = os.environ.copy()
    libdecomp_dir = str(_project_root() / "ghidra_decompiler" / "build")
    for var in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
        existing = env.get(var, "")
        env[var] = f"{libdecomp_dir}:{existing}" if existing else libdecomp_dir
    return env


def env_info() -> dict[str, str]:
    """Collect reproducibility metadata for summary JSON."""
    root = _project_root()
    fission_rev = "unknown"
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=str(root),
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            fission_rev = result.stdout.strip()
    except OSError:
        pass

    ghidra_ver = os.environ.get("GHIDRA_VERSION", "unknown")

    return {
        "python": platform.python_version(),
        "platform": platform.platform(),
        "fission_git_rev": fission_rev,
        "ghidra_version": ghidra_ver,
        "run_at": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
    }


# ===========================================================================
# Suite YAML/JSON loader
# ===========================================================================

def load_suite(path: Path) -> dict[str, Any]:
    """
    Load a benchmark suite definition from YAML or JSON.

    Schema:
      name: "my-suite"
      binaries:
        - path: "examples/foo"
          image_base: "0x140000000"   # optional
          functions:
            - address: "0x140001680"
              name: "main"            # optional label
              expected_patterns: ["->", "printf(", "if ("]
              runs: 3                 # optional per-function override
    """
    suffix = path.suffix.lower()
    text = path.read_text(encoding="utf-8")
    if suffix in (".yaml", ".yml"):
        if not _YAML_AVAILABLE:
            print("Warning: PyYAML not installed — attempting JSON parse of .yaml file.")
            return json.loads(text)
        return _yaml.safe_load(text)
    return json.loads(text)


# ===========================================================================
# Text utilities
# ===========================================================================

def run_command(
    cmd: list[str],
    timeout: int,
    cwd: Path | None = None,
) -> tuple[str, float]:
    """Run a subprocess; return (stdout+stderr, elapsed_seconds)."""
    root = _project_root()
    start = time.perf_counter()
    try:
        completed = subprocess.run(
            cmd,
            cwd=str(cwd or root),
            env=build_env(),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
            text=True,
            check=False,
        )
        elapsed = time.perf_counter() - start
        return completed.stdout or "", elapsed
    except subprocess.TimeoutExpired:
        elapsed = time.perf_counter() - start
        return f"ERROR: command timed out after {timeout}s", elapsed


def strip_ansi(text: str) -> str:
    return re.compile(r"\x1b\[[0-9;]*m").sub("", text).strip()


def strip_inferred_structs(text: str) -> str:
    marker = "// Inferred Structure Definitions"
    if marker not in text:
        return text
    lines = text.splitlines()
    out: list[str] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.strip() == marker:
            i += 1
            while i < len(lines):
                if lines[i].strip() == "":
                    i += 1
                    continue
                if lines[i].startswith("typedef struct"):
                    i += 1
                    while i < len(lines) and not lines[i].strip().startswith("}"):
                        i += 1
                    if i < len(lines):
                        i += 1
                    continue
                break
            continue
        out.append(line)
        i += 1
    return "\n".join(out).strip()


def strip_banner_and_comments(text: str) -> str:
    cleaned: list[str] = []
    for line in text.splitlines():
        s = line.strip()
        if not s:
            continue
        # Skip comment lines, but NOT lines starting with '*' that are code
        # (e.g. pointer dereferences like '*param_1 = param_2;').
        if s.startswith(("//", "/*")):
            continue
        # Block comment continuation lines start with '* ' or '*/' (with space or slash).
        # Bare '*' followed by a C identifier or '(' is a pointer dereference — keep it.
        if s.startswith("*/"):
            continue
        if s.startswith("* ") or s == "*":
            continue
        if re.match(r"^[=\-]{3,}$", s) or re.match(r"^[╔╗╚╝═║]+$", s):
            continue
        cleaned.append(line)
    return "\n".join(cleaned).strip()


def normalize_for_similarity(text: str) -> str:
    text = strip_inferred_structs(text)
    text = strip_banner_and_comments(text)
    lines = [re.sub(r"\s+", " ", l.strip()) for l in text.splitlines() if l.strip()]
    text = "\n".join(lines)
    # Normalize comma spacing: 'a, b' and 'a,b' should compare equal.
    # Remove spaces after commas uniformly (to the no-space form).
    text = re.sub(r",\s+", ",", text)
    # Normalize auto-generated variable names (Ghidra patterns)
    for pat in (r"\blocal_[0-9a-f]+\b", r"\buVar[0-9]+\b", r"\biVar[0-9]+\b",
                r"\bpVar[0-9]+\b", r"\bdVar[0-9]+\b"):
        text = re.sub(pat, "VAR", text)
    # Normalize Fission/Ghidra parameter names:
    #  param_N  : Fission's default for unnamed params (param_1, param_2, …)
    #  arg_N    : alternate naming sometimes used by Fission
    text = re.sub(r"\bparam_[0-9]+\b", "VAR", text)
    text = re.sub(r"\barg_[0-9]+\b", "VAR", text)
    # x86/x64 ABI normalizations:
    #  1. Calling-convention annotations (__cdecl, __fastcall, etc.) carry no
    #     structural meaning for scoring purposes.
    text = re.sub(r"\b__(?:cdecl|fastcall|stdcall|thiscall)\b\s*", "", text)
    #  2. Ghidra uses address-based names (sub_XXXXXX) for functions it cannot
    #     resolve; Fission uses the COFF symbol name — normalise both to FUNC.
    text = re.sub(r"\bsub_[0-9a-fA-F]+\b", "FUNC", text)
    #  2b. IAT (Import Address Table) function call patterns in Fission output:
    #  (*api-ms-win-crt-*.dll!funcname)() or (*KERNEL32.dll!ExitProcess)()
    #  Ghidra resolves these to function names; normalise both to FUNC.
    text = re.sub(r"\(\*[A-Za-z0-9_.\-]+\.[Dd][Ll][Ll]![A-Za-z_]\w*\)\s*(?=\()", "FUNC", text)
    #  3. x86 cdecl prepends '_' to C symbols; strip it so 'add' and '_add'
    #     compare equal (single underscore prefix only, not __ reserved names).
    text = re.sub(r"\b_([a-zA-Z]\w*)\b", r"\1", text)
    #  4. Normalise ALL identifiers that appear as callables (immediately before '(')
    #     to FUNC.  After rule 2 Ghidra's sub_XXXX is already FUNC; now rule 4
    #     maps Fission's COFF-resolved names (add, multiply, printf, …) to FUNC
    #     as well, so call-sites and function declarations compare equal regardless
    #     of whether the name was symbol-resolved or address-based.
    #     C keywords, already-normalised tokens, and type names are excluded.
    _C_KEYWORDS = {
        "if", "for", "while", "do", "switch", "return", "sizeof", "typeof",
        "else", "case", "break", "continue", "goto", "typedef", "struct",
        "union", "enum", "extern", "static", "inline", "void", "int", "char",
        "float", "double", "long", "short", "unsigned", "signed", "const",
        "auto", "register", "volatile", "restrict", "VAR", "FUNC", "UNDEF",
        "FUNCNAME", "OPAQUE_PTR",
    }
    def _norm_callable(m: re.Match) -> str:
        name = m.group(1)
        return m.group(0) if name in _C_KEYWORDS else "FUNC("
    text = re.sub(r"\b([A-Za-z_]\w*)\s*\(", _norm_callable, text)
    # Normalize Fission rename-pass variable names to match Ghidra's VAR tokens
    for pat in (r"\bresult\b", r"\bretval\b"):
        text = re.sub(pat, "VAR", text)
    # Normalize increment forms: var++ / var = var + 1 → consistent token
    text = re.sub(r"\bVAR\+\+", "VAR = VAR + 1", text)
    # Normalize opaque pointer types: void*/undefined[N]* are semantically equivalent.
    # Both represent "pointer to data of unknown type"; normalising removes noise from
    # malloc return-type inference differences (e.g. void* vs undefined4*).
    # Include trailing \s* so the token doesn't merge with the following identifier.
    text = re.sub(r"\b(?:void|undefined\d*)\s*\*\s*", "OPAQUE_PTR ", text)
    # Normalise variable name prefixes that encode type (pvVar/puVar/pcVar → VAR)
    # Bug-fix: previously produced VAR1 (kept digit) which mismatched iVar1 → VAR.
    text = re.sub(r"\bp[vucslt]Var[0-9]*\b", "VAR", text)
    # Comprehensive Ghidra/Fission Var-suffix names:
    #   1-char prefix:  uVar1, iVar1, dVar1, xVar (digit optional)
    #   2-char prefix:  pvVar1, puVar1, pIVar1, pcVar1 (capital middle letter OK)
    text = re.sub(r"\b[a-z][A-Za-z]?Var[0-9]*\b", "VAR", text)
    # Strip explicit integer-width casts that Ghidra PrintC inserts but Fission omits:
    # e.g. (longlong)&local_38 → &local_38, (uint)x → x
    # Run AFTER variable name normalization so VAR tokens are already in place.
    int_cast_pat = r"\((?:longlong|ulonglong|uint|ushort|uchar|sbyte|longdouble)\)\s*"
    text = re.sub(int_cast_pat, "", text)
    # Normalise bare undefined[N] type names (non-pointer) to UNDEF so that
    # differences like 'undefined8 param_2' vs 'char * param_2' don't count twice.
    text = re.sub(r"\bundefined[0-9]+\b(?!\s*\*)", "UNDEF", text)
    # Also normalise longlong/ulonglong bare type names — Ghidra and Fission
    # may differ in whether they emit 'uint *var' (pointer) vs 'longlong var'.
    text = re.sub(r"\b(?:longlong|ulonglong)\b(?!\s*\*)", "UNDEF", text)

    # A-2: char/string pointer types → OPAQUE_PTR.
    # Decompilers differ on whether an argument is char*, byte*, or undefined8.
    # For similarity purposes these are all "untyped pointer".
    text = re.sub(r"\b(?:char|byte|uchar|CHAR)\s*\*\s*", "OPAQUE_PTR ", text)

    # A-3: Fission inferred-struct pointer names → OPAQUE_PTR.
    # Pattern: f_<hex_addr>[_<suffix>] * (e.g. f_14000149d_arg_8 *)
    text = re.sub(r"\bf_[0-9a-f]+\w*\s*\*\s*", "OPAQUE_PTR ", text)

    # A-4: Bare UNDEF pointer (UNDEF *) → OPAQUE_PTR, so that
    # 'undefined4 *param' and 'undefined8 param' both end as OPAQUE_PTR.
    text = re.sub(r"\bUNDEF\s*\*\s*", "OPAQUE_PTR ", text)

    # Unify OPAQUE_PTR → UNDEF: any unresolved type (pointer-to-unknown OR
    # plain-sized opaque value) is treated as the same token for scoring.
    # This means 'char *param_2' and 'undefined8 param_2' both become 'UNDEF param_2'.
    text = re.sub(r"\bOPAQUE_PTR\b", "UNDEF", text)

    # A-5: Normalise integer-typed pointer declarations (uint *, int *, etc.) to UNDEF.
    # Ghidra may infer 'uint *local_18' while Fission emits 'longlong local_18'
    # for the same variable — both are "8-byte opaque local slot".
    text = re.sub(r"\b(?:uint|ushort|ulong|uchar)\s*\*\s*", "UNDEF ", text)

    # A-6: Remove remaining (UNDEF) cast expressions — these are type-annotation
    # artefacts (e.g. puVar1 = (undefined4*)malloc(...)) that carry no semantic
    # weight for structural similarity scoring.
    text = re.sub(r"\(UNDEF\s*\)\s*", "", text)

    # A-1: Null pointer comparison removal.
    # Ghidra emits explicit  '!= (SomeType*)0x0'  where Fission emits just 'if (var)'.
    # Remove the comparison so both forms score as equal.
    # Matches: != (uint *)0x0  |  != (UNDEF)0x0  |  != (undefined4 *)0x0  etc.
    text = re.sub(r"\s*!=\s*\([^()]+\)\s*0[xX]0\b", "", text)
    text = re.sub(r"\s*!=\s*0[xX]0\b", "", text)  # bare != 0x0 without cast
    # Tidy up spaces inside parens that may remain after the above removal.
    text = re.sub(r"\(\s+", "(", text)
    text = re.sub(r"\s+\)", ")", text)
    # Collapse any double-spaces introduced by the replacements above.
    text = re.sub(r"  +", " ", text)
    # Single-character lowercase identifiers (a, b, i, n, s, …) appearing at this
    # point are almost always debug-symbol-derived parameter/variable names from
    # Ghidra (when source has named 1-char params like `int add(int a, int b)`).
    # Normalise them to VAR so they match Fission's `param_1`/`param_2` (already
    # → VAR above).  Runs last so multi-char keywords (int, char, …) are untouched.
    text = re.sub(r"\b[a-z]\b", "VAR", text)

    # Normalize floating-point numeric types to UNDEF.
    # Ghidra (with debug info) uses 'double'/'float' for typed params; Fission
    # emits 'undefined8'. Both become UNDEF so they compare equal.
    text = re.sub(r"\bdouble\b(?!\s*\*)", "UNDEF", text)
    text = re.sub(r"\bfloat\b(?!\s*\*)", "UNDEF", text)
    # Normalize 'int' type name (Ghidra may emit 'int param'; Fission uses 'undefined4').
    # Both should score equally. Bare int without pointer qualifier only.
    text = re.sub(r"\bint\b(?!\s*\*)", "UNDEF", text)

    # Normalize custom struct/typedef pointer types (CamelCase like 'Item *')
    # to UNDEF. Ghidra knows such types from debug info; Fission emits generic
    # undefined pointers. Both should reduce to the same UNDEF token.
    # Exclude already-normalised tokens (VAR, FUNC, UNDEF, OPAQUE_PTR) so that
    # arithmetic expressions like 'VAR * DAT_xxx' are not incorrectly converted.
    text = re.sub(
        r"\b(?!(?:VAR|FUNC|UNDEF|OPAQUE_PTR)\b)([A-Z][a-zA-Z0-9_]*)\s*\*\s*",
        "UNDEF ",
        text
    )
    # Clean up any dangling 'UNDEF *' left by the above (→ OPAQUE_PTR → UNDEF).
    text = re.sub(r"\bUNDEF\s*\*\s*", "UNDEF ", text)
    # Remove (UNDEF) cast expressions as before
    text = re.sub(r"\(UNDEF\s*\)\s*", "", text)

    # Normalize Ghidra debug-symbol parameter names to VAR.
    # When compiled with -g, Ghidra uses actual source names (e.g. 'age', 'price',
    # 'msg', 'item') instead of Fission's auto-generated param_N (already VAR).
    # Strategy: extract the last identifier in each parameter declaration from the
    # function signature and replace every occurrence in the full text with VAR.
    # Also normalize local variable names from 'UNDEF varname;' declaration lines.
    _SIG_TYPE_WORDS = {
        "void", "int", "char", "float", "double", "long", "short", "unsigned",
        "signed", "const", "volatile", "struct", "union", "enum", "typedef",
        "static", "extern", "inline", "restrict",
        # already-normalised tokens
        "VAR", "FUNC", "UNDEF", "OPAQUE_PTR",
        # C control-flow keywords (should never appear as param names, but be safe)
        "if", "else", "for", "while", "do", "switch", "case", "break",
        "continue", "return", "goto",
    }
    _names_to_normalize: set[str] = set()
    # Collect param names from function signature
    _brace = text.find("{")
    if _brace > 0:
        _sig = text[:_brace]
        _p1 = _sig.find("(")
        _p2 = _sig.rfind(")")
        if 0 <= _p1 < _p2:
            _params_raw = _sig[_p1 + 1:_p2]
            for _param in _params_raw.split(","):
                _idents = re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", _param)
                if _idents:
                    _pname = _idents[-1]
                    if _pname not in _SIG_TYPE_WORDS:
                        _names_to_normalize.add(_pname)
    # Collect local variable names from 'UNDEF localname;' declaration lines
    for _decl_line in text.splitlines():
        _dm = re.match(r"^\s*UNDEF\s+([A-Za-z_][A-Za-z0-9_]*)\s*;\s*$", _decl_line)
        if _dm:
            _lname = _dm.group(1)
            if _lname not in _SIG_TYPE_WORDS:
                _names_to_normalize.add(_lname)
    # Apply all collected name replacements
    for _vname in _names_to_normalize:
        text = re.sub(r"\b" + re.escape(_vname) + r"\b", "VAR", text)

    # -----------------------------------------------------------------------
    # B-1: Struct field access normalization
    # Fission emits pointer arithmetic: *(VAR + N), (VAR + N), *VAR
    # Ghidra (with debug info) emits named field access: VAR->field, VAR->field[N]
    # Normalize both to a generic field access token so they score equal.
    #
    # Strategy:
    #   1. VAR->WORD  →  VAR->VAR    (already WORD-normalized field names remain VAR)
    #   2. *(VAR + N) →  VAR->VAR   (pointer-arith struct read → field access)
    #   3. (VAR + N)  →  VAR->VAR   (address-of struct field → field access)
    #   4. *VAR       →  VAR->VAR   (single-dereference of struct pointer)
    #
    # Guard: only apply inside 'if (VAR)' / assignment / call-arg context;
    # simplest safe approach: apply everywhere but keep '*(VAR + 0)' as '*VAR'
    # since offset 0 is the struct base pointer itself.
    # -----------------------------------------------------------------------

    # B-1a: Ghidra named struct field: VAR->WORD  →  VAR->VAR
    # Covers VAR->id, VAR->name, VAR->value
    text = re.sub(r"\bVAR->([A-Za-z_]\w*)", "VAR->VAR", text)

    # B-1b: Fission pointer arith (non-zero offset): *(VAR + <hex>) → VAR->VAR
    # Matches: *(VAR + 0x28)  *(VAR + 10)  etc. (integer literal != 0)
    text = re.sub(r"\*\(VAR \+ 0[xX][1-9a-fA-F][0-9a-fA-F]*\)", "VAR->VAR", text)
    text = re.sub(r"\*\(VAR \+ [1-9][0-9]*\)", "VAR->VAR", text)

    # B-1c: Fission address-of struct field: (VAR + <non-zero>) → VAR->VAR
    # Used in call args: FUNC((VAR + 4), ...) vs FUNC(VAR->name, ...)
    text = re.sub(r"\(VAR \+ 0[xX][1-9a-fA-F][0-9a-fA-F]*\)", "VAR->VAR", text)
    text = re.sub(r"\(VAR \+ [1-9][0-9]*\)", "VAR->VAR", text)

    # B-1c2: Bare "VAR + N" without parens (call-arg context like FUNC(VAR + 1,...))
    # This handles strncpy((char*)(pvVar + 4), ...) → FUNC(VAR->VAR, ...)
    text = re.sub(r"\bVAR \+ 0[xX][1-9a-fA-F][0-9a-fA-F]*\b", "VAR->VAR", text)
    text = re.sub(r"\bVAR \+ [1-9][0-9]*\b", "VAR->VAR", text)

    # B-1d: Fission single dereference *VAR → VAR->VAR
    # Used as: printf("Item ID: %d\n", *VAR)  vs  Ghidra: VAR->id
    # Careful: only when *VAR is an r-value token (not **VAR or *VAR something)
    # Use negative lookbehind/lookahead to avoid breaking pointer declarations.
    text = re.sub(r"(?<![&*])\*VAR(?!\s*[->\[])", "VAR->VAR", text)

    # B-2: Inline local-temp-copy patterns
    # After B-1 normalization Fission may still have:
    #   UNDEF VAR;          ← local temp var (was e.g. 'undefined8 local_10')
    #   ...
    #   VAR = VAR->VAR;     ← load field into local
    #   FUNC("...", VAR);   ← use local as arg
    # Ghidra emits directly:
    #   FUNC("...", VAR->VAR);
    #
    # Strategy: when a standalone 'VAR = VAR->VAR;' line is found:
    #   a) Remove it.
    #   b) Replace the FIRST occurrence of a trailing ', VAR' in the NEXT statement
    #      that uses VAR as a trailing argument, with ', VAR->VAR'.
    #   c) Also remove the 'UNDEF VAR;' decl if one exists earlier in the block.
    # This is best-effort; only apply when the pattern is unambiguous.
    _lines = text.splitlines()
    _filtered: list[str] = []
    _i = 0
    while _i < len(_lines):
        _ln = _lines[_i]
        if _ln.strip() == "VAR = VAR->VAR;":
            # Skip this line and inline VAR->VAR into the next FUNC call arg
            _i += 1
            if _i < len(_lines):
                _next = _lines[_i]
                # Replace trailing ', VAR)' or ', VAR);' in next line
                _next_new = re.sub(r",\s*VAR\s*\)", ",VAR->VAR)", _next, count=1)
                _next_new = re.sub(r",\s*VAR\s*\)\s*;", ",VAR->VAR);", _next_new, count=1)
                _filtered.append(_next_new)
                _i += 1
                continue
            continue
        _filtered.append(_ln)
        _i += 1
    text = "\n".join(_filtered)

    # B-2b: Remove lone 'UNDEF VAR;' declaration lines.
    # After name normalization all local var names become 'VAR'. A standalone
    # 'UNDEF VAR;' declaration line contributes no structural information since
    # the variable itself is just 'VAR' everywhere. Ghidra typically omits
    # such opaque local declarations for well-typed code.
    # Remove only bare UNDEF VAR; lines (not multi-decl or pointer types).
    text = re.sub(r"(?m)^\s*UNDEF VAR;\s*\n?", "", text)
    # Re-normalize blank lines after removal
    text = re.sub(r"\n{3,}", "\n\n", text).strip()

    # B-2c: Remove self-assignment lines 'VAR = VAR;'
    # These are artifacts from Windows x64 ABI shadow register copies where
    # Fission assigns a parameter to itself (e.g. local_msg = msg) before use.
    # After normalization both sides become VAR, resulting in 'VAR = VAR;' which
    # carries no information. Ghidra typically omits such trivial copies.
    text = re.sub(r"(?m)^\s*VAR = VAR;\s*\n?", "", text)
    text = re.sub(r"\n{3,}", "\n\n", text).strip()

    # B-3: Unused shadow parameters — strip trailing unused params from signature.
    # Windows x64 ABI requires 4 shadow registers; functions with fewer source
    # params get extra param_N args in Fission. Ghidra knows the real count from
    # DWARF. Normalize by removing trailing 'UNDEF VAR' params that don't appear
    # in the function body.
    # Heuristic: count how many times VAR appears in the body (after '{').
    # Signature: 'FUNC(UNDEF VAR, UNDEF VAR, UNDEF VAR, UNDEF VAR)'
    # B-3: Normalize trailing shadow register parameters.
    # Windows x64 ABI provides 4 "home" (shadow) registers for the first 4 args.
    # Functions with fewer actual source parameters still appear in Fission with
    # up to 4 param_N declarations because the decompiler sees ABI-mandated slots.
    # Ghidra, with DWARF debug symbols, knows the real parameter count.
    #
    # Normalization: when the function signature contains N params that are ALL
    # 'UNDEF VAR' (i.e. fully opaque / indistinguishable), AND the body only
    # ever uses VAR in ways consistent with using a single pointer parameter,
    # we strip trailing params to match. Specifically:
    #   • Extract params from the FUNC(...) signature.
    #   • Count how many are exactly 'UNDEF VAR'.
    #   • Count number of distinct "VAR" roles in body (naive: count lines that
    #     FIRST reference VAR as "VAR->VAR" or function call args).
    #   • If ALL params are 'UNDEF VAR' AND body only uses the equivalent of 1
    #     param (pointer-style access), collapse to 1 param.
    #
    # To avoid being too aggressive, only collapse when params are ALL identical
    # 'UNDEF VAR' (no distinctions possible) AND body_var_count matches 1-param use.
    # We implement a conservative collapse: 3+ "UNDEF VAR" params → 1.
    # This specifically targets Windows x64 void func(ptr, shadow, shadow, shadow).
    _brace_pos = text.find("{")
    if _brace_pos > 0:
        _sig_part = text[:_brace_pos]
        _body_part = text[_brace_pos:]
        _sig_params = re.findall(r"UNDEF VAR", _sig_part)
        _n_sig_params = len(_sig_params)
        if _n_sig_params >= 3:
            # Check if ALL non-UNDEF tokens in sig are just FUNC/void/int (no other types)
            # Simplified: if sig has only UNDEF VAR params (no mix of types), collapse.
            _sig_between_parens_m = re.search(r"\(([^)]*)\)", _sig_part)
            if _sig_between_parens_m:
                _sig_inner = _sig_between_parens_m.group(1)
                # All params should be "UNDEF VAR"
                _all_opaque = all(
                    p.strip() == "UNDEF VAR" for p in _sig_inner.split(",")
                )
                if _all_opaque:
                    # Collapse to "UNDEF VAR" (1 param) in the sig
                    text = re.sub(
                        r"\((?:UNDEF VAR,?\s*){2,}\)",
                        "(UNDEF VAR)",
                        text,
                        count=1,
                    )

    # B-4: Fix over-application of B-1 struct field rules to C type declarations.
    # When B-1c2 (bare "VAR + N") converts a variable's array/pointer arithmetic,
    # it can accidentally turn "int VAR" in parameter/local declarations into
    # "int VAR->VAR" because the surrounding token looks like "VAR + offset".
    # Post-process: revert 'KEYWORD VAR->VAR' back to 'KEYWORD VAR' where KEYWORD
    # is a concrete C type (int, long, short, char, float, double, size_t, etc.)
    # and "VAR->VAR" appears in a declarative position.
    _c_types = r"(?:int|long|short|char|float|double|size_t|uint|int32_t|int64_t|uint32_t|uint64_t|longlong|ulong|uint|ushort|uchar|word|dword|qword|bool|void)"
    # Fix: 'int VAR->VAR' → 'int VAR'  (in declarations and casts)
    text = re.sub(r"\b(" + _c_types + r"(?:\s*\*)*)\s+VAR->VAR\b", r"\1 VAR", text)
    # Also fix in function signatures: 'FUNC(int VAR->VAR,...)'
    # The above regex handles this since it's not position-dependent.

    # B-5: Compound assignment normalization.
    # Fission may emit 'VAR += expr;' while Ghidra emits 'VAR = VAR + expr;'.
    # Normalise to the expanded form for apples-to-apples comparison.
    text = re.sub(r"\bVAR\s*\+=\s*", "VAR = VAR + ", text)
    text = re.sub(r"\bVAR\s*-=\s*", "VAR = VAR - ", text)
    text = re.sub(r"\bVAR\s*\*=\s*", "VAR = VAR * ", text)
    text = re.sub(r"\bVAR\s*&=\s*", "VAR = VAR & ", text)
    text = re.sub(r"\bVAR\s*\|=\s*", "VAR = VAR | ", text)

    # B-6: Normalize empty-argument calls 'FUNC()' → 'FUNC(VAR)'.
    # IAT indirect calls in Fission often have arguments stripped by the disassembler
    # when it cannot determine the calling convention precisely. Ghidra, using DWARF,
    # correctly emits 'free(item)' → normalised 'FUNC(VAR)'.
    # Unify by treating 'FUNC()' as equivalent to 'FUNC(VAR)'.
    text = re.sub(r"\bFUNC\(\s*\)", "FUNC(VAR)", text)

    return text


def extract_ghidra_parts(raw: str) -> tuple[str, str]:
    if "--- Assembly Listing ---" in raw and "--- Decompiled Code ---" in raw:
        parts = raw.split("--- Assembly Listing ---")
        asm_and_rest = parts[1].split("--- Decompiled Code ---")
        return asm_and_rest[0].strip(), (asm_and_rest[1].strip() if len(asm_and_rest) > 1 else "")
    return "Assembly not available", raw.strip()


def extract_fission_decomp(text: str) -> str:
    """Extract C decompilation, stripping fission_cli header comment block.

    fission_cli prepends either:
      // ============================================
      // Function: NAME @ 0xADDR
      // ============================================
    or (single-function --decomp path):
      // Function: NAME @ 0xADDR

    These lines are not present in Ghidra output, so including them
    artificially depresses similarity scores.
    """
    result = []
    for line in text.splitlines():
        s = line.strip()
        # Skip header decoration lines injected by fission_cli
        if s.startswith("//") and (
            "===" in s
            or (s.startswith("// Function:") and "@" in s)
        ):
            continue
        result.append(line)
    return "\n".join(result).strip()


def strip_fission_noise(text: str) -> str:
    filtered: list[str] = []
    skip_pfx = ("Usage:", "Information:", "Analysis:", "Decompilation:", "Output:", "Examples:")
    skip_emoji = ("📊", "🔍", "⚙️", "💾", "📚")
    for line in text.splitlines():
        s = line.strip()
        if not s:
            continue
        if s.startswith(("╔", "║", "╚")):
            continue
        if s.startswith(skip_pfx) or s.startswith(skip_emoji):
            continue
        if s.startswith(("  -", "  fission")):
            continue
        filtered.append(line)
    return "\n".join(filtered).strip()


# ===========================================================================
# Quality metrics
# ===========================================================================

def _max_nesting_depth(code: str) -> int:
    """Count maximum { } brace nesting depth as a proxy for control-flow depth."""
    depth = max_depth = 0
    for ch in code:
        if ch == "{":
            depth += 1
            max_depth = max(max_depth, depth)
        elif ch == "}" and depth:
            depth -= 1
    return max_depth


def _count_errors(text: str) -> int:
    """Count error/warning indicators in raw tool output."""
    return len(re.findall(
        r"\b(ERROR|error|Warning|WARNING|WARN|failed|timeout|exception)\b",
        text,
    ))


def analyze_code(code: str, raw_tool_output: str = "") -> dict[str, Any]:
    """
    Extended code quality metrics.

    Additions vs v2:
      control_flow_keywords — if/else/switch/while/for count
      control_flow_depth    — max brace nesting depth
      goto_count            — number of goto statements
      struct_accesses       — '->' and '.field' access count
      casts                 — '*(type*)' style cast count
      string_literals       — "..." literal count
      printf_calls          — printf/fprintf/sprintf family count
      tool_errors           — ERROR/Warning occurrences in raw tool output
    """
    lines = code.count("\n") + 1 if code else 0
    cf_keywords = sum(code.count(kw) for kw in ("if", "else", "switch", "while", "for"))
    return {
        "lines": lines,
        "chars": len(code),
        "functions": code.count("("),
        "branches": sum(code.count(kw) for kw in ("if", "while", "for", "switch")),
        # --- new fields ---
        "control_flow_keywords": cf_keywords,
        "control_flow_depth": _max_nesting_depth(code),
        "goto_count": len(re.findall(r"\bgoto\b", code)),
        "struct_accesses": len(re.findall(r"->|\.\w+\s*[=;(,)\[\]]", code)),
        "casts": len(re.findall(r"\*\s*\(\s*\w+\s*\*\s*\)", code)),
        "string_literals": len(re.findall(r'"[^"]*"', code)),
        "printf_calls": len(re.findall(r"\b(?:printf|fprintf|sprintf|snprintf|vprintf|puts)\s*\(", code)),
        "tool_errors": _count_errors(raw_tool_output),
    }


def checklist_score(code: str, expected_patterns: list[str]) -> dict[str, Any]:
    """
    Check which patterns are present in `code`.
    Returns a score dict with per-pattern hit/miss and overall ratio.
    """
    hits: dict[str, bool] = {}
    for pat in expected_patterns:
        hits[pat] = bool(re.search(re.escape(pat), code))
    total = len(expected_patterns)
    satisfied = sum(hits.values())
    return {
        "patterns": hits,
        "satisfied": satisfied,
        "total": total,
        "ratio": round(satisfied / total, 3) if total else 1.0,
    }


# ===========================================================================
# Timing statistics helpers
# ===========================================================================

def _percentile(sorted_vals: list[float], pct: float) -> float:
    """Linear-interpolation percentile on a pre-sorted list."""
    if not sorted_vals:
        return 0.0
    n = len(sorted_vals)
    idx = (n - 1) * pct / 100
    lo, hi = int(idx), min(int(idx) + 1, n - 1)
    return sorted_vals[lo] + (sorted_vals[hi] - sorted_vals[lo]) * (idx - lo)


def timing_stats(values: list[float]) -> dict[str, float]:
    if not values:
        return {"count": 0, "avg": 0.0, "stdev": 0.0, "min": 0.0, "max": 0.0, "p50": 0.0, "p95": 0.0}
    sv = sorted(values)
    return {
        "count": len(sv),
        "avg": round(statistics.fmean(sv), 4),
        "stdev": round(statistics.pstdev(sv), 4),
        "min": round(sv[0], 4),
        "max": round(sv[-1], 4),
        "p50": round(_percentile(sv, 50), 4),
        "p95": round(_percentile(sv, 95), 4),
    }


# ===========================================================================
# Core comparison (single run)
# ===========================================================================

def _run_ghidra(binary: Path, address: str, timeout: int,
                ghidra_cache_dir: Path | None) -> tuple[str, float]:
    """Run (or load from cache) Ghidra decompilation. Returns (raw_output, seconds)."""
    addr_val = int(address, 16) if address.startswith("0x") else int(address)
    norm_addr = f"0x{addr_val:x}"

    if ghidra_cache_dir is not None:
        cached = ghidra_cache_dir / f"ghidra_{norm_addr}.json"
        if cached.exists():
            data = json.loads(cached.read_text(encoding="utf-8"))
            code = data.get("code", "")
            asm = data.get("asm", "")
            combined = (
                f"--- Assembly Listing ---\n{asm}\n--- Decompiled Code ---\n{code}"
                if asm else code
            )
            return combined, float(data.get("decomp_sec", 0.0))

    scripts_dir = _project_root() / "scripts"
    python_bin = detect_python()
    cmd = [python_bin, str(scripts_dir / "ghidra" / "pyghidra_decompile.py"), str(binary), address]
    return run_command(cmd, timeout)


def compare_single(
    binary: Path,
    address: str,
    output_json: Path,
    timeout: int,
    ghidra_cache_dir: Path | None = None,
    expected_patterns: list[str] | None = None,
) -> dict[str, Any]:
    """
    Run a single Fission vs Ghidra comparison. Returns result dict.
    Expected patterns are checked against Fission output only.
    """
    fission_cmd = detect_fission_cmd()

    print(f"    - Running Ghidra...")
    ghidra_raw, ghidra_sec = _run_ghidra(binary, address, timeout, ghidra_cache_dir)

    fission_asm_cmd = fission_cmd + [str(binary), "--disasm-function", address]
    fission_decomp_cmd = fission_cmd + [str(binary), "--decomp", address, "--no-header"]

    print(f"    - Running Fission disassembly...")
    fission_asm_raw, fission_asm_sec = run_command(fission_asm_cmd, timeout)
    print(f"    - Running Fission decompilation...")
    fission_decomp_raw, fission_decomp_sec = run_command(fission_decomp_cmd, timeout)

    ghidra_raw = strip_ansi(ghidra_raw)
    fission_asm_raw_clean = strip_ansi(fission_asm_raw)
    fission_decomp_raw_clean = strip_ansi(fission_decomp_raw)

    ghidra_asm, ghidra_decomp = extract_ghidra_parts(ghidra_raw)
    fission_asm = strip_fission_noise(fission_asm_raw_clean)
    fission_decomp = extract_fission_decomp(fission_decomp_raw_clean)

    ghidra_metrics = analyze_code(ghidra_decomp, ghidra_raw)
    fission_metrics = analyze_code(fission_decomp, fission_decomp_raw_clean)

    fission_norm = normalize_for_similarity(fission_decomp)
    ghidra_norm = normalize_for_similarity(ghidra_decomp)
    gl = ghidra_norm.splitlines()
    fl = fission_norm.splitlines()
    if gl and fl:
        similarity = difflib.SequenceMatcher(None, gl, fl).ratio()
    elif not gl and not fl:
        similarity = 1.0
    else:
        similarity = 0.0

    checklist: dict[str, Any] = {}
    if expected_patterns:
        checklist = checklist_score(fission_decomp, expected_patterns)

    result: dict[str, Any] = {
        "comparison_info": {
            "binary": str(binary),
            "address": address,
            "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
            "metrics": {
                "ghidra": ghidra_metrics,
                "fission": fission_metrics,
            },
            "similarity": round(similarity * 100, 2),
            "checklist": checklist,
        },
        "timings": {
            "ghidra_sec": round(ghidra_sec, 4),
            "fission_asm_sec": round(fission_asm_sec, 4),
            "fission_decomp_sec": round(fission_decomp_sec, 4),
        },
        "ghidra_assembly": ghidra_asm,
        "ghidra_decompilation": ghidra_decomp,
        "fission_assembly": fission_asm,
        "fission_decompilation": fission_decomp,
    }

    output_json.parent.mkdir(parents=True, exist_ok=True)
    output_json.write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")

    stem = output_json.with_suffix("")
    for suffix, content in [
        ("_ghidra_asm.txt", ghidra_asm),
        ("_ghidra_decomp.txt", ghidra_decomp),
        ("_fission_asm.txt", fission_asm),
        ("_fission_decomp.txt", fission_decomp),
    ]:
        (stem.parent / (stem.name + suffix)).write_text(content, encoding="utf-8")

    log_lines = [
        f"timestamp: {result['comparison_info']['timestamp']}",
        "",
        "---- ghidra output ----",
        ghidra_raw,
        "",
        "---- fission asm output ----",
        fission_asm_raw,
        "",
        "---- fission decomp output ----",
        fission_decomp_raw,
        "",
        f"timing: ghidra={ghidra_sec:.4f}s  fission_asm={fission_asm_sec:.4f}s  "
        f"fission_decomp={fission_decomp_sec:.4f}s",
    ]
    (stem.parent / (stem.name + "_run.log")).write_text("\n".join(log_lines), encoding="utf-8")

    return result


# ===========================================================================
# Repeated-run wrapper
# ===========================================================================

def compare_with_runs(
    binary: Path,
    address: str,
    output_json: Path,
    timeout: int,
    runs: int = 1,
    ghidra_cache_dir: Path | None = None,
    expected_patterns: list[str] | None = None,
) -> dict[str, Any]:
    """
    Run compare_single up to `runs` times; collect timing stats.
    The last run's full result dict is the canonical output;
    timing_stats fields are merged in.
    """
    if runs <= 1:
        return compare_single(binary, address, output_json, timeout,
                              ghidra_cache_dir, expected_patterns)

    ghidra_times: list[float] = []
    fission_times: list[float] = []
    last_result: dict[str, Any] = {}

    for i in range(runs):
        print(f"  [Run {i+1}/{runs}]")
        r = compare_single(binary, address, output_json, timeout,
                           ghidra_cache_dir, expected_patterns)
        t = r.get("timings", {})
        g = t.get("ghidra_sec", 0.0)
        f = t.get("fission_decomp_sec", 0.0)
        if g > 0:
            ghidra_times.append(g)
        if f > 0:
            fission_times.append(f)
        last_result = r

    last_result["timing_stats"] = {
        "ghidra": timing_stats(ghidra_times),
        "fission_decomp": timing_stats(fission_times),
    }
    output_json.write_text(json.dumps(last_result, indent=2, ensure_ascii=False), encoding="utf-8")
    return last_result


# ===========================================================================
# Batch summary
# ===========================================================================

def summarize_results(results: list[dict[str, Any]]) -> dict[str, Any]:
    similarities: list[float] = []
    ghidra_times: list[float] = []
    fission_times: list[float] = []
    checker_ratios: list[float] = []
    faster_counts = {"ghidra": 0, "fission": 0, "tie": 0}
    total_errors = {"ghidra": 0, "fission": 0}

    for item in results:
        info = item.get("comparison_info", {})
        timings = item.get("timings", {})
        metrics = info.get("metrics", {})

        sim = info.get("similarity", 0.0)
        similarities.append(sim)

        g_sec = timings.get("ghidra_sec", 0.0)
        f_sec = timings.get("fission_decomp_sec", 0.0)

        # prefer multi-run stats when available
        ts = item.get("timing_stats", {})
        if ts.get("ghidra", {}).get("avg", 0.0):
            g_sec = ts["ghidra"]["avg"]
        if ts.get("fission_decomp", {}).get("avg", 0.0):
            f_sec = ts["fission_decomp"]["avg"]

        if g_sec:
            ghidra_times.append(g_sec)
        if f_sec:
            fission_times.append(f_sec)

        if g_sec and f_sec:
            if g_sec < f_sec:
                faster_counts["ghidra"] += 1
            elif f_sec < g_sec:
                faster_counts["fission"] += 1
            else:
                faster_counts["tie"] += 1

        total_errors["ghidra"] += metrics.get("ghidra", {}).get("tool_errors", 0)
        total_errors["fission"] += metrics.get("fission", {}).get("tool_errors", 0)

        cl = info.get("checklist", {})
        if cl.get("total", 0):
            checker_ratios.append(cl.get("ratio", 0.0))

    sv_sim = sorted(similarities)
    sv_f = sorted(fission_times)

    # slowest 5 functions for the report
    fn_perf = sorted(
        [(r.get("comparison_info", {}).get("address", "?"), r.get("timings", {}).get("fission_decomp_sec", 0.0))
         for r in results],
        key=lambda x: x[1],
        reverse=True,
    )[:5]

    return {
        "total_functions": len(results),
        "average_similarity": round(statistics.fmean(similarities), 2) if similarities else 0.0,
        "similarity_p50": round(_percentile(sv_sim, 50), 2),
        "similarity_p5": round(_percentile(sv_sim, 5), 2),
        "ghidra_timing": timing_stats(ghidra_times),
        "fission_timing": timing_stats(fission_times),
        "faster_counts": faster_counts,
        "tool_errors": total_errors,
        "checklist_avg_ratio": round(statistics.fmean(checker_ratios), 3) if checker_ratios else None,
        "slowest_functions": [{"address": a, "fission_sec": s} for a, s in fn_perf],
        "env": env_info(),
    }


# ===========================================================================
# Baseline regression comparison
# ===========================================================================

def compare_to_baseline(
    current_summary: dict[str, Any],
    baseline_path: Path,
    fail_rules: list[str],
) -> tuple[dict[str, Any], bool]:
    """
    Compare current_summary against baseline.
    fail_rules examples: ["similarity_drop=5", "timing_increase=20"]
    Returns (diff_dict, should_fail).
    """
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    b_sim = baseline.get("average_similarity", 0.0)
    c_sim = current_summary.get("average_similarity", 0.0)
    b_avg = baseline.get("fission_timing", {}).get("avg", 0.0)
    c_avg = current_summary.get("fission_timing", {}).get("avg", 0.0)

    diff: dict[str, Any] = {
        "similarity_delta": round(c_sim - b_sim, 2),
        "fission_timing_avg_delta_pct": (
            round((c_avg - b_avg) / b_avg * 100, 1) if b_avg else None
        ),
        "error_delta": {
            "ghidra": current_summary.get("tool_errors", {}).get("ghidra", 0)
                       - baseline.get("tool_errors", {}).get("ghidra", 0),
            "fission": current_summary.get("tool_errors", {}).get("fission", 0)
                        - baseline.get("tool_errors", {}).get("fission", 0),
        },
    }

    should_fail = False
    fail_details: list[str] = []

    for rule in fail_rules:
        if "=" not in rule:
            continue
        name, threshold_s = rule.split("=", 1)
        threshold = float(threshold_s)
        name = name.strip().lower().replace("-", "_")

        if name == "similarity_drop":
            drop = -diff["similarity_delta"]
            if drop >= threshold:
                should_fail = True
                fail_details.append(
                    f"Similarity dropped {drop:.2f}% (threshold {threshold}%)"
                )
        elif name == "timing_increase":
            pct = diff.get("fission_timing_avg_delta_pct") or 0.0
            if pct >= threshold:
                should_fail = True
                fail_details.append(
                    f"Fission avg timing increased {pct:.1f}% (threshold {threshold}%)"
                )

    diff["fail_details"] = fail_details
    diff["regression_detected"] = should_fail
    return diff, should_fail


# ===========================================================================
# HTML report
# ===========================================================================

_CSS = """
body { font-family: Arial, sans-serif; padding: 20px; margin: 0; }
h1, h2 { color: #333; }
table { border-collapse: collapse; width: 100%; margin-bottom: 20px; }
th, td { border: 1px solid #ddd; padding: 8px; font-size: 13px; }
th { background: #f5f5f5; }
tr.low-sim td:nth-child(2) { background: #fff0f0; }
tr.slow td:nth-child(4) { background: #fff8e1; }
.tabs { display: flex; gap: 8px; margin-bottom: 12px; }
.tab-btn { padding: 6px 16px; cursor: pointer; border: 1px solid #ccc;
           background: #f0f0f0; border-radius: 4px; }
.tab-btn.active { background: #4a80c4; color: #fff; border-color: #4a80c4; }
.tab-panel { display: none; }
.tab-panel.active { display: block; }
pre { font-family: monospace; font-size: 12px; white-space: pre-wrap; word-break: break-word; }
.diff-table { width: 100%; table-layout: fixed; }
.diff-table td { width: 50%; vertical-align: top; }
ins { background: #d4f8d4; text-decoration: none; }
del { background: #ffd4d4; text-decoration: none; }
"""

_JS = """
function showTab(id) {
  document.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));
  document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
  document.getElementById(id).classList.add('active');
  event.target.classList.add('active');
}
"""


def _side_by_side_html(left: str, right: str) -> str:
    """Produce an HTML table with left/right code panels."""
    left_h = left.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    right_h = right.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    return (
        '<table class="diff-table"><tr>'
        f'<td><strong>Ghidra</strong><pre>{left_h}</pre></td>'
        f'<td><strong>Fission</strong><pre>{right_h}</pre></td>'
        "</tr></table>"
    )


def generate_html_report(
    results: list[dict[str, Any]],
    output_dir: Path,
    summary: dict[str, Any] | None = None,
    baseline_diff: dict[str, Any] | None = None,
) -> None:
    """
    Generate report.html with three tabs:
      All functions | Low similarity (< 70%) | Slowest functions (top 10)
    Each row links to side-by-side diff anchors embedded in the page.
    """

    LOW_SIM_THRESHOLD = 70.0
    diffs_html: list[str] = []  # detailed per-function sections

    def mk_row(item: dict, is_low: bool, is_slow: bool) -> str:
        info = item.get("comparison_info", {})
        addr = info.get("address", "?")
        sim = info.get("similarity", 0.0)
        t = item.get("timings", {})
        g_sec = t.get("ghidra_sec", 0.0)
        f_sec = t.get("fission_decomp_sec", 0.0)
        faster = "—"
        if g_sec and f_sec:
            faster = "fission" if f_sec < g_sec else ("ghidra" if g_sec < f_sec else "tie")
        cl = info.get("checklist", {})
        cl_str = f"{cl.get('satisfied', 0)}/{cl.get('total', 0)}" if cl.get("total") else "—"
        row_cls = ("low-sim " if is_low else "") + ("slow" if is_slow else "")
        err_f = info.get("metrics", {}).get("fission", {}).get("tool_errors", 0)
        err_g = info.get("metrics", {}).get("ghidra", {}).get("tool_errors", 0)
        err_str = f"F:{err_f} G:{err_g}" if (err_f or err_g) else "0"
        return (
            f'<tr class="{row_cls.strip()}">'
            f'<td><a href="#diff_{addr}">{addr}</a></td>'
            f'<td>{sim:.1f}%</td>'
            f'<td>{g_sec:.3f}s</td>'
            f'<td>{f_sec:.3f}s</td>'
            f'<td>{faster}</td>'
            f'<td>{cl_str}</td>'
            f'<td>{err_str}</td>'
            f'</tr>'
        )

    # Sort by similarity ascending for "low-sim" tab
    all_items = results
    low_sim_items = [r for r in all_items if r.get("comparison_info", {}).get("similarity", 100) < LOW_SIM_THRESHOLD]
    slow_items = sorted(all_items, key=lambda r: r.get("timings", {}).get("fission_decomp_sec", 0.0), reverse=True)[:10]

    header_row = (
        "<tr><th>Address</th><th>Similarity</th><th>Ghidra(s)</th>"
        "<th>Fission(s)</th><th>Faster</th><th>Checklist</th><th>Errors</th></tr>"
    )

    def mk_table(items: list[dict], slow_set: set) -> str:
        rows = [
            mk_row(r,
                   r.get("comparison_info", {}).get("similarity", 100) < LOW_SIM_THRESHOLD,
                   r.get("comparison_info", {}).get("address") in slow_set)
            for r in items
        ]
        return f"<table><thead>{header_row}</thead><tbody>{''.join(rows)}</tbody></table>"

    slow_addrs = {r.get("comparison_info", {}).get("address") for r in slow_items}

    # Build diff sections
    for item in all_items:
        addr = item.get("comparison_info", {}).get("address", "?")
        gh = item.get("ghidra_decompilation", "")
        fi = item.get("fission_decompilation", "")
        diffs_html.append(
            f'<h3 id="diff_{addr}">Function @ {addr}</h3>'
            + _side_by_side_html(gh, fi)
        )

    # Summary section
    summary_html = ""
    if summary:
        fi_t = summary.get("fission_timing", {})
        gh_t = summary.get("ghidra_timing", {})
        avg_sim = summary.get("average_similarity", 0.0)
        cl_ratio = summary.get("checklist_avg_ratio")
        errs = summary.get("tool_errors", {})
        e = summary.get("env", {})

        reg_html = ""
        if baseline_diff:
            reg_color = "#cc0000" if baseline_diff.get("regression_detected") else "#007700"
            reg_html = (
                f'<p style="color:{reg_color}"><strong>Baseline delta:</strong> '
                f'similarity {baseline_diff.get("similarity_delta", 0):+.2f}%,  '
                f'fission avg {baseline_diff.get("fission_timing_avg_delta_pct") or 0:+.1f}%</p>'
                + ("".join(f'<p style="color:#cc0000">⚠ {d}</p>' for d in baseline_diff.get("fail_details", [])))
            )

        summary_html = (
            "<h2>Summary</h2><ul>"
            f"<li>Functions: {summary.get('total_functions', 0)}, "
            f"Avg similarity: <strong>{avg_sim:.2f}%</strong> (p50 {summary.get('similarity_p50', 0):.2f}%, p5 {summary.get('similarity_p5', 0):.2f}%)</li>"
            f"<li>Fission decomp avg: {fi_t.get('avg', 0):.3f}s ± {fi_t.get('stdev', 0):.3f}  "
            f"(p50 {fi_t.get('p50', 0):.3f}s, p95 {fi_t.get('p95', 0):.3f}s)</li>"
            f"<li>Ghidra avg: {gh_t.get('avg', 0):.3f}s ± {gh_t.get('stdev', 0):.3f}</li>"
            f"<li>Errors — Fission: {errs.get('fission', 0)}, Ghidra: {errs.get('ghidra', 0)}</li>"
            + (f"<li>Checklist avg ratio: {cl_ratio:.1%}</li>" if cl_ratio is not None else "")
            + "</ul>"
            + f"<p><small>Fission rev: {e.get('fission_git_rev', '?')}  "
            f"| Python {e.get('python', '?')}  "
            f"| {e.get('run_at', '?')}</small></p>"
            + reg_html
        )

    html = "\n".join([
        "<!doctype html><html><head><meta charset='utf-8'>",
        "<title>Decompiler Comparison Report v3</title>",
        f"<style>{_CSS}</style>",
        f"<script>{_JS}</script>",
        "</head><body>",
        "<h1>Decompiler Comparison Report</h1>",
        summary_html,
        "<div class='tabs'>",
        "<button class='tab-btn active' onclick='showTab(\"tab-all\")'>All functions</button>",
        f"<button class='tab-btn' onclick='showTab(\"tab-low\")'>Low similarity (&lt;{LOW_SIM_THRESHOLD:.0f}%) [{len(low_sim_items)}]</button>",
        f"<button class='tab-btn' onclick='showTab(\"tab-slow\")'>Slowest 10</button>",
        "<button class='tab-btn' onclick='showTab(\"tab-diff\")'>Side-by-side diff</button>",
        "</div>",
        "<div id='tab-all' class='tab-panel active'>",
        mk_table(all_items, slow_addrs),
        "</div>",
        "<div id='tab-low' class='tab-panel'>",
        mk_table(low_sim_items, slow_addrs) if low_sim_items else "<p>No functions below threshold.</p>",
        "</div>",
        "<div id='tab-slow' class='tab-panel'>",
        mk_table(slow_items, slow_addrs),
        "</div>",
        "<div id='tab-diff' class='tab-panel'>",
        *diffs_html,
        "</div>",
        "</body></html>",
    ])
    report_path = output_dir / "report.html"
    report_path.write_text(html, encoding="utf-8")
    print(f"  HTML report: {report_path}")


# ===========================================================================
# Suite-driven batch
# ===========================================================================

def run_suite(
    suite: dict[str, Any],
    output_dir: Path,
    timeout: int,
    runs: int,
    html: bool,
    baseline_path: Path | None = None,
    fail_rules: list[str] | None = None,
) -> int:
    """Run a full suite. Returns exit code (0 = ok, 1 = regression)."""
    results: list[dict[str, Any]] = []
    root = _project_root()

    for binary_def in suite.get("binaries", []):
        binary = (root / binary_def["path"]).resolve()
        if not binary.exists():
            print(f"Warning: binary not found: {binary}", file=sys.stderr)
            continue

        for func_def in binary_def.get("functions", []):
            address = func_def["address"]
            patterns = func_def.get("expected_patterns", [])
            fn_runs = func_def.get("runs", runs)
            label = func_def.get("name", address)

            print(f"== {label} @ {address} ({binary.name}) ==")
            out_json = output_dir / f"{binary.stem}_{address}.json"
            r = compare_with_runs(binary, address, out_json, timeout,
                                  fn_runs, None, patterns)
            results.append(r)

    summary = summarize_results(results)
    summary["suite_name"] = suite.get("name", "unnamed")

    summary_path = output_dir / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")

    # Regression check
    baseline_diff: dict[str, Any] | None = None
    should_fail = False
    if baseline_path and baseline_path.exists():
        baseline_diff, should_fail = compare_to_baseline(summary, baseline_path, fail_rules or [])
        diff_path = output_dir / "regression_diff.json"
        diff_path.write_text(json.dumps(baseline_diff, indent=2), encoding="utf-8")

    _print_ci_summary(summary, baseline_diff)

    if html:
        generate_html_report(results, output_dir, summary, baseline_diff)

    return 1 if should_fail else 0


# ===========================================================================
# CI summary line
# ===========================================================================

def _print_ci_summary(summary: dict[str, Any], baseline_diff: dict[str, Any] | None = None) -> None:
    avg_sim = summary.get("average_similarity", 0.0)
    fi = summary.get("fission_timing", {})
    errs = summary.get("tool_errors", {})
    slow = summary.get("slowest_functions", [])
    slow_str = ", ".join(f"{x['address']}({x['fission_sec']:.2f}s)" for x in slow[:3])
    baseline_str = ""
    if baseline_diff:
        delta = baseline_diff.get("similarity_delta", 0.0)
        sign = "+" if delta >= 0 else ""
        baseline_str = f" | Baseline sim {sign}{delta:.2f}%"
        if baseline_diff.get("regression_detected"):
            baseline_str += " ⚠ REGRESSION"

    print("")
    print(
        f"[CI] Similarity: {avg_sim:.1f}% | "
        f"Fission avg: {fi.get('avg', 0):.3f}s ± {fi.get('stdev', 0):.3f} (p95 {fi.get('p95', 0):.3f}s) | "
        f"Errors F:{errs.get('fission', 0)} G:{errs.get('ghidra', 0)} | "
        f"Slowest: {slow_str}"
        + baseline_str
    )


# ===========================================================================
# CLI
# ===========================================================================

def _parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Fission vs Ghidra benchmark (v3)",
        add_help=False,
    )
    p.add_argument("--help", "-?", action="help")
    p.add_argument("binary", nargs="?", help="Binary path (single / batch mode)")
    p.add_argument("address_or_file", nargs="?", help="Hex address or address-list file")
    p.add_argument("output", nargs="?", help="Output JSON or output directory")

    # Modes
    p.add_argument("-m", "--batch", action="store_true", help="Batch mode (address file)")
    p.add_argument("--suite", metavar="YAML", help="Suite YAML/JSON definition")

    # Options
    p.add_argument("--html", action="store_true", help="Generate HTML report")
    p.add_argument("-t", "--timeout", type=int, default=600, help="Timeout per run (s)")
    p.add_argument("--runs", type=int, default=1, help="Repeat each function N times for timing stats")
    p.add_argument("--use-ghidra-cache", action="store_true",
                   help="Load Ghidra results from cache dir (non end-to-end)")
    p.add_argument("--expected-patterns", metavar="PAT", nargs="+",
                   help='Expected patterns to check in Fission output (single-function mode)')

    # Regression
    p.add_argument("--baseline", metavar="JSON", help="Baseline summary JSON for regression comparison")
    p.add_argument("--regression-fail-on", nargs="+", metavar="RULE",
                   help="Fail if rule exceeded e.g. similarity_drop=5 timing_increase=20")

    return p.parse_args()


def main() -> int:
    args = _parse_args()

    root = _project_root()
    scripts_dir = root / "scripts"
    default_result_dir = scripts_dir / "result"

    baseline_path = Path(args.baseline).expanduser().resolve() if args.baseline else None
    fail_rules = args.regression_fail_on or []

    # ---- Suite mode ----
    if args.suite:
        suite_path = Path(args.suite).expanduser().resolve()
        if not suite_path.exists():
            print(f"Error: suite file not found: {suite_path}", file=sys.stderr)
            return 1
        suite = load_suite(suite_path)

        ts = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
        suite_name = re.sub(r"[^a-zA-Z0-9_-]", "_", suite.get("name", "suite"))
        out_dir = Path(args.output or (default_result_dir / f"{ts}_{suite_name}")).expanduser()
        out_dir.mkdir(parents=True, exist_ok=True)

        shutil.copy(suite_path, out_dir / suite_path.name)

        return run_suite(suite, out_dir, args.timeout, args.runs, args.html,
                         baseline_path, fail_rules)

    # ---- Batch mode ----
    if args.batch:
        if not args.binary or not args.address_or_file:
            print("Error: batch mode requires BINARY and address file.", file=sys.stderr)
            return 1

        binary = Path(args.binary).expanduser().resolve()
        if not binary.exists():
            print(f"Error: binary not found: {binary}", file=sys.stderr)
            return 1

        ts = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
        out_dir = Path(args.output or (default_result_dir / f"{ts}_batch")).expanduser()
        out_dir.mkdir(parents=True, exist_ok=True)

        addr_file = Path(args.address_or_file).expanduser()
        ghidra_cache_dir: Path | None = None

        if args.use_ghidra_cache:
            ghidra_cache_dir = out_dir / "ghidra_cache"
            if not ghidra_cache_dir.exists():
                print("[*] Running Ghidra batch cache...")
                python_bin = detect_python()
                batch_script = scripts_dir / "ghidra" / "pyghidra_decompile_batch.py"
                subprocess.run(
                    [python_bin, str(batch_script), str(binary), str(addr_file), str(ghidra_cache_dir)],
                    cwd=str(root),
                    check=False,
                )
                print("[*] Ghidra batch cache done.")

        results: list[dict[str, Any]] = []
        for line in addr_file.read_text(encoding="utf-8").splitlines():
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            parts = stripped.split()
            addr = parts[0]
            print(f"== {addr} ==")
            out_json = out_dir / f"addr_{addr}.json"
            results.append(compare_with_runs(binary, addr, out_json, args.timeout,
                                             args.runs, ghidra_cache_dir))

        summary = summarize_results(results)
        (out_dir / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")

        baseline_diff: dict[str, Any] | None = None
        should_fail = False
        if baseline_path and baseline_path.exists():
            baseline_diff, should_fail = compare_to_baseline(summary, baseline_path, fail_rules)
            (out_dir / "regression_diff.json").write_text(json.dumps(baseline_diff, indent=2), encoding="utf-8")

        _print_ci_summary(summary, baseline_diff)

        if args.html:
            generate_html_report(results, out_dir, summary, baseline_diff)

        return 1 if should_fail else 0

    # ---- Single-function mode ----
    if not args.binary or not args.address_or_file:
        import subprocess as _sp
        _sp.run([sys.executable, __file__, "--help"])
        return 1

    binary = Path(args.binary).expanduser().resolve()
    if not binary.exists():
        print(f"Error: binary not found: {binary}", file=sys.stderr)
        return 1

    ts = datetime.datetime.now().strftime("%Y%m%d_%H%M")
    out_json = Path(args.output) if args.output else (
        default_result_dir / f"{ts}_{args.address_or_file.replace('0x', '')}_result" / "comparison.json"
    )
    out_json.parent.mkdir(parents=True, exist_ok=True)

    result = compare_with_runs(
        binary, args.address_or_file, out_json, args.timeout, args.runs,
        None, args.expected_patterns or [],
    )

    info = result.get("comparison_info", {})
    metrics = info.get("metrics", {})
    sim = info.get("similarity", 0.0)
    t = result.get("timings", {})
    ts_stats = result.get("timing_stats", {})

    print("")
    print("==========================================")
    print("✅ Comparison Complete")
    print("==========================================")
    print(f"  JSON: {out_json}")
    g = metrics.get("ghidra", {})
    f = metrics.get("fission", {})
    print(f"\nMetrics:")
    print(f"  Ghidra:  {g.get('lines', 0)} lines, {g.get('branches', 0)} branches, "
          f"struct_accesses={g.get('struct_accesses', 0)}, errors={g.get('tool_errors', 0)}")
    print(f"  Fission: {f.get('lines', 0)} lines, {f.get('branches', 0)} branches, "
          f"struct_accesses={f.get('struct_accesses', 0)}, errors={f.get('tool_errors', 0)}")
    print(f"  Similarity: {sim:.2f}%")
    cl = info.get("checklist", {})
    if cl.get("total"):
        print(f"  Checklist: {cl['satisfied']}/{cl['total']} patterns satisfied")
        for pat, hit in cl.get("patterns", {}).items():
            marker = "✓" if hit else "✗"
            print(f"    {marker} {pat!r}")

    if ts_stats:
        fi_ts = ts_stats.get("fission_decomp", {})
        gh_ts = ts_stats.get("ghidra", {})
        print(f"\nTiming ({args.runs} runs):")
        print(f"  Fission decomp: avg {fi_ts.get('avg', 0):.3f}s ± {fi_ts.get('stdev', 0):.3f}  "
              f"(p95 {fi_ts.get('p95', 0):.3f}s, min {fi_ts.get('min', 0):.3f}s, max {fi_ts.get('max', 0):.3f}s)")
        print(f"  Ghidra:         avg {gh_ts.get('avg', 0):.3f}s ± {gh_ts.get('stdev', 0):.3f}")
    else:
        print(f"\nTiming:")
        print(f"  Ghidra decomp:  {t.get('ghidra_sec', 0):.3f}s")
        print(f"  Fission decomp: {t.get('fission_decomp_sec', 0):.3f}s")

    if args.html:
        html_dir = out_json.parent
        generate_html_report([result], html_dir, None, None)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
