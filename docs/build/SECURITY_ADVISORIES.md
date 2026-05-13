# Security Advisory Remediation Notes

This page tracks dependency advisories that are visible to GitHub Dependabot but are currently blocked by transitive dependency constraints.

> [!IMPORTANT]
> `cargo-deny` / `cargo-audit` ignores do not close GitHub Dependabot alerts. GitHub alerts close only when the vulnerable package version no longer appears in the scanned manifest/lockfile, or when the alert is dismissed in GitHub Security UI.

## Active Dependabot blockers

| Advisory | Package | Vulnerable range | Patched version | Current source |
|---|---|---:|---:|---|
| Unsoundness in `Iterator` / `DoubleEndedIterator` for `glib::VariantStrIter` | `glib` | `>=0.15.0, <0.20.0` | `0.20.0` | GTK/WebKit/Tauri Linux stack (`atk`, `cairo-rs`, related `gtk-rs` 0.18 crates) |
| Rand unsound with custom logger using `rand::rng()` / `thread_rng()` | `rand` | `>=0.7.0, <0.8.6` | `0.8.6` | transitive dependency chain plus dev/test dependency graph |

## Intended remediation

Run these from a normal networked checkout with crates.io access:

```bash
# Prefer the exact patched rand patch release.
cargo update -p rand --precise 0.8.6

# The glib advisory requires escaping the gtk-rs 0.18 family. This usually
# requires refreshing the Tauri/Wry/GTK stack together rather than updating
# `glib` alone, because gtk-rs crates pin compatible major versions.
cargo update -p tauri -p tauri-build -p tauri-plugin-dialog -p tauri-plugin-opener
cargo update -p glib --precise 0.20.0
```

Then validate:

```bash
cargo tree -i glib
cargo tree -i rand
cargo audit
cargo deny check advisories
cargo test --all
```

Expected success criteria:

```text
Cargo.lock has no glib version < 0.20.0
Cargo.lock has no rand version >= 0.7.0 and < 0.8.6
GitHub Dependabot alerts #7 and #15 close after GitHub rescans the lockfile
```

## If `cargo update` cannot solve `glib`

`glib` is part of a semver-coordinated gtk-rs family. If Cargo reports that `glib = 0.20` is incompatible, update the owning GUI stack instead of hand-editing `Cargo.lock`:

1. Refresh the Tauri Rust crates and plugins together.
2. Check whether `wry`, `webkit2gtk`, `gtk`, `gdk`, `gio`, `glib`, `cairo-rs`, and `atk` move as a coherent family.
3. Keep CLI/core crates buildable without requiring Linux GUI system packages.
4. Re-run `cargo tree -i glib` to verify the remaining owner path.

Do **not** manually edit checksums or package versions in `Cargo.lock`; that can produce a lockfile Cargo rejects or, worse, a misleading security PR.

## Current baseline

The advisories are temporarily ignored in both:

- [`deny.toml`](../../deny.toml)
- [`.cargo/audit.toml`](../../.cargo/audit.toml)

Those ignores are a local CI baseline only. They should be removed in the same PR that removes the vulnerable package versions from `Cargo.lock`.
