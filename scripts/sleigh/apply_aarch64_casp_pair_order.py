#!/usr/bin/env python3
"""Compare and store AArch64 64-bit CASP pairs in register order.

Arm ARM (DDI 0487), CASP/CASPA/CASPAL/CASPL, Operation: the 128-bit compare and new
values are `X[s+1]:X[s]` (little-endian) or `X[s]:X[s+1]` (big-endian), so in both
byte orders the doubleword at the lower address pairs with Rs (and Rt) and the one
at the higher address with Rs+1 (Rt+1). Upstream Ghidra's 64-bit constructor swaps
the pairs for little-endian ("swap Xss/Xs and Xtt/Xt"), so it compares the first
doubleword with Rs+1 and stores Rt+1 there. An Apple M4 run natively stores the
pair when memory equals {Rs, Rs+1} and writes Rt at the lower address; the lift
left memory unchanged (found once fission-systems/Fission#1391 made the registers
right).

The swap block is removed. The 32-bit form already builds its 64-bit value by byte
order and is not touched.

Usage: apply_aarch64_casp_pair_order.py IN.sinc OUT.sinc   (IN is AARCH64base.sinc)
It is idempotent.
"""
import sys

SWAP = """@if DATA_ENDIAN == "little"
	# for little endian, swap Xss/Xs and Xtt/Xt
	tmp_s = aa_Xss;
	tmp_ss = aa_Xs;
	tmp_t = aa_Xtt;
	tmp_tt = aa_Xt;
@endif

"""


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    if SWAP not in text:
        assert "swap Xss/Xs and Xtt/Xt" not in text
        open(dst, "w").write(text)
        print("already applied")
        return
    assert text.count(SWAP) == 1
    open(dst, "w").write(text.replace(SWAP, "", 1))
    print("applied")


main()
