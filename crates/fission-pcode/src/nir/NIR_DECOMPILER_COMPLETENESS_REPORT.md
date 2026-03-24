# NIR Decompiler Completeness Report

Last Updated: 2026-03-24

## Scope

This report summarizes the current completeness of Fission's NIR decompiler pipeline by comparing implemented capabilities against reference decompilers:

- Ghidra (vendor reference):
  - `vendor/ghidra/ghidra-Ghidra_11.4.2_build/Ghidra/Features/Decompiler/src/decompile/cpp/block.cc`
  - `vendor/ghidra/ghidra-Ghidra_11.4.2_build/Ghidra/Features/Decompiler/src/decompile/cpp/blockaction.cc`
  - `vendor/ghidra/ghidra-Ghidra_11.4.2_build/Ghidra/Features/Decompiler/src/decompile/cpp/block.hh`
- RetDec (vendor reference):
  - `vendor/retdec-5.0/src/llvmir2hll/`

Primary Fission code reviewed:

- `crates/fission-pcode/src/nir/structuring/`
- `crates/fission-pcode/src/nir/normalize/`
- `crates/fission-pcode/src/nir/types.rs`

---

## Overall Completeness (Estimated)

**Current overall completeness: ~45%** (relative to mature structuring behavior in Ghidra + RetDec references).

The current state is strong on conditional reconstruction and normalization, but still behind in loop-control recovery, switch structuring breadth, and goto cleanup.

---

## Category Matrix

| Category | Fission Completeness | Notes |
|---|:---:|---|
| CFG Analysis | 60% | `cfg_analysis.rs` has edge classes, dominators/postdominators, Tarjan SCC, irreducible detection; currently underused by primary reducers |
| Loop Structuring | 35% | `while`/`do-while` present; missing robust `for`, `break`/`continue`, infinite-loop reducers, irreducible loop treatment |
| Conditional Structuring | 65% | Strong `if`, `if-else`, short-circuit (`&&`, `||`), shared-tail handling |
| Switch Structuring | 25% | Basic/canonical patterns only; lacks broader CFG-native switch/fallthrough handling |
| Goto Elimination / Recovery | 35% | Region linearization + label cleanup exists; no full goto-optimization pass equivalent |
| Post-processing | 55% | Good arithmetic/slot/cleanup normalization; missing richer semantic post-passes |
| Type / Variable Recovery | 20% | Limited in this layer (mainly slot surfacing and cleanup) |
| Overall Output Quality | 45% | Good for many medium CFGs; large/complex CFGs still rely on force-linear/recovery paths |

---

## Implemented Strengths

1. **Conditional reconstruction quality is solid**
   - Modules: `conditionals/if_else.rs`, `conditionals/plain_if.rs`, `conditionals/short_circuit.rs`, `linear.rs`
   - Handles canonical and several non-trivial shared-tail patterns.

2. **Normalization is practical and mature for many patterns**
   - Modules: `normalize/arith.rs`, `normalize/slots.rs`, `normalize/cleanup.rs`, `normalize/bitstream.rs`
   - Includes expression simplification, slot surfacing, label/goto cleanup.

3. **CFG analysis foundation is now in place**
   - Module: `structuring/cfg_analysis.rs`
   - Includes:
     - `EdgeClass` (Tree/Back/Forward/Cross)
     - `DomTree`, `PostDomTree`
     - `SccAnalysis` + irreducible multi-header SCC detection

4. **Telemetry and automation integration improved**
   - `NirBuildStats` now tracks SCC/irreducible counters and conservative fallback rejections.
   - `fission-automation` reports and deltas consume these fields.

---

## Main Gaps vs Ghidra/RetDec

1. **Loop control structuring gap**
   - Missing broad `break`/`continue` shaping and robust `for` recovery.

2. **Switch breadth gap**
   - Missing richer switch/default/fallthrough patterns seen in mature decompilers.

3. **CFG analysis integration gap**
   - Dom/postdom/SCC data exists but is not yet deeply used in all core reducer decisions.

4. **Goto optimization gap**
   - No dedicated post-pass comparable to full goto simplification/rewriting optimizers.

5. **Semantic post-pass gap**
   - Missing higher-level readability transforms such as broad early-return reshaping and if-to-switch style upgrades.

---

## Current Test Footprint Snapshot

NIR test distribution currently includes strong coverage in:

- `normalize_arith` (31)
- `structuring_misc` (30)
- `structuring_conditionals` (22)
- `normalize_slots` (14)

Total `fission-pcode` unit tests currently pass in local verification.

---

## Recommended Next Steps (Priority)

1. **Loop control reducers** (`break`/`continue`/infinite-loop patterns)
2. **Switch recovery expansion** (default/fallthrough-heavy CFGs)
3. **Promote dom/postdom/SCC into primary reducer decisions**
4. **Dedicated goto cleanup post-pass**
5. **Additional semantic post-structuring passes**

---

## Notes

- This is an engineering completeness estimate, not a claim of binary compatibility with Ghidra/RetDec outputs.
- Scores should be updated as new reducers and CFG-driven decisions move from diagnostic support to primary structuring flow.
