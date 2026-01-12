#!/usr/bin/env python3
"""
PyGhidra Decompilation Script
Simple comparison tool using PyGhidra
"""

import sys
import os
import shutil
import tempfile
import pyghidra

def decompile_function(binary_path, address_str):
    """Decompile a function at the given address using PyGhidra"""
    
    # Set Ghidra installation path
    ghidra_path = "/Users/sjkim1127/Fission/ghidra_11.4.2_PUBLIC"
    os.environ['GHIDRA_INSTALL_DIR'] = ghidra_path
    
    # Parse address
    if address_str.startswith("0x"):
        address_int = int(address_str, 16)
    else:
        address_int = int(address_str)
    
    print("=== Ghidra Decompilation (PyGhidra) ===")
    print(f"Binary: {binary_path}")
    print(f"Address: 0x{address_int:X}")
    print()
    
    temp_project_dir = tempfile.mkdtemp(prefix="fission_ghidra_")
    try:
        # Launch PyGhidra and analyze binary
        with pyghidra.open_program(
            binary_path,
            analyze=True,
            project_location=temp_project_dir,
            project_name="fission_tmp",
            nested_project_location=False,
        ) as flat_api:
            program = flat_api.getCurrentProgram()
            
            # Get address
            addr_factory = program.getAddressFactory()
            addr = addr_factory.getAddress(f"0x{address_int:X}")
            
            # Get or create function at address
            func_mgr = program.getFunctionManager()
            function = func_mgr.getFunctionAt(addr)
            if function is None:
                function = func_mgr.getFunctionContaining(addr)
            if function is None:
                try:
                    flat_api.disassemble(addr)
                except Exception as e:
                    print(f"Warning: disassemble failed at 0x{address_int:X}: {e}")
                try:
                    flat_api.createFunction(addr, f"sub_{address_int:X}")
                except Exception as e:
                    print(f"Warning: createFunction failed at 0x{address_int:X}: {e}")
                function = func_mgr.getFunctionAt(addr)
                if function is None:
                    function = func_mgr.getFunctionContaining(addr)
            
            if function is None:
                print(f"Error: No function found at address 0x{address_int:X}")
                return 1
            
            print(f"Function: {function.getName()}")
            print(f"Entry Point: 0x{function.getEntryPoint().getOffset():X}")
            print()
            
            # Get assembly listing
            print("--- Assembly Listing ---")
            listing = program.getListing()
            func_body = function.getBody()
            instruction_iter = listing.getInstructions(func_body, True)
            
            instr_count = 0
            for instruction in instruction_iter:
                addr_str = f"0x{instruction.getAddress().getOffset():X}"
                mnemonic = instruction.getMnemonicString()
                operands = instruction.getDefaultOperandRepresentation(0)
                
                # Get all operands
                op_count = instruction.getNumOperands()
                op_list = []
                for i in range(op_count):
                    op_list.append(instruction.getDefaultOperandRepresentation(i))
                operands_str = ", ".join(op_list) if op_list else ""
                
                print(f"  {addr_str:16s} {mnemonic:8s} {operands_str}")
                instr_count += 1
                if instr_count >= 50:  # Limit output
                    print("  ... (truncated)")
                    break
            
            print()
            print("--- Decompiled Code ---")
            
            # Decompile
            from ghidra.app.decompiler import DecompInterface
            from ghidra.util.task import ConsoleTaskMonitor
            
            decompiler = DecompInterface()
            decompiler.openProgram(program)
            
            monitor = ConsoleTaskMonitor()
            results = decompiler.decompileFunction(function, 30, monitor)
            
            if results.decompileCompleted():
                decomp_source = results.getDecompiledFunction()
                if decomp_source is not None:
                    print(decomp_source.getC())
                else:
                    print("Error: Decompilation returned null")
                    return 1
            else:
                print("Error: Decompilation failed")
                print(results.getErrorMessage())
                return 1
            
            decompiler.dispose()
            return 0
            
    except Exception as e:
        print(f"Error: {e}")
        import traceback
        traceback.print_exc()
        return 1
    finally:
        shutil.rmtree(temp_project_dir, ignore_errors=True)

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: pyghidra_decompile.py <binary> <address>")
        sys.exit(1)
    
    binary = sys.argv[1]
    address = sys.argv[2]
    
    sys.exit(decompile_function(binary, address))
