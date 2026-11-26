#!/usr/bin/env python3
"""Set AF in x86 arithmetic and make the SSE float-to-integer conversions exact.

Both were found by running single instructions on an x86-64 host against Fission's
lift (the native differential, crates/fission-emulator/tests/native_differential).

1. AF (fission-systems/Fission#1048). Intel SDM Vol. 1, 3.4.3.1: AF is "set if an
   arithmetic operation generates a carry or a borrow out of bit 3 of the result";
   Vol. 2, ADD/ADC/SUB/SBB/CMP/CMPS/SCAS/NEG/INC/DEC/XADD/CMPXCHG, Flags Affected:
   "The OF, SF, ZF, AF, CF, and PF flags are set according to the result." Upstream
   never writes AF ("# AF not implemented"). The carry into bit 4 of a + b (+ c) is
   bit 4 of a ^ b ^ result, for subtraction too (a borrow), so `addflags`, `subflags`,
   `addCarryFlags`, `subCarryFlags` and `negflags` (a negation is 0 - a) now set
   AF = ((a ^ b ^ result) & 0x10) != 0 from their operands, before the destination is
   written. INC and DEC are written inline: INC carries out of bit 3 exactly when the
   low nibble was 0xF, DEC borrows exactly when it was 0, and they now say so.

2. CVTSD2SI/CVTSS2SI/CVTPD2DQ/CVTPS2DQ round by MXCSR.RC (#1049), and they and the
   truncating CVTT* forms return the integer indefinite (#1050). SDM Vol. 2,
   CVTSD2SI: "When a conversion is inexact, the value returned is rounded according to
   the rounding control bits in the MXCSR register. If a converted result exceeds the
   range limits of signed doubleword integer (in non-64-bit modes or 64-bit mode with
   REX.W/VEX.W/EVEX.W=0), the floating-point invalid exception is raised, and if this
   exception is masked, the indefinite integer value (80000000H) is returned"
   (8000000000000000H for a quadword); a NaN is invalid too. Upstream lifted the
   rounding forms as `trunc(round(x))` (FLOAT_ROUND rounds ties away from zero),
   CVTPD2DQ/CVTPS2DQ as a bare `trunc` (no rounding at all), and every form as a
   `trunc` whose out-of-range result p-code leaves undefined.
   Rounding is to nearest with ties to even, MXCSR.RC = 00, the reset value and the
   Linux/macOS/Windows default; other modes are not modelled (MXCSR is not a modelled
   register). It is built exactly from FLOOR, as apply_aarch64_frint_exact.py does:
     f = floor(x); d = x - f; up = d > 0.5 || (d == 0.5 && f is odd)  (f/2 - floor(f/2) != 0)
     r = f + (up ? 1.0 : 0.0)
   The result is valid iff -2^(N-1) <= r < 2^(N-1) (false for a NaN or an infinity);
   for the truncating forms iff the operand truncates into that range, i.e.
   x > -2^31 - 1 for a double converted to 32 bits and x >= -2^(N-1) otherwise (no
   float lies strictly between -2^(N-1) - 1 and -2^(N-1) there). An invalid result is
   replaced by the indefinite with a mask, so the lift stays branch-free.
   Floats are bit patterns in p-code, so 0.5, 1.0 and the limits are integer constants
   of the operand's width. The VEX/EVEX forms (VCVT*) and the MMX CVT*PI forms are not
   changed.

Usage: apply_x86_af_and_cvt.py LANGDIR   (LANGDIR is utils/sleigh-specs/languages/x86;
edits ia.sinc and lockable.sinc in place). It is idempotent. Recompile x86.slaspec and
x86-64.slaspec with the Ghidra `support/sleigh` of the same release and refresh
MANIFEST.sha256.json.
"""
import os
import re
import sys

MACRO_EDITS = [
    (
        "addflags AF",
        "macro addflags(op1,op2) {\n CF = carry(op1,op2);\n OF = scarry(op1,op2);\n}\n",
        "macro addflags(op1,op2) {\n CF = carry(op1,op2);\n OF = scarry(op1,op2);\n"
        " AF = ((op1 ^ op2 ^ (op1 + op2)) & 0x10) != 0;\n}\n",
    ),
    (
        "subflags AF",
        "macro subflags(op1,op2) {\n CF = op1 < op2;\n OF = sborrow(op1,op2);\n}\n",
        "macro subflags(op1,op2) {\n CF = op1 < op2;\n OF = sborrow(op1,op2);\n"
        " AF = ((op1 ^ op2 ^ (op1 - op2)) & 0x10) != 0;\n}\n",
    ),
    (
        "addCarryFlags AF",
        "\top1 = result + CFcopy;\n\t# AF not implemented\n",
        "\tlocal sum = result + CFcopy;\n\tAF = ((op1 ^ op2 ^ sum) & 0x10) != 0;\n\top1 = sum;\n",
    ),
    (
        "subCarryFlags AF",
        "\top1 = result - CFcopy;\n\t# AF not implemented\n",
        "\tlocal diff = result - CFcopy;\n\tAF = ((op1 ^ op2 ^ diff) & 0x10) != 0;\n\top1 = diff;\n",
    ),
    (
        "negflags AF",
        "macro negflags(op1) {\n CF = (op1 != 0);\n OF = sborrow(0,op1);\n}\n",
        "macro negflags(op1) {\n CF = (op1 != 0);\n OF = sborrow(0,op1);\n"
        " AF = ((op1 ^ (0 - op1)) & 0x10) != 0;\n}\n",
    ),
]

INC = re.compile(r"OF = scarry\((\w+),1\);(?! AF)")
DEC = re.compile(r"OF = sborrow\((\w+),1\);(?! AF)")


def af_inc_dec(text):
    text, n_inc = INC.subn(lambda m: f"{m.group(0)} AF = ({m.group(1)} & 0xf) == 0xf;", text)
    text, n_dec = DEC.subn(lambda m: f"{m.group(0)} AF = ({m.group(1)} & 0xf) == 0;", text)
    return text, n_inc, n_dec


# float bits -> (half, one)
CONST = {32: (0x3F000000, 0x3F800000), 64: (0x3FE0000000000000, 0x3FF0000000000000)}
# (float bits, integer bits) -> (-2^(N-1), 2^(N-1)) as floats
LIMIT = {
    (32, 32): (0xCF000000, 0x4F000000),
    (32, 64): (0xDF000000, 0x5F000000),
    (64, 32): (0xC1E0000000000000, 0x41E0000000000000),
    (64, 64): (0xC3E0000000000000, 0x43E0000000000000),
}
# -2^31 - 1 as a double: the exclusive lower bound of a truncating double -> int32
DOUBLE_TRUNC_I32_LO = 0xC1E0000000200000
MARK = "# Fission: MXCSR rounding and the integer indefinite"
ROUNDING = {"CVTSD2SI", "CVTSS2SI", "CVTPD2DQ", "CVTPS2DQ"}
TRUNCATING = {"CVTTSD2SI", "CVTTSS2SI", "CVTTPD2DQ", "CVTTPS2DQ"}
CTOR = re.compile(r"^:(CVTT?(?:SD2SI|SS2SI|PD2DQ|PS2DQ)) [^\n]*\n\{\n(?:[^\n}]*\n)*?\}\n", re.M)
STMT = re.compile(
    r"^(?P<ind>\s*)(?P<lhs>Reg32|Reg64|XmmReg1?\[\d+,32\]) = trunc\(\s*(?:round\((?P<rarg>[^()]*)\)|(?P<arg>[^()]*?))\s*\);$"
)


def width(operand):
    m = re.search(r"\[\d+,(\d+)\]$", operand)
    if m:
        return int(m.group(1))
    return int(re.fullmatch(r"(?:m|Reg)(32|64)", operand.strip()).group(1))


def convert(k, ind, lhs, arg, rounding):
    fb, ib = width(arg), width(lhs)
    b, n = fb // 8, ib // 8
    lo, hi = LIMIT[(fb, ib)]
    indef = 1 << (ib - 1)
    out = [f"local cx{k}:{b} = {arg};"]
    if rounding:
        half, one = CONST[fb]
        out += [
            f"local cf{k}:{b} = floor(cx{k});",
            f"local cd{k}:{b} = cx{k} f- cf{k};",
            f"local ch{k}:{b} = cf{k} f* {half:#x}:{b};",
            f"local cu{k}:1 = (cd{k} f> {half:#x}:{b}) || ((cd{k} f== {half:#x}:{b}) && ((ch{k} f- floor(ch{k})) f!= 0:{b}));",
            f"local cr{k}:{b} = cf{k} f+ (zext(cu{k}) * {one:#x}:{b});",
            f"local cv{k}:1 = (cr{k} f>= {lo:#x}:{b}) && (cr{k} f< {hi:#x}:{b});",
            f"local ci{k}:{n} = trunc(cr{k});",
        ]
    else:
        low = (
            f"(cx{k} f> {DOUBLE_TRUNC_I32_LO:#x}:{b})"
            if (fb, ib) == (64, 32)
            else f"(cx{k} f>= {lo:#x}:{b})"
        )
        out += [
            f"local cv{k}:1 = {low} && (cx{k} f< {hi:#x}:{b});",
            f"local ci{k}:{n} = trunc(cx{k});",
        ]
    out += [
        f"local cm{k}:{n} = 0 - zext(cv{k});",
        f"{lhs} = (ci{k} & cm{k}) | ({indef:#x}:{n} & ~cm{k});",
    ]
    return "\n".join(ind + line for line in out)


def fix_ctor(m):
    c = m.group(0)
    if MARK in c:
        return c
    rounding = m.group(1) in ROUNDING
    assert rounding or m.group(1) in TRUNCATING
    out, k = [], 0
    for line in c.split("\n"):
        s = STMT.match(line)
        if not s:
            out.append(line)
            continue
        arg = s.group("rarg") if s.group("rarg") is not None else s.group("arg")
        # the scalar rounding forms are trunc(round(x)); the packed ones a bare trunc
        assert (s.group("rarg") is not None) == (rounding and "2SI" in m.group(1)), line
        out.append(convert(k, s.group("ind"), s.group("lhs"), arg.strip(), rounding))
        k += 1
    assert k in (1, 2, 4), c
    return "\n".join(out).replace("{\n", "{\n  " + MARK + "\n", 1)


def main():
    langdir = sys.argv[1]
    ia = os.path.join(langdir, "ia.sinc")
    lock = os.path.join(langdir, "lockable.sinc")
    text = open(ia).read()
    for what, old, new in MACRO_EDITS:
        if new in text and old not in text:
            continue
        assert text.count(old) == 1, f"{what}: found {text.count(old)} times"
        text = text.replace(old, new)
    text, n_inc, n_dec = af_inc_dec(text)
    assert (n_inc, n_dec) in ((0, 0), (6, 6)), (n_inc, n_dec)
    text, n_cvt = CTOR.subn(fix_ctor, text)
    assert n_cvt == 24, n_cvt
    open(ia, "w").write(text)

    text = open(lock).read()
    text, n_inc, n_dec = af_inc_dec(text)
    assert (n_inc, n_dec) in ((0, 0), (4, 4)), (n_inc, n_dec)
    open(lock, "w").write(text)
    print("applied")


main()
