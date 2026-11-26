#!/usr/bin/env python3
"""Negate operands, not results, in the AArch64 scalar fused multiply-adds, and order FADDP .4H.

Found by the native differential (an Apple M4 host) at the nightly case count.

1. FMADD, FMSUB, FNMADD, FNMSUB (H, S, D). Arm ARM (DDI 0487), Operation:
   `FPMulAdd(addend, element1, element2)` with FMSUB negating element1, FNMADD the addend
   and element1, and FNMSUB the addend (each by FPNeg, which flips the sign bit of a NaN
   too). Upstream subtracts instead (`Ra f- (Rm f* Rn)`, `f-(Rn f* Rm) f- Ra`,
   `(Rn f* Rm) f- Ra`), so a propagated NaN kept the wrong sign (`fmsub s0, s1, s2, s3`:
   0x7fcfffa7 on the CPU, 0xffcfffa7 lifted). Each is now `addend f+ (element1 f*
   element2)` with the operand negated first, in FPMulAdd's operand order.

   Not modelled: p-code has no fused operation, so the lift still rounds twice
   (fission-systems/Fission#1072), and the NaN priority of FPProcessNaNs3 (a signalling NaN
   in any operand before a quiet one) follows the product and then the sum.

2. FADDP .4H put the sums of Vm's pairs in the low lanes. The result is the pairwise sums of
   the concatenation Vm:Vn, so Vn's pairs are lanes 0 and 1 (as the .8H form already does).

Usage: apply_aarch64_fpneg_faddp.py IN.sinc OUT.sinc   (IN is AARCH64neon.sinc). It is
idempotent.
"""
import re
import sys

MARK = "# Fission: FPMulAdd operand order and FPNeg"

# mnemonic -> (negate addend, negate element1)
NEGATE = {"fmadd": (False, False), "fmsub": (False, True), "fnmadd": (True, True), "fnmsub": (True, False)}
CTOR = re.compile(
    r"^(?P<head>:(?P<op>fn?m(?:add|sub)) Rd_FPR(?P<n>16|32|64), [^\n]*\nis [^\n]*\n)\{\n(?P<body>(?:\t[^\n]*\n|\n)*?)\}\n",
    re.M,
)


def fix_muladd(m):
    body, op, n = m.group("body"), m.group("op"), m.group("n")
    if MARK in body:
        return m.group(0)
    lines = body.split("\n")
    stmt = [i for i, l in enumerate(lines) if l.startswith(f"\tRd_FPR{n} = ")]
    assert len(stmt) == 1, (op, n, body)
    neg_a, neg_1 = NEGATE[op]
    a = f"(f- Ra_FPR{n})" if neg_a else f"Ra_FPR{n}"
    e1 = f"(f- Rn_FPR{n})" if neg_1 else f"Rn_FPR{n}"
    lines[stmt[0]] = f"\t{MARK}\n\tRd_FPR{n} = {a} f+ ({e1} f* Rm_FPR{n});"
    return m.group("head") + "{\n" + "\n".join(lines) + "}\n"


FADDP = re.compile(r"^(?P<head>:faddp Rd_VPR64\.4H, [^\n]*\nis [^\n]*\n)\{\n(?P<body>(?:\t[^\n]*\n|\n)*?)\}\n", re.M)


def fix_faddp(m):
    body = m.group("body")
    lanes = re.findall(r"local tmp2 = (R[nm])_VPR64", body)
    assert len(lanes) == 1, body
    if lanes[0] == "Rn":
        return m.group(0)
    body = body.replace("Rm_VPR64.4H[", "\0[").replace("Rn_VPR64.4H[", "Rm_VPR64.4H[").replace("\0[", "Rn_VPR64.4H[")
    body = body.replace("f+(Rm_VPR64.4H,Rn_VPR64.4H)", "f+(Rn_VPR64.4H,Rm_VPR64.4H)")
    return m.group("head") + "{\n" + body + "}\n"


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    text, n1 = CTOR.subn(fix_muladd, text)
    text, n2 = FADDP.subn(fix_faddp, text)
    assert (n1, n2) == (12, 1), (n1, n2)
    open(dst, "w").write(text)
    print("applied")


main()
