# Two different stack slots, one name

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/.../binaries/bin_008.elf`, `sub_ee70` at `0xee70`
- Current measured output: six declarations that nothing in the body mentions,
  five of them suffixed duplicates:

```c
undefined local_10_18;
undefined local_18_19;
undefined local_20_20;
undefined local_28_21;
undefined local_30_22;
```

Across the 250 scored functions, 123 of the 224 never-referenced declarations
have this shape. It is the largest single class, and the reason the
declaration-pruning work was worth starting.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [x] Builder · [ ] Normalize · [ ] Structuring
- [ ] Type/data recovery · [ ] Printer · [ ] Benchmark/automation

The suffix is not the defect. `unique_stack_slot_binding_name` appends one when
a slot's display name is already taken, which is correct: two distinct slots
must not share a name.

The defect is that the two slots are distinct only because the *name* made them
so. Instrumenting every suffix taken on the corpus:

```text
[SLOT-DUP] base_name=local_10 taken; base=Rsp offset=-16 frame=0
[SLOT-DUP] base_name=local_18 taken; base=Rsp offset=-24 frame=0
[SLOT-DUP] base_name=local_8  taken; base=Rsp offset=+8  frame=0
```

`rsp_local_display_offset` normalizes an `rsp`-relative offset into a
frame-relative one:

```rust
if offset >= 0 && self.stack_frame_size > offset {
    self.stack_frame_size - offset
} else {
    offset.unsigned_abs() as i64
}
```

With `stack_frame_size == 0` the first branch can never be taken, so every
offset falls to `unsigned_abs()` -- **which throws the sign away.** `rsp+16`
and `rsp-16` are 32 bytes apart and both display as `local_10`. The second one
to arrive collides, gets suffixed, and the two names then describe whatever the
rest of the pipeline made of two entries that were never reconciled.

Measured across 60 binaries: **63 collisions, every one of them at
`stack_frame_size == 0`**, in 20 functions; 6 of those functions produced both
a positive and a negative offset.

So there are two failures stacked, and the outer one is the enabling condition:

1. **The frame size was never determined** (`stack_frame_size == 0`), so
   `rsp`-relative offsets cannot be placed in the frame at all.
2. **The fallback loses the sign**, so slots on opposite sides of the stack
   pointer are named identically.

## 3. Generality / Invariant Proof

Not formulated -- this scopes the defect, it does not fix it. The invariant a
fix must establish:

```text
Two stack accesses share a binding if and only if they name the same storage.
A display name is a function of the storage, so it must not be lossy about
which storage it came from.
```

`unsigned_abs()` violates the second sentence directly. Whether the right
answer is to make the name carry the sign, or to establish the frame size so
the normalizing branch actually runs, depends on why `stack_frame_size` is zero
for these functions -- which this has not measured.

## 4. Risk And Ownership Check

A naming change touches every function that has stack locals, so its diff is
large and mostly cosmetic; the frame-size fix is narrower in output but deeper
in analysis. Neither is a small change, and doing the naming half alone would
paper over the frame-size half, which is the one that also decides whether
`rsp` offsets are placed correctly at all.

Note what does *not* work: pruning the leftover declarations. That was
attempted (`2e35db20e`), and restricted to safety it reaches four of the 224 --
because the 123 in this class are stack-derived, and stack bindings cannot be
pruned in the normalize pass without deleting bindings a later stage still
needs. Pruning them would also have hidden this.

## 5. Validation Matrix

- [x] Every suffix taken across 60 binaries instrumented, with base, offset and
      frame size.
- [x] Correlation established: 63 of 63 at `stack_frame_size == 0`.
- [x] The leftover declarations in the anchor function match the suffixed names
      one for one.
- [ ] Why `stack_frame_size` is zero in these functions. **This is where the
      next cycle starts.**

## 6. AI Review / Prompt Firewall

- No external model was consulted. The diagnostic that produced these numbers
  was temporary and is not committed.
