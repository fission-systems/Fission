# String-to-function navigation validation — 2026-10-12

## Scope and owners

Baseline: `main` commit `30fda95c0612849c39f608958a5590b58f7fbda0`.
Implementation and validation used an isolated clean worktree and checkout-local
Cargo target. Primary checkout work, the abandoned decompiler edits and PR #176
were preserved.

The baseline already offered string addresses/referrers and a whole-program
call graph. Its `strings --xrefs` fallback could be an instruction address
rather than a known function entry. Following evidence and callers/callees
required separate index joins, and the callgraph emitter did not carry xref
coverage. No existing open issue owned this bounded navigation work.

- `fission-static::xref_index`: span queries over the existing target index.
  The same target lookup changes from hash map to ordered map for bounded range
  lookup; records, IDs, extraction and existing address queries are retained.
- `fission-static::string_xrefs`: borrowed typed reference/function/call joins.
  No new persistent program-fact store or disassembly pass is introduced.
- CLI: explicit `strings --context` envelope, literal `--contains` filter,
  discovery options and `callgraph --function`; names come from the existing
  loaded metadata with the saved project applied.

Ghidra's vendor function-call graph reference was consulted for the separation
of incoming/outgoing relationships. No vendor implementation or runtime/build
dependency was copied into production.

## Results

| Gate | Result |
| --- | --- |
| Focused string/xref/callgraph + new CLI nextest | 38 passed; 167 filtered out |
| Full `fission-static` + `fission-cli` nextest | 204 passed; 1 ignored fixture test |
| Affected-crate check and clippy with CI warning policy | Passed |
| Locked release CLI build, workspace fmt and diff checks | Passed |
| Navigation release smoke | 24 commands + 5 binary SHA-256 checks passed |
| Existing project/CLI release regression smoke | 43 commands + 2 binary SHA-256 checks passed |
| Existing JSON comparison on PE/ELF | All 6 pre-existing payload projections unchanged |
| New context repeat-generation | Byte-identical stdout on all 5 binaries |

The pre-existing ignored test is
`fact_coverage_fixtures_match_control_flow_facts`; this run does not assert its
fixture coverage. Final focused/full runs had no leak warnings. An earlier
focused run reported one nextest leak warning; it is not process-cleanup proof.

Release fixtures: `win_x64_write.exe`, `x64_static_printf_malloc.elf`,
`x64_dyn_printf_malloc.elf`, `linux_x64_hello_sys.elf`, and
`linux_aarch64_readlinkat.elf`. Every emitted string reference matched its full
canonical `xrefs --json` record at the reported ID; coverage matched that index.
Control-flow/string anchors did not become data uses, executable printable
runs did not become string leads, and unknown owners were retained as references.

The static ELF format-string lead produced a discovered entry with five indexed
callees. `callgraph --function` returned the same immediate relationships.
Some other string uses had no discovered owner; those correctly remained
instruction references instead of becoming invented functions. No increase in
xref precision/recall or discovery completeness is claimed.

Regressions cover suffix references, explicit terminator span bounds, pointer
storage versus instruction ownership, source evidence and ID retention,
zero-length/overflow spans, filter and legacy array compatibility, empty query
coverage, saved names and `--no-db`, selected graph entries, and non-success
stdout on unknown function/unsupported ISA. Unknown ELF machines are rejected
by the loader before an index is built; they do not produce a successful empty
navigation result.

## Reproduce

```bash
cargo nextest run -p fission-static -p fission-cli \
  -E 'test(string) | test(callgraph) | test(xref) | binary(string_navigation_cli)'
cargo nextest run -p fission-static -p fission-cli --status-level all
cargo check -p fission-static -p fission-cli
cargo clippy -p fission-static -p fission-cli --locked -- \
  -D warnings -A clippy::style -A clippy::complexity
cargo build -p fission-cli --release --locked
cargo fmt --all --check
fission_cli strings sample.elf --contains 'msg=%s' --context --json
fission_cli callgraph sample.elf --function <reported-entry> --json
```

Local raw before/after output, source/binary hash manifest, smoke script and
command logs are under
`benchmark/artifacts/re-workspace/2026-10-12-string-navigation/` in the primary
checkout. Those artifacts are ignored by Git; this report and regression code
are committed.

## Limits

This is printable-ASCII and one-hop indexed navigation. It does not recursively
follow pointer chains, recover unresolved indirect calls, treat tail jumps as
calls, run p-code analysis or execute the binary. Empty lists are scoped indexed
results, not whole-program negative evidence. Discovery and decoding remain
bounded by existing profiles and the emitted coverage report. Legacy
`--xrefs` string rows retain their fallback convention for compatibility; the
new envelope is the explicit ownership/evidence path.

Local validation was macOS arm64; Windows was not validated. No decompiler
algorithms, DecBench scores/ranking or full RE-tool maturity claims are included.
