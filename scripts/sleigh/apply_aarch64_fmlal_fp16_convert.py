#!/usr/bin/env python3
"""Fix the AArch64 FMLAL/FMLSL widening multiply-adds and the half-precision fixed-point converts.

Found by the native differential (an Apple M4 host), behind its `~fp16` entry.

1. FMLAL, FMLAL2, FMLSL, FMLSL2 (vector and by element). Arm ARM (DDI 0487), Operation:
   `Elem[result, e, 2*esize] = FPMulAdd(Elem[operand3, e, 2*esize], element1, element2, fpcr)`
   with both elements converted to single precision first (FMLSL negates element1).
   Upstream multiplies in half precision (one rounding), widens, and then accumulates with
   the *integer* `+`/`-`, adding float bit patterns: `fmlal2 v29.2s, v21.2h, v13.2h` wrote
   about -4e33 where the CPU gives a small value. A half-precision product is exact in single
   precision (11 + 11 significant bits, exponents well inside the single range), so each lane
   is now `acc f+ (float2float(a) f* float2float(b))`, which rounds once, as FPMulAdd does;
   FMLSL negates the first element before the multiply, as FPNeg does (a NaN's sign too).

2. SCVTF/UCVTF Hd, Wn|Xn, #fbits. Upstream converts the integer to half precision, which
   overflows to infinity above 65504, and divides by 2^fbits as a half-precision constant,
   which is infinity for fbits >= 16, so `scvtf h6, x10, #53` gave NaN where the CPU gives a
   subnormal. The integer is now converted to double, divided by 2^fbits in double (exact, a
   power of two) and rounded to half precision once. A 32-bit source is exact in double, so
   the result is exactly FixedToFP's; a 64-bit source with more than 53 significant bits is
   rounded twice (to double, then to half), which can differ from the hardware in rare ties.

3. The by-element FMLAL/FMLAL2/FMLSL/FMLSL2 take the element index from H:L:M (Arm ARM,
   "<index> encoded in H:L:M", Rm restricted to V0-V15). They are the only users of the
   `b_2223=2` rules of `vIndexHLM` and `Re_VPR128Lo.H.sel` (AARCH64instructions.sinc),
   which upstream decodes as H:L, as for 32-bit elements: `fmlal v0.4s, v1.4h, v2.h[3]`
   disassembled as `v2.H[0x1]` and read element 1. Those rules now use H:L:M like the
   others (both endiannesses).

Usage: apply_aarch64_fmlal_fp16_convert.py NEON.sinc INSTRUCTIONS.sinc
(AARCH64neon.sinc and AARCH64instructions.sinc, rewritten in place). It is idempotent.
"""
import re
import sys

MARK = "# Fission: FPMulAdd on single-precision operands"
CTOR = re.compile(r"^(:(?:fmlal2?|fmlsl2?) [^\n]*\n)(is [^\n]*\n)\{\n(.*?)\n\}\n", re.M | re.S)
MUL = re.compile(r"^\t(?P<dst>TMP\w+\[\d+,16\]) = (?P<a>[^;]+?) f\* (?P<b>[^;]+?);$")
WIDEN = re.compile(r"^\t(?P<dst>TMP\w+\[\d+,32\]) = float2float\((?P<src>TMP\w+\[\d+,16\])\);$")
ACC = re.compile(r"^\t(?P<dst>Rd_\S+\[\d+,32\]) = (?P=dst) (?P<op>[+-]) (?P<p>TMP\w+\[\d+,32\]);$")


def fix_fmlal(m):
    head, pat, body = m.groups()
    if MARK in body:
        return m.group(0)
    muls, out, counts = {}, [], [0, 0, 0]
    # FMLSL negates element1 (FPNeg, which also flips a NaN's sign) and then adds.
    negate = head.startswith(":fmlsl")
    for line in body.split("\n"):
        mm, wm, am = MUL.match(line), WIDEN.match(line), ACC.match(line)
        if mm:
            muls[mm.group("dst")] = (mm.group("a"), mm.group("b"))
            counts[0] += 1
            continue
        if wm:
            a, b = muls[wm.group("src")]
            first = f"(f- float2float({a}))" if negate else f"float2float({a})"
            out.append(f"\t{wm.group('dst')} = {first} f* float2float({b});")
            counts[1] += 1
            continue
        if am:
            assert am.group("op") == ("-" if negate else "+"), line
            out.append(f"\t{am.group('dst')} = {am.group('dst')} f+ {am.group('p')};")
            counts[2] += 1
            continue
        if line.lstrip().startswith("# simd infix TMP") and " f* " in line:
            continue
        out.append(line)
    assert counts[0] == counts[1] == counts[2] and counts[0] in (2, 4), (head, counts)
    return head + pat + "{\n\t" + MARK + "\n" + "\n".join(out) + "\n}\n"


CONVERT = re.compile(
    r"^(?P<head>:(?P<op>[su])cvtf Rd_FPR16, Rn_GPR(?P<w>32|64), FBitsOp\n)(?P<pat>is [^\n]*& FBits16[^\n]*)\n"
    r"\{\n(?P<body>.*?)\n\}\n",
    re.M | re.S,
)
FIXED_MARK = "# Fission: FixedToFP through double"


def fix_convert(m):
    if FIXED_MARK in m.group("body"):
        return m.group(0)
    op, w = m.group("op"), m.group("w")
    src = f"Rn_GPR{w}"
    if op == "u":
        conv = [f"\tlocal fxi:{8 if w == '32' else 9} = zext({src});", "\tlocal fxd:8 = int2float(fxi);"]
    else:
        conv = [f"\tlocal fxd:8 = int2float({src});"]
    body = "\n".join(
        ["{", "\t" + FIXED_MARK] + conv + ["\tfxd = fxd f/ FBits64;", "\tRd_FPR16 = float2float(fxd);",
         "\tzext_zh(Zd); # zero upper 30 bytes of Zd", "}"]
    )
    pat = m.group("pat").replace("& FBits16", "& FBits16 & FBits64", 1)
    return m.group("head") + pat + "\n" + body + "\n"


INDEX_EDITS = [
    (
        "vIndexHLM: val is b_2223=2 & b_2121 & b_1111 [ val = b_1111 << 1 | b_2121; ] { export *[const]:8 val; }",
        "vIndexHLM: val is b_2223=2 & b_2121 & b_1111 & b_2020 [ val = b_1111 << 2 | b_2121 << 1 | b_2020; ] { export *[const]:8 val; }",
    ),
    (
        "Re_VPR128Lo.H.sel: Re_VPR128Lo, val is Re_VPR128Lo & b_2223=2 & b_2121 & b_1111 [ val = 0x5000 + 32*Re_VPR128Lo + (b_1111 * 2 + b_2121)*2; ] { export *[register]:2 val; }",
        "Re_VPR128Lo.H.sel: Re_VPR128Lo, val is Re_VPR128Lo & b_2223=2 & b_2121 & b_1111 & b_2020 [ val = 0x5000 + 32*Re_VPR128Lo + (b_1111*4 + b_2121*2 + b_2020)*2; ] { export *[register]:2 val; }",
    ),
    (
        "Re_VPR128Lo.H.sel: Re_VPR128Lo, val is Re_VPR128Lo & b_2223=2 & b_2121 & b_1111 [ val = 0x501e + 32*Re_VPR128Lo - (b_1111 * 2 + b_2121)*2; ] { export *[register]:2 val; }",
        "Re_VPR128Lo.H.sel: Re_VPR128Lo, val is Re_VPR128Lo & b_2223=2 & b_2121 & b_1111 & b_2020 [ val = 0x501e + 32*Re_VPR128Lo - (b_1111*4 + b_2121*2 + b_2020)*2; ] { export *[register]:2 val; }",
    ),
]


def main():
    neon, instructions = sys.argv[1:3]
    text = open(neon).read()
    text, n1 = CTOR.subn(fix_fmlal, text)
    text, n2 = CONVERT.subn(fix_convert, text)
    assert (n1, n2) == (16, 4), (n1, n2)
    open(neon, "w").write(text)
    text = open(instructions).read()
    for old, new in INDEX_EDITS:
        if new in text and old not in text:
            continue
        assert text.count(old) == 1, old
        text = text.replace(old, new)
    open(instructions, "w").write(text)
    print("applied")


main()
