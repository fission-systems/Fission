#!/usr/bin/env python3
"""Make the AArch64 FRINTN/FRINTI/FRINTX/FRINTZ and FRINT32/64 lifts exact.

Run after apply_aarch64_frint_rounding.py, whose FRINT{N,I,X} are FLOAT_ROUND (ties away
from zero) and whose FRINTZ is an integer round trip (`int2float(trunc(x))`), which loses
the sign of -0.0 and saturates at 2^63. An Apple M4 run natively shows both
(fission-systems/Fission#1064). Arm ARM (DDI 0487) FPRoundInt: FRINTN rounds to nearest
with ties to even, FRINTI/FRINTX use FPCR.RMode (round to nearest, ties to even, when
RMode = 00, the reset value and the Linux/macOS default; other modes are not modelled),
FRINTZ toward zero; a zero result keeps the operand's sign and an integral or infinite
operand is returned unchanged. FLOOR, CEIL, subtraction of the floor and halving are exact
in IEEE arithmetic, so both roundings are built from them:

  ties to even:  f = floor(x); d = x - f;  up = d > 0.5 || (d == 0.5 && f is odd)
                 (f is odd iff f/2 - floor(f/2) != 0);  r = (f + (up ? 1.0 : 0.0)) | sign(x)
  toward zero:   r = x < 0 ? ceil(x) : floor(x)   (selected with a mask on the sign bit)

FRINT32Z/FRINT32X/FRINT64Z/FRINT64X (FEAT_FRINTTS; #1070) were lifted upstream as
`trunc`, integer bits in a float register (and the scalar FRINT32Z left the register
unchanged when run). FPRoundIntN rounds the same way (toward zero for Z, FPCR for X) and
returns the most negative N-bit integer, as a float, for a NaN, an infinity or a result
outside the N-bit range. They now do that.

Floats are bit patterns in p-code, so 0.5, 1.0 and the range limits are written as
integer constants of the lane's width.

Usage: apply_aarch64_frint_exact.py IN.sinc OUT.sinc   (IN is AARCH64neon.sinc, after
apply_aarch64_frint_rounding.py). It is idempotent.
"""
import re
import sys

# lane bits -> (half, one, sign) bit patterns
CONST = {
    16: (0x3800, 0x3C00, 0x8000),
    32: (0x3F000000, 0x3F800000, 0x80000000),
    64: (0x3FE0000000000000, 0x3FF0000000000000, 0x8000000000000000),
}
# lane bits -> {N: (-2^(N-1), 2^(N-1))} as floats of the lane's width
LIMIT = {
    32: {32: (0xCF000000, 0x4F000000), 64: (0xDF000000, 0x5F000000)},
    64: {32: (0xC1E0000000000000, 0x41E0000000000000), 64: (0xC3E0000000000000, 0x43E0000000000000)},
}
MARK = "# Fission: exact FPRoundInt"


def lane_bits(lhs):
    m = re.search(r"\[\d+,(\d+)\]$", lhs)
    if m:
        return int(m.group(1))
    return int(re.search(r"FPR(\d+)", lhs).group(1))


def round_even(k, n, arg):
    half, one, sign = CONST[n]
    b = n // 8
    return [
        f"\tlocal fx{k}:{b} = {arg};",
        f"\tlocal ff{k}:{b} = floor(fx{k});",
        f"\tlocal fd{k}:{b} = fx{k} f- ff{k};",
        f"\tlocal fh{k}:{b} = ff{k} f* {half:#x}:{b};",
        f"\tlocal fu{k}:1 = (fd{k} f> {half:#x}:{b}) || ((fd{k} f== {half:#x}:{b}) && ((fh{k} f- floor(fh{k})) f!= 0:{b}));",
        f"\tlocal fr{k}:{b} = (ff{k} f+ (zext(fu{k}) * {one:#x}:{b})) | (fx{k} & {sign:#x}:{b});",
    ]


def round_zero(k, n, arg):
    b = n // 8
    return [
        f"\tlocal fx{k}:{b} = {arg};",
        f"\tlocal fm{k}:{b} = zext(fx{k} s< 0);",
        f"\tfm{k} = 0 - fm{k};",
        f"\tlocal fr{k}:{b} = (floor(fx{k}) & ~fm{k}) | (ceil(fx{k}) & fm{k});",
    ]


def clamp(k, n, bits):
    lo, hi = LIMIT[n][bits]
    b = n // 8
    return [
        f"\tlocal fk{k}:{b} = zext((fr{k} f>= {lo:#x}:{b}) && (fr{k} f< {hi:#x}:{b}));",
        f"\tfk{k} = 0 - fk{k};",
        f"\tfr{k} = (fr{k} & fk{k}) | ({lo:#x}:{b} & ~fk{k});",
    ]


FRINT = re.compile(r"^:frint\^frint_([vs])mode_([nixz]) [^\n]*\nis [^\n]*\n\{\n(?:\t[^\n]*\n|\n)*?\}\n", re.M)
ROUND_STMT = re.compile(r"^\t(?P<lhs>[^=\n]+?) = round\((?P<arg>[^()\n]*)\);$")
Z_STMT = re.compile(r"^\tlocal fz\d+:\d+ = trunc\((?P<arg>[^()\n]*)\);\n\t(?P<lhs>[^=\n]+?) = int2float\(fz\d+\);$", re.M)


def fix_frint(m):
    c = m.group(0)
    if MARK in c:
        return c
    mode = m.group(2)
    k = [0]

    def emit(lhs, arg, gen):
        n = lane_bits(lhs)
        lines = gen(k[0], n, arg) + [f"\t{lhs} = fr{k[0]};"]
        k[0] += 1
        return "\n".join(lines)

    if mode == "z":
        c, count = Z_STMT.subn(lambda s: emit(s.group("lhs"), s.group("arg"), round_zero), c)
    else:
        out, count = [], 0
        for line in c.split("\n"):
            s = ROUND_STMT.match(line)
            if s:
                out.append(emit(s.group("lhs"), s.group("arg"), round_even))
                count += 1
            else:
                out.append(line)
        c = "\n".join(out)
    assert count, m.group(0)
    return c.replace("{\n", "{\n\t" + MARK + "\n", 1)


# FRINT32Z/X, FRINT64Z/X: `lhs = trunc(arg);`, or `result = trunc(arg);` then `lhs = zext(result)` / `result[0,N]`
NN = re.compile(r"^:frint(32|64)([xz]) [^\n]*\nis [^\n]*\n\{\n(?:\t[^\n]*\n|\n)*?\}\n", re.M)
NN_DIRECT = re.compile(r"^\t(?P<lhs>[^=\n]+?) = trunc\((?P<arg>[^()\n]*)\);$")
NN_VIA = re.compile(r"^\t(?:local )?result(?::\d)? = trunc\((?P<arg>[^()\n]*)\);$")
NN_VIA_WRITE = re.compile(r"^\t(?P<lhs>[^=\n]+?) = (?:zext\(result\)|result\[0,\d+\]);$")


def fix_nn(m):
    c = m.group(0)
    if MARK in c:
        return c
    bits, mode = int(m.group(1)), m.group(2)
    gen = round_zero if mode == "z" else round_even
    out, k, pending = [], 0, None
    for line in c.split("\n"):
        d, v, w = NN_DIRECT.match(line), NN_VIA.match(line), NN_VIA_WRITE.match(line)
        if v:
            pending = v.group("arg")
            continue
        if w and pending is not None:
            lhs, arg = w.group("lhs"), pending
            pending = None
        elif d:
            lhs, arg = d.group("lhs"), d.group("arg")
        else:
            out.append(line)
            continue
        n = lane_bits(lhs)
        out.append("\n".join(gen(k, n, arg) + clamp(k, n, bits) + [f"\t{lhs} = fr{k};"]))
        k += 1
    assert k and pending is None, m.group(0)
    c = "\n".join(out).replace("{\n", "{\n\t" + MARK + "\n", 1)
    # V[d, 128] = result: the rest of the register is zeroed (upstream wrote the lanes only)
    dest = re.match(r":frint\S+ Rd_(FPR32|FPR64|VPR64|VPR128)", c).group(1)
    zext = {"FPR32": "zs", "FPR64": "zd", "VPR64": "zd", "VPR128": "zq"}[dest]
    head, pat, rest = c.split("\n", 2)
    if not pat.endswith("& Zd"):
        pat += " & Zd"
    rest = rest[: rest.rindex("}")] + f"\tzext_{zext}(Zd); # zero the rest of Zd\n}}\n"
    return "\n".join([head, pat, rest])


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    assert "frint_vmode_n" in text, "run apply_aarch64_frint_rounding.py first"
    text, n1 = FRINT.subn(fix_frint, text)
    text, n2 = NN.subn(fix_nn, text)
    assert n1 == 4 * (5 + 3) and n2 == 20, (n1, n2)
    open(dst, "w").write(text)
    print("applied")


main()
