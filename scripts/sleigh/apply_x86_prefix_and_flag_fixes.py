#!/usr/bin/env python3
"""Fix x86 prefix decoding and three flag results in ia.sinc.

Each was found by running single instructions on an x86-64 host against Fission's
lift (the native differential, crates/fission-emulator/tests/native_differential).

1. One-byte 0x90 with a REX prefix (fission-systems/Fission#1059). Intel SDM Vol. 2B,
   XCHG: "XCHG (E)AX, (E)AX (encoded instruction byte is 90H) is an alias for NOP
   regardless of data size prefixes, including REX.W", and `90+rd` takes REX.B to
   select R8..R15 (Vol. 2A, 2.2.1.2). Upstream treats 0x90 as NOP only with no REX or
   with REX.W alone, so `42 90` (REX.X) lifts as `xchg eax, eax` and clears RAX[63:32].
   NOP now requires only REX.B = 0, and PAUSE (`F3 90`) likewise, so `f3 41 90` is
   `xchg r8d, eax`.

2. A repeated operand-size prefix (#1061). SDM Vol. 2A, 2.1.1: a prefix selects its
   group's behaviour whether it appears once or more; `66 66 03 e8` executes as
   `add bp, ax`. Upstream toggles `opsize` (`$xor 1`) at every 0x66, so two cancel.
   The first 0x66 now records itself in `prefix_66` and later ones change nothing.

3. SHR and SHRD with a masked count of 0 (#1051). SDM Vol. 2B, SAL/SAR/SHL/SHR and
   SHRD, Flags Affected: "If the count is 0, the flags are not affected"; OF is the
   original operand's sign bit for a 1-bit SHR and the sign change for a 1-bit SHRD.
   Upstream's `shrflags`/`shrdflags` keyed OF on `count==1` with the operands swapped
   (count 1 gave 0, count 0 rewrote OF). They now keep OF when the count is 0 and
   compute it otherwise (it is undefined for counts above 1).

4. PUSHF/PUSHFD/PUSHFQ store reserved bit 1 as 1 (#1052): SDM Vol. 1, 3.4.3, Figure
   3-8 (EFLAGS bit 1 is reserved and reads as 1). `packflags` now sets it, as LAHF
   already does.

Usage: apply_x86_prefix_and_flag_fixes.py IN.sinc OUT.sinc   (IN is ia.sinc)
It is idempotent. Recompile x86.slaspec and x86-64.slaspec with the Ghidra
`support/sleigh` of the same release and refresh MANIFEST.sha256.json.
"""
import sys

EDITS = [
    (
        "NOP with REX",
        ":NOP            is vexMode=0 & byte=0x90 & (mandover=0 | mandover=4 | mandover=1) & (rexprefix=0 | rexWRXBprefix=8 | rexWRXBprefix=0)  { }\n",
        ":NOP            is vexMode=0 & byte=0x90 & (mandover=0 | mandover=4 | mandover=1) & (rexprefix=0 | rexBprefix=0)  { }\n",
    ),
    (
        "PAUSE opsize=0",
        ":PAUSE          is vexMode=0 & opsize=0 & $(PRE_F3) & byte=0x90     {  }\n",
        ":PAUSE          is vexMode=0 & opsize=0 & $(PRE_F3) & (rexprefix=0 | rexBprefix=0) & byte=0x90     {  }\n",
    ),
    (
        "PAUSE opsize=1",
        ":PAUSE          is vexMode=0 & opsize=1 & $(PRE_F3) & byte=0x90     {  }\n",
        ":PAUSE          is vexMode=0 & opsize=1 & $(PRE_F3) & (rexprefix=0 | rexBprefix=0) & byte=0x90     {  }\n",
    ),
    (
        "repeated 0x66",
        ":^instruction is instrPhase=0 & over=0x66; instruction     [ opsize=opsize $xor 1; mandover = mandover $xor 1; ] {} # Operand size override\n",
        ":^instruction is instrPhase=0 & over=0x66 & prefix_66=0; instruction     [ opsize=opsize $xor 1; mandover = mandover $xor 1; ] {} # Operand size override\n"
        ":^instruction is instrPhase=0 & over=0x66 & prefix_66=1; instruction     {} # A repeated operand size override changes nothing\n",
    ),
    (
        "shrflags OF",
        " conditionalAssign(OF, count==1, 0, (op1 s< 0)); \n",
        " conditionalAssign(OF, count==0, OF, (op1 s< 0)); \n",
    ),
    (
        "shrdflags OF",
        " conditionalAssign(OF, count==1, 0, ((op1 s< 0) ^ (result s< 0))); \n",
        " conditionalAssign(OF, count==0, OF, ((op1 s< 0) ^ (result s< 0))); \n",
    ),
    (
        "packflags bit 1",
        "                | (0x0004 * zext(PF&1)) | (0x0001 * zext(CF&1));\n",
        "                | (0x0004 * zext(PF&1)) | 0x0002 | (0x0001 * zext(CF&1));\n",
    ),
]


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    for what, old, new in EDITS:
        if new in text and old not in text:
            continue
        assert text.count(old) == 1, f"{what}: found {text.count(old)} times"
        text = text.replace(old, new)
    open(dst, "w").write(text)
    print("applied")


main()
