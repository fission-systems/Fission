# Reuse an instruction bind probe when fallthrough is unobserved

Status: mechanical performance proposal, before implementation. No semantic, readability, metric or ranking gain is claimed.

## Measured anchor and owner

Clean main 2b1fa5c6adb54c7b8b98266c043f098d2fd2a8db (tree identical to 686f52329) is the baseline. Its frozen release SHA256 is 2ebaf8d171886efe8e1d14fa5f4b6d816708f97313ac2331a213642fe48685fd. Previous clean-main qualification measurements: large six-function batch 42.364 s; small Linux/Windows/macOS medians 2.256/0.972/1.366 s. Fresh post-merge symbolized diagnostic samples at 12 and 24 seconds show ControlFlowFacts::assemble waiting on sequential no_return_functions_for: 1580/1631 and 1624/1630 samples in that holder, with 1316 and 1335 in native instruction decode/lift. Other Rayon workers wait on the existing CF OnceLock. Profiled timings are excluded from performance comparisons. Raw diagnostics: benchmark/artifacts/performance/2026-10-15-decode/.

The canonical owner is compiled_table/walker.rs. Root binding currently walks the entire selected constructor tree twice, first to learn full length and then to resolve early inst_next expressions against that length. No-return, FID, xref and function lifting all pay this cost. Ghidra reference sleigh.cc separates parser resolution from handle resolution and retains parser contexts; it is consulted for invariants only. No reference implementation is copied or depended on.

## Invariant and scope

The only changing input between root walks is InstNextShared length-estimation state. Its bound_end, unbound_min and root_min_length fields are read only by inst_next_addr (writes update estimation, not constructor selection or output independently). If the first successful walk and all descendants never observe inst_next_addr, the second walk cannot change any handle, constructor selection, length, context, export or trace stored in RuntimeConstructState. Reuse that first state. Record a local Cell<bool> on the already shared root state, set in the canonical inst_next_addr read. Otherwise retain the original complete second walk exactly. Preserve initial failures and the full two-pass path for RIP-relative targets, trailing immediates, nested exports and any other observed fallthrough expression. InstNext2 remains unsupported where it was already unsupported.

This does not share results between different input windows, addresses, contexts, images or specifications. No persistent/global instruction cache or program-fact map is added. Full p-code emission, pending context commits, FID matching, no-return admission, instruction limits, workers, diagnostics schema and error handling stay in their existing owners. Diagnostic traversal traces may contain fewer repeated probe events.

## Validation and acceptance

- Compare complete states to an explicit original two-pass reference on real packaged x86, AArch64 and Thumb instructions, including prefixes, direct/relative flow, RIP memory with trailing immediate, nested exports, context and truncated failures. Assert both observed/unobserved paths are exercised. Existing delay-slot/context/FID golden tests remain required.
- Targeted nextest, full sleigh/static/decompiler/CLI/pcode suites and crate checks, release CLI, workspace formatting and owner checks.
- Freeze candidate separately. Five alternating small batch pairs and repeated alternating large pairs against immutable current main, no own concurrent builds/tests/profilers, same resources and worker policies. Measure wall/CPU/RSS and compare all non-time function fields.
- Fresh cache-free 250-function stripped qualification replay against retained clean-main output. Only seven existing duration keys may differ. No source/debug information passed to producers; no executable behavior parity or score claim.
- Keep the patch only with measured practical improvement and output preservation; otherwise record/revert the experiment. Kuna-level performance is not assumed.

## Accepted local evidence

Three focused tests and 1,831 related tests pass (five skipped), with crate checks, release CLI, formatting and owner-boundary checks passing. Successful-bind state comparisons exercise both reuse and original fallthrough-sensitive paths; first-probe errors propagate before the gate. Small five-pair medians improve 7.8%/4.2%/27.7%; large three-pair median improves 42.9857 -> 31.6041 s, with 250/250 fresh non-time records unchanged. Memory remains about 3.9 GB. Fission remains 3.07–11.66 times slower than pinned Kuna 1.808. See docs/validation/instruction-probe-performance.md and its compact JSON for all bounds and reproduction evidence.
