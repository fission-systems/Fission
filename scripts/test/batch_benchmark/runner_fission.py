import os
import sys
import subprocess
import time
import json
import tempfile
from pathlib import Path

# Provide a helper to fetch the function list via Fission CLI
def get_function_list_via_fission(binary_path: str, fission_dir: str) -> list[tuple[str, str]]:
    """
    Returns a list of (address, function_name) by parsing Fission's CLI --list output.
    """
    cmd = ["cargo", "run", "--release", "-p", "fission-cli", "--bin", "fission_cli", "--", binary_path, "--list"]
    functions = []
    
    try:
        result = subprocess.run(
            cmd,
            cwd=fission_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=True
        )
        for line in result.stdout.splitlines():
            parts = line.split()
            # Look for lines that look like: 0x0000000100000460        -  EXP   _my_test_function
            if len(parts) >= 3 and parts[0].startswith("0x"):
                addr = parts[0]
                name = parts[-1]
                functions.append((addr, name))
                
        return functions
    except Exception as e:
        print(f"[-] Error fetching function list: {e}")
        return []

def run_fission_batch(binary_path: str, functions: list[tuple[str, str]], fission_dir: str):
    """
    Runs Fission CLI in single-process batch mode using --decomp-all --benchmark.
    This is a fair comparison with PyGhidra which also loads the binary once and
    decompiles all functions sequentially in a single process.
    
    Returns (init_sec, results) where init_sec is the Rust initialization time
    and results is a list of per-function timing dicts.
    """
    # Pre-build to ensure cargo build overhead is eliminated
    subprocess.run(
        ["cargo", "build", "--release", "-p", "fission-cli"],
        cwd=fission_dir, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    
    # Collect target addresses for subset filtering
    target_addrs = set()
    for addr, _ in functions:
        # Normalize addresses: strip 0x prefix and leading zeros, lowercase
        normalized = addr.lower().replace("0x", "").lstrip("0") or "0"
        target_addrs.add(normalized)
    
    with tempfile.NamedTemporaryFile("w+", suffix=".json", delete=False) as tmp:
        tmp_path = tmp.name
    
    # Run single-process batch: --decomp-all --benchmark -o <tmpfile>
    cmd = [
        "cargo", "run", "--release",
        "-p", "fission-cli", "--bin", "fission_cli",
        "--", binary_path,
        "--decomp-all", "--benchmark",
        "-o", tmp_path
    ]
    
    init_sec = 0.0
    results = []
    
    try:
        proc = subprocess.run(
            cmd,
            cwd=fission_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=300  # 5 min timeout for large binaries
        )
        
        # Parse the JSON benchmark output
        try:
            with open(tmp_path, "r") as f:
                data = json.load(f)
            
            meta = data.get("_meta", {})
            init_sec = meta.get("init_sec", 0.0)
            total_decomp_sec = meta.get("total_decomp_sec", 0.0)
            
            for entry in data.get("functions", []):
                addr_str = entry.get("address", "0x0")
                normalized = addr_str.lower().replace("0x", "").lstrip("0") or "0"
                
                # Filter to only the functions we were asked to benchmark
                if target_addrs and normalized not in target_addrs:
                    continue
                
                name = entry.get("name", "unknown")
                decomp_sec = entry.get("decomp_sec", 0.0)
                has_error = "error" in entry
                
                results.append({
                    "address": addr_str,
                    "name": name,
                    "time_s": decomp_sec,
                    "success": not has_error
                })
                
        except Exception as e:
            print(f"[-] Error parsing Fission batch JSON: {e}")
            # Fallback: return empty with error
            for addr, name in functions:
                results.append({
                    "address": addr,
                    "name": name,
                    "time_s": 0.0,
                    "success": False
                })
    
    except subprocess.TimeoutExpired:
        print("[-] Fission batch timed out (300s)")
        for addr, name in functions:
            results.append({
                "address": addr,
                "name": name,
                "time_s": 0.0,
                "success": False
            })
    
    except Exception as e:
        print(f"[-] Fission batch error: {e}")
        for addr, name in functions:
            results.append({
                "address": addr,
                "name": name,
                "time_s": 0.0,
                "success": False
            })
    
    finally:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)
    
    return init_sec, results
