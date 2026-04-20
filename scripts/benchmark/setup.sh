#!/usr/bin/env bash
# Quick setup script for benchmark environment

set -e

echo "🚀 Fission Performance Benchmark Setup"
echo "======================================"

# Ensure benchmark directories exist
mkdir -p benchmark/{results,history,binary/x86-64/window/{small,medium,large,commercial_binary}}

# Ensure scripts directory exists
mkdir -p scripts/benchmark

# Make scripts executable
chmod +x scripts/benchmark/*.py 2>/dev/null || true

echo "✅ Benchmark directories created"
echo "📁 Structure:"
echo "   benchmark/results/     - Benchmark output files"
echo "   benchmark/history/     - Performance history tracking"
echo "   benchmark/binary/      - Sample binaries for testing"

# Check Python version
if ! command -v python3 &> /dev/null; then
    echo "❌ Python3 not found. Please install Python 3.8+"
    exit 1
fi

echo "✅ Python3 found: $(python3 --version)"

# Quick benchmark test
echo ""
echo "📊 Running quick benchmark test (CFG analysis only)..."
echo "This may take 1-2 minutes on first run..."

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

cargo bench -p fission-analysis --bench benchmark -- \
    cfg_analysis_64 \
    --output-format bencher \
    --warm-up-time 1 \
    --sample-size 5 2>&1 | grep -E "(cfg_analysis|time:)" || true

echo ""
echo "✅ Benchmark setup complete!"
echo ""
echo "📖 Next steps:"
echo "   1. Review benchmark/BENCHMARK_GUIDE.md"
echo "   2. Run full benchmarks: cargo bench -p fission-analysis --bench benchmark"
echo "   3. Compare with baseline: cargo bench -p fission-analysis --bench benchmark -- --baseline main"
