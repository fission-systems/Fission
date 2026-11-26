#!/usr/bin/env bash
# The model loop of free-model-draft.yml, with the build-error repair loop.
#
# For each free model in turn, from a clean checkout:
#   1. `opencode run` drafts the fix; free_model_draft.py check accepts or refuses the edits.
#   2. If the accepted edits touch Rust under crates/, the WORKFLOW (never the
#      model, which has no shell) runs `cargo fmt -p <pkg>` and
#      `cargo check --locked -p <pkg> --all-targets` for each touched package.
#   3. On a compiler (or rustfmt) error the model gets the diagnostics in a new
#      `opencode run` and must fix them, at most MAX_REPAIRS rounds, each
#      followed by `check` and cargo again. Still failing: the next model.
#
# Budgets, shared by model switches and repairs (free_model_draft.py begin-call):
# - at most MAX_INVOCATIONS_PER_TASK (5) `opencode run` calls per task;
# - no opencode or cargo call starts if its timeout would cross BUDGET_SECS: the
#   timeout is shortened to what is left, or the call is skipped.
#
# Every attempt is classified (free_model_draft.py classify) and the class goes to
# the step summary. A class that stops the whole task ends the script with a
# distinct status (`stop-code`: 75 RATE_LIMITED, 76 PROVIDER_UNAVAILABLE,
# 77 INFRA_FAILURE, 78 BUDGET_EXHAUSTED) and the outputs stop_class / retry_after.
# A cargo log that says the registry was unreachable never goes to the model: cargo
# is retried once, then the task stops as INFRA_FAILURE.
#
# `opencode run --format json` stdout of invocation n is kept in
# $DRAFT/metrics/n.jsonl; free_agent_metrics.py turns it into metrics.json.
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
MODEL_TIMEOUT_SECS="${MODEL_TIMEOUT_SECS:-720}"
REPAIR_TIMEOUT_SECS="${REPAIR_TIMEOUT_SECS:-480}"
CARGO_TIMEOUT_SECS="${CARGO_TIMEOUT_SECS:-1500}"
MAX_REPAIRS="${MAX_REPAIRS:-2}"
MAX_INVOCATIONS_PER_TASK="${MAX_INVOCATIONS_PER_TASK:-5}"
# Stop starting new work after this many seconds in all (the job times out later).
BUDGET_SECS="${BUDGET_SECS:-6600}"
OUT="${GITHUB_OUTPUT:-/dev/null}"
SUMMARY="${GITHUB_STEP_SUMMARY:-/dev/null}"
HERE="$(cd "$(dirname "$0")" && pwd)"
HELPER="$HERE/free_model_draft.py"
METRICS_PY="$HERE/free_agent_metrics.py"
STATE="$DRAFT/state"
METRICS="$DRAFT/metrics"
export CARGO_TERM_COLOR=never

mkdir -p "$STATE" "$METRICS"
OPENCODE_CONFIG_CONTENT="$(cat "$DRAFT/permissions.json")"
export OPENCODE_CONFIG_CONTENT

helper() { python3 "$HELPER" "$@"; }
metrics() { python3 "$METRICS_PY" "$@"; }

set_outcome() { echo "$1" > "$STATE/outcome"; }

# One row of the per-attempt table in the step summary: model stage class note.
attempt() {
  local n
  touch "$STATE/attempts"
  n=$(($(wc -l < "$STATE/attempts") + 1))
  # shellcheck disable=SC2016  # the backticks are Markdown, not a command substitution
  printf '| %s | `%s` | %s | %s | %s |\n' "$n" "$1" "$2" "$3" "${4:-}" | tee -a "$STATE/attempts" >> "$SUMMARY"
}

PKGS=()
load_packages() {
  PKGS=()
  local pkg
  while IFS= read -r pkg; do
    [ -n "$pkg" ] && PKGS+=("$pkg")
  done < <(helper rust-packages)
}

OUT_OF_TIME=0
STOP_CLASS=""
STOP_DETAIL=""
CALL_N=0
CALL_SECS=0

# Record the invocation just made (metrics + attempt row).
conclude() { # model kind class [note]
  metrics record --draft "$DRAFT" --n "$CALL_N" --model "$1" --kind "$2" --seconds "$CALL_SECS" --class "$3"
  attempt "$1" "$2" "$3" "${4:-}"
}

# One `opencode run`, inside the shared budgets. Returns
#   0  the provider answered (CALL_N, CALL_SECS set; the caller checks the edits),
#   1  this model is done (MODEL_UNAVAILABLE) or no time is left (OUT_OF_TIME=1),
#   30 the whole task stops (STOP_CLASS, STOP_DETAIL).
opencode_call() { # model promptfile stage requested_secs kind
  local model="$1" promptfile="$2" stage="$3" requested="$4" kind="$5"
  local secs rc=0 status=0 edits=0 cls t0 t1
  secs="$(helper begin-call --state "$STATE" --kind opencode --requested "$requested" \
    --budget "$BUDGET_SECS" --max-invocations "$MAX_INVOCATIONS_PER_TASK")" || rc=$?
  if [ "$rc" -eq 2 ]; then
    attempt "$model" "$stage" OUT_OF_TIME "no call started"
    OUT_OF_TIME=1
    return 1
  elif [ "$rc" -ne 0 ]; then
    STOP_CLASS=BUDGET_EXHAUSTED
    STOP_DETAIL=""
    attempt "$model" "$stage" BUDGET_EXHAUSTED "$MAX_INVOCATIONS_PER_TASK opencode runs spent"
    return 30
  fi
  CALL_N="$(cat "$STATE/invocations")"
  t0="$(date +%s)"
  timeout "$secs" opencode run --format json --model "$model" "$(cat "$promptfile")" \
    > "$METRICS/$CALL_N.jsonl" 2> "$METRICS/$CALL_N.err" || status=$?
  t1="$(date +%s)"
  CALL_SECS=$((t1 - t0))
  echo "$model ($stage) exited with status $status after ${CALL_SECS}s, $(wc -l < "$METRICS/$CALL_N.jsonl" | tr -d " ") events"
  cat "$METRICS/$CALL_N.err"
  grep -h '"type":"error"' "$METRICS/$CALL_N.jsonl" || true
  git add -A
  if ! git diff --cached --quiet; then edits=1; fi
  cls="$(helper classify --kind opencode --status "$status" --edits "$edits" \
    --file "$METRICS/$CALL_N.jsonl" --file "$METRICS/$CALL_N.err")"
  case "$cls" in
    RATE_LIMITED | PROVIDER_UNAVAILABLE)
      STOP_CLASS="$cls"
      STOP_DETAIL="$(helper retry-after --file "$METRICS/$CALL_N.jsonl" --file "$METRICS/$CALL_N.err")"
      conclude "$model" "$kind" "$cls" "${STOP_DETAIL:+retry after $STOP_DETAIL}"
      return 30
      ;;
    MODEL_UNAVAILABLE)
      conclude "$model" "$kind" "$cls"
      return 1
      ;;
  esac
  return 0
}

# The edit check after a call; records the attempt. 0: accepted.
check_edits() { # model kind
  local cls=OK
  if ! helper check --issue-json "$ISSUE_JSON" 2> "$DRAFT/check.err"; then
    cat "$DRAFT/check.err" >&2
    cls="$(helper classify --kind check --file "$DRAFT/check.err")"
  fi
  if [ "$2" = recheck ]; then
    attempt "$1" "recheck after cargo" "$cls"  # no new invocation to record
  else
    conclude "$1" "$2" "$cls"
  fi
  [ "$cls" = OK ]
}

# fmt and check on every touched package; the output goes to $DRAFT/cargo.log.
# 0 passed, 1 failed, 2 no time left. A package whose rustfmt fails is not checked:
# rustfmt's message (a syntax error) is what the model gets.
cargo_once() {
  local pkg status=0 secs fmt_bad=" "
  : > "$DRAFT/cargo.log"
  for pkg in ${PKGS[@]+"${PKGS[@]}"}; do
    secs="$(helper begin-call --state "$STATE" --kind cargo --requested "$CARGO_TIMEOUT_SECS" \
      --budget "$BUDGET_SECS")" || return 2
    echo "+ cargo fmt -p $pkg"
    if ! timeout "$secs" cargo fmt -p "$pkg" >> "$DRAFT/cargo.log" 2>&1; then
      status=1
      fmt_bad="$fmt_bad$pkg "
    fi
  done
  git add -A
  for pkg in ${PKGS[@]+"${PKGS[@]}"}; do
    case "$fmt_bad" in *" $pkg "*) continue ;; esac
    secs="$(helper begin-call --state "$STATE" --kind cargo --requested "$CARGO_TIMEOUT_SECS" \
      --budget "$BUDGET_SECS")" || return 2
    echo "+ cargo check --locked -p $pkg --all-targets"
    timeout "$secs" cargo check --locked -p "$pkg" --all-targets >> "$DRAFT/cargo.log" 2>&1 \
      || status=1
  done
  return "$status"
}

# cargo_once, retried once when the registry could not be reached.
# 0 passed, 1 compile (or rustfmt) error, 2 no time left, 3 infrastructure failure.
cargo_check() {
  local rc=0 cls
  cargo_once || rc=$?
  [ "$rc" -eq 1 ] || return "$rc"
  cls="$(helper classify --kind cargo --file "$DRAFT/cargo.log")"
  [ "$cls" = INFRA_FAILURE ] || return 1
  echo "cargo could not reach the registry (INFRA_FAILURE, never sent to the model): retrying once"
  attempt "cargo" "cargo" INFRA_FAILURE "retrying once"
  rc=0
  cargo_once || rc=$?
  if [ "$rc" -eq 1 ]; then
    cls="$(helper classify --kind cargo --file "$DRAFT/cargo.log")"
    [ "$cls" = INFRA_FAILURE ] && return 3
  fi
  return "$rc"
}

CARGO="skipped"
REPAIRS=0

# 0: accepted, 1: this model failed, 10: Rust is needed but not installed yet,
# 30: the whole task stops.
verify() {
  local model="$1" round=0 rc
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
    rc=0
    cargo_check || rc=$?
    case "$rc" in
      0)
        if ! check_edits "$model" "recheck"; then return 1; fi
        attempt "$model" cargo OK "round $round"
        CARGO="passed"; REPAIRS="$round"
        return 0
        ;;
      2)
        attempt "$model" cargo OUT_OF_TIME "no cargo call started"
        OUT_OF_TIME=1
        return 1
        ;;
      3)
        STOP_CLASS=INFRA_FAILURE
        STOP_DETAIL=""
        attempt "$model" cargo INFRA_FAILURE "registry unreachable twice"
        return 30
        ;;
    esac
    echo "cargo check failed"
    attempt "$model" cargo COMPILATION_ERROR "round $round"
    metrics reclass --draft "$DRAFT" --class COMPILATION_ERROR
    if [ "$round" -ge "$MAX_REPAIRS" ]; then
      echo "$model still fails after $round repair rounds"
      return 1
    fi
    round=$((round + 1))
    helper errors --file "$DRAFT/cargo.log" > "$DRAFT/errors.txt"
    if [ -z "$(tr -d '[:space:]' < "$DRAFT/errors.txt")" ]; then
      # Not a diagnostic (a tool failure): give the log's tail.
      tail -n 40 "$DRAFT/cargo.log" > "$DRAFT/errors.raw"
      helper errors --file "$DRAFT/errors.raw" > "$DRAFT/errors.txt" || true
    fi
    cat "$DRAFT/errors.txt"
    git diff --cached > "$DRAFT/current.diff"
    helper repair-prompt --issue-json "$ISSUE_JSON" --diff-file "$DRAFT/current.diff" \
      --errors-file "$DRAFT/errors.txt" > "$DRAFT/repair.txt"
    echo "repair round $round with $model"
    rc=0
    opencode_call "$model" "$DRAFT/repair.txt" "repair $round" "$REPAIR_TIMEOUT_SECS" repair || rc=$?
    if [ "$rc" -ne 0 ]; then return "$rc"; fi
    if ! check_edits "$model" repair; then return 1; fi
  done
}

finish() {
  local model="$1"
  git add -A
  git diff --cached > "$DRAFT/edits.patch"
  set_outcome ACCEPTED
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

# End the whole task: outputs, summary, distinct exit status.
stop_task() {
  local code
  code="$(helper stop-code "$STOP_CLASS")"
  set_outcome "$STOP_CLASS"
  {
    echo "stop_class=$STOP_CLASS"
    echo "retry_after=$STOP_DETAIL"
    echo "need_rust=false"
  } >> "$OUT"
  {
    echo
    echo "**Task stopped: $STOP_CLASS** (exit status $code)${STOP_DETAIL:+, retry after $STOP_DETAIL}. No other model was tried."
  } >> "$SUMMARY"
  echo "::error::task stopped: $STOP_CLASS (exit status $code)${STOP_DETAIL:+, retry after $STOP_DETAIL}"
  exit "$code"
}

if [ ! -f "$STATE/models.list" ]; then
  opencode models opencode > "$DRAFT/models.txt"
  echo "Free models listed:"; cat "$DRAFT/models.txt"
  # Free models have usage limits: the run number picks which one goes first,
  # and a model that fails or leaves no acceptable edits hands over to the
  # next. A rate-limited or unreachable provider stops the task instead.
  helper order-models --models-file "$DRAFT/models.txt" --seed "$SEED" > "$STATE/models.list"
  date +%s > "$STATE/start"
  {
    echo "### Attempts"
    echo
    echo "| # | model | stage | class | note |"
    echo "|---|---|---|---|---|"
  } >> "$SUMMARY"
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
    git reset -q --hard HEAD
    git clean -fdq
    rc=0
    opencode_call "$model" "$DRAFT/prompt.txt" draft "$MODEL_TIMEOUT_SECS" draft || rc=$?
    if [ "$rc" -eq 30 ]; then
      echo "::endgroup::"
      stop_task
    fi
    if [ "$rc" -ne 0 ]; then
      echo "::endgroup::"
      if [ "$OUT_OF_TIME" -eq 1 ]; then
        echo "out of time, no more models"
        break
      fi
      continue
    fi
    if ! check_edits "$model" draft; then
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
  if [ "$rc" -eq 30 ]; then
    stop_task
  fi
  if [ "$OUT_OF_TIME" -eq 1 ]; then
    echo "out of time, no more models"
    break
  fi
done

if [ "$OUT_OF_TIME" -eq 1 ]; then set_outcome OUT_OF_TIME; else set_outcome NO_ACCEPTABLE_DRAFT; fi
echo "::error::no free model left an acceptable draft (tried: ${MODELS[*]})"
exit 1
