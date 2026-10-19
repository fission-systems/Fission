# Share selected immutable SLEIGH constructors

Mechanical performance experiment; baseline main 2091f9038f9337260174bb7b619e67759b956d35 / immutable CLI 6f39d552d6dee077a45272ef89cc9821091eb4b327ca1d984b97472a9d080762. Five earlier allocation prototypes did not meet native latency acceptance and were archived, not shipped.

## Measured owner evidence

Existing full native process sampling repeatedly observes CompiledParserWalker::walk, state and handle destruction, memory allocation/copying. The bounded native probe measured 22,937 states across 5,576 linear root instructions, including 17,361 boxed descendants. Source inspection shows each state copies seven immutable metadata fields from the selected constructor (mnemonic, template kind, template Arc, named sections Vec, context commits Vec, display template Arc, display operands Arc). Constructor walk frames separately allocate source and mnemonic strings for error-only diagnostics on every successful walk. Counts and profile samples are structural diagnostics, not phase wall-time estimates. Ghidra context.hh/context.cc are reference-only evidence of constructor metadata and instruction-state lifetime separation; no code/dependency is imported.

## Invariant and ownership

Store canonical executable constructors as Arc values in existing compiled subtables. Selection borrows that exact Arc; runtime state owns one clone instead of copying its seven metadata fields. RuntimeConstructState dereferences read-only to the selected constructor for existing field reads. Constructor error frames retain the same immutable constructor rather than two newly allocated string copies. Preserve decision matching, complete selection identity, instruction-specific handles/context/lengths, operands, all template behavior, order and errors. No global cache, program fact map, worker/timeout change, ISA-specific shortcut, new production dependency or semantic pass. Context changes are still applied per instruction; only their immutable descriptions are shared.

The Rust construction API changes: subtable constructors are Arc values; runtime state supplies a constructor Arc instead of seven separate metadata fields. Explicit fixture callers construct complete metadata. Mutation requires explicit Arc::make_mut at that owner and never mutates other bound states; no DerefMut is provided. Retain logical state Debug with original fields/order/content. Compiled serde JSON and bincode cache bytes must remain identical to a legacy owned-constructor representation in both directions. rkyv current-version roundtrip is tested; raw old rkyv memory layout is not promised, and no persisted frontend-rkyv consumer is present. CLI/JSON/DecBench contracts unchanged.

## Validation and acceptance

Test pointer identity and lifetime independence from frontend; cloning/drop and explicit clone-on-write isolation; error stack equality; complete 1,024 raw-state/lift/error records and FID hash/resolver-order matrix; legacy JSON/bincode compatibility and current rkyv roundtrip. Targeted nextest then seven related crates, checks, CI-scoped Clippy, release, fmt, owner scan. Freeze exact source/binary. Three alternating native large-batch pairs must exceed 2% median latency reduction; run five independent inverted-order confirmation pairs requiring >=2% median paired reduction and no material CPU regression. If useful, small-input controls, fresh 250 complete non-duration producer records, both fixed Kuna settings, report RSS and all tradeoffs. No quality/ranking or guest-execution claim; reject nonreproducible gains.

## Result

Accepted mechanical candidate: initial large median latency -2.21%; independent confirmation median paired latency -5.03%, CPU -7.21%, RSS +0.27%. Fresh 250 complete non-duration records unchanged. Small inputs within +/-0.6%, no gain claimed. Related tests 2,209 pass/9 skip (one leaky pass). Fixed Kuna still 2.73-6.90x faster. See docs/validation/constructor-ownership-performance.md and .json.
