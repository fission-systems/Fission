# Telemetry (opt-in Sentry)

Fission can report internal panics and `tracing::error!` events to Sentry.
Telemetry is **off by default**.

## Enable

Build with the `sentry` Cargo feature and set both environment variables:

```bash
cargo build -p fission-cli --features sentry
FISSION_SENTRY=1 SENTRY_DSN=<project DSN> fission-cli ...
```

Nothing is sent unless the feature is compiled in *and* both variables are set.
Official benchmark runs and distributed binaries should leave them unset.

## What is sent

- Panic and `tracing::error!` events.
- Redacted message/stack frames (file basenames only, no source context, no locals).
- Release/environment metadata.

## What is never sent

- Binaries, decompiled output, memory dumps, or analyzed input data.
- Environment variables, request bodies, user identity, breadcrumbs, extra/tags.
- Full file paths; paths are reduced to basenames.
- Obvious secrets/tokens and long hex blobs are redacted from messages.

## Release tracking

Set `SENTRY_RELEASE` (preferred) or `FISSION_RELEASE` to associate events with a
commit SHA, for example `fission-cli@<git-sha>`. CI should set this from the
trusted build environment; do not expose secrets to fork PRs.

## Expected failures

Normal CLI errors and expected analysis failures are not reported automatically.
Use `telemetry::capture_error` only for internal defects or infrastructure failures.
