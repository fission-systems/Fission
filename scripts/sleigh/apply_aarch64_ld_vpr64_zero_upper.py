#!/usr/bin/env python3
"""Zero V[127:64] after AArch64 LD1/LD2/LD3/LD4 (and the R forms) with a 64-bit arrangement.

Arm ARM (DDI 0487), LD1/LD2/LD3/LD4 (multiple structures) and LD1R..LD4R, Operation:
for a `.8B/.4H/.2S/.1D` arrangement `datasize = 64` and each register is written as
`V[t, datasize] = rval`, and the V[] setter zero-extends a 64-bit write into the
whole register ("writes zeros to bits [127:64]"; with SVE, to the end of Z). Upstream
Ghidra writes only the 64 loaded bits and marks it `# neglected zexts`, so bits
127:64 keep their old value; an Apple M4 run natively clears them
(fission-systems/Fission#1067: `ld1 {v1.4h, v2.4h}, [x1]`, `ld1r {v3.4h}, [x1]`).

Every `:ld*` constructor that writes `R*_VPR64` registers now replaces its
`# neglected zexts` with `zext_zd(Z*)` for each register, the macro the spec uses for
every other 64-bit SIMD result. The 128-bit arrangements and single-lane forms keep
the rest of the register by definition and are not touched, nor are the stores.

Usage: apply_aarch64_ld_vpr64_zero_upper.py IN.sinc OUT.sinc   (IN is AARCH64ldst.sinc)
It is idempotent.
"""
import re
import sys

CTOR = re.compile(r"^(?P<head>:ld[1-4]r? [^\n]*\n)(?P<pat>is [^\n]*\n)(?P<body>\{\n.*?\n\}\n)", re.M | re.S)
REGS = [("Rt_VPR64", "Zt"), ("Rtt_VPR64", "Ztt"), ("Rttt_VPR64", "Zttt"), ("Rtttt_VPR64", "Ztttt")]
EXPECTED = 41  # LD1 16, LD1R/LD2R/LD3R/LD4R 4 each, LD2/LD3/LD4 3 each


def fix(m, counter):
    pat, body = m.group("pat"), m.group("body")
    if "_VPR64" not in pat or "# neglected zexts" not in body:
        return m.group(0)
    lines = []
    for reg, z in REGS:
        if re.search(r"\b" + reg + r"\b", pat):
            assert re.search(r"\b" + z + r"\b", pat), (m.group("head"), z)
            assert reg + "[" in body or reg + " =" in body, (m.group("head"), reg)
            lines.append(f"\tzext_zd({z}); # zero upper 24 bytes of {z}\n")
    assert lines, m.group("head")
    counter[0] += 1
    return m.group("head") + pat + body.replace("\t# neglected zexts\n", "".join(lines), 1)


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    counter = [0]
    text = CTOR.sub(lambda m: fix(m, counter), text)
    if counter[0] == 0:
        done = len([m for m in CTOR.finditer(text) if "_VPR64" in m.group("pat") and "zext_zd(Zt)" in m.group("body")])
        assert done == EXPECTED, f"{done} constructors already fixed, expected {EXPECTED}"
        print("already applied")
    else:
        assert counter[0] == EXPECTED, f"rewrote {counter[0]} constructors, expected {EXPECTED}"
        print("applied")
    open(dst, "w").write(text)


main()
