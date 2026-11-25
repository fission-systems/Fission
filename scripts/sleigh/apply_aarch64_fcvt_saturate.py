#!/usr/bin/env python3
"""Give the AArch64 FCVT{A,M,N,P,Z}{S,U} lifts the Arm ARM FPToFixed semantics.

Run after apply_aarch64_fcvt_rounding.py, which gave each rounding mode its own display
table (`fcvt_{v,s}mnemonic_{a,m,n,p,z}`) but left the S and U mnemonic of a mode in one
table, so both shared `lhs = trunc(OP(x))`. An Apple M4 run natively
(fission-systems/Fission#1056) shows what that gets wrong:

- out-of-range values and infinities wrapped instead of saturating;
- a NaN gave the wrapped integer instead of 0;
- the unsigned forms converted as signed (negative inputs, and inputs at or above 2^(N-1));
- FCVTN* rounded ties away from zero (FLOAT_ROUND), and FCVTA* rounded negative ties up.

Arm ARM (DDI 0487) FPToFixed: round the operand to an integer in the instruction's mode,
then saturate it to the N-bit signed or unsigned range (SatQ); a NaN gives 0. FLOOR, CEIL,
subtracting the floor, halving and multiplying by a power of two are exact in IEEE
arithmetic, so every step is built from them:

  M floor(x)      P ceil(x)      Z x (FLOAT_TRUNC rounds toward zero itself)
  N ties to even: f = floor(x); d = x - f; up = d > 0.5 || (d == 0.5 && f is odd)
  A ties away:    f = floor(|x|); up = |x| - f >= 0.5; r = (f + up) with the sign of x
  signed:   NaN -> 0, r < -2^(N-1) -> INT_MIN, r >= 2^(N-1) -> INT_MAX, else trunc(r)
  unsigned: NaN -> 0, r < 0 -> 0, r >= 2^N -> UINT_MAX, r >= 2^(N-1) ->
            trunc(r - 2^(N-1)) ^ 2^(N-1) (exact, Sterbenz), else trunc(r)

Half-precision operands are widened to single precision first (exactly), because 2^31
and 2^63 are not half-precision numbers. The results are selected with masks, so the lift
stays straight-line p-code.

The script splits every mode table into one table per mnemonic (`..._ms`, `..._mu`),
clones each constructor once for S and once for U, and rewrites each `trunc` statement.
It also rewrites the fixed-point forms outside the tables, which have the same defect:
`fcvtz{s,u} Wd/Xd, Hn/Sn/Dn, #fbits` (upstream `trunc(x f* 2^fbits)`; the half-precision
FCVTZU multiplied by the integer fbits instead of 2^fbits) and the scalar SIMD
`fcvtzu Hd/Sd/Dd, Hn/Sn/Dn, #fbits`, whose H and D forms read the shift with the S form's
immediate table (wrong fbits and wrong disassembly). Scaling by 2^fbits (fbits <= 64) is
done as two exact multiplications by powers of two of at most 2^32.

Usage: apply_aarch64_fcvt_saturate.py IN.sinc OUT.sinc   (IN is AARCH64neon.sinc, after
apply_aarch64_fcvt_rounding.py). It is idempotent.
"""
import re
import sys

MARK = "# Fission: FPToFixed"

# float bytes -> (sign, one, half) bit patterns
FCONST = {4: (0x80000000, 0x3F800000, 0x3F000000), 8: (0x8000000000000000, 0x3FF0000000000000, 0x3FE0000000000000)}
BIAS = {4: (127, 23), 8: (1023, 52)}


def pow2(fb, k):
    """2^k as a float of `fb` bytes."""
    bias, mant = BIAS[fb]
    return (bias + k) << mant


def fp_to_fixed(k, arg, src_bits, n, signed, mode, fbits=None):
    """Lines computing FPToFixed(arg) into local `cr{k}` (n bits); returns (lines, var)."""
    fb = 4 if src_bits <= 32 else 8
    nb = n // 8
    sign, one, half = FCONST[fb]
    x = f"cx{k}"
    if src_bits == 16:
        out = [f"\tlocal {x}:{fb} = float2float({arg});"]
    else:
        out = [f"\tlocal {x}:{fb} = {arg};"]
    if fbits is not None:
        out += [
            f"\tlocal cs{k}:8 = zext({fbits});",
            f"\tlocal ch{k}:8 = cs{k} >> 1;",
            f"\tlocal cp{k}:{fb} = int2float(1:8 << ch{k});",
            f"\tlocal cq{k}:{fb} = int2float(1:8 << (cs{k} - ch{k}));",
            f"\t{x} = ({x} f* cp{k}) f* cq{k};",
        ]
    r = f"cr{k}"
    if mode == "z":
        out.append(f"\tlocal {r}:{fb} = {x};")
    elif mode == "m":
        out.append(f"\tlocal {r}:{fb} = floor({x});")
    elif mode == "p":
        out.append(f"\tlocal {r}:{fb} = ceil({x});")
    elif mode == "n":
        out += [
            f"\tlocal cf{k}:{fb} = floor({x});",
            f"\tlocal cd{k}:{fb} = {x} f- cf{k};",
            f"\tlocal ce{k}:{fb} = cf{k} f* {half:#x}:{fb};",
            f"\tlocal cu{k}:1 = (cd{k} f> {half:#x}:{fb}) || ((cd{k} f== {half:#x}:{fb}) && ((ce{k} f- floor(ce{k})) f!= 0:{fb}));",
            f"\tlocal {r}:{fb} = (cf{k} f+ (zext(cu{k}) * {one:#x}:{fb})) | ({x} & {sign:#x}:{fb});",
        ]
    elif mode == "a":
        out += [
            f"\tlocal ca{k}:{fb} = {x} & {sign ^ ((1 << (8 * fb)) - 1):#x}:{fb};",
            f"\tlocal cf{k}:{fb} = floor(ca{k});",
            f"\tlocal cu{k}:1 = (ca{k} f- cf{k}) f>= {half:#x}:{fb};",
            f"\tlocal {r}:{fb} = (cf{k} f+ (zext(cu{k}) * {one:#x}:{fb})) | ({x} & {sign:#x}:{fb});",
        ]
    else:
        raise AssertionError(mode)
    v = f"cv{k}"
    if signed:
        lo, hi = pow2(fb, n - 1) | sign, pow2(fb, n - 1)
        imin, imax = 1 << (n - 1), (1 << (n - 1)) - 1
        out += [
            f"\tlocal cl{k}:1 = {r} f< {lo:#x}:{fb};",
            f"\tlocal cg{k}:1 = {r} f>= {hi:#x}:{fb};",
            f"\tlocal ci{k}:1 = !(nan({x}) || cl{k} || cg{k});",
            f"\tlocal {v}:{nb} = (trunc({r}) * zext(ci{k})) | (zext(cg{k}) * {imax:#x}:{nb}) | (zext(cl{k}) * {imin:#x}:{nb});",
        ]
    else:
        top, big = pow2(fb, n), pow2(fb, n - 1)
        umax, bit = (1 << n) - 1, 1 << (n - 1)
        out += [
            f"\tlocal cl{k}:1 = {r} f< 0:{fb};",
            f"\tlocal cg{k}:1 = {r} f>= {top:#x}:{fb};",
            f"\tlocal cb{k}:1 = {r} f>= {big:#x}:{fb};",
            f"\tlocal ci{k}:1 = !(nan({x}) || cl{k} || cg{k});",
            f"\tlocal ct{k}:{nb} = trunc({r} f- (zext(cb{k}) * {big:#x}:{fb})) ^ (zext(cb{k}) * {bit:#x}:{nb});",
            f"\tlocal {v}:{nb} = (ct{k} * zext(ci{k})) | (zext(cg{k}) * {umax:#x}:{nb});",
        ]
    return out, v


def width(expr):
    m = re.search(r"\[\d+,(\d+)\]$", expr)
    if m:
        return int(m.group(1))
    return int(re.search(r"[FG]PR(\d+)", expr).group(1))


STMT = re.compile(r"^\t(?P<lhs>[^=\n]+?) = trunc\((?:(?P<op>floor|ceil|round)\()?(?P<arg>[^()\n]+)\)?\);$")


def rewrite_trunc(body, signed, mode):
    out, k = [], 0
    for line in body.split("\n"):
        s = STMT.match(line)
        if not s:
            out.append(line)
            continue
        lhs, arg = s.group("lhs"), s.group("arg")
        lines, v = fp_to_fixed(k, arg, width(arg), width(lhs), signed, mode)
        out.append("\n".join(lines + [f"\t{lhs} = {v};"]))
        k += 1
    assert k, body
    return "\n".join(out)


def split_tables(text):
    for form in "vs":
        for mode in "amnpz":
            entries = re.findall(rf'^fcvt_{form}mnemonic_{mode}: "(fcvt{mode}([su]))" is ([^\n]*?) \{{ \}}\n', text, re.M)
            assert len(entries) == 2, (form, mode, entries)
            for name, su, pat in entries:
                old = f'fcvt_{form}mnemonic_{mode}: "{name}" is {pat} {{ }}\n'
                text = text.replace(old, f'fcvt_{form}mnemonic_{mode}{su}: "{name}" is {pat} {{ }}\n', 1)
    return text


TABLE_CTOR = re.compile(r"^:\^fcvt_([vs])mnemonic_([amnpz]) [^\n]*\nis [^\n]*\n\{\n(?:\t[^\n]*\n|\n)*?\}\n", re.M)


def split_constructor(m):
    form, mode = m.group(1), m.group(2)
    out = []
    for su in "su":
        c = m.group(0)
        c = c.replace(f":^fcvt_{form}mnemonic_{mode} ", f":^fcvt_{form}mnemonic_{mode}{su} ", 1)
        c = c.replace(f"& fcvt_{form}mnemonic_{mode} &", f"& fcvt_{form}mnemonic_{mode}{su} &", 1)
        assert f"fcvt_{form}mnemonic_{mode}{su} &" in c, c
        c = rewrite_trunc(c, su == "s", mode)
        out.append(c.replace("{\n", "{\n\t" + MARK + "\n", 1))
    return "\n".join(out)


# fcvtz{s,u} Wd/Xd, Hn/Sn/Dn, #fbits
FIXED_GPR = re.compile(r"^:fcvtz([su]) (Rd_GPR(32|64)), (Rn_FPR(16|32|64)), FBitsOp\nis [^\n]*\n\{\n(?:\t[^\n]*\n|\n)*?\}\n", re.M)


def fix_fixed_gpr(m):
    c = m.group(0)
    if MARK in c:
        return c
    signed, rd, n, rn, src = m.group(1) == "s", m.group(2), int(m.group(3)), m.group(4), int(m.group(5))
    lines, v = fp_to_fixed(0, rn, src, n, signed, "z", fbits="FBitsOp")
    body = [MARK] + [l.strip() for l in lines] + [f"{rd} = {v};"]
    if n == 32:
        body.append("zext_rs(Rd_GPR64); # zero upper 28 bytes of Rd_GPR64")
    head = c[: c.index("{\n") + 2]
    return head + "".join(f"\t{l}\n" for l in body) + "}\n"


# fcvtzu Hd/Sd/Dd, Hn/Sn/Dn, #fbits (scalar SIMD); upstream reads every size with Imm_shr_imm32
FIXED_FPR = re.compile(r"^:fcvtzu Rd_FPR(16|32|64), Rn_FPR\1, Imm_shr_imm(\d+)\nis [^\n]*\n\{\n(?:\t[^\n]*\n|\n)*?\}\n", re.M)
ZEXT = {16: "zh", 32: "zs", 64: "zd"}


def fix_fixed_fpr(m):
    c = m.group(0)
    if MARK in c:
        return c
    n = int(m.group(1))
    c = c.replace(f"Imm_shr_imm{m.group(2)}", f"Imm_shr_imm{n}", 2)  # display operand and pattern
    lines, v = fp_to_fixed(0, f"Rn_FPR{n}", n, n, False, "z", fbits=f"Imm_shr_imm{n}")
    body = [MARK] + [l.strip() for l in lines] + [f"Rd_FPR{n} = {v};", f"zext_{ZEXT[n]}(Zd); # zero the rest of Zd"]
    head = c[: c.index("{\n") + 2]
    return head + "".join(f"\t{l}\n" for l in body) + "}\n"


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    assert "fcvt_vmnemonic_m" in text, "run apply_aarch64_fcvt_rounding.py first"
    if "fcvt_vmnemonic_ms" not in text:
        text = split_tables(text)
        text, n1 = TABLE_CTOR.subn(split_constructor, text)
        assert n1 == 70, n1
    text, n2 = FIXED_GPR.subn(fix_fixed_gpr, text)
    text, n3 = FIXED_FPR.subn(fix_fixed_fpr, text)
    assert n2 == 12 and n3 == 3, (n2, n3)
    open(dst, "w").write(text)
    print("applied")


main()
