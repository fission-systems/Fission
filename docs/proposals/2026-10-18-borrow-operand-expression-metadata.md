# Borrow operand-expression metadata from the frontend

Mechanical CPU performance proposal. Base main 49ea4ebc175a4b28b28ed17266c367c42f3caea0, accepted release SHA256 7a3eb9c923537a3280fb80407822fa1a35188b1c8bb71e25279249347ef3ae61, Rust1.97.0. GPU work discarded; PR505 closed and its branch removed. Performance checkout clean and seven unrelated linked-main files preserved.

## Measured owner before implementation

An immutable symbol-enabled executable from the same source tree profiles the canonical `info --identity --detections` path on a real 156 MiB arm64 Mach-O LLVM library. Its complete output equals the accepted release. Native unprofiled baseline: 41.02 real / 476.34 user+sys seconds, RSS 2168684544 bytes; two further controls run before edits. A preliminary symbol-demangling hypothesis is not implemented: the profile instead points to runtime function discovery/SLEIGH binding.

The diagnostic profile totals 448881845 positive sampled thread-CPU microseconds. `CompiledParserWalker::eval_pattern_expression` has 151582399 self-weighted microseconds. Its offsets 0x294 and 0x5e0 account for 79203318 and 64036441; disassembly places them immediately after Arc increment/decrement instructions in inlined `eval_operand_value_expression`. This is sampled CPU attribution, not a wall-time percentage or proof of a patch. Raw profile, symbols, commands and disassembly retained locally.

## Invariant and owner

RuntimeSelection stores `&'a Arc<CompiledExecutableConstructor>`, borrowed from the frontend. Constructor templates and their operand specs are immutable. During operand-value expression evaluation, an Arc clone of the selected spec is made solely to avoid a self-borrow across recursive mutable walker operations; that local owner never escapes. A reference with the frontend lifetime is sufficient even if walker selection/mutable state changes, because the original selected constructor remains alive in the borrowed frontend. Preserve owned Arc clones for actual returned RuntimeHandle/RuntimeConstructState payloads.

Add a private typed lookup returning `Result<&'a CompiledOperandSpec>` and use it in this one evaluator. Preserve missing-operand errors, offset resolution, operand/pattern recursion and order, decode side effects, inst_next/PC witness capture, pending handle reads, full state/lift/error behavior, FID hashes and resolver ordering. No new caches, telemetry, compiler IR/serialization shapes, runtime dependency, ISA or function/address predicates. Rust lifetimes enforce immutable-owner retention. Reference-only Ghidra context.hh keeps Constructor pointers in ConstructState while ParserWalker traverses them; no code copying/binding.

## Predeclared validation / acceptance

Focused borrow/lifetime and independent legacy-clone differential tests, all existing 1024 full raw-state/lift/error plus 512 FID hash/resolver records, related nextest/checks/scoped Clippy/release/fmt/owner scan. Same compiler/source/binary/input/resource hashes before comparisons. All build/test/profile work finishes before native timings.

Primary acceptance is the real large LLVM `info` job: initial three alternating pairs require >=2% separate-median wall improvement; independent five inverted pairs require >=2% median paired wall improvement and CPU <=+2%; report RSS. Complete metadata must match in every pair. Report the two other real large LLVM tools as broader controls, without tuning on them. Keep existing x86 large six-request batch and three small platform cells as latency/CPU regression controls (no >2% median latency regression on confirmation; document noise/failures).

Only after native gate passes: 45 full identity/detection JSONs, 45 complete function lists, fresh 250 stripped NIR requests compared to baseline with only the seven documented duration fields excluded; four fixed Kuna1.808 batches at jobs1/14 if claiming updated relative speeds. No source-semantic, readability, score, official rank, execution parity or Kuna-parity claim. Unsuccessful candidates are archived and restored.

Commit accepted code/docs/results only from linked main; preserve unrelated files, push a review PR, and merge after exact-head CI.

## Results

See the [measured validation report](../validation/operand-expression-metadata-performance.md). The production change remains limited to the private expression metadata lookup and evaluator. The independent original clone-based evaluator is test-only.
