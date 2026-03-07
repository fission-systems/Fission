import os
import sys
import argparse
from pathlib import Path

# Local imports
from runner_ghidra import decompile_with_ghidra
from runner_fission import decompile_with_fission
from normalizer import normalize_c_code
from analyzer import compare_codes

def main():
    parser = argparse.ArgumentParser(description="Compare Ghidra vs Fission Decompiler Outputs")
    parser.add_argument("binary", type=str, help="Path to the binary file to analyze")
    parser.add_argument("function", type=str, help="Function name or address (e.g., 'main' or '0x140001234')")
    parser.add_argument("--ghidra-dir", type=str, default=None, help="Path to Ghidra installation directory")
    parser.add_argument("--aggressive", action="store_true", help="Use aggressive normalization (strip variable names, etc.)")
    parser.add_argument("--no-color", action="store_true", help="Disable colored diff output")
    
    args = parser.parse_args()

    # Fallback to local vendor/ghidra if not specified
    ghidra_install_dir = args.ghidra_dir
    if not ghidra_install_dir:
        # Use relative path from the script or absolute path
        possible_dir = Path(__file__).resolve().parent.parent.parent.parent / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC"
        if possible_dir.exists():
            ghidra_install_dir = str(possible_dir)

    binary_path = os.path.abspath(args.binary)
    if not os.path.exists(binary_path):
        print(f"[-] Error: Binary file '{binary_path}' not found.")
        sys.exit(1)

    print(f"[*] Starting Comparison for {args.function} in {os.path.basename(binary_path)}")

    # 1. Run Fission
    print("[*] Running Fission CLI...")
    fission_out = decompile_with_fission(binary_path, args.function)
    
    # 2. Run Ghidra
    print("[*] Running PyGhidra...")
    ghidra_out = decompile_with_ghidra(binary_path, args.function, ghidra_install_dir)

    # 3. Check for failures
    if not fission_out:
        print("[-] Fission returned empty string or failed.")
    if not ghidra_out:
        print("[-] Ghidra returned empty string or failed.")

    # 4. Normalize
    print(f"[*] Normalizing code (Aggressive={args.aggressive})...")
    norm_fission = normalize_c_code(fission_out, aggressive=args.aggressive)
    norm_ghidra = normalize_c_code(ghidra_out, aggressive=args.aggressive)

    # 5. Analyze Map
    print("[*] Analyzing differences...")
    similarity, diff_text = compare_codes(norm_ghidra, norm_fission)

    # 6. Report
    print("=" * 60)
    print(f"Similarity Score: {similarity * 100:.2f}%")
    print("=" * 60)

    if diff_text:
        print("DIFF Output (Ghidra vs Fission):")
        if not args.no_color:
            try:
                from colorama import init, Fore, Style
                init(autoreset=True)
                colored_diff = ""
                for line in diff_text.splitlines():
                    if line.startswith("+"):
                        colored_diff += Fore.GREEN + line + "\n"
                    elif line.startswith("-"):
                        colored_diff += Fore.RED + line + "\n"
                    elif line.startswith("@@"):
                        colored_diff += Fore.CYAN + line + "\n"
                    else:
                        colored_diff += Style.DIM + line + "\n"
                print(colored_diff, end="")
            except ImportError:
                print(diff_text)
        else:
            print(diff_text)
    else:
        print("No differences found after normalization!")

if __name__ == "__main__":
    main()
