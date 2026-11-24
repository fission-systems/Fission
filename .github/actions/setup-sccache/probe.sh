#!/usr/bin/env bash
# Decide, before anything is built, whether this job compiles through sccache.
#
# A cache is an optimisation (ADR 0016, rule 1: caches fail open). sccache
# checks its storage when its server starts, and a failed check is fatal to the
# compile that started it: `sccache: error: Server startup failed: cache
# storage failed to read`. On 2026-11-23 and 11-24 that happened three times
# at the build's first `rustc -vV`, because the old probe (`--show-stats`) ran
# before SCCACHE_GHA_ENABLED was set, so it started a local-disk server that
# never touched the cache service, and the real server only started inside
# the build.
#
# So the probe runs the server the build will use: the same environment, the
# storage check, and one real compile through it. Any failure here means the
# job builds without RUSTC_WRAPPER. Nothing here ever sees the project's code,
# so a compiler error in the project can never turn into a fallback.
#
# Inputs (environment):
#   INSTALL_OUTCOME   outcome of the sccache install step
#   SIMULATE_OUTAGE   "true" points the cache service at a host that cannot
#                     resolve, to prove the fallback (ci.yml sccache-fallback)
# Outputs: `enabled=true|false` in $GITHUB_OUTPUT; the build environment in
# $GITHUB_ENV.
set -uo pipefail

# Signed cache URLs carry a SAS token in their query string; keep it out of the log.
redact() { sed -E 's/\?[^[:space:])"]*/?<redacted>/g'; }

# Settings the build gets whether or not the cache is used, so the cache
# never changes what is compiled.
echo "CARGO_INCREMENTAL=0" >> "$GITHUB_ENV"

disable() {
  local reason="$1" detail="${2:-}"
  if [ -n "$detail" ]; then
    echo "::group::sccache probe output"
    printf '%s\n' "$detail" | redact
    echo "::endgroup::"
  fi
  sccache --stop-server >/dev/null 2>&1 || true
  echo "::warning title=sccache disabled::${reason}; building without a compilation cache (slower, not red: ADR 0016)"
  echo "enabled=false" >> "$GITHUB_OUTPUT"
  exit 0
}

[ "${INSTALL_OUTCOME:-}" = "success" ] || disable "sccache did not install (outcome: ${INSTALL_OUTCOME:-unknown})"
command -v sccache >/dev/null 2>&1 || disable "sccache is not on PATH"

# The server reads its configuration once, at startup, so this is the build's
# environment, set before the server starts.
#   SCCACHE_IDLE_TIMEOUT=0: the server started here lives for the whole job.
#     The default (600 s) let it exit during a long test run, and the next
#     cargo step would start a new one and repeat the storage check.
#   SCCACHE_IGNORE_SERVER_IO_ERROR=1: if the server goes away mid-compile, the
#     client compiles that unit locally instead of failing it. This covers only
#     the client/server connection; a compiler error is returned as it is.
export SCCACHE_GHA_ENABLED=true
export SCCACHE_IDLE_TIMEOUT=0
export SCCACHE_IGNORE_SERVER_IO_ERROR=1
export SCCACHE_ERROR_LOG="${RUNNER_TEMP}/sccache.log"

if [ "${SIMULATE_OUTAGE:-false}" = "true" ]; then
  echo "Simulating a cache-service outage: the cache endpoints point at sccache-outage.invalid"
  export ACTIONS_RESULTS_URL="https://sccache-outage.invalid/"
  export ACTIONS_CACHE_URL="https://sccache-outage.invalid/"
fi

# A server already running was started with some other environment.
sccache --stop-server >/dev/null 2>&1 || true

# 1. Start the server. This performs the storage read/write check.
if ! out="$(sccache --start-server 2>&1)"; then
  first_error="$(printf '%s\n' "$out" | grep -m1 'error:' | sed 's/^sccache: error: //' | redact)"
  disable "the sccache server did not start: ${first_error:-no error message}" "$out"
fi

# 2. Compile one crate through it: a cache lookup (storage read), a compile,
#    and a cache write. The source is a fixed, valid crate, so a failure here
#    is sccache's, never the project's.
tmp_root="$RUNNER_TEMP"
command -v cygpath >/dev/null 2>&1 && tmp_root="$(cygpath -u "$RUNNER_TEMP")"  # Windows runners
probe_dir="$(mktemp -d "${tmp_root}/sccache-probe.XXXXXX")"
printf 'pub fn sccache_probe() -> u32 { 42 }\n' > "${probe_dir}/lib.rs"
rustc_bin="$(rustup which rustc 2>/dev/null || command -v rustc || true)"
[ -n "$rustc_bin" ] || disable "no rustc to probe sccache with"
mkdir -p "${probe_dir}/out"
# Arguments in cargo's form: sccache only caches what it can parse, and an
# uncacheable call would never touch the storage.
if ! out="$(sccache "$rustc_bin" --crate-name sccache_probe --edition=2021 "${probe_dir}/lib.rs" \
      --crate-type lib --emit=dep-info,metadata,link -C debuginfo=0 \
      --out-dir "${probe_dir}/out" 2>&1)"; then
  disable "a probe compile through sccache failed" "$out"
fi

stats="$(sccache --show-stats 2>&1)" || disable "sccache stopped answering after the probe compile" "$stats"
printf '%s\n' "$stats"
location="$(printf '%s\n' "$stats" | sed -n 's/^Cache location[[:space:]]*//p')"
read_errors="$(printf '%s\n' "$stats" | sed -n 's/^Cache read errors[[:space:]]*//p')"
case "$location" in
  ghac*) ;;
  *) disable "sccache is not using the GitHub Actions cache (location: ${location:-unknown})" ;;
esac
hits="$(printf '%s\n' "$stats" | sed -n 's/^Cache hits[[:space:]]*\([0-9]*\)$/\1/p')"
misses="$(printf '%s\n' "$stats" | sed -n 's/^Cache misses[[:space:]]*\([0-9]*\)$/\1/p')"
[ "$(( ${hits:-0} + ${misses:-0} ))" -ge 1 ] || disable "the probe compile never reached the cache (no hit or miss recorded)" "$stats"
# A read error is counted as a miss, so the build would still pass, but every
# lookup would pay for the failing request. Without the cache it is faster.
[ "${read_errors:-0}" = "0" ] || disable "the cache service failed ${read_errors} read(s) during the probe"

{
  echo "SCCACHE_GHA_ENABLED=true"
  echo "SCCACHE_IDLE_TIMEOUT=0"
  echo "SCCACHE_IGNORE_SERVER_IO_ERROR=1"
  echo "SCCACHE_ERROR_LOG=${SCCACHE_ERROR_LOG}"
  echo "RUSTC_WRAPPER=sccache"
} >> "$GITHUB_ENV"
echo "enabled=true" >> "$GITHUB_OUTPUT"
echo "sccache is up and serving from ${location}"
