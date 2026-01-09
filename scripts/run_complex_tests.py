#!/usr/bin/env python3
"""
Automated test runner for complex decompilation test cases.
Runs all test binaries through Ghidra and Fission comparison pipeline.
"""

import os
import sys
import json
import subprocess
import time
from pathlib import Path
from typing import Dict, List, Tuple
from datetime import datetime

# ANSI color codes
class Colors:
    GREEN = '\033[0;32m'
    RED = '\033[0;31m'
    YELLOW = '\033[1;33m'
    BLUE = '\033[0;34m'
    CYAN = '\033[0;36m'
    BOLD = '\033[1m'
    NC = '\033[0m'  # No Color

class TestCase:
    """Represents a single test case"""
    def __init__(self, name: str, binary: str, addresses: str, category: str, difficulty: int):
        self.name = name
        self.binary = binary
        self.addresses = addresses
        self.category = category
        self.difficulty = difficulty
        self.result_dir = None
        self.similarity = None
        self.status = "pending"
        self.error = None
        self.duration = 0

# Test configuration
TEST_CASES = [
    TestCase(
        name="Nested Loops",
        binary="test/bin_x64/nested_loops_x64.exe",
        addresses="test/addresses/nested_loops_addrs.txt",
        category="Control Flow",
        difficulty=3
    ),
    TestCase(
        name="Switch-Case",
        binary="test/bin_x64/switch_case_x64.exe",
        addresses="test/addresses/switch_case_addrs.txt",
        category="Control Flow",
        difficulty=2
    ),
    TestCase(
        name="Recursion",
        binary="test/bin_x64/recursion_x64.exe",
        addresses="test/addresses/recursion_addrs.txt",
        category="Control Flow",
        difficulty=4
    ),
    TestCase(
        name="Complex Structs",
        binary="test/bin_x64/complex_structs_x64.exe",
        addresses="test/addresses/complex_structs_addrs.txt",
        category="Data Structures",
        difficulty=4
    ),
    TestCase(
        name="Function Pointers",
        binary="test/bin_x64/function_pointers_x64.exe",
        addresses="test/addresses/function_pointers_addrs.txt",
        category="Pointers",
        difficulty=5
    ),
    TestCase(
        name="Virtual Functions",
        binary="test/bin_x64/virtual_functions_x64.exe",
        addresses="test/addresses/virtual_functions_addrs.txt",
        category="C++ Features",
        difficulty=5
    ),
]

def print_header(text: str):
    """Print a formatted header"""
    print(f"\n{Colors.BOLD}{Colors.CYAN}{'=' * 70}{Colors.NC}")
    print(f"{Colors.BOLD}{Colors.CYAN}{text:^70}{Colors.NC}")
    print(f"{Colors.BOLD}{Colors.CYAN}{'=' * 70}{Colors.NC}\n")

def print_section(text: str):
    """Print a section header"""
    print(f"\n{Colors.BOLD}{Colors.YELLOW}{'─' * 70}{Colors.NC}")
    print(f"{Colors.BOLD}{Colors.YELLOW}{text}{Colors.NC}")
    print(f"{Colors.BOLD}{Colors.YELLOW}{'─' * 70}{Colors.NC}")

def run_test(test: TestCase, base_output_dir: str) -> bool:
    """Run a single test case"""
    print(f"\n{Colors.BLUE}[{test.category}]{Colors.NC} {Colors.BOLD}{test.name}{Colors.NC}")
    print(f"  Binary: {test.binary}")
    print(f"  Difficulty: {'⭐' * test.difficulty}")
    
    # Create result directory
    test_slug = test.name.lower().replace(' ', '_').replace('-', '_')
    test.result_dir = os.path.join(base_output_dir, f"result_{test_slug}")
    
    # Check if files exist
    if not os.path.exists(test.binary):
        test.status = "error"
        test.error = f"Binary not found: {test.binary}"
        print(f"  {Colors.RED}✗ Error: {test.error}{Colors.NC}")
        return False
    
    if not os.path.exists(test.addresses):
        test.status = "error"
        test.error = f"Address file not found: {test.addresses}"
        print(f"  {Colors.RED}✗ Error: {test.error}{Colors.NC}")
        return False
    
    # Count functions to test
    with open(test.addresses, 'r') as f:
        func_count = len([line for line in f if line.strip()])
    print(f"  Functions to test: {func_count}")
    
    # Run comparison script
    cmd = [
        "python3",
        "scripts/compare_decompilers_v2.py",
        test.binary,
        test.addresses,
        test.result_dir,
        "--batch"
    ]
    
    print(f"  {Colors.YELLOW}Running comparison...{Colors.NC}")
    start_time = time.time()
    
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=600  # 10 minute timeout
        )
        
        test.duration = time.time() - start_time
        
        if result.returncode != 0:
            test.status = "error"
            test.error = f"Comparison script failed (exit {result.returncode})"
            print(f"  {Colors.RED}✗ Failed: {test.error}{Colors.NC}")
            if result.stderr:
                print(f"  Error output: {result.stderr[:200]}")
            return False
        
        # Parse results
        summary_file = os.path.join(test.result_dir, "comparison_summary.json")
        if os.path.exists(summary_file):
            with open(summary_file, 'r') as f:
                summary = json.load(f)
                test.similarity = summary.get('average_similarity', 0.0)
                test.status = "success"
                
                print(f"  {Colors.GREEN}✓ Complete{Colors.NC}")
                print(f"  Similarity: {Colors.BOLD}{test.similarity:.2f}%{Colors.NC}")
                print(f"  Duration: {test.duration:.1f}s")
                return True
        else:
            test.status = "error"
            test.error = "Summary file not found"
            print(f"  {Colors.RED}✗ Error: {test.error}{Colors.NC}")
            return False
            
    except subprocess.TimeoutExpired:
        test.status = "timeout"
        test.error = "Test timed out (>10 minutes)"
        test.duration = time.time() - start_time
        print(f"  {Colors.RED}✗ Timeout: {test.error}{Colors.NC}")
        return False
    except Exception as e:
        test.status = "error"
        test.error = str(e)
        test.duration = time.time() - start_time
        print(f"  {Colors.RED}✗ Error: {test.error}{Colors.NC}")
        return False

def generate_summary(tests: List[TestCase], output_dir: str):
    """Generate summary report"""
    print_section("📊 Test Summary")
    
    # Calculate statistics
    total = len(tests)
    success = sum(1 for t in tests if t.status == "success")
    failed = sum(1 for t in tests if t.status == "error")
    timeout = sum(1 for t in tests if t.status == "timeout")
    
    successful_tests = [t for t in tests if t.status == "success"]
    avg_similarity = sum(t.similarity for t in successful_tests) / len(successful_tests) if successful_tests else 0
    
    total_duration = sum(t.duration for t in tests)
    
    # Print summary table
    print(f"\n{Colors.BOLD}Results by Category:{Colors.NC}\n")
    
    categories = {}
    for test in tests:
        if test.category not in categories:
            categories[test.category] = []
        categories[test.category].append(test)
    
    print(f"┌{'─' * 30}┬{'─' * 12}┬{'─' * 12}┬{'─' * 10}┐")
    print(f"│ {'Category':<28} │ {'Test':<10} │ {'Similarity':<10} │ {'Status':<8} │")
    print(f"├{'─' * 30}┼{'─' * 12}┼{'─' * 12}┼{'─' * 10}┤")
    
    for category, cat_tests in sorted(categories.items()):
        for i, test in enumerate(cat_tests):
            cat_name = category if i == 0 else ""
            name_short = test.name[:10]
            
            if test.similarity is not None:
                sim_str = f"{test.similarity:.2f}%"
            else:
                sim_str = "N/A"
            
            status_color = {
                "success": Colors.GREEN,
                "error": Colors.RED,
                "timeout": Colors.YELLOW,
                "pending": Colors.NC
            }.get(test.status, Colors.NC)
            
            status_str = f"{status_color}{test.status:>8}{Colors.NC}"
            
            print(f"│ {cat_name:<28} │ {name_short:<10} │ {sim_str:>10} │ {status_str} │")
    
    print(f"└{'─' * 30}┴{'─' * 12}┴{'─' * 12}┴{'─' * 10}┘")
    
    # Overall statistics
    print(f"\n{Colors.BOLD}Overall Statistics:{Colors.NC}")
    print(f"  Total tests: {total}")
    print(f"  {Colors.GREEN}Success: {success}{Colors.NC}")
    if failed > 0:
        print(f"  {Colors.RED}Failed: {failed}{Colors.NC}")
    if timeout > 0:
        print(f"  {Colors.YELLOW}Timeout: {timeout}{Colors.NC}")
    print(f"  {Colors.BOLD}Average similarity: {avg_similarity:.2f}%{Colors.NC}")
    print(f"  Total duration: {total_duration:.1f}s ({total_duration/60:.1f} minutes)")
    
    # Difficulty analysis
    print(f"\n{Colors.BOLD}By Difficulty:{Colors.NC}")
    for diff in range(2, 6):
        diff_tests = [t for t in successful_tests if t.difficulty == diff]
        if diff_tests:
            avg_sim = sum(t.similarity for t in diff_tests) / len(diff_tests)
            print(f"  {'⭐' * diff}: {avg_sim:.2f}% avg ({len(diff_tests)} tests)")
    
    # Save JSON summary
    summary_data = {
        "timestamp": datetime.now().isoformat(),
        "total_tests": total,
        "success": success,
        "failed": failed,
        "timeout": timeout,
        "average_similarity": avg_similarity,
        "total_duration": total_duration,
        "tests": [
            {
                "name": t.name,
                "category": t.category,
                "difficulty": t.difficulty,
                "status": t.status,
                "similarity": t.similarity,
                "duration": t.duration,
                "error": t.error,
                "result_dir": t.result_dir
            }
            for t in tests
        ]
    }
    
    summary_file = os.path.join(output_dir, "complex_tests_summary.json")
    with open(summary_file, 'w') as f:
        json.dump(summary_data, f, indent=2)
    print(f"\n{Colors.GREEN}✓ Summary saved to: {summary_file}{Colors.NC}")
    
    # Generate detailed report
    generate_html_report(tests, output_dir, summary_data)

def generate_html_report(tests: List[TestCase], output_dir: str, summary_data: dict):
    """Generate HTML report"""
    html_file = os.path.join(output_dir, "complex_tests_report.html")
    
    html_content = f"""<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Fission Complex Tests Report</title>
    <style>
        body {{
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            max-width: 1200px;
            margin: 40px auto;
            padding: 20px;
            background: #f5f5f5;
        }}
        .header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 30px;
            border-radius: 10px;
            margin-bottom: 30px;
            box-shadow: 0 4px 6px rgba(0,0,0,0.1);
        }}
        .header h1 {{
            margin: 0;
            font-size: 2.5em;
        }}
        .header .subtitle {{
            margin-top: 10px;
            opacity: 0.9;
        }}
        .summary {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            margin-bottom: 30px;
        }}
        .stat-card {{
            background: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .stat-value {{
            font-size: 2em;
            font-weight: bold;
            color: #667eea;
        }}
        .stat-label {{
            color: #666;
            margin-top: 5px;
        }}
        table {{
            width: 100%;
            background: white;
            border-radius: 8px;
            overflow: hidden;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
            border-collapse: collapse;
        }}
        th {{
            background: #667eea;
            color: white;
            padding: 15px;
            text-align: left;
        }}
        td {{
            padding: 12px 15px;
            border-bottom: 1px solid #eee;
        }}
        tr:hover {{
            background: #f8f9fa;
        }}
        .success {{ color: #28a745; font-weight: bold; }}
        .error {{ color: #dc3545; font-weight: bold; }}
        .timeout {{ color: #ffc107; font-weight: bold; }}
        .difficulty {{ color: #ff9800; }}
        .similarity {{
            font-weight: bold;
            padding: 5px 10px;
            border-radius: 4px;
            display: inline-block;
        }}
        .sim-excellent {{ background: #d4edda; color: #155724; }}
        .sim-good {{ background: #d1ecf1; color: #0c5460; }}
        .sim-fair {{ background: #fff3cd; color: #856404; }}
        .sim-poor {{ background: #f8d7da; color: #721c24; }}
        .category {{
            font-weight: bold;
            color: #764ba2;
        }}
        .timestamp {{
            text-align: center;
            color: #666;
            margin-top: 30px;
            padding-top: 20px;
            border-top: 1px solid #ddd;
        }}
    </style>
</head>
<body>
    <div class="header">
        <h1>🧪 Fission Complex Tests Report</h1>
        <div class="subtitle">Comprehensive decompilation quality assessment across complex patterns</div>
    </div>
    
    <div class="summary">
        <div class="stat-card">
            <div class="stat-value">{summary_data['total_tests']}</div>
            <div class="stat-label">Total Tests</div>
        </div>
        <div class="stat-card">
            <div class="stat-value">{summary_data['average_similarity']:.2f}%</div>
            <div class="stat-label">Avg Similarity</div>
        </div>
        <div class="stat-card">
            <div class="stat-value">{summary_data['success']}</div>
            <div class="stat-label">Success</div>
        </div>
        <div class="stat-card">
            <div class="stat-value">{summary_data['total_duration']/60:.1f}m</div>
            <div class="stat-label">Total Time</div>
        </div>
    </div>
    
    <table>
        <thead>
            <tr>
                <th>Category</th>
                <th>Test Name</th>
                <th>Difficulty</th>
                <th>Similarity</th>
                <th>Status</th>
                <th>Duration</th>
            </tr>
        </thead>
        <tbody>
"""
    
    current_category = None
    for test in tests:
        cat_display = test.category if test.category != current_category else ""
        current_category = test.category
        
        if test.similarity is not None:
            if test.similarity >= 90:
                sim_class = "sim-excellent"
            elif test.similarity >= 80:
                sim_class = "sim-good"
            elif test.similarity >= 70:
                sim_class = "sim-fair"
            else:
                sim_class = "sim-poor"
            sim_display = f'<span class="similarity {sim_class}">{test.similarity:.2f}%</span>'
        else:
            sim_display = "N/A"
        
        status_class = test.status
        status_display = test.status.upper()
        
        html_content += f"""
            <tr>
                <td class="category">{cat_display}</td>
                <td>{test.name}</td>
                <td class="difficulty">{'⭐' * test.difficulty}</td>
                <td>{sim_display}</td>
                <td class="{status_class}">{status_display}</td>
                <td>{test.duration:.1f}s</td>
            </tr>
"""
    
    html_content += f"""
        </tbody>
    </table>
    
    <div class="timestamp">
        Generated: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}
    </div>
</body>
</html>
"""
    
    with open(html_file, 'w') as f:
        f.write(html_content)
    
    print(f"{Colors.GREEN}✓ HTML report saved to: {html_file}{Colors.NC}")

def main():
    """Main test runner"""
    print_header("🧪 Fission Complex Test Suite Runner")
    
    # Create output directory
    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
    output_dir = f"scripts/result_complex_tests_{timestamp}"
    os.makedirs(output_dir, exist_ok=True)
    
    print(f"Output directory: {Colors.CYAN}{output_dir}{Colors.NC}")
    print(f"Total test cases: {Colors.BOLD}{len(TEST_CASES)}{Colors.NC}")
    
    # Run all tests
    print_section("🚀 Running Tests")
    
    success_count = 0
    for i, test in enumerate(TEST_CASES, 1):
        print(f"\n{Colors.BOLD}[{i}/{len(TEST_CASES)}]{Colors.NC}", end=" ")
        if run_test(test, output_dir):
            success_count += 1
    
    # Generate summary
    generate_summary(TEST_CASES, output_dir)
    
    # Final message
    print_header("✨ Test Run Complete")
    print(f"Results saved to: {Colors.CYAN}{output_dir}{Colors.NC}")
    print(f"View HTML report: {Colors.CYAN}{output_dir}/complex_tests_report.html{Colors.NC}")
    
    # Exit code based on success
    if success_count == len(TEST_CASES):
        print(f"\n{Colors.GREEN}✓ All tests passed!{Colors.NC}\n")
        return 0
    else:
        print(f"\n{Colors.YELLOW}⚠ Some tests failed or timed out{Colors.NC}\n")
        return 1

if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print(f"\n\n{Colors.YELLOW}Test run interrupted by user{Colors.NC}")
        sys.exit(130)
    except Exception as e:
        print(f"\n{Colors.RED}Fatal error: {e}{Colors.NC}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
