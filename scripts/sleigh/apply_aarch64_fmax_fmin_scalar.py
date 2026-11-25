#!/usr/bin/env python3
"""Lift the AArch64 scalar FMAX/FMIN/FMAXNM/FMINNM by the Arm ARM pseudocode.

Arm ARM (DDI 0487), FMAX/FMIN (scalar) use FPMax/FPMin and FMAXNM/FMINNM (scalar)
use FPMaxNum/FPMinNum (shared/functions/float):

- FPMax/FPMin: FPProcessNaNs first (a signalling NaN of op1, then of op2, else op1's
  quiet NaN, else op2's; the result is the quieted NaN, FPCR.DN = 0); otherwise the
  larger (smaller) value, and for two zeros the sign is sign1 AND sign2 (OR for min).
- FPMaxNum/FPMinNum: when neither operand is a signalling NaN, a lone quiet NaN is
  replaced by -Infinity (+Infinity for min) before FPMax/FPMin, so the number wins.

Upstream Ghidra lifts FMAX/FMIN as `Rd = Rn; if (Rn f> Rm) goto inst_next; Rd = Rm;`.
That ignores a NaN in Rn (the comparison is false, so Rm is returned) and, when Rd is
also Rm (`fmax d0, d1, d0`), overwrites Rm before comparing, so the result is always
Rn. FMAXNM uses the same body, which returns a lone quiet NaN; FMINNM is the userop
NEON_fminnm. All twelve scalar constructors (H, S and D) now lift the pseudocode with
the operands copied first (fission-systems/Fission#1063; native evidence from an
Apple M4, the native differential).

The vector forms stay userops (#1058) and are not touched.

Usage: apply_aarch64_fmax_fmin_scalar.py IN.sinc OUT.sinc   (IN is AARCH64neon.sinc)
It is idempotent.
"""
import re
import sys

SIZES = {
    "FPR64": (8, "zd", 0x0008000000000000, 0x7FF0000000000000),
    "FPR32": (4, "zs", 0x00400000, 0x7F800000),
    "FPR16": (2, "zh", 0x0200, 0x7C00),
}
OPS = {
    "fmax": ("f>", "&", None),
    "fmin": ("f<", "|", None),
    "fmaxnm": ("f>", "&", "neg"),
    "fminnm": ("f<", "|", "pos"),
}
MARK = "# Fission: Arm ARM FPMax/FPMin pseudocode"


def body(op, reg):
    n, zext, quiet, inf = SIZES[reg]
    cmp, zero_combine, nm = OPS[op]
    lines = [
        "{",
        f"\t{MARK}",
        f"\tlocal fa:{n} = Rn_{reg};",
        f"\tlocal fb:{n} = Rm_{reg};",
        f"\tlocal fq:{n} = {quiet:#x};",
        f"\tlocal fr:{n} = fb;",
    ]
    if nm:
        replacement = inf | (1 << (8 * n - 1)) if nm == "neg" else inf
        lines += [
            "\t# FPMaxNum/FPMinNum: with no signalling NaN, a lone quiet NaN gives way to the number",
            "\tlocal fsnan:1 = (nan(fa) && ((fa & fq) == 0)) || (nan(fb) && ((fb & fq) == 0));",
            "\tif (fsnan) goto <fp_compare>;",
            "\tif (!nan(fa) || nan(fb)) goto <fp_nm_b>;",
            f"\tfa = {replacement:#x};",
            "<fp_nm_b>",
            "\tif (!nan(fb) || nan(fa)) goto <fp_compare>;",
            f"\tfb = {replacement:#x};",
            "<fp_compare>",
        ]
    lines += [
        "\tif (nan(fa) || nan(fb)) goto <fp_nan>;",
        f"\tif (fa {cmp} fb) goto <fp_a>;",
        "\tfr = fb;",
        "\tif (fa f!= fb) goto <fp_done>;",
        f"\tfr = fa {zero_combine} fb; # two zeros: combine the signs",
        "\tgoto <fp_done>;",
        "<fp_a>",
        "\tfr = fa;",
        "\tgoto <fp_done>;",
        "<fp_nan>",
        "\t# FPProcessNaNs: signalling op1, signalling op2, quiet op1, quiet op2; quieted",
        "\tfr = fa;",
        "\tif (nan(fa) && ((fa & fq) == 0)) goto <fp_quiet>;",
        "\tfr = fb;",
        "\tif (nan(fb) && ((fb & fq) == 0)) goto <fp_quiet>;",
        "\tfr = fa;",
        "\tif (nan(fa)) goto <fp_quiet>;",
        "\tfr = fb;",
        "<fp_quiet>",
        "\tfr = fr | fq;",
        "<fp_done>",
        f"\tRd_{reg} = fr;",
        f"\tzext_{zext}(Zd); # zero the rest of Zd",
        "}",
    ]
    return "\n".join(lines) + "\n"


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    for op in OPS:
        for reg in SIZES:
            head = f":{op} Rd_{reg}, Rn_{reg}, Rm_{reg}\n"
            ctor = re.compile("^" + re.escape(head) + r"(is [^\n]*& Zd\n)\{\n.*?\n\}\n", re.M | re.S)
            found = list(ctor.finditer(text))
            assert len(found) == 1, (op, reg, len(found))
            m = found[0]
            text = text[: m.start()] + head + m.group(1) + body(op, reg) + text[m.end():]
    open(dst, "w").write(text)
    print("applied")


main()
