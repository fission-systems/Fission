#!/usr/bin/env python3
"""Sign-extend AArch64 byte and halfword loads into a W register to 32 bits.

LDRSB/LDRSH (immediate, register, pre- and post-indexed) and LDURSB/LDURSH/LDTRSB/
LDTRSH with a W destination sign-extend the loaded value to 32 bits and, as every
W-register write does, zero bits 63:32 of X. Arm ARM pseudocode: `regsize = 32` when
opc<0> is 1, then `X[t, regsize] = SignExtend(data, regsize)`, and `X[n, 32] = value`
stores `ZeroExtend(value, 64)`.

Upstream Ghidra lifts the eight W constructors in AARCH64base.sinc as
`Rt_GPR64 = sext(*:N addr)`, which sign-extends into all 64 bits, and the unscaled/
unprivileged byte form (`ld^UnscPriv^"rsb" Rt_GPR32`) as `zext(*:1 addr)`, which
loses the sign. Each now reads

    local tmp:4 = sext(*:N addr); Rt_GPR64 = zext(tmp);

The X forms, LDRSW and LDAPURSB/LDAPURSH are already right and are not touched.

Upstream report: NationalSecurityAgency/ghidra#6471.

Usage: apply_aarch64_w_signed_loads.py IN.sinc OUT.sinc   (IN is AARCH64base.sinc)
It is idempotent. After it, recompile the three AARCH64*.slaspec with the Ghidra
`support/sleigh` of the same release as the specs and refresh MANIFEST.sha256.json.
"""
import re
import sys

# A W-destination signed byte/halfword load constructor and its one-statement body.
CTOR = re.compile(
    r'^(?P<head>:ld(?:rs|\^UnscPriv\^"rs)(?P<sz>[bh])"? Rt_GPR32, (?P<addr>addr\w+)\n'
    r"is [^\n]*& Rt_GPR32 & Rt_GPR64\n\{\n)"
    r"\tRt_GPR64 = (?:sext|zext)\(\*:(?P<n>[12]) (?P=addr)\);\n\}",
    re.M,
)
FIXED = "\tlocal tmp:4 = sext(*:{n} {addr}); Rt_GPR64 = zext(tmp);\n}}"
EXPECTED = 8  # ldrsb x4, ldrsh x4 (one of each is ld{ur,tr}s{b,h})


def fix(match):
    n, addr, sz = match.group("n"), match.group("addr"), match.group("sz")
    assert n == {"b": "1", "h": "2"}[sz], match.group(0)
    return match.group("head") + FIXED.format(n=n, addr=addr)


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    applied = text.count("local tmp:4 = sext(*:")
    if applied == EXPECTED and not CTOR.search(text):
        open(dst, "w").write(text)
        print("already applied")
        return
    text, count = CTOR.subn(fix, text)
    assert count == EXPECTED, f"rewrote {count} constructors, expected {EXPECTED}"
    open(dst, "w").write(text)
    print("applied")


main()
