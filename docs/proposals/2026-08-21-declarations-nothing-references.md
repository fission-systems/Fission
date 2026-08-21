# Declarations nothing references, and why the big half cannot be pruned here

## 1. Baseline Row Anchor

- Command: release `fission_cli decomp <binary> --layer nir --addr <addr>` over
  the 250 scored sample-set functions.
- Current measured output: of **6,774 declared locals, 224 (3%) appear nowhere
  in the body at all** -- neither read nor written -- across **78 of 250
  functions (31%)**.

```text
class                                             count
suffixed duplicate stack slot (local_8_3)           123
aggregate field (field_N)                            35
temporary (xVarN)                                    30
other                                                19
stack slot (local_X)                                 17
```

Found while investigating the 24 locals still spelled `undefined`: one of them,
`xVar135`, was declared and never mentioned again, which is not a width problem
at all.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [ ] Builder · [x] Normalize · [ ] Structuring
- [ ] Type/data recovery · [ ] Printer · [ ] Benchmark/automation

`cleanup::temp_var::prune_unused_temp_bindings` already exists and already
prunes unused bindings, so this is coverage rather than a missing pass. Its
gate is `is_trivial_temp_name(name) || binding.is_temp_like()`: names shaped
`uVar`/`iVar`/`xVar`/`bVar` (plus `result`, `retval`, `reg`), or an origin of
`Temp`/`TempPreserved`. A `local_8_3` is neither -- its name is not
temp-shaped and its origin is `StackOffset`.

The pass also could not express "appears nowhere". `DefUseMap.use_count`
deliberately counts **reads only** (`count_lvalue`'s `Var(_) => {}`, "the
defined name is a write site — not an rvalue use"), because a write-only stack
home is a real thing the pipeline keeps and re-declares. So `use_count == 0`
cannot distinguish "written but never read" from "absent entirely".

## 3. Generality / Invariant Proof

```text
A binding the body neither reads nor writes, and whose initializer has no side
effects, has no effect the emitted C can express. Its declaration is noise.
```

`DefUseMap` gains `def_count` so the two halves can be asked separately, which
is what makes "appears nowhere" expressible at all.

**And the invariant is restricted to bindings that can only be referenced by
name.** That restriction is the whole subtlety and it was learned the hard way.

## 4. Risk And Ownership Check

The first version applied the rule to every origin. It passed the corpus sweep
-- 250 of 250 decompiling, gotos and short-circuit terms unchanged -- and
removed 134 of the 224. It was **wrong**.

This pass runs at some thirty pipeline stages, including ones before slot
addresses have been rewritten into slot names. A stack local referenced only as
`Load { ptr: <slot address> }` appears nowhere *by name* and is still live;
pruning it there deletes the binding a later stage was going to rewrite into.
Nothing in the emitted text breaks, because the name was absent either way --
so **no corpus measurement can see it.**
`preview_type_hints_apply_stack_local_type_to_surfaced_slot_alias` could, and
did.

So stack-derived origins are excluded: `StackOffset`, `HomeSlot`,
`OutgoingArgSlot`, `DerivedFromStackOffset`, `VaRegion`. What remains reachable
is temporaries and scaffolding, which are name-referenced by construction.

## 5. Validation Matrix

- [x] `cargo nextest run -p fission-pcode -p fission-midend-normalize`
      (1,325 passed).
- [x] DecBench sample-set rerun: 250 of 250, gotos 1,118 and short-circuit
      terms 228 both unchanged.

Measured, restricted form:

```text
declared locals    6,774 -> 6,725
appearing nowhere    224 ->   220
functions affected    78 ->    76
```

**Four declarations, one function.** That is the honest size of it. The 123
suffixed duplicate stack slots -- the majority of the problem and the reason
it was worth looking at -- are exactly the ones the restriction excludes.

## 6. What the real fix is

Not this pass. The extended rule is correct *after* slot addresses have been
rewritten into slot names, and unsafe before; running it once at a late stage
rather than at all thirty would reach the stack slots without the hazard.
Finding that stage is the next piece of work, and it is a different change from
this one.

Two facts to start from: `DefUseMap.def_count` now exists, and the restriction
in `prune_unused_temp_bindings` is expressed as a named condition
(`address_referable`) that a late-stage caller could be permitted to ignore.

Also unresolved, and separate: **why 123 suffixed duplicate slots exist at
all.** `local_8_3` means a slot was created whose base name was already taken;
that they are then consumed by nothing suggests the duplication itself is the
defect, and pruning the declaration would only have hidden it. Owner is the
builder's stack-slot management, where `StackSlot` carries `ty: NirType` and no
size field of its own.

## 7. AI Review / Prompt Firewall

- No external model was consulted.
- Measurement ran on the sample-set's decompiled text; production code names no
  binary or function.
