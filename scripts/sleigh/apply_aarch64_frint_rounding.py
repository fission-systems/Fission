#!/usr/bin/env python3
"""Give each AArch64 FRINT{A,I,M,N,P,X,Z} rounding mode its own semantics.

Upstream Ghidra defines one display table per form (`frint_vmode` for the vector
forms, `frint_smode` for the scalar ones) whose seven modes share one constructor
body, `Rd = trunc(Rn)`. FRINT rounds a float to an integral *float*, but `trunc` is
FLOAT_TRUNC, a float-to-integer conversion, so the result was integer bits written
into a floating-point register for every mode.

    FRINTM  toward -inf          -> floor(Rn)
    FRINTP  toward +inf          -> ceil(Rn)
    FRINTN  to nearest, even     -> round(Rn)              (approximation, below)
    FRINTA  to nearest, away     -> round(Rn)              (approximation, below)
    FRINTI  current FPCR mode    -> round(Rn)              (approximation, below)
    FRINTX  current mode, inexact-> round(Rn)              (approximation, below)
    FRINTZ  toward zero          -> int2float(trunc(Rn))   (approximation, below)

Approximations. SLEIGH has FLOAT_FLOOR, FLOAT_CEIL and FLOAT_ROUND (floor(x + 0.5))
and nothing else, so: N differs at exact .5 ties (2.5 -> 3, hardware 2); A at
negative ties (-2.5 -> -2, hardware -3); I and X follow the dynamic FPCR rounding
mode, which is not modelled, and are lifted as N; Z has no float-to-float truncate,
so it goes through an integer (8 bytes for single and double, 4 for half) and loses
precision for magnitudes at or above 2^63 (2^31 for half) and the sign of -0.0.

The script splits each display table into one single-entry table per mode and
clones every constructor once per mode. It is idempotent.

Usage: apply_aarch64_frint_rounding.py IN.sinc OUT.sinc
After it, recompile the three AARCH64*.slaspec with a Ghidra `sleigh` of the same
release as the specs and refresh MANIFEST.sha256.json.
"""
import re
import sys

# mode letter -> (p-code operation, or None for the integer round trip)
MODES = {
    "a": "round",
    "i": "round",
    "m": "floor",
    "n": "round",
    "p": "ceil",
    "x": "round",
    "z": None,
}
FORMS = {"v": 5, "s": 3}  # form -> constructors upstream

CTOR = re.compile(
    r"^:frint\^frint_([vs])mode [^\n]*\nis [^\n]*\n\{\n(?:\t[^\n]*\n|\n)*?\}\n", re.M
)
TRUNC_STMT = re.compile(r"^\t(?P<lhs>[^=\n]+?) = trunc\((?P<arg>[^()\n]*)\);$")


def split_tables(text):
    for form in FORMS:
        entries = re.findall(
            rf'^frint_{form}mode: "([a-z])" is ([^\n]*?) \{{ \}}\n', text, re.M
        )
        assert [m for m, _ in entries] and sorted(m for m, _ in entries) == sorted(MODES), (
            form,
            entries,
        )
        block = "".join(f'frint_{form}mode: "{m}" is {p} {{ }}\n' for m, p in entries)
        assert block in text, f"table {form} is not one contiguous block"
        new = "# Fission: one table per FRINT rounding mode (see apply_aarch64_frint_rounding.py).\n"
        for mode in MODES:
            pattern = dict(entries)[mode]
            new += f'frint_{form}mode_{mode}: "{mode}" is {pattern} {{ }}\n'
        text = text.replace(block, new, 1)
    return text


def lane_bytes(lhs):
    """Integer width, in bytes, for the Z round trip of the value written to `lhs`."""
    m = re.search(r"\[\d+,(\d+)\]$", lhs)
    bits = int(m.group(1)) if m else int(re.search(r"FPR(\d+)", lhs).group(1))
    return 4 if bits == 16 else 8


def rewrite_body(ctor, mode):
    op = MODES[mode]
    out, temp = [], 0
    for line in ctor.split("\n"):
        m = TRUNC_STMT.match(line)
        if not m:
            # the generated "# simd unary Rd = trunc(Rn)" comment names the old operation
            if op and line.lstrip().startswith("#") and "trunc(" in line:
                line = line.replace("trunc(", f"{op}(")
            out.append(line)
            continue
        lhs, arg = m.group("lhs"), m.group("arg")
        if op:
            out.append(f"\t{lhs} = {op}({arg});")
        else:
            width = lane_bytes(lhs)
            out.append(f"\tlocal fz{temp}:{width} = trunc({arg});")
            out.append(f"\t{lhs} = int2float(fz{temp});")
            temp += 1
    return "\n".join(out)


def split_constructors(text):
    counters = {form: 0 for form in FORMS}

    def clones(match):
        form = match.group(1)
        counters[form] += 1
        out = []
        for mode in MODES:
            c = match.group(0)
            c = c.replace(f":frint^frint_{form}mode ", f":frint^frint_{form}mode_{mode} ", 1)
            c = c.replace(f"& frint_{form}mode &", f"& frint_{form}mode_{mode} &", 1)
            assert c.count("trunc(") >= 1, c
            c = rewrite_body(c, mode)
            code = [line for line in c.split("\n") if not line.lstrip().startswith("#")]
            assert MODES[mode] is None or not any("trunc(" in line for line in code), c
            out.append(c)
        return "\n".join(out)

    text = CTOR.sub(clones, text)
    assert counters == FORMS, (counters, FORMS)
    return text


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    if "frint_vmode_m" in text:
        open(dst, "w").write(text)
        print("already applied")
        return
    text = split_constructors(split_tables(text))
    assert "frint_vmode:" not in text and "frint_smode:" not in text
    open(dst, "w").write(text)
    print("applied")


main()
