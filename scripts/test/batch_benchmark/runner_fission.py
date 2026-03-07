import os
import sys
import subprocess
import time
from pathlib import Path

# Provide a helper to fetch the function list via Fission CLI
def get_function_list_via_fission(binary_path: str, fission_dir: str) -> list[tuple[str, str]]:
    """
    Returns a list of (address, function_name) by parsing Fission's CLI --list output.
    """
    cmd = ["cargo", "run", "--release", "-p", "fission-cli", "--", "--cli", "--list", binary_path]
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
            if len(parts) >= 4 and parts[0].startswith("0x"):
                addr = parts[0]
                name = parts[-1]
                # Filter out some very common noisy stubs if needed
                functions.append((addr, name))
                
        return functions
    except Exception as e:
        print(f"[-] Error fetching function list: {e}")
        return []

def run_fission_batch(binary_path: str, functions: list[tuple[str, str]], fission_dir: str):
    """
    Runs fission CLI decompilation sequentially for each function and records time.
    Note: Currently Fission doesn't have a batch CLI mode, so this incurs process overhead per call.
    """
    times = []
    
    # Pre-build to ensure cargo build overhead is minimum
    subprocess.run(["cargo", "build", "--release", "-p", "fission-cli"], cwd=fission_dir, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    
    for addr, name in functions:
        cmd = ["cargo", "run", "--release", "-p", "fission-cli", "--", "--cli", binary_path, addr]
        
        start_t = time.perf_counter()
        try:
            result = subprocess.run(
                cmd,
                cwd=fission_dir,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=True
            )
            elapsed = time.perf_counter() - start_t
            
            # Simple verification that it actually decompiled something
            success = "[*] Decompiling...\n" in result.stdout
            times.append({
                "address": addr,
                "name": name,
                "time_s": elapsed,
                "success": success
            })
            
        except subprocess.CalledProcessError:
            elapsed = time.perf_counter() - start_t
            times.append({
                "address": addr,
                "name": name,
                "time_s": elapsed,
                "success": False
            })
            
    return times
