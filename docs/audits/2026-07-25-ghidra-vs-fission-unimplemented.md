# Ghidra 12.0.4 vs Fission — unimplemented / partial subsystems

- **Date:** 2026-07-25
- **Ghidra tree (reference only):**  
  `vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Features/Decompiler/src/decompile/cpp/`  
  ([ADR 0005](../adr/0005-ghidra-reference-boundary.md) — no runtime dependency, no copy-paste of C++)
- **Related:** [gap investigation](2026-07-25-fission-ghidra-gap-investigation.md),  
  [problem inventory](2026-07-10-decompiler-problem-inventory.md),  
  [ADR 0008](../adr/0008-nir-substrate-and-owner-boundaries.md)

## Why this doc (and what it is not)

Recent residual work (Index DSE, XMM Aggregate→Float, **post-infloop multi-emit strip**) can look like  
“heuristic bandages.” That is a fair critique when the **graph-native** algorithm is missing.

This document answers: **what does Ghidra’s decompiler actually run that Fission does not have as a first-class owner?**  
Use it to pick the next **invariant-based** implementation, not another suffix cleanup.

**Not:** “copy Ghidra print style.”  
**Not:** printer patches for semantic gaps.  
**Yes:** cleanroom reimplementation of algorithms under Fission owners.

---

## 1. Ghidra architecture (reference)

### 1.1 Single SSA-ish IR + Action pipeline

Ghidra does **not** lower basic blocks to goto-C first and then “structure.”  
It keeps a **Funcdata / p-code graph**, repeatedly applies **Actions** and **Rules**, then  
**structures the control-flow graph in place**, then **merges high variables** and **prints**.

Root construction: `ActionDatabase::universalAction` in `coreaction.cc`  
(~5462–5734). The `decompile` group members include (order matters):

| Phase (Ghidra group / action) | Role (one line) |
|---|---|
| `ActionStart` / constbase / `ActionDefaultParams` | Start, constants, default prototypes |
| `ActionPrototypeTypes` / `ActionFuncLink` | Prototype / call linkage |
| **mainloop (repeat)** | |
| `ActionUnreachable` | Drop dead CFG |
| **`ActionHeritage`** | **SSA heritage: MULTIEQUAL, address-tied memory, stack** |
| `ActionDirectWrite` / ActiveParam / ReturnRecovery | Param & return discovery |
| `ActionRestrictLocal` / **`ActionDeadCode`** | Dead code with cover discipline |
| `ActionRestructureVarnode` / Spacebase / NonzeroMask | Locals, stack base, masks |
| **`ActionInferTypes`** | Type recovery fixed-point |
| **oppool1** (~100+ `Rule*`) | Algebraic / boolean / float / subvar rewrite |
| `ActionLaneDivide` | Vector lane split |
| MultiCse / ShadowVar / Deindirect | CSE / indirect calls |
| **`ActionStackPtrFlow`** | **Stack-pointer flow across calls** |
| RedundBranch / **`ActionBlockStructure`** | **CFG collapse → structured blocks** |
| ConstantPtr / oppool2 (PtrArith, Load/Store Varnode) | Stack slots as vars, ptr arith |
| NodeJoin / ConditionalExe | Join + conditional execution |
| **after mainloop** | |
| SwitchNorm / ReturnSplit / ActiveReturn | Switch normalize, return split |
| cleanup pool | Late cleanup rules |
| PreferComplement / **StructureTransform** / NormalizeBranches | Structure polish |
| **Merge\*** (Required, Copy, Adjacent, Type, …) | **HighVariable merge via Cover** |
| NameVars / MapGlobals / prototypes | Naming & globals |
| print (`printc.cc`) | C emission |

**Structuring core (not print):**

| Class | File | Algorithm |
|---|---|---|
| **`CollapseStructure`** | `blockaction.hh/.cc` | Fixed-point **collapse** of CFG components (if/loop/switch shapes) into structured nodes |
| **`LoopBody`** | same | Tarjan-style loop body: head, tails, exit, containments |
| **`TraceDAG`** | same | DAG path tracing → **likely unstructured edges (gotos)** when stuck |
| **`ActionBlockStructure`** | same | Driver action that runs collapse |
| **`ActionFinalStructure` / StructureTransform** | same | Final structure transforms |
| **`Heritage`** | `heritage.hh/.cc` | Address-tied SSA / MULTIEQUAL / load-store heritage |
| **`Cover` / HighVariable merge** | `cover.hh`, merge actions | Variable identity by **liveness cover**, not name heuristics |
| **`Rule*` pool** | `ruleaction.*` | Hundreds of local p-code rewrites (float, subvar, div, …) |

Critical property: when Ghidra collapses a loop, **nodes disappear from the residual graph**  
(component → single structure node). There is no second linear scan that **re-emits the same basic block** with the same label.

---

## 2. Fission architecture (current)

```text
p-code CFG
  → materialize / incremental heritage (builder)     ≈ partial Heritage
  → Dir* body (block stmts + terminators + labels)   ⚠ early textual CFG
  → normalize (many Rule-like passes)                ≈ partial oppool + cleanup
  → structuring (SESE collapse + linear residual)    ≈ partial BlockStructure
  → HIR presentation (clone polish)                  ⚠ not Ghidra structure
  → print                                            ≈ printc orthography only
```

| Ghidra | Fission owner today | Status |
|---|---|---|
| SLEIGH / p-code | `fission-sleigh` + `fission-pcode` | Strong (parity often ≈Ghidra) |
| Heritage (full address-tied SSA) | `run_incremental_heritage`, `apply_memory_heritage`, MULTIEQUAL materialize | **Partial** — not full Cover/Heritage fixed-point on Funcdata |
| StackPtrFlow | bits of call_recovery / stack slots | **Partial** |
| InferTypes + Rule pool | normalize stages + type_infer / use_type_infer | **Partial** — many Rules missing or weaker |
| **CollapseStructure + TraceDAG** | `sese_driver`, `loops`, `conditionals`, `linear_*`, `graph` | **Partial** — has collapse *rules* + SESE; **not** full TraceDAG likely-goto selection or graph-only collapse without multi-emit |
| HighVariable / Cover merge | `apply_variable_merge_pass` + materialize binding keys | **Partial** — cover-based merge incomplete |
| LaneDivide (XMM lanes) | materialize retype Aggregate→Float; no first-class LanedRegister | **Partial / ad-hoc** |
| StructureTransform / FinalStructure | structuring cleanup + presentation | **Partial** — presentation may paper over structure |
| printc orthography | `render/printer` + layered typedefs | **OK if IR types right** |

---

## 3. Unimplemented / weak vs Ghidra (prioritized)

Priority = impact on measured residual (matrix_multiply, O1/O2, m32, pair/kv) ×  
how clearly Ghidra has a named algorithm.

### P0 — Graph-native structuring (matches “duplicate label / multi-emit”)

| Missing piece | Ghidra | Fission today | Why residual looks “heuristic” |
|---|---|---|---|
| **Collapse on BlockGraph only** | `CollapseStructure::collapseAll` mutates graph; residual nodes are never re-scanned as raw blocks | SESE `active_child_map` + **final reconstruction still walks residual blocks** and can **lower the same `block_key` again** | `strip_post_total_infloop_*` / duplicate Label strip **paper over multi-emit** instead of forbidding it |
| **TraceDAG likely-gotos** | When collapse stuck, **score unstructured edges** once | Linear recovery / force labels / residual goto | Gotos appear as byproduct of emission, not of a DAG edge decision |
| **LoopBody model** | Head + multi-tail + exit edges + containments | SCC / while/do-while try_lower with admission gates | Nested O1 loops partially recovered then residual re-emitted |
| **Irreducible edges as first-class** | mark unstructured, continue | partial `irreducible` / virtualize | Incomplete |

**Correct direction (not cleanup heuristics):**  
Implement / finish **graph-native collapse**: once a region is selected for `[idx, skip_to)`,  
**no residual linear path may `lower_block_stmts` for keys in that range.**  
TraceDAG-style **unstructured edge selection** when no SESE shape fits.

Owner: `fission-midend-structuring` (`sese_driver` + `blockaction`-class logic).  
Ghidra refs: `blockaction.hh` (`CollapseStructure`, `TraceDAG`, `LoopBody`).

**Landed (partial, 2026-07-25):**

1. **Exclusive emission** in `lower_loop_body_subgraph` + SESE reconstruction tombstones.  
2. **TraceDAG likely-goto:** `TraceDag::select_likely_unstructured_edge` +  
   `select_bad_edge` prefers TraceDAG scores (Ghidra `selectBadEdge`); collapse-loop  
   virtualization **default on** (`FISSION_COLLAPSE_LOOP=0` to disable).  
3. **Heritage type witness:** memory heritage promotes float-typed stack slots as  
   `Float` (not default uint) when store/load elem type is float. Full Cover still open.  
4. **residual strip_post_total** remains belt-and-suspenders only after exclusive  
   emission + TraceDAG virtualization.

### P0 — Heritage / Cover (value identity)

| Missing piece | Ghidra | Fission | Symptom |
|---|---|---|---|
| Full **Heritage** passes on all spaces | `ActionHeritage` | Incremental heritage + memory heritage | Wrong merges, missing stores, param alias order |
| **Cover**-based HighVariable merge | `Cover` + merge actions | Binding names / partial merge | Distinct values share names; return/join noise |
| Stack space as first-class spacebase | SpacebaseSpace + Load/Store Varnode rules | stack_slots + ptr_arith | Undeclared locals, wrong slots |

Owner: materialize + normalize heritage; not printer.

### P1 — Stack pointer flow & prototypes

| Missing piece | Ghidra | Fission |
|---|---|---|
| `ActionStackPtrFlow` | Cross-call SP adjustment | Partial / ABI tables |
| `ActionDefaultParams` / ActiveParam / ReturnRecovery | Full proto recovery loop | terminator + call_recovery partial |
| FuncLink / deindirect | Indirect call resolve | Partial deindirect |

### P1 — Type recovery depth

| Missing piece | Ghidra | Fission |
|---|---|---|
| `ActionInferTypes` fixed-point with RulePtr\* | Strong | type_infer + use_type_infer partial |
| `RuleFloatCast` / `RuleSubfloatConvert` / floatprecision | Explicit float precision rules | Partial float_sign / subfloat; **no full x87 80-bit model** |
| `ActionLaneDivide` | Explicit SIMD lanes | XMM full-reg Aggregate vs float lane still fragile |

**m32 `float80`:** Ghidra models float precision through **floatprecision rules + types**, not by printing `float80`.  
Fission should either recover float32 after x87 promote/demote, or keep extended float with a **typed** IR + standard C orthography — not invent midend-agnostic printer renames as the “fix.”

### P2 — Rule pool density

Ghidra’s `oppool1` alone registers **~100 Rule classes** (div, shift, boolean, piece, float, subvar, …).  
Fission normalize has many analogous passes, but:

- not the same fixed-point schedule (mainloop / fullloop / stackstall nesting);
- many Rules have **no equivalent** or only tests-only coverage;
- CPU-specific extra_pool_rules path exists in Ghidra; Fission prefers ISA-agnostic invariants (ADR 0009) — good, but **data-driven rule coverage** still lags.

Owner: normalize rules, one invariant at a time, measured.

### P2 — Merge / naming / globals

| Ghidra | Fission |
|---|---|
| AssignHigh, MergeRequired/Copy/Adjacent/Type, NameVars | variable_merge, type_hints, global recover |
| MapGlobals / DynamicSymbols | partial globals in render/static |

### P3 — Present but intentionally different

| Topic | Stance |
|---|---|
| printc quirks | Do **not** clone as success metric (ADR 0011) |
| DIR dual path | Exploratory (ADR 0014); Ghidra has no DIR product |
| Emulator/solver validation | Fission extra; not Ghidra decomp core |

---

## 4. How recent residuals map (no “quality via cleanup”)

| Residual | Bandage (heuristic surface) | Principled owner gap |
|---|---|---|
| Duplicate `block_*` labels / re-run loop bodies | `strip_post_total_infloop_*`, strip duplicate labels | **CollapseStructure / no multi-emit residual** (P0 structure) |
| `fission_agg16` float accum | — (fixed midend binding type) | LaneDivide / float lane typing (P1) |
| Index DSE drop of `c[i]=` | — (fixed MemSSA Index key) | Alias = partition correctness (heritage/memory model) |
| m32 `float80` compile | printer `long double` (rejected) | floatprecision / demote-to-float32 after x87 |
| Os undeclared `xmm0_db` | rescue undeclared names | Heritage + materialize of all live lanes |
| O1/O2 sem=0.2 after structure cleanup | more cleanup | Correct loop-carried updates under **graph structure**, not residual text |

---

## 5. Recommended implementation order (invariant-based)

1. **P0 Structure emission contract**  
   Spec: *For each CFG block key K, at most one emission of K’s body into the structured result; consumed range `[entry, skip_to)` is tombstoned.*  
   Prove with a unit test that multi-entry nested loops cannot redefine `block_LABEL`.  
   Align with `CollapseStructure` + `emitted` set discipline (Ghidra collapses nodes away).

2. **P0 TraceDAG / unstructured edge selection**  
   When no SESE shape fits, mark **one** edge unstructured (goto), continue collapse — instead of linear residual multi-emit.

3. **P0/P1 Heritage + Cover**  
   Strengthen address-tied SSA and merge so bindings match Ghidra HighVariable identity rules.

4. **P1 Float precision / lanes**  
   x87 80-bit and XMM lanes as typed models (cspec + rules), not print renames.

5. **P2 Rule gaps**  
   Only when a **measured** row shows a missing rewrite; implement as shared Rule-like pass.

---

## 6. How to use Ghidra vendor tree correctly

For each gap:

1. Open the **named class** in `blockaction` / `heritage` / `coreaction` / `ruleaction`.  
2. Extract **invariants** (what must be true of the CFG / SSA / types).  
3. Implement under the **Fission owner** in the table above.  
4. Remeasure (`fission-benchmark` local docker; no Pages promotion).  
5. Do **not** copy C++ or link vendor.

---

## 7. Bottom line

| Layer | Vs Ghidra |
|---|---|
| Lift / raw p-code | Roughly on par (not the main product gap) |
| Heritage / Cover / StackPtr / InferTypes | **Materially incomplete** |
| **Block structure (CollapseStructure + TraceDAG)** | **Materially incomplete** — root of multi-emit / goto soup |
| Rule density | Incomplete but growing |
| Print orthography | Fine when IR is right; **not** the semantic owner |

The user’s critique is correct: **post-hoc residual strip is a symptom fix.**  
The Ghidra-shaped missing piece for that class of failure is **graph-native structure collapse with exclusive block consumption and TraceDAG unstructured-edge selection**, not more printer or more suffix heuristics.
