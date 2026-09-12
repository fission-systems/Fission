#!/usr/bin/env bash
# Which of this workspace's build configurations have ever been compiled?
#
# Five times in one week a defect was found in code that nothing built: the
# debug layer behind `interactive_runtime`, the CLI's `debugger` dispatcher, a
# dead `decomp_debug` feature, a Linux-only `use` that kept main red for ten
# commits, and 269 errors in the Win32 debugger backend. Each was invisible for
# the same reason -- the configuration holding it was not in any build -- and
# each was found by accident rather than by looking.
#
# This looks. It compiles every configuration the workspace can be built in and
# reports which ones hold. Three platforms are reachable from one machine:
#
#   macOS    native
#   Linux    a container (`--platform linux/amd64` to match CI)
#   Windows  `x86_64-pc-windows-gnu`, which mingw makes a cross target;
#            `check` does not link, so the gnu ABI is close enough to find
#            every source-level break the msvc build would hit
#
# Usage:
#   scripts/audit/compile_surface.sh              # host only
#   scripts/audit/compile_surface.sh --all        # host + windows cross + linux container
#
# Prerequisites for --all:
#   rustup target add x86_64-pc-windows-gnu
#   brew install mingw-w64      (or any x86_64-w64-mingw32-gcc)
#   docker, for the Linux container

set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

ALL=0
[[ "${1:-}" == "--all" ]] && ALL=1

pass=0
fail=0
declare -a FAILURES=()

# Run one configuration and record the outcome. A configuration that is
# *expected* to fail (the unported Win32 backend) is marked, so the report
# distinguishes "known broken" from "newly broken" -- the second is the one
# worth waking up for.
check() {
  local label="$1" expect="$2"
  shift 2
  printf '%-58s ' "${label}"
  if "$@" >/tmp/compile_surface.log 2>&1; then
    if [[ "${expect}" == "broken" ]]; then
      echo "UNEXPECTEDLY OK (the port may be finished)"
    else
      echo "ok"
    fi
    pass=$((pass + 1))
  else
    if [[ "${expect}" == "broken" ]]; then
      echo "broken (known)"
      pass=$((pass + 1))
    else
      echo "FAILED"
      fail=$((fail + 1))
      FAILURES+=("${label}")
      grep -E '^error' /tmp/compile_surface.log | head -3 | sed 's/^/    /'
    fi
  fi
}

echo "── host ($(uname -s)) ────────────────────────────────────────────"
check "workspace, default features" ok \
  cargo check --workspace --all-targets --locked
# A *debug* run, because that is where arithmetic overflow is checked. A
# release-only habit hid a shift past sixty-three in the p-code evaluator: it
# panics in debug and silently wraps in release, so the release runs said
# nothing and the observer reported a wrong number.
check "workspace tests, debug profile" ok \
  cargo test --workspace --no-run --locked
check "fission-dynamic +interactive_runtime" ok \
  cargo check -p fission-dynamic --features interactive_runtime --all-targets --locked
check "fission-script +emulator" ok \
  cargo check -p fission-script --features emulator --all-targets --locked
check "fission-cli, no default features" ok \
  cargo check -p fission-cli --no-default-features --locked
check "fission-emulator +softfloat" ok \
  cargo check -p fission-emulator --features softfloat --all-targets --locked
check "fission-plugin +interactive_runtime" ok \
  cargo check -p fission-plugin --features interactive_runtime --all-targets --locked
check "fission-cli +allocator-jemallocator" ok \
  cargo check -p fission-cli --no-default-features --features allocator-jemallocator --locked
check "fission-automation +allocator-mimalloc" ok \
  cargo check -p fission-automation --features allocator-mimalloc --all-targets --locked

# On a Windows host the unported Win32 backend can be checked natively. It is
# expected to fail, so it does not gate -- but it is listed, so the day someone
# finishes the port this script says "UNEXPECTEDLY OK" rather than staying
# silent about a feature nobody builds.
if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "${OS:-}" == "Windows_NT" ]]; then
  check "fission-dynamic +windows_native_debugger" broken \
    cargo check -p fission-dynamic \
      --features interactive_runtime,windows_native_debugger --all-targets --locked
fi

if [[ "${ALL}" -eq 1 ]]; then
  echo
  echo "── windows (x86_64-pc-windows-gnu) ───────────────────────────────"
  if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
    export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar
    check "fission-cli" ok \
      cargo check -p fission-cli --target x86_64-pc-windows-gnu --locked
    check "fission-dynamic +interactive_runtime" ok \
      cargo check -p fission-dynamic --features interactive_runtime \
        --target x86_64-pc-windows-gnu --locked
    check "  ...+windows_native_debugger" broken \
      cargo check -p fission-dynamic \
        --features interactive_runtime,windows_native_debugger \
        --target x86_64-pc-windows-gnu --locked
  else
    echo "  skipped: no x86_64-w64-mingw32-gcc (brew install mingw-w64)"
  fi

  echo
  echo "── linux (container) ─────────────────────────────────────────────"
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    mkdir -p /tmp/fission-linux-target
    docker_check() {
      docker run --rm --platform linux/amd64 \
        -v "$PWD":/w -v /tmp/fission-linux-target:/target \
        -w /w -e CARGO_TARGET_DIR=/target rust:1-slim bash -c "
          apt-get update -qq >/dev/null 2>&1
          apt-get install -y -qq build-essential pkg-config >/dev/null 2>&1
          $1"
    }
    check "fission-cli, default features" ok \
      docker_check "cargo check -p fission-cli --all-targets --locked"
    check "fission-dynamic +interactive_runtime" ok \
      docker_check "cargo check -p fission-dynamic --features interactive_runtime --all-targets --locked"
  else
    echo "  skipped: no running docker"
  fi
fi

echo
echo "═════════════════════════════════════════════════════════════════"
echo "${pass} configuration(s) as expected, ${fail} newly broken"
for f in "${FAILURES[@]:-}"; do
  [[ -n "${f}" ]] && echo "  FAILED: ${f}"
done
exit "$([[ "${fail}" -eq 0 ]] && echo 0 || echo 1)"
