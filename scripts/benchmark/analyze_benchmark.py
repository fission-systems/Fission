#!/usr/bin/env python3
"""
Performance benchmark result parser and comparison tool.
Detects performance regressions and generates reports.
"""

import json
import sys
import argparse
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Optional, Tuple
import re

class BenchmarkResult:
    """Represents a single benchmark result."""
    
    def __init__(self, name: str, mean_ns: float, std_dev: float, samples: int):
        self.name = name
        self.mean_ns = mean_ns  # nanoseconds
        self.std_dev = std_dev
        self.samples = samples
    
    @property
    def mean_ms(self) -> float:
        """Mean in milliseconds."""
        return self.mean_ns / 1_000_000
    
    @property
    def mean_us(self) -> float:
        """Mean in microseconds."""
        return self.mean_ns / 1_000
    
    def get_formatted_mean(self) -> str:
        """Return human-readable mean time."""
        if self.mean_ns < 1_000:
            return f"{self.mean_ns:.2f} ns"
        elif self.mean_ns < 1_000_000:
            return f"{self.mean_us:.2f} µs"
        else:
            return f"{self.mean_ms:.2f} ms"
    
    def compare_with(self, other: 'BenchmarkResult') -> Tuple[float, str]:
        """
        Compare with another benchmark result.
        Returns (percentage_change, emoji_indicator).
        """
        if self.mean_ns == 0:
            return 0.0, "✓"
        
        percentage_change = ((other.mean_ns - self.mean_ns) / self.mean_ns) * 100
        
        if percentage_change < -5:
            emoji = "🟢"  # Improvement
        elif percentage_change > 10:
            emoji = "🔴"  # Regression
        elif percentage_change > 5:
            emoji = "🟡"  # Minor regression
        else:
            emoji = "✓"   # No significant change
        
        return percentage_change, emoji


class BenchmarkParser:
    """Parse criterion benchmark output."""
    
    @staticmethod
    def parse_criterion_output(text: str) -> Dict[str, BenchmarkResult]:
        """
        Parse criterion benchmark output.
        Expected format from 'cargo bench -- --output-format bencher'
        """
        results = {}
        
        # Parse criterion output format
        # Format: "benchmark_name time: [time_value unit] +/- [deviation unit]"
        lines = text.split('\n')
        
        for line in lines:
            # Example: "cfg_analysis_16:  time: [123.45 ms +/- 1.23 ms]"
            match = re.search(
                r'(\w+):\s+time:\s+\[([0-9.]+)\s+([a-z]+)\s+\+/-\s+([0-9.]+)\s+\w+\]',
                line
            )
            if match:
                name = match.group(1)
                mean_value = float(match.group(2))
                unit = match.group(3)
                std_dev = float(match.group(4))
                
                # Convert to nanoseconds
                unit_multipliers = {
                    'ns': 1,
                    'µs': 1_000,
                    'us': 1_000,
                    'ms': 1_000_000,
                    's': 1_000_000_000,
                }
                
                mean_ns = mean_value * unit_multipliers.get(unit, 1)
                std_dev_ns = std_dev * unit_multipliers.get(unit, 1)
                
                results[name] = BenchmarkResult(name, mean_ns, std_dev_ns, 1)
        
        return results
    
    @staticmethod
    def parse_json_results(json_path: Path) -> Dict[str, BenchmarkResult]:
        """Parse criterion JSON output."""
        results = {}
        
        if not json_path.exists():
            return results
        
        with open(json_path, 'r') as f:
            data = json.load(f)
        
        # Criterion outputs benchmarks as a dict with benchmark names as keys
        for bench_name, bench_data in data.items():
            if 'mean' in bench_data and 'std_dev' in bench_data:
                mean_ns = bench_data['mean']['point_estimate']
                std_dev_ns = bench_data['std_dev']['point_estimate']
                samples = len(bench_data.get('samples', []))
                
                results[bench_name] = BenchmarkResult(
                    bench_name, mean_ns, std_dev_ns, samples
                )
        
        return results


class PerformanceHistory:
    """Manage performance benchmark history."""
    
    def __init__(self, history_dir: Path):
        self.history_dir = Path(history_dir)
        self.history_dir.mkdir(parents=True, exist_ok=True)
    
    def save_result(self, commit: str, timestamp: str, results: Dict[str, BenchmarkResult]):
        """Save benchmark results to history."""
        history_file = self.history_dir / f"{commit[:8]}.json"
        
        data = {
            'commit': commit,
            'timestamp': timestamp,
            'benchmarks': {
                name: {
                    'mean_ns': result.mean_ns,
                    'std_dev_ns': result.std_dev,
                    'samples': result.samples,
                }
                for name, result in results.items()
            }
        }
        
        with open(history_file, 'w') as f:
            json.dump(data, f, indent=2)
        
        return history_file
    
    def load_result(self, commit: str) -> Optional[Dict[str, BenchmarkResult]]:
        """Load benchmark results from history."""
        history_file = self.history_dir / f"{commit[:8]}.json"
        
        if not history_file.exists():
            return None
        
        with open(history_file, 'r') as f:
            data = json.load(f)
        
        results = {}
        for name, bench_data in data['benchmarks'].items():
            results[name] = BenchmarkResult(
                name,
                bench_data['mean_ns'],
                bench_data['std_dev_ns'],
                bench_data['samples']
            )
        
        return results
    
    def get_latest_main_results(self) -> Optional[Dict[str, BenchmarkResult]]:
        """Get the latest main branch results."""
        main_file = self.history_dir / "main_latest.json"
        
        if not main_file.exists():
            return None
        
        with open(main_file, 'r') as f:
            data = json.load(f)
        
        results = {}
        for name, bench_data in data['benchmarks'].items():
            results[name] = BenchmarkResult(
                name,
                bench_data['mean_ns'],
                bench_data['std_dev_ns'],
                bench_data['samples']
            )
        
        return results
    
    def update_main_latest(self, results: Dict[str, BenchmarkResult]):
        """Update the main branch latest results."""
        main_file = self.history_dir / "main_latest.json"
        
        data = {
            'timestamp': datetime.utcnow().isoformat(),
            'benchmarks': {
                name: {
                    'mean_ns': result.mean_ns,
                    'std_dev_ns': result.std_dev,
                    'samples': result.samples,
                }
                for name, result in results.items()
            }
        }
        
        with open(main_file, 'w') as f:
            json.dump(data, f, indent=2)


def generate_markdown_report(
    current: Dict[str, BenchmarkResult],
    baseline: Optional[Dict[str, BenchmarkResult]] = None,
    commit: str = "unknown",
) -> str:
    """Generate a markdown report of benchmark results."""
    
    report = []
    report.append("## 📊 Performance Benchmark Results\n")
    report.append(f"**Commit:** `{commit[:8]}`\n")
    report.append(f"**Timestamp:** {datetime.utcnow().isoformat()} UTC\n\n")
    
    # Group benchmarks by category
    categories = {}
    for name in current.keys():
        category = name.split('_')[0]
        if category not in categories:
            categories[category] = []
        categories[category].append(name)
    
    for category in sorted(categories.keys()):
        report.append(f"### {category.upper()}\n")
        report.append("| Benchmark | Current | Baseline | Change | Status |\n")
        report.append("|-----------|---------|----------|--------|--------|\n")
        
        for bench_name in sorted(categories[category]):
            current_result = current[bench_name]
            
            if baseline and bench_name in baseline:
                baseline_result = baseline[bench_name]
                change_pct, emoji = current_result.compare_with(baseline_result)
                change_str = f"{change_pct:+.1f}%"
                status = emoji
            else:
                change_str = "—"
                status = "🆕"
            
            line = (
                f"| {bench_name} | {current_result.get_formatted_mean()} | "
                f"{baseline[bench_name].get_formatted_mean() if baseline and bench_name in baseline else '—'} | "
                f"{change_str} | {status} |\n"
            )
            report.append(line)
        
        report.append("\n")
    
    # Add regression detection summary
    if baseline:
        regressions = []
        improvements = []
        
        for name in current.keys():
            if name in baseline:
                change_pct, emoji = current[name].compare_with(baseline[name])
                
                if emoji == "🔴":
                    regressions.append((name, change_pct))
                elif emoji == "🟢":
                    improvements.append((name, change_pct))
        
        if regressions:
            report.append("### ⚠️ Performance Regressions\n")
            for name, change_pct in sorted(regressions, key=lambda x: x[1], reverse=True):
                report.append(f"- **{name}**: {change_pct:+.1f}%\n")
            report.append("\n")
        
        if improvements:
            report.append("### ✅ Performance Improvements\n")
            for name, change_pct in sorted(improvements, key=lambda x: x[1]):
                report.append(f"- **{name}**: {change_pct:+.1f}%\n")
            report.append("\n")
    
    return ''.join(report)


def main():
    parser = argparse.ArgumentParser(description='Parse and compare benchmark results')
    parser.add_argument('--current', required=True, help='Current benchmark result file')
    parser.add_argument('--baseline', help='Baseline benchmark result file')
    parser.add_argument('--output', help='Output markdown report file')
    parser.add_argument('--commit', default='unknown', help='Commit hash')
    parser.add_argument('--history-dir', default='benchmark/history', help='History directory')
    
    args = parser.parse_args()
    
    # Parse current results
    with open(args.current, 'r') as f:
        current_text = f.read()
    
    current_results = BenchmarkParser.parse_criterion_output(current_text)
    
    # Parse baseline results if provided
    baseline_results = None
    if args.baseline:
        with open(args.baseline, 'r') as f:
            baseline_text = f.read()
        baseline_results = BenchmarkParser.parse_criterion_output(baseline_text)
    else:
        # Try to load from history
        history = PerformanceHistory(args.history_dir)
        baseline_results = history.get_latest_main_results()
    
    # Generate report
    report = generate_markdown_report(current_results, baseline_results, args.commit)
    
    # Output report
    if args.output:
        with open(args.output, 'w') as f:
            f.write(report)
        print(f"Report written to {args.output}")
    else:
        print(report)
    
    # Save to history
    history = PerformanceHistory(args.history_dir)
    history.save_result(args.commit, datetime.utcnow().isoformat(), current_results)
    
    return 0


if __name__ == '__main__':
    sys.exit(main())
