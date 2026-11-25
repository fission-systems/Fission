#!/usr/bin/env python3
"""Make the saturating and rounding AArch64 NEON lifts saturate and round.

Upstream Ghidra lifts these instructions as plain p-code that wraps, and its rounding
shifts drop the rounding constant; an Apple M4 run natively shows it
(fission-systems/Fission#1068). Arm ARM (DDI 0487) semantics, per element of esize bits:

  SUQADD    Vd = SatQ(SInt(Vd) + UInt(Vn), signed)
  SQNEG     Vd = SatQ(-SInt(Vn), signed)                 (INT_MIN -> INT_MAX)
  SQDMULL   Vd = SatQ(2 * SInt(Vn) * SInt(Vm), 2*esize)  (only INT_MIN * INT_MIN saturates)
  SQDMLAL   Vd = SatQ(SInt(Vd) + SatQ(2 * Vn * Vm))      (two saturations)
  SQDMLSL   Vd = SatQ(SInt(Vd) - SatQ(2 * Vn * Vm))
  SQDMULH   Vd = SatQ((2 * Vn * Vm) >> esize)
  SQRDMULH  Vd = SatQ((2 * Vn * Vm + 2^(esize-1)) >> esize)
  SRSRA     Vd = Vd + ((SInt(Vn) + 2^(shift-1)) >> shift)  (wraps; no saturation)
  URSRA     Vd = Vd + ((UInt(Vn) + 2^(shift-1)) >> shift)

Every step stays within 2*esize bits (at most 64), so no 128-bit integers are needed:
- a signed add of an unsigned operand overflows exactly when INT_SCARRY differs from the
  unsigned operand's sign bit, and then only upward (SUQADD);
- 2*a*b wraps only for a = b = INT_MIN, to INT_MIN of the doubled width, which saturates
  to INT_MAX (= INT_MIN - 1); adding the rounding constant cannot overflow otherwise;
- an accumulating add saturates toward the accumulator's sign when INT_SCARRY/INT_SBORROW
  says it overflowed (INT_MAX + 1 wraps to INT_MIN);
- a rounding shift is floor(x / 2^s) + bit s-1 of x, which also holds for s = esize.

Only the forms upstream lifts as p-code are rewritten; those that call a NEON_* user op
(the vector SQDMULH/SQRDMULH, for example) are left as they are. All sources are read before
any destination element is written, because Vd may be Vn or Vm.

Usage: apply_aarch64_neon_saturate.py IN.sinc OUT.sinc   (IN is AARCH64neon.sinc). It is
idempotent.
"""
import re
import sys

MARK = "# Fission: NEON saturation/rounding"
KINDS = {
    "suqadd": "suqadd", "sqneg": "sqneg",
    "sqdmull": "mull", "sqdmull2": "mull", "sqdmlal": "mlal", "sqdmlal2": "mlal",
    "sqdmlsl": "mlsl", "sqdmlsl2": "mlsl",
    "sqdmulh": "mulh", "sqrdmulh": "rmulh", "srsra": "srsra", "ursra": "ursra",
}
EXPECT = {"suqadd": 11, "sqneg": 11, "sqdmull": 6, "sqdmull2": 4, "sqdmlal": 8, "sqdmlal2": 4,
          "sqdmlsl": 8, "sqdmlsl2": 4, "sqdmulh": 6, "sqrdmulh": 6, "srsra": 8, "ursra": 8}
BITS = {"B": 8, "H": 16, "S": 32, "D": 64}


class Op:
    def __init__(self, text):
        self.text = text
        if m := re.fullmatch(r"R[dnm]_VPR(64|128)\.(\d+)([BHSD])", text):
            self.kind, self.lanes, self.bits = "vec", int(m.group(2)), BITS[m.group(3)]
        elif m := re.fullmatch(r"R[dnm]_FPR(\d+)", text):
            self.kind, self.lanes, self.bits = "scalar", 1, int(m.group(1))
        elif m := re.fullmatch(r"Re_VPR128(?:Lo)?\.([HS])\.vIndex(?:HLM)?", text):
            self.kind, self.lanes, self.bits = "elem", 1, BITS[m.group(1)]
        elif m := re.fullmatch(r"Imm_shr_imm(\d+)", text):
            self.kind, self.lanes, self.bits = "imm", 0, int(m.group(1))
        else:
            raise AssertionError(text)

    def lane(self, i, bits=None, upper=False):
        if self.kind != "vec":
            return self.text
        bits = bits or self.bits
        return f"{self.text}[{i * bits + (64 if upper else 0)},{bits}]"


def smin(bits):
    return 1 << (bits - 1)


def smax(bits):
    return (1 << (bits - 1)) - 1


def body_lines(name, ops):
    kind = KINDS[name]
    d = ops[0]
    out = []
    if kind in ("suqadd", "sqneg"):
        n = ops[1]
        e, b = d.bits, d.bits // 8
        assert n.bits == e and n.lanes == d.lanes
        for i in range(d.lanes):
            D, N = d.lane(i), n.lane(i)
            if kind == "suqadd":
                out += [
                    f"local sd{i}:{b} = {D};",
                    f"local sn{i}:{b} = {N};",
                    f"local ss{i}:1 = scarry(sd{i}, sn{i}) != (sn{i} s< 0);",
                    f"{D} = ((sd{i} + sn{i}) * zext(!ss{i})) | (zext(ss{i}) * {smax(e):#x}:{b});",
                ]
            else:
                out += [
                    f"local qn{i}:{b} = {N};",
                    f"{D} = (0 - qn{i}) - zext(qn{i} == {smin(e):#x}:{b});",
                ]
        return out
    if kind in ("mull", "mlal", "mlsl"):
        n, m = ops[1], ops[2]
        e = n.bits
        w, B = 2 * e, 2 * e // 8
        assert d.bits == w and m.bits == e
        upper = name.endswith("2")
        if m.kind == "elem":
            out.append(f"local qe:{e // 8} = {m.text};")
        for i in range(d.lanes):
            out.append(f"local qa{i}:{B} = sext({n.lane(i, upper=upper)});")
            out.append(f"local qb{i}:{B} = sext({'qe' if m.kind == 'elem' else m.lane(i, upper=upper)});")
        for i in range(d.lanes):
            D = d.lane(i)
            out += [
                f"local qp{i}:{B} = qa{i} * qb{i} * 2:{B};",
                f"qp{i} = qp{i} - zext(qp{i} == {smin(w):#x}:{B});",
            ]
            if kind == "mull":
                out.append(f"{D} = qp{i};")
            else:
                carry, op = ("scarry", "+") if kind == "mlal" else ("sborrow", "-")
                out += [
                    f"local qd{i}:{B} = {D};",
                    f"local qo{i}:1 = {carry}(qd{i}, qp{i});",
                    f"{D} = ((qd{i} {op} qp{i}) * zext(!qo{i})) | (zext(qo{i}) * ({smax(w):#x}:{B} + zext(qd{i} s< 0)));",
                ]
        return out
    if kind in ("mulh", "rmulh"):
        n, m = ops[1], ops[2]
        e, eb, B = d.bits, d.bits // 8, d.bits // 4
        assert n.bits == e and m.kind == "elem" and m.bits == e and n.lanes == d.lanes
        rnd = f" + {1 << (e - 1):#x}:{B}" if kind == "rmulh" else ""
        out += [f"local he:{eb} = {m.text};", f"local hb:{B} = sext(he);"]
        for i in range(d.lanes):
            out.append(f"local hn{i}:{eb} = {n.lane(i)};")
        for i in range(d.lanes):
            out += [
                f"local hp{i}:{B} = sext(hn{i}) * hb * 2:{B}{rnd};",
                f"local hr{i}:{B} = hp{i} s>> {e}:1;",
                f"{d.lane(i)} = hr{i}:{eb} - zext((hn{i} == {smin(e):#x}:{eb}) && (he == {smin(e):#x}:{eb}));",
            ]
        return out
    if kind in ("srsra", "ursra"):
        n, imm = ops[1], ops[2]
        e, b = d.bits, d.bits // 8
        assert imm.kind == "imm" and n.bits == e and n.lanes == d.lanes
        sh = "s>>" if kind == "srsra" else ">>"
        out.append(f"local rk:1 = {imm.text}:1;")
        for i in range(d.lanes):
            out.append(f"local rn{i}:{b} = {n.lane(i)};")
        for i in range(d.lanes):
            D = d.lane(i)
            out.append(f"{D} = {D} + ((rn{i} {sh} rk) + ((rn{i} {sh} (rk - 1)) & 1:{b}));")
        return out
    raise AssertionError(kind)


CTOR = re.compile(r"^:(\w+) ([^\n]*)\nis [^\n]*\n\{\n((?:\t[^\n]*\n|\n)*?)\}\n", re.M)


def main():
    src, dst = sys.argv[1:3]
    text = open(src).read()
    counts = {}

    def fix(m):
        name = m.group(1)
        if name not in KINDS:
            return m.group(0)
        body = m.group(3)
        if MARK in body:
            counts[name] = counts.get(name, 0) + 1
            return m.group(0)
        if re.search(r"\b(NEON|MP)_\w+\(", body):
            return m.group(0)
        counts[name] = counts.get(name, 0) + 1
        ops = [Op(o.strip()) for o in m.group(2).split(",")]
        lines = [MARK] + body_lines(name, ops)
        lines += [l.strip() for l in body.split("\n") if l.strip().startswith("zext_")]
        head = m.group(0)[: m.group(0).index("{\n") + 2]
        return head + "".join(f"\t{l}\n" for l in lines) + "}\n"

    text = CTOR.sub(fix, text)
    assert counts == EXPECT, counts
    open(dst, "w").write(text)
    print("applied")


main()
