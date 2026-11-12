#!/usr/bin/env python3
"""Give each AArch64 FCVT{A,M,N,P,Z}{S,U} rounding mode its own semantics.

Upstream Ghidra defines one display table per form (`fcvt_vmnemonic` for the
SIMD/scalar-FP register forms, `fcvt_smnemonic` for the float-to-general-register
forms) whose ten mnemonics share a single constructor body, `Rd = trunc(Rn)`.
That is right for FCVTZ* (round toward zero) and wrong for the other four modes:

    FCVTM* toward -inf       -> trunc(floor(Rn))
    FCVTP* toward +inf       -> trunc(ceil(Rn))
    FCVTN* to nearest        -> trunc(round(Rn))   (see "Approximation" below)
    FCVTA* to nearest, away  -> trunc(round(Rn))   (see "Approximation" below)
    FCVTZ* toward zero       -> trunc(Rn)          (unchanged)

`floor`/`ceil`/`round` are float-to-float p-code operations, so each result is
still converted to an integer by `trunc`.

Approximation: SLEIGH has FLOAT_ROUND (floor(x + 0.5)) but no ties-to-even and no
ties-away operation, so FCVTN* differs from the real instruction at exact .5 ties
(2.5 -> 3, hardware gives 2) and FCVTA* at negative ties (-2.5 -> -2, hardware gives
-3). Saturation and the signed/unsigned distinction are unchanged from upstream.

Upstream has two groups of constructors per form. One matches only the FCVTA*
encodings (a "2 DOCUMENTED OPCODES" group), the other the other eight (an "8
DOCUMENTED OPCODES" group); both use `trunc`. The script splits each display table
into one table per rounding mode and clones every constructor once per mode, then
asks the SLEIGH compiler which clones are impossible patterns (a constructor whose
fixed bits select one mode cannot match the others) and drops exactly those. It is
idempotent.

Usage: apply_aarch64_fcvt_rounding.py IN.sinc OUT.sinc --sleigh SLEIGH
where SLEIGH is a Ghidra `sleigh` / `sleigh_opt` compiler of the same release as the
specs (the Ghidra 12.2 release ships its sources under
Ghidra/Features/Decompiler/src/decompile/cpp; `make sleigh_opt` builds it). IN.sinc
must sit next to the .slaspec files that include it so the check can compile
AARCH64.slaspec in that directory.
"""
import os
import re
import subprocess
import sys
import tempfile

# rounding mode letter (the fifth character of the mnemonic) -> (p-code op, description)
MODES = {
    "a": ("round", "to nearest, ties away from zero (approximated by round)"),
    "m": ("floor", "toward minus infinity"),
    "n": ("round", "to nearest, ties to even (approximated by round)"),
    "p": ("ceil", "toward plus infinity"),
    "z": (None, "toward zero"),
}
FORMS = {"v": 16, "s": 12}  # form -> constructors upstream


def split_tables(text):
    for form in FORMS:
        entries = re.findall(rf'^fcvt_{form}mnemonic: "(fcvt[amnpz][su])" is ([^\n]*?) \{{ \}}\n', text, re.M)
        assert len(entries) == 10, (form, len(entries))
        block = "".join(
            f'fcvt_{form}mnemonic: "{n}" is {p} {{ }}\n' for n, p in entries
        )
        assert block in text, f"table {form} is not one contiguous block"
        new = ""
        for mode, (_, desc) in MODES.items():
            new += f"# Fission: FCVT{mode.upper()}* rounds {desc}.\n"
            for n, p in entries:
                if n[4] == mode:
                    new += f'fcvt_{form}mnemonic_{mode}: "{n}" is {p} {{ }}\n'
        text = text.replace(block, new, 1)
    return text


CTOR = re.compile(
    r"^:\^fcvt_([vs])mnemonic [^\n]*\nis [^\n]*\n\{\n(?:\t[^\n]*\n|\n)*?\}\n", re.M
)


def clone(ctor, form, mode, op):
    c = ctor.replace(f":^fcvt_{form}mnemonic ", f":^fcvt_{form}mnemonic_{mode} ", 1)
    c = c.replace(f"& fcvt_{form}mnemonic &", f"& fcvt_{form}mnemonic_{mode} &", 1)
    n = c.count("trunc(")
    assert n >= 1, c
    if op:
        c, k = re.subn(r"trunc\(([^()]*)\)", rf"trunc({op}(\1))", c)
        assert k == n, (mode, c)
    return c


def split_constructors(text, drop):
    """Clone each constructor once per rounding mode, except the pairs in `drop`.

    `plan` lists the clones in file order -- the forms interleave in the spec -- so
    that compiler line numbers can be matched to them."""
    plan = []  # (form, constructor index within its form, mode)
    counters = {form: 0 for form in FORMS}

    def clones(match):
        form = match.group(1)
        index = counters[form]
        counters[form] += 1
        out = []
        for mode, (op, _) in MODES.items():
            if (form, index, mode) in drop:
                continue
            plan.append((form, index, mode))
            out.append(clone(match.group(0), form, mode, op))
        assert out, f"every mode of {form} constructor {index} was dropped"
        return "\n".join(out)

    text = CTOR.sub(clones, text)
    assert counters == FORMS, (counters, FORMS)
    return text, plan


def impossible_pairs(text, plan, sleigh, spec_dir):
    """Compile AARCH64.slaspec with `text` as AARCH64neon.sinc; return the plan
    entries the compiler calls impossible patterns."""
    headers = [i + 1 for i, line in enumerate(text.split("\n"))
               if re.match(r":\^fcvt_[vs]mnemonic_[amnpz] ", line)]
    assert len(headers) == len(plan), (len(headers), len(plan))
    by_line = {}
    for line_no, entry in zip(headers, plan):
        by_line[line_no] = entry
        by_line[line_no + 1] = entry  # the `is` line

    neon = os.path.join(spec_dir, "AARCH64neon.sinc")
    original = open(neon).read()
    try:
        open(neon, "w").write(text)
        with tempfile.TemporaryDirectory() as tmp:
            run = subprocess.run(
                [sleigh, os.path.join(spec_dir, "AARCH64.slaspec"), os.path.join(tmp, "x.sla")],
                capture_output=True, text=True, cwd=spec_dir,
            )
    finally:
        open(neon, "w").write(original)
    log = run.stdout + run.stderr
    bad = set()
    for m in re.finditer(r"Impossible pattern: for table \"instruction\" constructor starting at line (\d+)", log):
        entry = by_line.get(int(m.group(1)))
        assert entry, f"impossible pattern at line {m.group(1)} is not one of the generated clones"
        bad.add(entry)
    other = [l for l in log.splitlines() if "ERROR" in l and "Impossible pattern" not in l]
    assert not other, other[:3]
    return bad


def main():
    args = sys.argv[1:]
    assert "--sleigh" in args, "pass --sleigh PATH"
    sleigh = args[args.index("--sleigh") + 1]
    src, dst = args[0], args[1]
    text = open(src).read()
    if "fcvt_vmnemonic_m" in text or "fcvt_vmnemonic_a" in text:
        open(dst, "w").write(text)
        print("already applied")
        return
    text = split_tables(text)
    spec_dir = os.path.dirname(os.path.abspath(src))
    drop = set()
    for _ in range(6):
        out, plan = split_constructors(text, drop)
        bad = impossible_pairs(out, plan, sleigh, spec_dir)
        if not bad:
            break
        drop |= bad
    else:
        raise SystemExit("did not converge")
    assert "fcvt_vmnemonic:" not in out and "fcvt_smnemonic:" not in out
    open(dst, "w").write(out)
    kept = {}
    for form, _, mode in plan:
        kept[(form, mode)] = kept.get((form, mode), 0) + 1
    print("applied; clones kept:", dict(sorted(kept.items())), "dropped as impossible:", len(drop))


main()
