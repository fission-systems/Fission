---
name: fission-cli
description: Use the Fission CLI (fission_cli) to inspect a binary, recover C-like pseudocode and disassembly, follow strings and call relationships, and check findings by execution, for CTF reversing, vulnerability hunting, malware triage and program understanding. Covers using Fission, not developing the decompiler (for that, see the repository's SKILL.md).
---

# Fission for reversing binaries

Fission is a headless CLI (`fission_cli`) on PATH, not an MCP server. It holds no
resident session: every call loads and analyses the binary again. On ELF that cost
about 0.01 s for a 28 KB binary and 0.07-0.14 s for 3-6 MB binaries with up to
4,200 functions (PE and Mach-O were not measured), so many small calls are cheap;
the time is in a single large function, not in reloading. The one thing it saves
is the optional analyst database described under "Keep your decisions".

`fission_cli skill` prints this guide from the binary you are about to drive, so it
matches that version. In a checkout the built CLI is `target/release/fission_cli`
(`cargo build -p fission-cli --release`).

Primary target is x86 and x86-64 ELF, PE and Mach-O. Other architectures decode but
are measured less; say so when you rely on them.

## Always ask for the machine contract

Put `--contract v1` before the subcommand and give the command its own `--json`:

```bash
fission_cli --contract v1 list ./target.bin --json
```

- stdout is exactly one JSON document, success or failure. Logs and the
  `[fission] Loaded config` line go to stderr, so do not merge the streams.
- Branch on `ok`, `error.code` and the exit code. `error.message` and `error.hint`
  are for reading. `error.retryable` says whether the same call can work later.
- `data` is the command's normal JSON (arrays stay arrays). Per-command shapes are
  not typed yet, so read a first result before writing a parser.
- `fission_cli schema` prints the envelope schema, `fission_cli schema --error-codes`
  the stable error codes.

| Exit | Meaning | Next step |
|------|---------|-----------|
| 0 | Done. A batch with some good items still exits 0; failures are listed in `data`. | Check each item's `error` and `fell_back`. |
| 1 | No usable result (`all_items_failed`, `quality_budget_exceeded`, `unclassified`) | Read `error.message` and the item `error` fields. |
| 2 | Bad arguments (`usage_error`, `json_required`) | Fix the call; `--help` on the subcommand. |
| 3 | Input missing, unreadable or unsupported (`input_not_found`, `input_unsupported_format`), or a `--name` that did not resolve (`function_not_found`, `function_ambiguous`) | Check the path and the file type; for names, `list` and pass `--addr`. |
| 4 | Some items failed and `--fail-on-partial` was given (`partial_failure`) | Retry the failed addresses one by one. |
| 5 | Time limit hit (`function_timeout`, retryable) | Raise `--timeout-ms` or narrow the scope. |
| 6 | Resources missing (`resource_root_missing`) | Set `--resource-root` or `FISSION_RESOURCE_ROOT`; `fission_cli resources status` shows what resolved. |
| 70 | The command crashed | Record the command and report it; try another address. |

A text file, a truncated header or an empty file is `input_unsupported_format`
(exit 3). `unclassified` is what is left when no specific code applies yet: read
`error.message`.

## Orient, then choose the scope

```bash
fission_cli --contract v1 info ./target.bin --json            # format, arch, entry, counts
fission_cli --contract v1 list ./target.bin --json            # discovered functions
fission_cli --contract v1 strings ./target.bin --contains 'password' --context --json
```

- `info --detections` names packers and compilers. Entries marked `(Low)` are weak
  matches and often wrong: a plain GCC binary showed "Ezuri" and "Zig" at Low.
  Believe a detection only when it is not Low or other evidence agrees.
- `list` gives `address` (hex string), `name`, `size`, `kind`, `origin`. On a
  stripped binary names are empty and the entries come from the unwind tables
  (`kind: eh_frame`, `origin: elf-eh_frame`), so select by `size` and address, not
  by name or `kind: code`. Filter with a script, not by eye:

  ```bash
  fission_cli list ./target.bin --json | python3 -c \
    "import json,sys;[print(f['address'],f['size'],f['name']) for f in json.load(sys.stdin) if f['size']>0]"
  ```

- `strings --context` returns, for each string, the functions that reference it and
  their immediate `callers` and `callees`. Start from the success/failure/prompt
  strings and follow them to code.
- An empty `references` list means no **indexed** evidence, not proof that nothing
  uses the string. Dispatch through registers and jump-only tail calls are not
  asserted as call edges.

## Navigate

```bash
fission_cli --contract v1 xrefs ./target.bin --to 0x401230 --json
fission_cli --contract v1 callgraph ./target.bin --function 0x40067a --json
fission_cli search ./target.bin --text 'license'               # addresses, feed them to hex/disasm
fission_cli search ./target.bin --bytes '48 8b ?? 24' --section .text --limit 20
fission_cli --contract v1 hex ./target.bin --addr 0x401230 --count 64 --json
fission_cli --contract v1 disasm ./target.bin --addr 0x401230 --function --json
```

- Addresses are virtual addresses in hex, the same ones every command speaks.
- `callgraph --function` needs a discovered function **entry**; anything else fails
  instead of returning an empty graph.
- `--function` on `disasm` uses the function's boundaries; without it you get a fixed
  window (`--count`).
- Use `hex` for lookup tables and encoded data (immediate bytes, XOR keys), and
  `search --value` for a constant in the binary's endianness.

## Decompile

`--addr` takes a hex address. To select by name, use `--name` (exact match; a name
that stands for several addresses fails with `function_ambiguous` and lists them,
and a missing one with `function_not_found`). Stripped binaries have no names, so
use `list` and `--addr` there.

```bash
fission_cli --contract v1 decomp ./target.bin --name main --json
fission_cli --contract v1 decomp ./target.bin --addr 0x401230 --json
fission_cli --contract v1 decomp ./target.bin --all --limit 20 --json > funcs.json
fission_cli --contract v1 decomp ./target.bin --addresses-file addrs.txt --json
```

- Decompile once for the scope you need, write the JSON to disk, then search it
  with `rg`/`jq`/Python. Prefer one batch to a loop of single calls: each call
  reloads the binary (about 0.1 s on the larger ELFs measured), which adds up over
  hundreds of functions, and a saved file can be searched again for free.
  `--addresses-file` takes one hex address per line.
- `--all` is bounded batch output; use `--limit`. For whole-binary facts use
  `fission_cli inventory function-facts` (JSONL plus a summary), which is a
  different workflow from `decomp --all`.
- Each item has `code`, `variables` (name, type, size, stack offset), `line_mappings`
  (printed line to instruction addresses, only where provable), `engine_used`,
  `fell_back` and `fallback_reason`. A failed item carries an `error` string and
  `fell_back: true`; do not read its `code` as a result.
- `--timeout-ms` sets a per-call time limit. A function that hits it is reported as
  a timeout; retry that address alone with a larger value.

### Treat the C as an estimate

Pseudocode is a reading aid, not ground truth. Before you rely on an answer-critical
branch, comparison width, signedness, loop bound or indirect call, confirm it in the
`disasm` of that function or by running it. Names the tool invents (`sub_...`,
`local_18`, `param_1`) are positions, not meaning.

## Check a finding by running it

Static reading can be wrong in a way only execution shows.

```bash
fission_cli --contract v1 sandbox ./target.bin --json --max-inst 100000 \
  --fail-on-budget --max-hle-misses 0 --max-unknown-syscalls 0 -- candidate_input
```

- The budget flags make the run fail when the guest touches libc or syscalls the
  emulator does not model: a small program that calls `fopen` and `fprintf` failed
  with `quality_budget_exceeded` under `--max-hle-misses 0`; the report is still
  printed. Drop the budget flags to see the output, and report the
  modelled gaps instead of the verdict.
- A zero process exit means the sandbox completed, not that the guest accepted the
  input. Read `halt_requested`, the guest `exit_code` and `guest_output.stdout`
  (byte arrays; `null` means unavailable, not empty).
- Some programs exit 0 on both the right and the wrong input. Compare the output
  text, and run wrong, truncated, extended and empty inputs as controls.
- A run that hit `--max-inst` without a guest exit is incomplete. Do not report it.
- `fission_cli verify --addr 0x... --tier concrete|ground-truth|symbolic|all` checks
  Fission's own IR against the machine code; use it when you suspect the
  decompiler, not the program.
- `raw-pcode`, `pcode-stages` and `nir-stats` show the lift beneath a suspect line.

The sandbox is an emulator with a modelled Linux x86-64 runtime, not a full libc or
loader. Anti-debugging, packing and opaque indirect checks may defeat it; say so
instead of guessing a result. Do not run unknown samples natively.

## Keep your decisions

`db` writes names, signatures, notes and breakpoints next to the input as
`<binary>.fission.json`, bound to the file's hash, and later analysis commands apply
them automatically. That is a **write beside the input**, so use `--db FILE` to put
it elsewhere or `--no-db` to ignore it.

```bash
fission_cli db ./target.bin name 0x401230 check_password --json
fission_cli db ./target.bin note 0x401230 'xors input with 0x24' --json
fission_cli db ./target.bin show --json
fission_cli db ./target.bin undo --json
```

Keep solver scripts, derived keys and useful addresses in your own notes as well; a
re-analysis can change names the tool generated.

## Change bytes

`patch` writes a modified **copy**; the input is never touched, and without
`--output` it only reports the change.

```bash
fission_cli patch ./target.bin --addr 0x401760 --nop 5                 # report only
fission_cli patch ./target.bin --addr 0x401760 --nop 5 --output patched.bin
```

## Name recovery and comparison

- `identify` matches functions against bundled Ghidra FID databases. It helps on
  statically linked binaries; on dynamically linked ones there is little to match.
- `similar` builds a structural index across binaries
  (`--update-index functions.json`, then `--index functions.json --function 0x...`).
  Its fingerprints are opaque hashes: a high score is a lead, not proof that two
  functions do the same thing.

## Special inputs

| Input | What to do |
|-------|------------|
| Fat Mach-O | `--arch x86_64\|arm64\|arm64e` picks the slice; `info` marks the default. |
| Headerless or firmware image | `--language processor:endian:size:variant` (for example `x86:LE:64:default`) skips auto-detection. |
| Archive (`.zip`, `.tgz`) | Extract it and analyse the executable inside. |
| Packed binary | Fission has no unpacker. Unpack with another tool first, then analyse the output. |
| Missing signatures/detectors | `--resource-root DIR` or `FISSION_RESOURCE_ROOT`. |
| Your own FID databases | `--signature-path DIR` (repeatable) or `FISSION_SIGNATURE_PATH`; searched after the bundle. A directory that does not exist is an error. `resources status` shows what was found. |

## Report honestly

- Say what was observed (address, instruction, bytes, sandbox output) and what was
  inferred from pseudocode. Summarise; do not paste large C or assembly blocks.
- If a command fails after you applied the hint, record the exact command and error,
  fall back to `disasm` or dynamic analysis, and suggest filing an issue at the end
  of the task rather than working around the tool silently.

## Reference

`docs/CLI.md` (every command), `docs/CRACKME_WORKFLOW.md` (a measured solve from
evidence to an executed verdict), `docs/schemas/` (envelope and error codes).
