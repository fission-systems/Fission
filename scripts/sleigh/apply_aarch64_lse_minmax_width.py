#!/usr/bin/env python3
"""Compare AArch64 byte/halfword LDSMAX/LDSMIN/LDUMAX/LDUMIN at the element width.

Arm ARM (DDI 0487), LDSMAXB/LDSMAXH etc. (and the STSMAX/STUMAX aliases), Operation:

    value = X[s, datasize];  data = Mem[address, datasize];
    result = Max(SInt(data), SInt(value))      (UInt for the U forms, Min for MIN)
    Mem[address, datasize] = result

with datasize 8 or 16. Upstream Ghidra compares the zero-extended loaded byte or
halfword with the whole 32-bit Ws, so `ldsmaxalb w2, w3, [x1]` with [x1]=0x54 stores
the low byte of w2 when w2's bits above the byte make it larger (fission-systems/
Fission#1069), and a negative byte never wins a signed comparison. An Apple M4 run
natively keeps 0x54.

The eight `ls_opc1`/`ls_opc2` smax/smin/umax/umin subconstructors now extend both
operands from the element width first:

    local lse_d:4 = sext(tmp_ldWn:1); local lse_v:4 = sext(aa_Ws:1);   (zext for U)

The 32- and 64-bit forms compare whole registers already and are not touched.

Usage: apply_aarch64_lse_minmax_width.py IN.sinc OUT.sinc   (IN is AARCH64base.sinc)
It is idempotent.
"""
import re
import sys

LINE = re.compile(
    r"^(?P<head>ls_opc(?P<n>[12]): \"(?P<op>[su])(?P<mm>max|min)\" is [^{]*\{ build ls_data(?P=n); )"
    r"ls_opc_(?P=op)(?P=mm)\(tmp_ldWn, aa_Ws, tmp_stWn\);",
    re.M,
)


def fix(m):
    n, op, mm = m.group("n"), m.group("op"), m.group("mm")
    ext = "sext" if op == "s" else "zext"
    return (
        m.group("head")
        + f"local lse_d:4 = {ext}(tmp_ldWn:{n}); local lse_v:4 = {ext}(aa_Ws:{n}); "
        + f"ls_opc_{op}{mm}(lse_d, lse_v, tmp_stWn);"
    )


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    if text.count("local lse_d:4 = ") == 8 and not LINE.search(text):
        open(dst, "w").write(text)
        print("already applied")
        return
    text, count = LINE.subn(fix, text)
    assert count == 8, f"rewrote {count} subconstructors, expected 8"
    open(dst, "w").write(text)
    print("applied")


main()
