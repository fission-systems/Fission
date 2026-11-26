#!/usr/bin/env python3
"""Fix two x86 memory-operand exchange lifts in lockable.sinc.

1. XADD m8/m16/m32/m64, r (fission-systems/Fission#1053). Intel SDM Vol. 2B, XADD,
   Operation: `TEMP := SRC + DEST; SRC := DEST; DEST := TEMP;`, where DEST's effective
   address is formed from the registers before the instruction executes. Upstream
   Ghidra writes the register (`Reg8 = m8`) before the store (`m8 = tmp`), and the
   store forms its address from the handle again, so `xadd byte ptr [rbx], bh`
   stores to the address built from the updated RBX. The loaded value is now kept in
   a local, the store goes first and the register is written last:

       local old = m8; addflags(old, Reg8); local tmp = old + Reg8; m8 = tmp; Reg8 = old;

   (one LOAD instead of three; the 32-bit form still zero-extends with
   check_Reg32_dest after the register write).

2. XCHG m32, r32 (#1060). SDM Vol. 1, 3.4.1.1: "32-bit operands generate a 32-bit
   result, zero-extended to a 64-bit result in the destination general-purpose
   register." Upstream writes Reg32 alone; it now builds check_Reg32_dest as XADD m32
   and the register forms already do.

An x86-64 host run natively gives both results (the native differential).

Usage: apply_x86_xadd_xchg_memory_forms.py IN.sinc OUT.sinc   (IN is lockable.sinc)
It is idempotent. Recompile x86.slaspec and x86-64.slaspec with the Ghidra
`support/sleigh` of the same release and refresh MANIFEST.sha256.json.
"""
import sys

XADD = {
    "8": ("Reg8", ""),
    "16": ("Reg16", ""),
    "32": ("Reg32", "    build check_Reg32_dest;\n"),
    "64": ("Reg64", ""),
}


def xadd_old(m, reg, check):
    sp = " " if m == "8" else ""  # upstream spaces the byte form differently
    return (
        f"    build m{m};\n"
        f"    addflags({sp}m{m},{reg});\n"
        f"    local tmp ={sp} m{m} +{sp} {reg};\n"
        f"    {reg} = m{m};\n"
        + check
        + f"    m{m} = tmp;\n"
    )


def xadd_new(m, reg, check):
    return (
        f"    build m{m};\n"
        f"    local old = m{m};\n"
        f"    addflags(old,{reg});\n"
        f"    local tmp = old + {reg};\n"
        f"    m{m} = tmp;\n"
        f"    {reg} = old;\n"
        + check
    )


XCHG_OLD_HEAD = (
    ":XCHG^xacq_xrel_prefx^alwaysLock  m32,Reg32   is vexMode=0 & xacq_xrel_prefx & alwaysLock"
    " & opsize=1 & byte=0x87; m32 & Reg32 ...        \n"
)
XCHG_NEW_HEAD = (
    ":XCHG^xacq_xrel_prefx^alwaysLock  m32,Reg32   is vexMode=0 & xacq_xrel_prefx & alwaysLock"
    " & opsize=1 & byte=0x87; (check_Reg32_dest & Reg32) ... & m32\n"
)
XCHG_OLD_BODY = "  local tmp = m32; \n  m32 = Reg32; \n  Reg32 = tmp; \n  UNLOCK(); \n"
XCHG_NEW_BODY = "  local tmp = m32; \n  m32 = Reg32; \n  Reg32 = tmp; \n  build check_Reg32_dest;\n  UNLOCK(); \n"


def replace_once(text, old, new, what):
    if new in text and old not in text:
        return text
    assert text.count(old) == 1, f"{what}: found {text.count(old)} times"
    return text.replace(old, new)


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    for m, (reg, check) in XADD.items():
        text = replace_once(text, xadd_old(m, reg, check), xadd_new(m, reg, check), f"XADD m{m}")
    text = replace_once(text, XCHG_OLD_HEAD, XCHG_NEW_HEAD, "XCHG m32 pattern")
    start = text.index(XCHG_NEW_HEAD)
    end = text.index("\n}\n", start) + 1
    body = replace_once(text[start:end], XCHG_OLD_BODY, XCHG_NEW_BODY, "XCHG m32 body")
    text = text[:start] + body + text[end:]
    open(dst, "w").write(text)
    print("applied")


main()
