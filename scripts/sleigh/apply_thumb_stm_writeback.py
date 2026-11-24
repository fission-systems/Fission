#!/usr/bin/env python3
"""Always write the base back in the 16-bit Thumb `stmia Rn!, {list}` (STM T1).

ARMv7-M ARM, STM encoding T1: `n = UInt(Rn); registers = '00000000':register_list;
wback = TRUE;` and the operation ends with `if wback then R[n] = R[n] +
4*BitCount(registers);`. A listed Rn stores its own value when it is the lowest
register in the list and an UNKNOWN value otherwise; the writeback happens either way.
Only LDM T1 makes the writeback depend on the list (`wback = (registers<n> == '0')`).

Upstream Ghidra gives STM T1 LDM's operand pair `Rn_exclaim` / `Rn_exclaim_WB`, whose
eight list-specific variants display `Rn` without `!` and skip the writeback when Rn is
in the list. The constructor now displays `Rn!` and writes `mult_addr` back always:

    :stmia^ItCond	Rn0810!,stbrace	is TMode=1 & ItCond & op11=0x18 & Rn0810 & stbrace
    { build ItCond; mult_addr = Rn0810; build stbrace; Rn0810 = mult_addr; }

The stores read Rn before the writeback, so a listed Rn stores its entry value. LDM T1
(`ldmia`), Thumb-2 LDM/STM and ARM-mode LDM/STM are not touched.

Upstream report: NationalSecurityAgency/ghidra#10273.

Usage: apply_thumb_stm_writeback.py IN.sinc OUT.sinc   (IN is ARMTHUMBinstructions.sinc)
It is idempotent. After it, recompile the ARM*.slaspec that include it (every one with
T_VARIANT) with the Ghidra `support/sleigh` of the same release as the specs and
refresh MANIFEST.sha256.json.
"""
import sys

UPSTREAM = """:stmia^ItCond	Rn_exclaim,stbrace	is TMode=1 & ItCond & op11=0x18 & Rn_exclaim & stbrace & Rn_exclaim_WB
{
  build ItCond;
  build Rn_exclaim;
  build stbrace;
  build Rn_exclaim_WB;
}
"""
FIXED = """# Fission: STM (T1) writes the base back even when it is in the list; only LDM (T1)
# skips that (see scripts/sleigh/apply_thumb_stm_writeback.py).
:stmia^ItCond	Rn0810!,stbrace	is TMode=1 & ItCond & op11=0x18 & Rn0810 & stbrace
{
  build ItCond;
  mult_addr = Rn0810;
  build stbrace;
  Rn0810 = mult_addr;
}
"""


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    if FIXED in text:
        assert UPSTREAM not in text
        open(dst, "w").write(text)
        print("already applied")
        return
    assert text.count(UPSTREAM) == 1, "upstream stmia (T1) constructor not found exactly once"
    text = text.replace(UPSTREAM, FIXED)
    # ldmia (T1) keeps the list-dependent writeback.
    assert "& Rn_exclaim & ldbrace & Rn_exclaim_WB" in text
    open(dst, "w").write(text)
    print("applied")


main()
