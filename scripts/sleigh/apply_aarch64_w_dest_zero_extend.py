#!/usr/bin/env python3
"""Zero bits 63:32 of Xt/Xs when an AArch64 atomic or acquire load writes a W register.

Every write to a W register zero-extends into the X register: Arm ARM (DDI 0487)
pseudocode `X[n, 32] = value` stores `ZeroExtend(value, 64)` (the X[] setter). The
32-bit and narrower forms below assign the W register alone in upstream Ghidra, so
bits 63:32 of the X register keep their old value. An Apple M4 run natively clears
them (fission-systems/Fission#1067, #1057):

- LSE atomics with a W destination: LD{ADD,CLR,EOR,SET,SMAX,SMIN,UMAX,UMIN}{,A,AL,L}{B,H,}
  and SWP{,A,AL,L}{B,H,}: `X[t, regsize] = ZeroExtend(data, regsize)`;
- LDLARB, LDLARH, LDLAR (W): `X[t, regsize] = ZeroExtend(data, regsize)`;
- LDAPRB, LDAPRH, LDAPR (W): same;
- CASB, CASH, CAS (W) and CASP (W pair): `X[s, regsize] = ZeroExtend(data, regsize)`.

Each assignment `aa_Wt = v` becomes `aa_Xt = zext(v)` (and `aa_Ws`/`aa_Wss` likewise),
and the constructor's pattern gains the X operand (`& aa_Xt`), which shares the
register field. A zero-extending load already of the W width (`zext(*:1 addr)`)
becomes `zext(*:1 addr)` into the X register directly.

Usage: apply_aarch64_w_dest_zero_extend.py IN.sinc OUT.sinc   (IN is AARCH64base.sinc)
It is idempotent. After it, recompile the three AARCH64*.slaspec with the Ghidra
`support/sleigh` of the same release as the specs and refresh MANIFEST.sha256.json.
"""
import re
import sys

# (constructor head regex, list of (old body text, new body text), operand to add)
# Each entry rewrites one constructor; every head must match exactly once.
EDITS = [
    # LSE atomics, size b/h/w (the X form is already right)
    (r':ld\^ls_opc1\^ls_lor\^"b" aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    (r':ld\^ls_opc2\^ls_lor\^"h" aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    (r':ld\^ls_opc4\^ls_lor aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    (r':swp\^ls_lor\^"b" aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    (r':swp\^ls_lor\^"h" aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    (r':swp\^ls_lor aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    # LDLAR (LoadLOAcquire)
    (r':ldlarb aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = zext(*:1 Rn_GPR64xsp);", "aa_Xt = zext(*:1 Rn_GPR64xsp);", "aa_Xt"),
    (r':ldlarh aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = zext(*:2 Rn_GPR64xsp);", "aa_Xt = zext(*:2 Rn_GPR64xsp);", "aa_Xt"),
    (r':ldlar aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = *:4 Rn_GPR64xsp;", "aa_Xt = zext(*:4 Rn_GPR64xsp);", "aa_Xt"),
    # LDAPR (RCpc)
    (r':ldapr aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    (r':ldaprb aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    (r':ldaprh aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Wt = tmp_ldWn;", "aa_Xt = zext(tmp_ldWn);", "aa_Xt"),
    # CAS
    (r':cas\^cas_var\^"b" aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Ws = zext(data);", "aa_Xs = zext(data);", "aa_Xs"),
    (r':cas\^cas_var\^"h" aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Ws = zext(data);", "aa_Xs = zext(data);", "aa_Xs"),
    (r':cas\^cas_var aa_Ws, aa_Wt, \[Rn_GPR64xsp\]\n',
     "aa_Ws = data;", "aa_Xs = zext(data);", "aa_Xs"),
]

# CASP (W pair): both result registers, both endiannesses.
CASP_HEAD = r":casp\^cas_var aa_Ws, aa_Wss, aa_Wt, aa_Wtt, \[Rn_GPR64xsp\]\n"
CASP_BODY = [
    ("\taa_Ws = data(4);\n\taa_Wss = data:4;", "\taa_Xs = data >> 32;\n\taa_Xss = zext(data:4);"),
    ("\taa_Ws = data:4;\n\taa_Wss = data(4);", "\taa_Xs = zext(data:4);\n\taa_Xss = data >> 32;"),
]

# A constructor: head line, `is ...` pattern line, then the body up to the closing brace
# at the start of a line.
def ctor_re(head):
    return re.compile("^(?P<head>" + head + ")(?P<pat>is [^\n]*)\n(?P<body>\\{.*?\n?\\}\n)", re.M | re.S)


def add_operand(pat, operand):
    if re.search(r"& " + operand + r"\b", pat):
        return pat
    return pat + " & " + operand


def rewrite(text, head, pairs, operands):
    regex = ctor_re(head)
    matches = list(regex.finditer(text))
    assert len(matches) == 1, f"{head!r}: {len(matches)} constructors"
    m = matches[0]
    body = m.group("body")
    for old, new in pairs:
        if new in body:
            continue
        assert body.count(old) == 1 or (head == CASP_HEAD and body.count(old) <= 1), (head, old)
        body = body.replace(old, new)
    pat = m.group("pat")
    for operand in operands:
        pat = add_operand(pat, operand)
    return text[: m.start()] + m.group("head") + pat + "\n" + body + text[m.end():]


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    for head, old, new, operand in EDITS:
        text = rewrite(text, head, [(old, new)], [operand])
    text = rewrite(text, CASP_HEAD, CASP_BODY, ["aa_Xs", "aa_Xss"])
    open(dst, "w").write(text)
    print("applied")


main()
