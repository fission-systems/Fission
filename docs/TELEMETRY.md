# Telemetry (opt-in Sentry)

Fission can report internal panics and `tracing::error!` callsites to Sentry.
Telemetry is **off by default** and stays off unless all three hold:

1. the binary was built with the `sentry` Cargo feature (no default build,
   release archive or CI artifact enables it);
2. `FISSION_SENTRY=1` is set;
3. `SENTRY_DSN` is set to a valid DSN.

```bash
cargo build -p fission-cli --features sentry
FISSION_SENTRY=1 SENTRY_DSN=<project DSN> fission_cli ...
```

Official benchmark runs and distributed binaries leave these unset.

## Policy

Fission never transmits analyzed binaries, decompiled output, credentials or
memory dumps. Because any formatted string (a panic message, a `tracing`
message or field, an error's `Display`) can carry strings, symbol names or
bytes from the analyzed binary, or paths and secrets from your machine, **no
free text leaves the process**. Panic payloads and tracing messages/fields are
never read; there is no regex/blacklist scrub to get wrong. Events are built
from typed, compile-time facts, and a final allowlist scrub
(`crates/fission-cli/src/telemetry.rs`, `scrub`) rebuilds every event before
it is sent.

## What an event contains

| Field | Value |
|---|---|
| exception type | one of `panic`, `tracing_error`, `internal_defect` |
| exception value | the fixed template `<type> at <location>` or `<type> (message not sent)` |
| message, log entry | `[redacted]` if the SDK produced one (Fission's producers do not) |
| location | `file:line` from `PanicHookInfo::location`, `tracing::Metadata::file/line` or `Location::caller`, only in one of three public forms: Fission's source (`crates/fission-pcode/src/midend/x.rs:120`), the standard library (`library/core/src/...`) or a crates.io dependency (`deps/<crate>-<version>/...`); anything else is dropped |
| fingerprint | `[exception type, location]`, so one defect is one issue |
| tags | `location` only |
| stack frames | function name (hash stripped) for Rust symbols of a `fission_*` crate, `std`/`core`/`alloc`/`panic_*` or Rayon, `<other>` for every other frame; file and line under the same path rule; `in_app` for Fission frames. Release builds are stripped, so their events usually carry `<other>` frames and the location only |
| release | `fission-cli@<version>[+build]` from `SENTRY_RELEASE`/`FISSION_RELEASE`, else the package version |
| environment | `SENTRY_ENVIRONMENT` if it is one of `production`, `development`, `staging`, `ci`, `test`, `local`; else `production` |
| platform, SDK | from fixed lists; SDK version |
| event id, timestamp, level | as generated |

## What is never sent

- Panic messages, `tracing` messages and fields, error `Display`/`Debug` text,
  log entries.
- Analyzed binaries, their bytes, strings or symbol names; decompiled output;
  memory dumps.
- Instruction/symbol/image addresses, symbol names other than the frames
  above, frame `module`/`package`/absolute paths, source context, locals,
  registers, threads, debug images.
- Absolute paths, user names, host name, environment variables, command-line
  arguments, OS/device contexts, breadcrumbs, `extra`, tags other than
  `location`, logger, transaction, user, request, loaded-module lists.

## Volume

Each `[type, location]` group is sent at most once per process, and at most
64 groups per process. Panics caught per function (`decomp --all`) and errors
logged in loops therefore cost one event per distinct defect.

Under `--contract v1` only the re-executed child reports (it inherits the
environment); the supervising parent never initialises Sentry, so a child
panic is not reported twice.

## Limits

- The location and frame channels are trusted by provenance: they come from
  compile-time paths and from symbolizing Fission's own process, never from
  analyzed data. The scrub additionally enforces their shape (lowercase
  `.rs` paths in the three forms above, symbol roots from a fixed crate
  list), but a string that looked exactly like a Fission source path or
  symbol would pass that shape check; no code path puts data there.
- `fission_core::logging::error(...)` reports the helper's own callsite, so
  all errors logged through it share one group.
- Normal CLI errors and expected analysis failures are not reported. Call
  `telemetry::capture_defect()` (it takes no message; the caller's location
  identifies the defect) only for internal defects.

## Release tracking

Events carry `fission-cli@<version>` unless `SENTRY_RELEASE` (preferred) or
`FISSION_RELEASE` names another `fission-cli@...` release, for example
`fission-cli@<git-sha>`; any other value is ignored. Published releases are
recorded in Sentry by the `sentry-release` job (see
[CI release gates](CI_RELEASE_GATES.md#sentry-release-tracking)).

When running `sentry-cli` by hand, `SENTRY_ORG` is the **Sentry organization
slug**, `fission-systems`, and `SENTRY_PROJECT` is `fission`. The slug happens
to equal the GitHub organization name, but it is the Sentry slug that
matters. CI sets these from the trusted build environment; never expose
`SENTRY_AUTH_TOKEN` to fork PRs.

## CI defect events

CI reports its own defects to the same project, separately from this opt-in
CLI telemetry. `scripts/ci/sentry_defects.py` sends flaky tests, aborted
tests, native differential divergences, fixture replay mismatches and fuzz
crashes from trusted `main` runs, and never from pull requests or forks. Every
field is a closed-set value or a pattern-checked identifier, so no analyzed
program's text can be sent. The defect triage reads these events to file
GitHub issues. Sources, fields and limits:
[CI release gates](CI_RELEASE_GATES.md#ci-defect-events).
