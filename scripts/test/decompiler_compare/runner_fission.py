import os
import subprocess
import re
from pathlib import Path

def resolve_function_address(binary_path: str, function_name: str, fission_dir: str) -> str:
    """Run fission --cli --list to find the address of a function by name."""
    cmd = ["cargo", "run", "--release", "-p", "fission-cli", "--", "--cli", "--list", binary_path]
    try:
        result = subprocess.run(cmd, cwd=fission_dir, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=True)
        # Parse output: 0x0000000100000460        -  EXP   _my_test_function
        # Or without underscore: my_test_function
        for line in result.stdout.splitlines():
            parts = line.split()
            if len(parts) >= 4 and parts[0].startswith("0x"):
                name = parts[-1]
                if name == function_name or name == f"_{function_name}":
                    return parts[0]
        return ""
    except Exception as e:
        print(f"Error resolving function name: {e}")
        return ""

def decompile_with_fission(binary_path: str, address_or_name: str, fission_dir: str = None) -> str:
    """
    Run Fission CLI in headless mode to decompile the given binary at a specific address.
    """
    if not fission_dir:
        # Assume we are running from Fission root or inside the scripts dir
        # Move up from scripts/test/decompiler_compare/runner_fission.py -> 3 levels up
        fission_dir = str(Path(__file__).resolve().parent.parent.parent.parent)

    # Check if address_or_name is actually an address
    target_address = address_or_name
    if not address_or_name.startswith("0x"):
        print(f"[*] Fission: Resolving function name '{address_or_name}' to address...")
        resolved = resolve_function_address(binary_path, address_or_name, fission_dir)
        if not resolved:
            print(f"[-] Fission: Could not resolve '{address_or_name}' to an address via --list.")
            return ""
        print(f"[*] Fission: Resolved to {resolved}")
        target_address = resolved

    cmd = [
        "cargo", "run", "--release", "-p", "fission-cli", "--",
        "--cli",
        binary_path,
        target_address
    ]

    try:
        # Run Fission CLI
        result = subprocess.run(
            cmd,
            cwd=fission_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=True
        )
        
        output = result.stdout
        marker = "[*] Decompiling...\n"
        if marker in output:
            return output.split(marker, 1)[1].strip()
        return output.strip()
        
    except subprocess.CalledProcessError as e:
        print(f"Error running Fission CLI (Exit code {e.returncode}):")
        print(e.stderr)
        return ""
