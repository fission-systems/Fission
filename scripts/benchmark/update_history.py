#!/usr/bin/env python3
"""
Update performance benchmark history and generate graphs.
"""

import json
import sys
import argparse
from pathlib import Path
from datetime import datetime, timedelta
from typing import Dict, List, Optional
import subprocess

class PerformanceTracker:
    """Track and visualize performance over time."""
    
    def __init__(self, history_dir: Path):
        self.history_dir = Path(history_dir)
        self.history_dir.mkdir(parents=True, exist_ok=True)
        self.timeline_file = self.history_dir / "timeline.jsonl"
    
    def record_benchmark(self, commit: str, timestamp: str, branch: str, results: Dict):
        """Record a benchmark result in timeline."""
        entry = {
            'commit': commit,
            'timestamp': timestamp,
            'branch': branch,
            'results': results
        }
        
        with open(self.timeline_file, 'a') as f:
            f.write(json.dumps(entry) + '\n')
    
    def get_benchmark_history(self, benchmark_name: str, days: int = 30) -> List[Dict]:
        """Get historical data for a specific benchmark."""
        cutoff = datetime.utcnow() - timedelta(days=days)
        history = []
        
        if not self.timeline_file.exists():
            return history
        
        with open(self.timeline_file, 'r') as f:
            for line in f:
                entry = json.loads(line.strip())
                ts = datetime.fromisoformat(entry['timestamp'].replace('Z', '+00:00'))
                
                if ts < cutoff:
                    continue
                
                if benchmark_name in entry['results']:
                    history.append({
                        'timestamp': entry['timestamp'],
                        'commit': entry['commit'],
                        'branch': entry['branch'],
                        'value_ns': entry['results'][benchmark_name]['mean_ns'],
                    })
        
        return history
    
    def detect_regressions(self, benchmark_name: str, threshold_pct: float = 5.0) -> List[Dict]:
        """Detect performance regressions over time."""
        history = self.get_benchmark_history(benchmark_name)
        
        if len(history) < 2:
            return []
        
        regressions = []
        
        for i in range(1, len(history)):
            prev_value = history[i-1]['value_ns']
            curr_value = history[i]['value_ns']
            
            if prev_value == 0:
                continue
            
            pct_change = ((curr_value - prev_value) / prev_value) * 100
            
            if pct_change > threshold_pct:
                regressions.append({
                    'benchmark': benchmark_name,
                    'from_commit': history[i-1]['commit'],
                    'to_commit': history[i]['commit'],
                    'percentage_change': pct_change,
                    'prev_time_ns': prev_value,
                    'curr_time_ns': curr_value,
                })
        
        return regressions
    
    def generate_ascii_graph(self, benchmark_name: str, width: int = 60, height: int = 10) -> str:
        """Generate ASCII graph of benchmark performance."""
        history = self.get_benchmark_history(benchmark_name)
        
        if not history:
            return f"No data for {benchmark_name}"
        
        # Get values
        values = [h['value_ns'] for h in history]
        min_val = min(values)
        max_val = max(values)
        
        if min_val == max_val:
            return f"Constant performance: {min_val:.0f} ns"
        
        # Normalize values to graph height
        value_range = max_val - min_val
        normalized = [(v - min_val) / value_range * (height - 1) for v in values]
        
        # Build graph
        graph_lines = [[] for _ in range(height)]
        
        # Sample values to fit width
        step = max(1, len(normalized) // width)
        samples = [(i, normalized[i]) for i in range(0, len(normalized), step)][:width]
        
        for x, (idx, y) in enumerate(samples):
            y_idx = height - 1 - int(y)
            for row in range(height):
                if row == y_idx:
                    graph_lines[row].append('▓')
                elif row < y_idx:
                    graph_lines[row].append(' ')
                else:
                    graph_lines[row].append('░')
        
        # Format output
        result = [f"Performance History: {benchmark_name}\n"]
        result.append("Max: {:.2f} µs\n".format(max_val / 1000))
        
        for line in graph_lines:
            result.append(''.join(line) + '\n')
        
        result.append("Min: {:.2f} µs\n".format(min_val / 1000))
        
        return ''.join(result)


def parse_benchmark_file(filepath: Path) -> Dict[str, Dict]:
    """Parse benchmark output file."""
    results = {}
    
    with open(filepath, 'r') as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith('#'):
                continue
            
            # Try to extract benchmark name and time
            # Format: "bench_name time: [123.45 ms +/- 1.23 ms]"
            import re
            match = re.search(
                r'(\w+):\s+time:\s+\[([0-9.]+)\s+([a-z]+)\s+\+/-',
                line
            )
            if match:
                name = match.group(1)
                value = float(match.group(2))
                unit = match.group(3)
                
                unit_multipliers = {
                    'ns': 1,
                    'µs': 1_000,
                    'us': 1_000,
                    'ms': 1_000_000,
                    's': 1_000_000_000,
                }
                
                results[name] = {
                    'mean_ns': value * unit_multipliers.get(unit, 1)
                }
    
    return results


def main():
    parser = argparse.ArgumentParser(description='Update benchmark history and tracking')
    parser.add_argument('--result', required=True, help='Benchmark result file')
    parser.add_argument('--commit', required=True, help='Commit hash')
    parser.add_argument('--branch', default='main', help='Branch name')
    parser.add_argument('--timestamp', help='Timestamp (default: now)')
    parser.add_argument('--history-dir', default='benchmark/history', help='History directory')
    parser.add_argument('--report-file', help='Output report file')
    
    args = parser.parse_args()
    
    if not args.timestamp:
        args.timestamp = datetime.utcnow().isoformat() + 'Z'
    
    # Parse results
    results = parse_benchmark_file(Path(args.result))
    
    # Update history
    tracker = PerformanceTracker(Path(args.history_dir))
    tracker.record_benchmark(args.commit, args.timestamp, args.branch, results)
    
    # Generate report if requested
    if args.report_file:
        report_lines = []
        report_lines.append("# Performance History Report\n")
        report_lines.append(f"**Updated:** {datetime.utcnow().isoformat()}\n\n")
        
        # Check for regressions
        all_regressions = []
        for bench_name in results.keys():
            regressions = tracker.detect_regressions(bench_name, threshold_pct=5.0)
            all_regressions.extend(regressions)
        
        if all_regressions:
            report_lines.append("## ⚠️ Recent Regressions\n\n")
            for reg in all_regressions[:5]:  # Show top 5
                pct = reg['percentage_change']
                report_lines.append(
                    f"- **{reg['benchmark']}**: {pct:+.1f}% "
                    f"({reg['prev_time_ns']:.0f} → {reg['curr_time_ns']:.0f} ns)\n"
                )
            report_lines.append("\n")
        
        # Add performance graphs
        report_lines.append("## Performance Trends\n\n")
        for bench_name in sorted(results.keys()):
            graph = tracker.generate_ascii_graph(bench_name)
            report_lines.append(f"```\n{graph}\n```\n\n")
        
        with open(args.report_file, 'w') as f:
            f.writelines(report_lines)
    
    print(f"Updated benchmark history with {len(results)} benchmarks")
    print(f"History file: {tracker.timeline_file}")
    
    return 0


if __name__ == '__main__':
    sys.exit(main())
