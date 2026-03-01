#!/usr/bin/env bash
# ===========================================================================
# run_benchmark.sh — Fission vs Ghidra benchmark orchestrator (v4)
#
# Full pipeline:
#   Step 1: Extract symbols from test binaries
#   Step 2: Generate suite YAML files
#   Step 3: Cache Ghidra decompilation (one-time)
#   Step 4: Run benchmark comparison
#   Step 5: Generate HTML report
#
# Usage:
#   ./run_benchmark.sh                     # Full pipeline
#   ./run_benchmark.sh --skip-cache        # Skip Ghidra cache (use existing)
#   ./run_benchmark.sh --suite FILE        # Use specific suite file
#   ./run_benchmark.sh --step 4            # Start from step 4
#   ./run_benchmark.sh --category control  # Run only 'control' category
# ===========================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Defaults
SAMPLES_DIR="$PROJECT_ROOT/samples"
BENCHMARK_DIR="$SCRIPT_DIR"
SUITES_DIR="$SCRIPT_DIR/suites"
CACHE_DIR="$PROJECT_ROOT/benchmark_cache"
RESULTS_DIR=""  # auto-generated with timestamp
SUITE_FILE=""
SKIP_CACHE=false
START_STEP=1
CATEGORY=""
TIMEOUT=60
GHIDRA_METHOD="auto"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --samples DIR       Test binaries directory (default: samples/)
  --cache DIR         Ghidra cache directory (default: benchmark_cache/)
  --suite FILE        Use specific suite file (skip steps 1-2)
  --skip-cache        Skip Ghidra cache generation (step 3)
  --step N            Start from step N (1-5)
  --category CAT      Filter by category
  --timeout SEC       Per-function timeout (default: 60)
  --ghidra-method M   Cache method: batch|direct|auto (default: auto)
  -o, --output DIR    Results output directory
  -h, --help          Show this help

Steps:
  1  Extract symbols from binaries
  2  Generate suite YAML files
  3  Cache Ghidra decompilation (one-time)
  4  Run Fission benchmark
  5  Generate HTML report
EOF
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --samples)     SAMPLES_DIR="$2"; shift 2 ;;
        --cache)       CACHE_DIR="$2"; shift 2 ;;
        --suite)       SUITE_FILE="$2"; shift 2 ;;
        --skip-cache)  SKIP_CACHE=true; shift ;;
        --step)        START_STEP="$2"; shift 2 ;;
        --category)    CATEGORY="$2"; shift 2 ;;
        --timeout)     TIMEOUT="$2"; shift 2 ;;
        --ghidra-method) GHIDRA_METHOD="$2"; shift 2 ;;
        -o|--output)   RESULTS_DIR="$2"; shift 2 ;;
        -h|--help)     usage ;;
        *)             echo "Unknown option: $1"; exit 1 ;;
    esac
done

# Auto-generate results dir with timestamp
if [[ -z "$RESULTS_DIR" ]]; then
    RESULTS_DIR="$PROJECT_ROOT/benchmark_results/$(date +%Y%m%d_%H%M%S)"
fi

# Environment
export DYLD_LIBRARY_PATH="$PROJECT_ROOT/ghidra_decompiler/build:${DYLD_LIBRARY_PATH:-}"
export LD_LIBRARY_PATH="$PROJECT_ROOT/ghidra_decompiler/build:${LD_LIBRARY_PATH:-}"

# ===========================================================================
# Step functions
# ===========================================================================

step_header() {
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  Step $1: $2${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
}

step1_extract_symbols() {
    step_header 1 "Extract symbols from test binaries"

    if [[ ! -d "$SAMPLES_DIR" ]]; then
        echo -e "${RED}Error: Samples directory not found: $SAMPLES_DIR${NC}"
        exit 1
    fi

    echo "Samples directory: $SAMPLES_DIR"

    # Quick test with one binary
    local test_binary
    test_binary=$(find "$SAMPLES_DIR" -type f \( -name "test_*" -o -name "test_*.exe" \) | head -1)
    if [[ -n "$test_binary" ]]; then
        echo -e "${GREEN}Testing symbol extraction on: $(basename "$test_binary")${NC}"
        python3 "$BENCHMARK_DIR/extract_symbols.py" "$test_binary" --test-only --json | head -20
        echo ""
    fi

    echo -e "${GREEN}✓ Symbol extraction ready${NC}"
}

step2_generate_suites() {
    step_header 2 "Generate suite YAML files"

    mkdir -p "$SUITES_DIR"

    python3 "$BENCHMARK_DIR/generate_suites.py" \
        --samples-dir "$SAMPLES_DIR" \
        --patterns "$BENCHMARK_DIR/expected_patterns.yaml" \
        -o "$SUITES_DIR"

    local suite_count
    suite_count=$(find "$SUITES_DIR" -name "suite_*.yaml" -o -name "suite_*.json" 2>/dev/null | wc -l | tr -d ' ')
    echo -e "\n${GREEN}✓ Generated $suite_count suite files in $SUITES_DIR${NC}"

    # Show suite files
    ls -la "$SUITES_DIR"/suite_* 2>/dev/null || true
}

step3_cache_ghidra() {
    step_header 3 "Cache Ghidra decompilation"

    if $SKIP_CACHE; then
        echo -e "${YELLOW}Skipping Ghidra cache (--skip-cache)${NC}"
        return 0
    fi

    # Check if cache already exists
    if [[ -f "$CACHE_DIR/cache_manifest.json" ]]; then
        local existing
        existing=$(python3 -c "import json; m=json.load(open('$CACHE_DIR/cache_manifest.json')); print(m.get('total_functions', 0))" 2>/dev/null || echo "0")
        echo -e "${YELLOW}Existing cache found: $existing functions cached${NC}"
        echo "Use --skip-cache to reuse, or delete $CACHE_DIR to regenerate"
    fi

    # Determine suite to use
    local cache_suite
    if [[ -n "$SUITE_FILE" ]]; then
        cache_suite="$SUITE_FILE"
    elif [[ -f "$SUITES_DIR/suite_all.yaml" ]]; then
        cache_suite="$SUITES_DIR/suite_all.yaml"
    elif [[ -f "$SUITES_DIR/suite_all.json" ]]; then
        cache_suite="$SUITES_DIR/suite_all.json"
    else
        echo -e "${RED}Error: No suite file found. Run step 2 first.${NC}"
        exit 1
    fi

    echo "Using suite: $cache_suite"
    echo "Cache dir: $CACHE_DIR"

    python3 "$BENCHMARK_DIR/cache_ghidra.py" \
        --suite "$cache_suite" \
        -o "$CACHE_DIR" \
        --method "$GHIDRA_METHOD" \
        --timeout "$TIMEOUT"

    echo -e "\n${GREEN}✓ Ghidra cache complete${NC}"
}

step4_benchmark() {
    step_header 4 "Run Fission benchmark"

    # Check fission_cli exists
    if [[ -f "$PROJECT_ROOT/target/release/fission_cli" ]]; then
        echo "Using: target/release/fission_cli"
    elif [[ -f "$PROJECT_ROOT/target/debug/fission_cli" ]]; then
        echo -e "${YELLOW}Using: target/debug/fission_cli (debug build — consider release)${NC}"
    else
        echo -e "${YELLOW}fission_cli not found, will try cargo run${NC}"
    fi

    # Determine suite
    local bench_suite
    if [[ -n "$SUITE_FILE" ]]; then
        bench_suite="$SUITE_FILE"
    elif [[ -f "$SUITES_DIR/suite_all.yaml" ]]; then
        bench_suite="$SUITES_DIR/suite_all.yaml"
    elif [[ -f "$SUITES_DIR/suite_all.json" ]]; then
        bench_suite="$SUITES_DIR/suite_all.json"
    else
        echo -e "${RED}Error: No suite file found${NC}"
        exit 1
    fi

    local bench_args=(
        --suite "$bench_suite"
        --cache "$CACHE_DIR"
        -o "$RESULTS_DIR"
        --timeout "$TIMEOUT"
    )

    if [[ -n "$CATEGORY" ]]; then
        bench_args+=(--category "$CATEGORY")
    fi

    mkdir -p "$RESULTS_DIR"
    python3 "$BENCHMARK_DIR/benchmark_v4.py" "${bench_args[@]}"

    echo -e "\n${GREEN}✓ Benchmark complete — results in $RESULTS_DIR${NC}"
}

step5_report() {
    step_header 5 "Generate HTML report"

    if [[ ! -f "$RESULTS_DIR/results.json" ]] || [[ ! -f "$RESULTS_DIR/summary.json" ]]; then
        echo -e "${RED}Error: Results not found in $RESULTS_DIR${NC}"
        exit 1
    fi

    python3 "$BENCHMARK_DIR/report.py" \
        --results "$RESULTS_DIR/results.json" \
        --summary "$RESULTS_DIR/summary.json" \
        -o "$RESULTS_DIR/report.html"

    echo -e "\n${GREEN}✓ HTML report: $RESULTS_DIR/report.html${NC}"

    # Try to open in browser (macOS)
    if command -v open &>/dev/null; then
        echo "Opening report in browser..."
        open "$RESULTS_DIR/report.html" || true
    fi
}

# ===========================================================================
# Main
# ===========================================================================

echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║     Fission vs Ghidra Benchmark (v4)                       ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Project root: $PROJECT_ROOT"
echo "Samples:      $SAMPLES_DIR"
echo "Cache:        $CACHE_DIR"
echo "Output:       $RESULTS_DIR"
if [[ -n "$CATEGORY" ]]; then
    echo "Category:     $CATEGORY"
fi
if [[ -n "$SUITE_FILE" ]]; then
    echo "Suite:        $SUITE_FILE"
fi

# If suite file specified, skip steps 1-2
if [[ -n "$SUITE_FILE" ]] && [[ $START_STEP -lt 3 ]]; then
    START_STEP=3
fi

OVERALL_START=$(date +%s)

for step in $(seq "$START_STEP" 5); do
    case $step in
        1) step1_extract_symbols ;;
        2) step2_generate_suites ;;
        3) step3_cache_ghidra ;;
        4) step4_benchmark ;;
        5) step5_report ;;
    esac
done

OVERALL_END=$(date +%s)
ELAPSED=$((OVERALL_END - OVERALL_START))

echo -e "\n${GREEN}══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Benchmark complete in ${ELAPSED}s${NC}"
echo -e "${GREEN}  Results: $RESULTS_DIR${NC}"
echo -e "${GREEN}══════════════════════════════════════════════════════════════${NC}"
