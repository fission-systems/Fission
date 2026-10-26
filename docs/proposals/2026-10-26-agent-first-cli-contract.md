# Agent-first CLI contract, phase P0 (2026-10-26)

Status: proposal. No code changes in this document.

## Why

[ADR 0001](../adr/0001-cli-first-external-surface.md) makes `fission_cli` the
supported external surface. The next consumer of that surface is an AI agent
driving Fission unattended: it cannot read a human error message, it cannot ask
a follow-up question, and it must decide from the process result alone whether
to retry, skip, or stop. Today the CLI is headless and has `--json`, but a few
gaps stop an agent from trusting it. This proposal defines the smallest
contract that closes them (P0) and leaves discovery, streaming, persistence and
protocol work (P1 and later) for separate proposals.

## Observed baseline

Observed on a Windows release build of the CLI at `main` (2026-10-26), using a
system PE as input. These are observations of the current behavior, not claims
about any decompiler-quality metric.

| Case | Observed | Why it hurts an agent |
|------|----------|-----------------------|
| `list C:\nope.exe --json` | Human-readable miette text on stderr, nothing on stdout, exit 1 | A `--json` caller gets no JSON on failure; the cause cannot be classified. |
| `decomp <pe> --addr 0x1 --json` (unreadable address) | JSON array with one row whose `error` is set and `fell_back` is `true`, exit 0 | Failure is visible only inside the payload; the exit code reports success. |
| Any fatal error | Always exit 1 (`fission_cli.rs` prints `{:?}` and exits 1) | Usage, input, timeout and internal errors are indistinguishable. |
| `schema_version` | Present in `debug_decomp`, `nir_stats` and `debug_session` JSON; absent from `list`, `decomp`, `xrefs`, `callgraph` and others | The contract is not uniform, and there is no machine-readable description of any shape. |
| Top-level JSON shape | Bare arrays for `list` and `decomp`; objects elsewhere | An envelope cannot be added to arrays without changing them. |

Existing contract language: [`VERSIONING.md`](../VERSIONING.md) already allows
additive JSON fields in a minor release and treats breaking a documented CLI
contract as a major change. This proposal is designed to be additive.

## Design rules

1. **Opt-in, additive.** Legacy output and exit codes stay exactly as they are
   by default. The new behavior is selected by a contract version, so existing
   scripts and benchmark harnesses that read the current shape are unaffected.
2. **stdout is the payload, stderr is the log.** In contract mode stdout carries
   exactly one JSON document and nothing else; diagnostics, progress and
   `[fission] Loaded config...` stay on stderr.
3. **The process result is sufficient.** An agent can branch on the exit code
   alone, and read the envelope only for detail.
4. **Stable identifiers, free-form text.** Branching uses `error.code` and the
   exit code. `message` and `hint` are for humans and may change without notice.
5. **No silent success.** A run that produced a failed function row is not a
   success in contract mode.

## Proposal

### 1. Selecting the contract

A global option `--contract <VERSION>` (env `FISSION_CONTRACT`), initially only
`v1`. When absent, behavior is unchanged. `--json` keeps its current meaning
when used alone. `--contract v1` implies JSON output and conflicts with
human-oriented flags (for example `-o` text output) with a usage error.

Name and flag shape are open (see Open questions).

### 2. Envelope

Every contract-mode invocation, success or failure, writes one JSON object to
stdout:

```json
{
  "contract": "fission-cli/1",
  "command": "decomp",
  "ok": true,
  "schema_version": 1,
  "data": { },
  "warnings": [ { "code": "worker_restarted", "message": "..." } ],
  "tool": { "name": "fission_cli", "version": "0.2.3", "commit": "abc1234" },
  "elapsed_ms": 1840
}
```

- `contract` is the envelope version. It changes only when the envelope itself
  changes incompatibly.
- `schema_version` is the version of the `data` payload for this `command`. It
  is bumped when that payload changes incompatibly, independently of other
  commands. It unifies the existing per-command `schema_version` fields.
- `data` holds today's payload unchanged (including arrays), so existing
  per-command shapes are reused rather than redesigned.
- `tool` records the build that produced the result, so a stored result is
  traceable to a version (this also serves the provenance gap in #660).
- `warnings` carries non-fatal conditions an agent reading only stdout would
  otherwise miss (for example an input that was repaired or partially parsed).
  Each warning is `{code, message}`.
- Batch commands add `summary: {total, succeeded, failed}` next to `data`.
- Unknown keys must be tolerated by clients, matching `VERSIONING.md`.

Failure uses the same envelope with `ok: false` and no `data`:

```json
{
  "contract": "fission-cli/1",
  "command": "list",
  "ok": false,
  "error": {
    "code": "input_not_found",
    "kind": "input",
    "message": "binary path does not exist: C:\\nope.exe",
    "hint": "Check the path; relative paths resolve against the current directory.",
    "retryable": false,
    "context": { "path": "C:\\nope.exe" }
  },
  "tool": { "name": "fission_cli", "version": "0.2.3", "commit": "abc1234" },
  "elapsed_ms": 4
}
```

`error.kind` is a small closed set that maps to exit codes; `error.code` is a
finer, documented, append-only registry (for example `input_not_found`,
`input_unsupported_format`, `resource_root_missing`, `address_unreadable`,
`function_timeout`, `internal_panic`). `retryable` tells an agent whether the
same call could succeed later (for example a timeout under load) as opposed to
being permanently wrong.

Per-function failures inside a batch (`decomp --all`, `--addresses-file`) stay
rows in `data`, but each failed row carries the same `{code, kind, message,
retryable}` object instead of a free-form `error` string, and the envelope
exit code reflects them (next section). The ambiguous `fell_back` and `layer`
fields keep their current meaning in `data`; a separate cleanup can rename
them.

### 3. Exit codes (contract mode)

| Code | Kind | Meaning |
|------|------|---------|
| 0 | `ok` | Completed; every requested item succeeded. |
| 1 | `failed` | The invocation ran but produced no usable result (for example every requested function failed). |
| 2 | `usage` | Invalid arguments or option combination. Matches clap's existing usage-error code. |
| 3 | `input` | Input missing, unreadable or unsupported. |
| 4 | `partial` | Some items succeeded and some failed, and the caller asked for strictness with `--fail-on-partial`; `data` has both. |
| 5 | `timeout` | A configured time limit was reached and no result was produced. |
| 6 | `environment` | Required resources unavailable (resource root, signatures, runtime). |
| 70 | `internal` | Unexpected internal failure or panic caught at the boundary. |

Batch rule: a batch that produces at least one successful item exits 0 and
reports the failed items in `data` and in `summary` (`total`, `succeeded`,
`failed`); a batch that produces none exits 1. Exit 4 is returned only with
`--fail-on-partial`. The default favors keeping useful output: a harness that
discards stdout on any non-zero exit would lose the successful rows. This
matches how other agent-oriented decompiler CLIs treat per-function failures
(see "Prior art reviewed").

Legacy mode keeps today's behavior (exit 1 for fatal errors, exit 0 when only
function rows failed). The two worker binaries that already use exit code 2 are
internal and are not part of the contract.

### 4. `schema` subcommand

`fission_cli schema --list` prints the available command schemas and the error
code registry. `fission_cli schema <command>` prints the JSON Schema (draft
2020-12) for that command's envelope with its `data` payload. `--error-codes`
prints the registry. This lets an agent self-check output and discover codes
without reading docs.

Schemas are the source of truth in code and are also committed under
`docs/schemas/`. A test fails if the generated schema differs from the committed
one, so a payload change cannot land without an explicit schema (and, if
incompatible, a `schema_version`) change.

Generating schemas from Rust types would normally use `schemars`. The workspace
does not depend on it today; adding it is a dependency decision to be made in
the implementation PR. The alternative is hand-written schemas guarded by
golden-file tests.

### 5. Where it lives

Entirely in `fission-cli` (the ADR 0001 surface): a small contract module for
the envelope, error classification and exit-code mapping, plus a typed CLI
error that `fission_cli.rs::main` maps instead of printing `{:?}` and exiting
1. No decompiler, pcode or normalize crate changes, and no new quality pass or
metric. Errors raised deeper in the stack are classified at the CLI boundary by
downcasting to known error types; unclassified errors map to `failed` with
code `unclassified` until each source is typed.

## Prior art reviewed

Reference clones live under `vendor/` (gitignored, reference-only, nothing
copied). Ideas taken from reading `Noelo-Lab/kuna` (Apache-2.0), an
agent-first decompiler, and a few CLIs:

- **Self-describing commands.** Each subcommand's `--help` states its flags and
  the shape of its `--json` document and exits before loading anything. Adopted:
  `schema` plus `--help` text that names the contract shape (P0).
- **Broken pipe is not an error.** A reader closing stdout early must not
  panic or print a diagnostic, and the exit code must still reflect the work's
  verdict. Adopted as a design rule (P0).
- **Warnings reach stdout.** Repaired or degraded input is listed in the JSON,
  not only on stderr. Adopted: `warnings` in the envelope.
- **Exit code follows "did any useful output land".** Adopted as the batch rule
  above, with `--fail-on-partial` for strict callers.
- **Docs and the agent skill ship inside the binary** (`docs` topics,
  `install-skill`). Deferred to P1.
- **Option discovery as a structured catalog.** Deferred to P1 (`capabilities`).
- **Incremental output while a long batch runs** (an index file that can be read
  as items complete). Deferred to P1 as NDJSON streaming.
- Kuna deliberately keeps no state between invocations; Fission's `db` does.
  That is a difference to keep in mind for P2, not something to copy.

## Implementation approach (supervisor)

Fission has several hundred direct `println!` sites across its commands, so
wrapping every payload in-process would be a large refactor. P0 instead adds a
thin supervisor in `fission-cli`: with `--contract v1` the binary re-executes
itself (same executable, contract flag removed, an internal environment marker
set), captures stdout and stderr, and writes the envelope. In the child, fatal
errors are classified into `{kind, code}` and reported as one structured stderr
record; the supervisor maps exit status and that record to the envelope and
exit code. A crashed or panicking child becomes `internal` instead of a bare
backtrace, and a child whose stdout is not valid JSON becomes
`output_not_json`. This is a staging step: commands can later return typed
payloads in-process without changing the contract.

Costs: one extra process start and buffering the payload in memory, both small
next to analysis. Large `decomp --all` output should use P1 streaming instead.

## Compatibility and migration

1. Release N: add `--contract v1` and `schema`; default behavior unchanged;
   document in `docs/CLI.md` and `docs/EVALUATION.md`.
2. Release N+1 or later: announce deprecation of the legacy shape for commands
   that have a contract schema; decide the default flip as a major-version item
   under `VERSIONING.md`.
3. The DecBench harness and any script that reads the legacy shape need no
   change in step 1. Before step 2, check which harness code relies on
   exit 0 with failed rows.

## Validation plan

- Contract tests per error kind: missing file, unsupported format, unreadable
  address, bad arguments, timeout, missing resource root. Assert exit code,
  `ok`, `error.code`, and that stdout is a single valid JSON document.
- Batch test: `decomp --all` on a fixture with at least one failing and one
  passing function exits 4 and lists both.
- Legacy test: the same invocations without `--contract` keep today's output
  and exit codes (guards the opt-in promise).
- Schema test: generated schemas equal the committed files; sample outputs
  validate against them.
- Stream test: in contract mode stdout parses as exactly one JSON value even
  when stderr logging is verbose.
- Panic test: an induced worker panic maps to `internal` with a JSON envelope,
  not a bare stderr backtrace.

Timeout behavior needs verification before implementation: `--timeout-ms`
currently surfaces a `preview_timeout` message string, and how that interacts
with worker restarts (#630 fixups) must be confirmed so `timeout` and
`retryable` are accurate.

## Non-goals

- No change to decompiler output, quality, or any benchmark metric.
- No streaming, `capabilities` command, persistence or protocol-crate wiring;
  those are P1 and later.
- No removal or rename of existing JSON fields or flags in this phase.
- Fixing the working-directory-dependent resource lookup (the output differed
  between running from the repository root and from `/tmp`) is separate work.
  It is relevant here because an agent needs deterministic output, but it is
  tracked on its own.

## Open questions

1. Flag name and shape: `--contract v1`, `--output-contract v1`, or a
   `--format json-v1` value? Should the env var alone be enough for agents?
2. Should contract-mode errors for non-`--json` invocations of human commands
   (for example `hex`) be supported in P0, or only commands that already have
   `--json`?
3. Exact exit-code numbering. 70 follows `sysexits` (`EX_SOFTWARE`); is the
   small-integer range for the rest acceptable?
4. `schemars` versus hand-written schemas with golden tests.
5. Resolved by the batch rule: `partial` is opt-in via `--fail-on-partial`.
