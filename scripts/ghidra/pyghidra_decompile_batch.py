#!/usr/bin/env python3
"""
PyGhidra Batch Decompilation Script
Decompiles multiple functions in a single Ghidra session.
"""

import sys
import os
import shutil
import tempfile
import pyghidra
import json
from pathlib import Path

def decompile_batch(binary_path, address_file, output_dir):
    # Set Ghidra installation path
    ghidra_path = "/Users/sjkim1127/Fission/vendor/ghidra/ghidra_11.4.2_PUBLIC"
    os.environ['GHIDRA_INSTALL_DIR'] = ghidra_path
    
    # Load addresses
    addresses = []
    if not os.path.exists(address_file):
        print(f"Error: Address file not found: {address_file}")
        return 1
        
    with open(address_file, 'r') as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith('#'):
                parts = line.split()
                addr_str = parts[0]
                addresses.append(int(addr_str, 16) if addr_str.startswith('0x') else int(addr_str))

    print(f"[*] Starting batch decompilation of {len(addresses)} functions...")
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)

    temp_project_dir = tempfile.mkdtemp(prefix="fission_batch_")
    try:
        # Launch PyGhidra and analyze binary ONCE
        with pyghidra.open_program(
            binary_path,
            analyze=True,
            project_location=temp_project_dir,
            project_name="fission_batch",
            nested_project_location=False,
        ) as flat_api:
            program = flat_api.getCurrentProgram()
            
            from ghidra.app.decompiler import DecompInterface
            from ghidra.util.task import ConsoleTaskMonitor
            
            decompiler = DecompInterface()
            decompiler.openProgram(program)
            monitor = ConsoleTaskMonitor()
            
            found_count = 0
            for addr_int in addresses:
                # Use a more robust way to get address (handling potential space issues)
                ghidra_addr = program.getAddressFactory().getDefaultAddressSpace().getAddress(addr_int)
                addr_hex = f"0x{addr_int:x}"
                
                function = program.getFunctionManager().getFunctionAt(ghidra_addr)
                if function is None:
                    function = program.getFunctionManager().getFunctionContaining(ghidra_addr)
                
                # If still not found, search Symbol Table more aggressively
                if function is None:
                    try:
                        # 1. Check Symbol Table (including imports)
                        symbols = program.getSymbolTable().getSymbols(ghidra_addr)
                        symbol = symbols[0] if symbols else None
                        
                        if symbol:
                            res_obj = {
                                "name": symbol.getName(),
                                "address": addr_hex,
                                "code": f"// Symbol/Import Found: {symbol.getName()}\n// No decompilation needed for data/import slot."
                            }
                            with open(output_path / f"ghidra_{addr_hex}.json", "w", encoding="utf-8") as out:
                                json.dump(res_obj, out, indent=2)
                            found_count += 1
                            continue
                            
                        # 2. Try to force create a function if it's executable code
                        flat_api.disassemble(ghidra_addr)
                        # Wait a tiny bit (Ghidra sometimes needs a moment to update DB)
                        flat_api.createFunction(ghidra_addr, f"sub_{addr_int:x}")
                        function = program.getFunctionManager().getFunctionAt(ghidra_addr)
                    except Exception as e:
                        print(f"  [-] Error handling {addr_hex}: {e}")
                
                if function:
                    addr_hex = f"0x{addr_int:x}"
                    print(f"  [+] Decompiling {function.getName()} at {addr_hex}...")
                    results = decompiler.decompileFunction(function, 30, monitor)
                    if results.decompileCompleted():
                        decomp_source = results.getDecompiledFunction()
                        code = decomp_source.getC() if decomp_source else "// Error: Null decompilation"
                        
                        # Save result to individual JSON for compatibility with comparison script
                        res_obj = {
                            "name": function.getName(),
                            "address": addr_hex,
                            "code": code
                        }
                        with open(output_path / f"ghidra_{addr_hex}.json", "w", encoding="utf-8") as out:
                            json.dump(res_obj, out, indent=2)
                        found_count += 1
                else:
                    print(f"  [-] Warning: No function found at 0x{addr_int:X}")
            
            print(f"[*] Batch complete. Decompiled {found_count}/{len(addresses)} functions.")
            decompiler.dispose()
            return 0
            
    except Exception as e:
        print(f"Error in batch decompilation: {e}")
        import traceback
        traceback.print_exc()
        return 1
    finally:
        shutil.rmtree(temp_project_dir, ignore_errors=True)

if __name__ == "__main__":
    if len(sys.argv) < 4:
        print("Usage: pyghidra_decompile_batch.py <binary> <address_file> <output_dir>")
        sys.exit(1)
    
    sys.exit(decompile_batch(sys.argv[1], sys.argv[2], sys.argv[3]))
