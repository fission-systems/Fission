#!/usr/bin/env python3
"""Set C from the shifter in Thumb-2 ANDS/BICS/MOVS/MVNS (modified immediate) and
ANDS/MVNS (shifted register).

ARMv7-M ARM, AND (immediate) T1: `(imm32, carry) = ThumbExpandImm_C(i:imm3:imm8,
APSR.C);` and, when setflags, `APSR.N = result<31>; APSR.Z = IsZeroBit(result);
APSR.C = carry;` (V unchanged). BIC/MOV/MVN (immediate) are the same, and AND/MVN
(register) take `(shifted, carry) = Shift_C(R[m], shift_t, shift_n, APSR.C)`.
ThumbExpandImm_C returns the input carry for the unrotated patterns (imm12<11:10> ==
'00') and bit 31 of the rotated constant otherwise; Shift_C returns the last bit
shifted out, or the input carry for a zero shift.

Upstream Ghidra already computes that carry: every `ThumbExpandImm12` and `thshift2`
constructor sets the `shift_carry` register. These six constructors, though, build
`thSBIT_ZN`, which sets only Z and N, so C kept its old value:

    and  Rd, Rn, #imm      and.w Rd, Rn, Rm, shift      bic Rd, Rn, #imm
    mov.w Rd, #imm         mvn  Rd, #imm                mvn.w Rd, Rm, shift

They now build a new flag table, `thSBIT_SCZN`, whose "s" form sets
`CY = shift_carry` besides Z and N and whose plain form does nothing. Only the
flag-setting forms change; the p-code of the forms without S stays byte-for-byte
upstream's. (Upstream's other logical ops instead copy `tmpCY = shift_carry;` in the
body and build `thSBIT_CZN`; doing that here would also add a dead `tmpCY` write to
every non-flag-setting `and`/`bic`/`mov.w`/`mvn`, which changes nothing
architecturally but does change what a decompiler sees.) MOVS.W of an unshifted
register and the 16-bit forms are not touched.

Upstream report: NationalSecurityAgency/ghidra#10274.

Usage: apply_thumb2_logical_carry.py IN.sinc OUT.sinc   (IN is ARMTHUMBinstructions.sinc)
It is idempotent. After it, recompile the ARM*.slaspec with the Ghidra
`support/sleigh` of the same release as the specs and refresh MANIFEST.sha256.json.
"""
import re
import sys

TABLE_ANCHOR = 'thSBIT_ZN: "s" is thc0404=1\t    { ZR = tmpZR; NG = tmpNG; }\n'
TABLE = (
    "# Fission: N, Z and the shifter's carry-out (see apply_thumb2_logical_carry.py).\n"
    "thSBIT_SCZN:     is thc0404=0\t    { }   # Do nothing to the flag bits\n"
    'thSBIT_SCZN: "s" is thc0404=1\t    { CY = shift_carry; ZR = tmpZR; NG = tmpNG; }\n'
)
# The six constructors, by the start of their header line.
HEADS = [
    r":and\^thSBIT_ZN\^ItCond\tRd0811,Rn0003,ThumbExpandImm12 ",
    r':and\^thSBIT_ZN\^ItCond\^"\.w"\tRd0811,Rn0003,thshift2 ',
    r":bic\^thSBIT_ZN\^ItCond   Rd0811,Rn0003,ThumbExpandImm12 ",
    r':mov\^thSBIT_ZN\^ItCond\^"\.w"  Rd0811,ThumbExpandImm12 ',
    r":mvn\^thSBIT_ZN\^ItCond  Rd0811,ThumbExpandImm12 ",
    r':mvn\^thSBIT_ZN\^ItCond\^"\.w"  Rd0811,thshift2 ',
]
CTOR = re.compile(r"^(?:" + "|".join(HEADS) + r")[^\n]*\n(?:\{\n)?(?:[^\n]*\n)*?\}\n", re.M)


def fix(match):
    ctor = match.group(0)
    assert ctor.count("thSBIT_ZN") == 3, ctor  # display, pattern, build
    assert "shift_carry" not in ctor and ("ThumbExpandImm12" in ctor or "thshift2" in ctor)
    return ctor.replace("thSBIT_ZN", "thSBIT_SCZN")


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    if TABLE in text:
        assert not CTOR.search(text) and text.count("build thSBIT_SCZN;") == len(HEADS)
        open(dst, "w").write(text)
        print("already applied")
        return
    assert text.count(TABLE_ANCHOR) == 1, "thSBIT_ZN table not found"
    text = text.replace(TABLE_ANCHOR, TABLE_ANCHOR + TABLE)
    text, count = CTOR.subn(fix, text)
    assert count == len(HEADS), f"rewrote {count} constructors, expected {len(HEADS)}"
    open(dst, "w").write(text)
    print("applied")


main()
