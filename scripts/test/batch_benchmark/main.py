import os
import sys
import argparse
from pathlib import Path

# Local imports
from runner_fission import get_function_list_via_fission, run_fission_batch
from runner_ghidra import run_ghidra_batch

def print_table(title: str, results: list[dict], engine_time: float = 0.0):
    print(f"\n{'='*70}")
    print(f" {title} ".center(70, '='))
    print(f"{'='*70}")
    if engine_time > 0:
        print(f" Engine/Binary Load Time: {engine_time:.4f} seconds")
        print(f"{'-'*70}")
        
    print(f"{'Address':<20} | {'Name':<25} | {'Time (s)':<10} | {'Success':<8}")
    print(f"{'-'*70}")
    
    total_time = 0.0
    success_count = 0
    for r in results:
        total_time += r["time_s"]
        if r["success"]:
            success_count += 1
            
        name = r["name"]
        if len(name) > 22:
            name = name[:19] + "..."
            
        print(f"{r['address']:<20} | {name:<25} | {r['time_s']:<10.4f} | {str(r['success']):<8}")
        
    print(f"{'-'*70}")
    avg_time = total_time / len(results) if results else 0
    print(f" Total functions: {len(results)}")
    print(f" Successful:      {success_count}")
    print(f" Total Decomp Time: {total_time:.4f} s")
    print(f" Average Time/Func: {avg_time:.4f} s")
    print(f"{'='*70}\n")
    
    return total_time, avg_time, success_count

def main():
    parser = argparse.ArgumentParser(description="Batch Decompilation Benchmark (Ghidra vs Fission)")
    parser.add_argument("binary", type=str, help="Path to the binary file to analyze")
    parser.add_argument("--count", type=int, default=10, help="Number of functions to decompile (default: 10). Use 0 for ALL.")
    parser.add_argument("--ghidra-dir", type=str, default=None, help="Path to Ghidra installation directory")
    
    args = parser.parse_args()
    
    binary_path = os.path.abspath(args.binary)
    if not os.path.exists(binary_path):
        print(f"[-] Error: Binary file '{binary_path}' not found.")
        sys.exit(1)
        
    fission_dir = str(Path(__file__).resolve().parent.parent.parent.parent)
    ghidra_install_dir = args.ghidra_dir
    if not ghidra_install_dir:
        possible_dir = Path(fission_dir) / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC"
        if possible_dir.exists():
            ghidra_install_dir = str(possible_dir)

    print(f"[*] Fetching function list from {os.path.basename(binary_path)} via Fission...")
    all_functions = get_function_list_via_fission(binary_path, fission_dir)
    
    if not all_functions:
        print("[-] Failed to extract any functions from the binary.")
        sys.exit(1)
        
    print(f"[+] Found {len(all_functions)} functions in total.")
    
    target_functions = all_functions
    if args.count > 0 and len(all_functions) > args.count:
        target_functions = all_functions[:args.count]
        
    print(f"[*] Selected {len(target_functions)} functions for benchmark.")
    
    # ---------------------------------------------------------
    # 1. Run Fission Benchmark
    # ---------------------------------------------------------
    print(f"\n[*] Starting Fission Batch Analysis...")
    fission_results = run_fission_batch(binary_path, target_functions, fission_dir)
    f_total, f_avg, f_succ = print_table("Fission Results (CLI Overhead Included)", fission_results)

    # ---------------------------------------------------------
    # 2. Run Ghidra Benchmark
    # ---------------------------------------------------------
    print(f"\n[*] Starting PyGhidra Batch Analysis...")
    ghidra_load, ghidra_results = run_ghidra_batch(binary_path, target_functions, ghidra_install_dir)
    g_total, g_avg, g_succ = print_table("PyGhidra Results", ghidra_results, engine_time=ghidra_load)

    # ---------------------------------------------------------
    # 3. Final Comparison Summary
    # ---------------------------------------------------------
    print(f"{'='*70}")
    print(f" FINAL SUMMARY ".center(70, '='))
    print(f"{'='*70}")
    print(f" Target Binary: {os.path.basename(binary_path)}")
    print(f" Functions Measured: {len(target_functions)}")
    print(f"")
    print(f" [Fission]")
    print(f"   - Success Rate: {f_succ}/{len(target_functions)}")
    print(f"   - Total Time:   {f_total:.4f} s")
    print(f"   - Avg Time:     {f_avg:.4f} s/func (Includes process overhead)")
    print(f"")
    print(f" [Ghidra]")
    print(f"   - Success Rate: {g_succ}/{len(target_functions)}")
    print(f"   - Init Time:    {ghidra_load:.4f} s")
    print(f"   - Total Time:   {g_total:.4f} s (Pure decompilation)")
    print(f"   - Avg Time:     {g_avg:.4f} s/func")
    print(f"{'='*70}\n")

if __name__ == "__main__":
    main()
