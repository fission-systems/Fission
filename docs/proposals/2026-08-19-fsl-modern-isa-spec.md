# FSL and FIR: a SLEIGH successor and the semantics it lowers to

**Status:** proposal · **Date:** 2026-08-19

```
SLEIGH  ->  p-code        (what Ghidra has, what we run today)
FSL     ->  FIR           (proposed; FIR is a superset of p-code)
```

FSL is the spec language. FIR is the common semantics it lowers to. The
pairing matters: p-code is not just an instruction set, it is a *model* --
`(space, offset, size)` storage, one sequential thread, arbitrary CFG, untyped
values, opaque `CALLOTHER`. Every gap below is that model showing through, so
a better spec language with the same target would only be nicer syntax.

## The constraint

Keep the pipeline. `fission-pcode` -> normalize -> structure -> HIR is the
value below the ISA layer and none of it should change. So **FIR is a superset
of p-code**: every p-code program is a FIR program, the 73 existing opcodes
keep their meaning, and a midend pass that understands only p-code keeps
working on the subset. FSL and `.slaspec` coexist; nothing is ported under
duress.

That is affordable because of how narrow the runtime's actual appetite is.
Counted on 2026-08-19 across `crates/fission-sleigh/src/runtime/`:

```
12  .constructors      7  .sla_spaces          1  .sla_uniqbase
10  .subtables         4  .userops             1  .sla_uniqmask
 8  .default_context   3  .sla_register_space_index
```

and never `pattern_nodes`, `macros`, `definitions`, `pcode_ops`,
`include_manifest`, `defines`. A second front end has to produce that short
list and nothing more.

## What the p-code model cannot say

Each measured in specs we ship, not asserted.

**Predication becomes duplication.** `ARMinstructions.sinc` carries **80**
`^COND^` constructors, each repeating `build COND` because there is no way to
factor a condition across a group.

**Operand shape becomes duplication.** x86 `ADD` needs **19** constructors in
`ia.sinc`, identical semantics, differing only in width and addressing mode.

**Values are untyped.** A varnode is `(space, offset, size)`. This is the
direct cause of a defect measured today: `map_binary_op` collapses
`IntDiv | IntSDiv | FloatDiv` into one `Div`, destroying signedness and
float-ness at lowering, so the type-inference input table cannot exist
(`bf8822613`).

**Storage model is assumed, not declared.** JVM is a stack machine; SLEIGH has
no stack, so `JVM.slaspec` emulates one with an `SP` register over a `ram`
space. `iadd` -- "add the top two stack values" -- becomes three temporaries
and four memory accesses, and the `int`/`long`/`float` distinction survives
only in the opcode's *name*.

**Execution model is assumed.** One sequential thread. SIMT warps, VLIW
packets and dataflow accelerators have no way to say otherwise.

**Effects are opaque.** `CALLOTHER` arrives as a name: no signature, no
purity, no alias set. Every one is an optimisation barrier.

**Definitions have no provenance.** A SLEIGH constructor is asserted, full
stop. There is nowhere to record that an encoding was *inferred* rather than
read from a manual -- which is the normal case for SASS and the only case for
a VM protector.

## What FIR adds

Five declarations, all optional. Absent them, FIR *is* p-code.

```fir
model x86  { storage: registers+memory, exec: sequential,      cfg: arbitrary }
model jvm  { storage: stack(typed),     exec: sequential,      cfg: arbitrary }
model wasm { storage: stack+locals,     exec: sequential,      cfg: structured }
model sass { storage: registers(spaces), exec: simt(warp: 32), cfg: predicated }
```

1. **Typed values** -- a FIR value carries a metatype, so `FLOAT_DIV` and
   `INT_SDIV` do not have to be recovered after the fact.
2. **Declared storage** -- a stack machine says so, instead of being emulated.
3. **Declared execution** -- warp width, packet width, or nothing.
4. **Effect contracts** on intrinsics -- purity, alias sets, uniformity.
5. **Provenance** on every definition (below).

The midend may ignore all five. That is the compatibility guarantee.

## FSL, sketched

One parameterised constructor for the 19:

```fsl
isa x86 { endian: little, align: 1 }

class reg<W: 8|16|32|64> : int<W> @register;
class imm<W: 8|16|32>    : int<W> @immediate;

op ADD<W>(dst: reg<W> inout, src: imm<W>) {
  encoding  = 0x80 | modrm(reg_opcode = 0) | imm<W>;
  semantics = { dst = add_flags(dst, src); }
}
```

A modifier for the 80:

```fsl
modifier cond(c: ARMCond) applies_to group(arm_data_processing) {
  guard = eval_cond(c);
}
```

Types that survive lowering:

```fsl
op FDIV(dst: reg<64> : f64, a: reg<64> : f64, b: reg<64> : f64) {
  semantics = { dst = a / b; }    // FLOAT_DIV, and the operand types reach
}                                 // the midend as facts rather than guesses
```

Effects that are not a black box:

```fsl
intrinsic __shfl_sync(mask: u32, val: u32, lane: u32) -> u32
  { pure, warp_uniform: mask }
```

## Generated specs: confidence, scope, partiality

A VM protector ships a **different ISA per binary**, sometimes per build.
No `.slaspec` can be distributed for Themida or VMProtect, because the next
sample has different handlers. The spec has to be *produced* from the sample:

```
protected binary
  -> locate the VM dispatcher and handler table
  -> lift each handler (it is ordinary x86; Fission already does this)
  -> summarise each handler's effect  ->  emit FSL
  -> the existing pipeline lifts the bytecode
```

For that to work the language must admit things a hand-written spec never
needs, and these are the same three that SASS needs:

```fsl
// Scope: this definition is not universal.
scope { sample: sha256:1f3a…, build: vmprotect-3.x }

op h17(a: vstack, b: vstack) -> vstack
  { confidence: 0.86, evidence: ["handler@0x4021a0", "3/3 samples agree"] }
  semantics = { result = a + b; }

// Partiality: 31 of 40 handlers understood is still useful.
unknown op h23 @0x402540 { reason: "indirect dispatch through a computed table" }
```

- **Confidence** -- so a consumer can weigh an inferred definition against a
  documented one instead of treating both as fact.
- **Scope** -- so a spec that is valid for one sample, or one SM generation,
  says so. SASS needs exactly this.
- **Partiality** -- so lifting can proceed on the handlers that are understood
  and mark the rest, rather than failing whole. SLEIGH is all-or-nothing.

SASS and VM protectors are the same problem: an instruction stream whose ISA
was never published. One is inferred from silicon behaviour, the other from
handler code, and both need a spec format that can say "probably".

## Scope of the whole idea

In: anything that is **an instruction stream**. CPU, GPU, DSP, stack VMs,
eBPF, WASM, protector bytecode.

Out: FPGA bitstreams and P4 pipelines. They are configuration and dataflow,
not instructions. Forcing them in produces the JVM outcome -- expressible, but
with the interesting structure destroyed on the way in.

## Milestones, in order

1. **Re-express one existing `.slaspec` in FSL and produce byte-identical
   lifting on the corpus.** If FSL cannot say what SLEIGH says, it is a
   downgrade, and this is where that shows.
2. **Wire one FIR fact to a consumer** -- operand metatypes into `type_flow`.
   Until an added fact changes a measured number, this is nicer syntax.
3. **One toy VM**, open-source and simple, end to end: handler lifting ->
   generated FSL -> pseudocode. Proves the generation path separately from
   commercial anti-analysis.
4. **A GPU ISA**, where no `.slaspec` exists to compete with.
5. Only then a real protector.

Skipping 3 and going straight at Themida means a failure cannot be localised:
polymorphism, handler inlining, nested VMs and anti-debug all arrive at once.

## How this fails

- FSL cannot express an ISA SLEIGH can -> milestone 1 catches it.
- FIR facts never reach a consumer -> milestone 2 catches it; without it we
  have two front ends and one set of results.
- GPU divergence turns out not to be expressible over p-code's per-thread
  model -> found at milestone 4 on one real kernel, not after the language
  exists.
