# Decompiler Change Proposal: a value passed as a `void *` argument is a pointer

## 1. Baseline Row Anchor

- Binary: DecBench `O0/bzip2/bzip2`, `O0/bash/bash`, `O0/coreutils/cksum` (stripped)
- Function: `default_bzfree` (`0x6847`, `void default_bzfree(void *opaque, void *addr) { if (addr) free(addr); }`), `xrealloc` (`0xbde63`), `rl_free` (`0x116768`), `crc_sum_stream` (`0xd141`, `memcpy`)
- Corpus row or benchmark command: `~/fission-benchmark/decbench-full/sweep.py run --config full --workers 2`, baseline run `main-1f10b8c78`
- Current output summary: `sub_6847(unsigned long long param_1, long long param_2) { if (param_2) free(param_2); }`
- Failure category: type recovery -- a pointer argument declared as the pointer-width integer its register gave it.
- Observations: in the full-dataset baseline, 1,618 ground-truth pointer variables printed as integers are passed bare to a known C-library pointer parameter; 98% of those parameters are `void *` (`free` 1,325, `memcpy` 619, `memmove`, `memcmp`, `realloc`, ...), because a specific declaration (`char *`) is already surfaced.

## 2. Owner Proof

- [x] Type/data recovery: `fission-midend-normalize` `types/callsite_type_prop.rs`, the API-parameter loop of `apply_callsite_type_prop_pass`.

Evidence: the parameter type is resolved (`win_type_name_to_nir("void*") = Ptr(Unknown)`), but `tighten_binding_ty` only replaces `Unknown`, and `apply_api_surface_type_transitively` deliberately skips generic object pointers. The pointer fact is dropped.

## 3. Generality / Invariant Proof

```text
A generic object-pointer API parameter proves the argument value is a
pointer. When the argument name holds one value (a formal, or a local
defined once and not self-referential: binding_is_safe_for_backward_refine,
the guard operation-edge type flow uses), apply the existing pointer
contract (tighten_binding_ty_from_pointer_contract), which replaces only an
integer of pointer width. Names reused for several values keep the
monotone rule.
```

- [x] Not gated on an ISA; pointer width comes from `is_64bit`.
- [x] Synthetic test states the shape (formal passed to `free`; a two-definition register passed to `free` is left alone).

## 4. Risk And Ownership Check

- Existing owner extended; no new pass. `is_generic_void_pointer_type_name` is factored out of `call_target_surface.rs` and shared, so the two paths agree on what "generic" means.
- Specific pointer declarations are untouched (their surface type already reaches the argument; `callsite_type_prop_promotes_import_param_name_and_surface_type` still pins that).
- The printed type becomes `void *`, the least specific pointer; any later pointee evidence still refines `Ptr(Unknown)`.

## 5. Validation Matrix

- [x] Targeted test: `cargo nextest run -p fission-midend-normalize generic_pointer_parameter_makes` (fails with the rule disabled).
- [x] Crate gates: `cargo nextest run -p fission-midend-normalize`, `cargo nextest run -p fission-pcode`.
- [x] Full-dataset sweep before/after (see PR body).

## 6. AI Review / Prompt Firewall

- [x] No other model consulted.

## 7. Review Notes

- [x] No hardcoded binary/function/address/corpus guards.
- [x] No metric-only edits.
- [x] Extends the existing owner.
