# ISA Semantic Debt Inventory (P0)

- **Date:** 2026-07-10
- **Scope:** NIR semantic path under `crates/fission-pcode/src/nir/`
  (builder/materialize, normalize, structuring)
- **Policy:** [ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)
- **Method:** Manual grep + owner review of `CallingConvention::*` gates,
  emergency normalize passes, env-gated materialize features, and shared helpers
  introduced by recent m32/cmov/signum quality work.
- **Non-goal:** Big-bang rewrite. This table is the refactor backlog for
  opportunistic cleanup when touching related owners.

## Status legend

| Status | Meaning |
|--------|---------|
| **OK** | Shared CFG / def-use / register-family invariant; extend, do not fork |
| **MODEL** | Allowed ISA surface (cspec, namer, CC slot tables, stack-param classification) |
| **DEBT** | Semantic meaning gated on ISA string/CC, or emergency cleanup papering over an earlier owner |
| **ENV** | Opt-in env flag; treat as experimental debt until default-on with an invariant owner |

## Shared helpers (OK — prefer extending)

| Helper | Owner path | Invariant (ISA-agnostic statement) |
|--------|------------|-------------------------------------|
| Loop-carried register update detection | `builder/materialize/loop_carried/` (`shape.rs`, `binding.rs`, `seed.rs`) | Register-space def that reads itself (or is live across a backedge) keeps a stable binding name; param/HW names preferred over fresh temps |
| Same-block forward branch resolve | `nir/cfg.rs` → `same_block_forward_branch_target_op_idx` | CBranch target that lands later in the same block is a skip edge (relative seq *or* absolute code address), not an always-taken guarded Copy |
| Return-join live-in guard | `structuring/linear/mod.rs` → `forwarding_block_defines_return_tail_live_in` | A single-successor “forward” arm that defines a join Return’s live-in (including primary return reg) is not a trivial empty forward |
| Unconditional-only copy merge | `normalize/recovery/variable_merge.rs` → `collect_direct_copies` | Only top-level unconditional `v = w` establishes merge identity; path-sensitive cmov/if copies must not collapse clamp/min/max arms |
| Loop-carried candidate shape | `loop_carried/shape.rs` → `is_loop_carried_register_update_candidate` | Register-space, non-constant, size ≥ 4 — no CC enum on the shape check |

These are the patterns ADR 0009 expects new quality work to extend.

## MODEL surfaces (allowed — keep differences here)

| Surface | Owner path | Notes |
|---------|------------|-------|
| Register namer / primary return / param slots | `nir/cspec/register_model.rs`, `cspec/apply.rs` | CC tables map offsets → names and return/param sets |
| Stack-parameter slot classification | `builder/memory/stack_slots.rs`, `nir/abi.rs` | Offset tables per CC are model data |
| Stack push / call-arg recovery (x86-32 push idiom) | `builder/calls/call_recovery.rs` | Encoding decode into stack-arg list; **policy** of “call has N args” must stay CC-table driven |
| Status-flag offset map (x86 EFLAGS bits) | `register_model.rs` hw_name_at 0x200.. | Naming model; consumers should ask “is flag register?” via namer/pspec, not hard-code offsets |

## DEBT rows (semantic cores / emergency)

| ID | Location | Kind | Issue | Preferred fix direction |
|----|----------|------|-------|-------------------------|
| D1 | `normalize/cleanup/temp_var.rs` → `hoist_param_alias_copies_before_first_use` | Emergency pass | End-of-pipeline hoist of pure `v = param_N` when use precedes def (motivated by cmov/flag recovery). Papers over materialize ordering | Root owner: materialize / cmov arm binding so param aliases dominate first use. Mark pass removal when synthetic + clamp rows pass without it |
| D2 | `builder/materialize/loop_carried/binding.rs` → `loop_header_external_seed_binding_name_for_update` | ISA admission gate | Early `return None` unless WindowsX64 \| SystemVAmd64 \| X86_32. Core seed logic is already param/return/GPR | Drop CC gate; admit via `param_slot_for_varnode` / `is_primary_return_register` / register-space candidate. Phase ARM later with same rule |
| D3 | `builder/materialize/no_consumer.rs` → `is_x86_status_flag_output` | Hard-coded offsets | Suppress no-consumer pure flag defs using raw 0x200.. offsets | Query cspec/pspec “flag group” or namer `is_flag_register`; keep pure-RHS check |
| D4 | `builder/expr/lower_expr.rs` → `stack_pointer_register_name` | CC-local name table | X86_32 special-case ESP/EBP offsets before namer | Prefer namer/cspec only; delete dual path once namer covers m32 frame regs |
| D5 | `builder/calls/call_recovery.rs` → `is_x86_32_esp` / push helpers | Encoding helper with CC guard | ESP detection hard-codes offset 0x10 + X86_32 | Generalize to “stack pointer varnode” from cspec; keep push-store scan as encoding decode |
| D6 | `normalize/recovery/flag_recovery.rs` → `is_x86_flag_variable` | Name-string ISA gate | Flag recovery keyed on x86 flag name strings | Prefer typed flag facts from materialize/cspec; name lists as fallback only |
| D7 | `normalize/idioms/prologue.rs` (and related) | HW name lists | Prologue recognition via raw rbx/rbp/… / ARM r4… lists | Frame / callee-save sets from cspec; keep as MODEL if moved into tables |
| D8 | `builder/control/terminator.rs` (Arm32 branches) | CC-forked control paths | Multiple `calling_convention == Arm32` branches in terminator | Shared join/return/live-in helpers with CC-supplied register sets (same pattern as recent x86 return-join work) |

### Priority for P0 remediation

1. **D1** — emergency pass; highest architectural cost if left permanent  
2. **D2** — one-line admission gate blocking non-x86 reuse of a good helper  
3. **D3–D5** — model leakage into materialize/call recovery (batch when touching namer/cspec)  
4. **D6–D8** — normalize/terminator cleanup when next quality cycle hits those owners  

## ENV experimental flags (do not grow without owners)

| Flag | Path | Role |
|------|------|------|
| `FISSION_ENABLE_NO_CONSUMER_SUPPRESSION` | `materialize/no_consumer.rs` | Gates broader no-consumer suppress path |
| `FISSION_ENABLE_PARITY_CHAIN_MATERIALIZATION` | `materialize/same_block.rs` | Experimental parity-chain materialize |
| `FISSION_ENABLE_STACK_ADDR_FRAME_STABLE_REPLACEMENT` | `materialize/same_block.rs` | Frame-stable stack-addr replacement |
| `FISSION_ENABLE_COPY_OVERWRITE_RESTART` | `materialize/cross_block.rs` | Cross-block restart experiment |
| `FISSION_ENABLE_PREDICATE_REFRESH_RESTART` | `materialize/cross_block.rs` | Predicate refresh restart |
| `FISSION_ENABLE_EXPLICIT_MERGE_BINDING` | `materialize/cross_block.rs` | Explicit merge binding |
| `FISSION_ENABLE_BLOCKGRAPH_COLLAPSE` / `FISSION_ENABLE_MIR_BLOCKGRAPH` | `structuring/driver/admission.rs` | Alternate structuring admission |
| `FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION` | `normalize/analysis/defuse.rs` | Dead-assign rerun admission |
| `FISSION_COLLAPSE_LOOP` | `structuring/collapse_loop.rs` | Loop collapse opt-in |
| `FISSION_STRUCTURING_ENGINE` | `types/options.rs` | Engine selection |

Debug-only (`FISSION_PREVIEW_DEBUG`, `FISSION_PREVIEW_DIAG`, `FISSION_PREVIEW_PERF`, …) are observability, not semantic debt.

**Rule:** New production defaults must not add `FISSION_ENABLE_*` knobs. Promote only after an invariant owner + tests; otherwise remove or leave off.

## What is *not* debt (recent quality work)

These were motivated by m32 rows but restated as common facts (ADR 0009 §3):

- Absolute + relative same-block CBranch resolution for cmov chains  
- Loop-carried IntAdd/IntRight name stability  
- Conditional-copy merge barrier (cmov arms)  
- Return-arm live-in vs trivial-forward (signum-class diamonds)  
- Structuring has **no** `CallingConvention::` matches under `nir/structuring/` (good separation)

## Review checklist (when editing NIR semantics)

- [ ] Condition is CFG / dominance / def-use / register family / ABI *slot*, not `== X86_32` alone  
- [ ] Encoding decode (push, cmov absolute target, EFLAGS offset) ends in a shared fact  
- [ ] No new end-of-pipeline normalize pass if materialize/structuring can own the binding  
- [ ] Extend an **OK** helper above when the failure family matches  
- [ ] Update this table when a DEBT row is fixed or a new emergency pass is added  

## Related docs

- [ADR 0009 — ISA-agnostic semantic rules](../adr/0009-isa-agnostic-semantic-rules.md)  
- [ADR 0008 — NIR substrate / owner boundaries](../adr/0008-nir-substrate-and-owner-boundaries.md)  
- [NIR AGENTS.md](../../crates/fission-pcode/src/nir/AGENTS.md)  
- Older token scan (noisy): [2026-07-04-arch-isolation-scan.md](2026-07-04-arch-isolation-scan.md)  
