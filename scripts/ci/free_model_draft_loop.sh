#!/usr/bin/env bash
# The model loop of free-model-draft.yml, with the build-error repair loop.
#
# For each free model in turn, from a clean checkout:
#   1. `opencode run` drafts the fix; free_model_draft.py check accepts or refuses the edits.
#   2. If the accepted edits touch Rust under crates/, the WORKFLOW (never the
#      model, which has no shell) runs `cargo fmt -p <pkg>` and
#      `cargo check --locked -p <pkg> --all-targets` for each touched package.
#   3. On a compiler error the model gets the diagnostics in a new
#      `opencode run` and must fix them, at most MAX_REPAIRS rounds, each
#      followed by `check` and cargo again. Still failing: the next model.
#
# The Rust toolchain is installed by workflow steps only when needed. So the
# script runs in up to two stages: when a model's edits touch Rust and
# RUST_READY is not 1, it saves its state, sets the step output
# `need_rust=true` and exits 0. The workflow then installs Rust and runs the
# script again with RUST_READY=1, which resumes at that model. A docs-only
# draft never pauses and never installs Rust.
#
# Env: DRAFT (dir holding prompt.txt, permissions.json), ISSUE_JSON, SEED,
# RUST_READY. On success it writes model, cargo (passed or skipped) and
# repairs to GITHUB_OUTPUT, and the staged edits to $DRAFT/edits.patch.
#
# Plain bash 3.2 (no mapfile, no associative arrays): it is tested on macOS.
set -euo pipefail

DRAFT="${DRAFT:?DRAFT is required}"
ISSUE_JSON="${ISSUE_JSON:?ISSUE_JSON is required}"
SEED="${SEED:-0}"
RUST_READY="${RUST_READY:-0}"
MODEL_TIMEOUT="${MODEL_TIMEOUT:-12m}"
REPAIR_TIMEOUT="${REPAIR_TIMEOUT:-8m}"
CARGO_TIMEOUT="${CARGO_TIMEOUT:-25m}"
MAX_REPAIRS="${MAX_REPAIRS:-2}"
# Stop starting new work after this many seconds in all (the job times out later).
BUDGET_SECS="${BUDGET_SECS:-6600}"
OUT="${GITHUB_OUTPUT:-/dev/null}"
SUMMARY="${GITHUB_STEP_SUMMARY:-/dev/null}"
HELPER="$(cd "$(dirname "$0")" && pwd)/free_model_draft.py"
STATE="$DRAFT/state"
export CARGO_TERM_COLOR=never

mkdir -p "$STATE"
OPENCODE_CONFIG_CONTENT="$(cat "$DRAFT/permissions.json")"
export OPENCODE_CONFIG_CONTENT

helper() { python3 "$HELPER" "$@"; }

out_of_time() {
  [ $(($(date +%s) - $(cat "$STATE/start"))) -gt "$BUDGET_SECS" ]
}

PKGS=()
load_packages() {
  PKGS=()
  local pkg
  while IFS= read -r pkg; do
    [ -n "$pkg" ] && PKGS+=("$pkg")
  done < <(helper rust-packages)
}

# cargo fmt and cargo check on every touched package; the output goes to
# $DRAFT/cargo.log. Fails if any package fails to check.
cargo_check() {
  local pkg status=0
  : > "$DRAFT/cargo.log"
  for pkg in ${PKGS[@]+"${PKGS[@]}"}; do
    timeout "$CARGO_TIMEOUT" cargo fmt -p "$pkg" >> "$DRAFT/cargo.log" 2>&1 || true
  done
  git add -A
  for pkg in ${PKGS[@]+"${PKGS[@]}"}; do
    echo "+ cargo check --locked -p $pkg --all-targets"
    timeout "$CARGO_TIMEOUT" cargo check --locked -p "$pkg" --all-targets >> "$DRAFT/cargo.log" 2>&1 \
      || status=1
  done
  return "$status"
}

CARGO="skipped"
REPAIRS=0

# 0: accepted, 1: this model failed, 10: Rust is needed but not installed yet.
verify() {
  local model="$1" round=0
  while :; do
    load_packages
    if [ ${#PKGS[@]} -eq 0 ]; then
      CARGO="skipped"; REPAIRS="$round"
      return 0
    fi
    if [ "$RUST_READY" != "1" ]; then
      return 10
    fi
    echo "Rust touched in: ${PKGS[*]} (repair round $round of $MAX_REPAIRS)"
    if cargo_check; then
      helper check --issue-json "$ISSUE_JSON" || return 1
      CARGO="passed"; REPAIRS="$round"
      return 0
    fi
    echo "cargo check failed"
    if [ "$round" -ge "$MAX_REPAIRS" ]; then
      echo "$model still fails after $round repair rounds"
      return 1
    fi
    if out_of_time; then
      echo "out of time, no repair"
      return 1
    fi
    round=$((round + 1))
    helper errors --file "$DRAFT/cargo.log" > "$DRAFT/errors.txt"
    if [ -z "$(tr -d '[:space:]' < "$DRAFT/errors.txt")" ]; then
      # Not a diagnostic (a tool or network failure): give the log's tail.
      tail -n 40 "$DRAFT/cargo.log" > "$DRAFT/errors.raw"
      helper errors --file "$DRAFT/errors.raw" > "$DRAFT/errors.txt" || true
    fi
    cat "$DRAFT/errors.txt"
    git diff --cached > "$DRAFT/current.diff"
    helper repair-prompt --issue-json "$ISSUE_JSON" --diff-file "$DRAFT/current.diff" \
      --errors-file "$DRAFT/errors.txt" > "$DRAFT/repair.txt"
    echo "repair round $round with $model"
    timeout "$REPAIR_TIMEOUT" opencode run --model "$model" "$(cat "$DRAFT/repair.txt")" \
      || echo "$model exited with status $? in repair round $round"
    helper check --issue-json "$ISSUE_JSON" || return 1
  done
}

finish() {
  local model="$1"
  git add -A
  git diff --cached > "$DRAFT/edits.patch"
  {
    echo "model=$model"
    echo "cargo=$CARGO"
    echo "repairs=$REPAIRS"
    echo "need_rust=false"
  } >> "$OUT"
  {
    echo "### Free-model draft by \`$model\`"
    echo
    echo "cargo check: $CARGO, repair rounds: $REPAIRS"
    echo
    echo '```diff'
    cat "$DRAFT/edits.patch"
    echo '```'
  } >> "$SUMMARY"
}

if [ ! -f "$STATE/models.list" ]; then
  opencode models opencode > "$DRAFT/models.txt"
  echo "Free models listed:"; cat "$DRAFT/models.txt"
  # Free models have usage limits: the run number picks which one goes first,
  # and a model that fails, is rate-limited or leaves no acceptable edits
  # hands over to the next.
  helper order-models --models-file "$DRAFT/models.txt" --seed "$SEED" > "$STATE/models.list"
  date +%s > "$STATE/start"
fi

MODELS=()
while IFS= read -r line; do
  [ -n "$line" ] && MODELS+=("$line")
done < "$STATE/models.list"

PENDING=""
if [ -f "$STATE/pending" ]; then
  PENDING="$(cat "$STATE/pending")"
  rm -f "$STATE/pending"
fi

idx=-1
for model in "${MODELS[@]}"; do
  idx=$((idx + 1))
  if [ -n "$PENDING" ] && [ "$idx" -lt "$PENDING" ]; then
    continue
  fi
  echo "::group::$model"
  if [ -n "$PENDING" ] && [ "$idx" -eq "$PENDING" ]; then
    echo "resuming $model after the Rust setup"
    PENDING=""
  else
    if out_of_time; then
      echo "out of time, no more models"
      echo "::endgroup::"
      break
    fi
    git reset -q --hard HEAD
    git clean -fdq
    timeout "$MODEL_TIMEOUT" opencode run --model "$model" "$(cat "$DRAFT/prompt.txt")" \
      || echo "$model exited with status $?"
    if ! helper check --issue-json "$ISSUE_JSON"; then
      echo "::endgroup::"
      continue
    fi
  fi
  rc=0
  verify "$model" || rc=$?
  echo "::endgroup::"
  if [ "$rc" -eq 10 ]; then
    echo "$idx" > "$STATE/pending"
    echo "need_rust=true" >> "$OUT"
    echo "$model touched Rust: installing the toolchain before checking it"
    exit 0
  fi
  if [ "$rc" -eq 0 ]; then
    finish "$model"
    exit 0
  fi
done

echo "::error::no free model left an acceptable draft (tried: ${MODELS[*]})"
exit 1
