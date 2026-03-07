import os
from pathlib import Path

def decompile_with_ghidra(binary_path: str, address_or_name: str, ghidra_install_dir: str = None) -> str:
    """
    Run PyGhidra in headless mode to decompile the given binary at a specific address.
    This requires `pyghidra` (e.g. `pip install pyghidra`).
    """
    if ghidra_install_dir:
        os.environ["GHIDRA_INSTALL_DIR"] = ghidra_install_dir

    try:
        import pyghidra
    except ImportError:
        print("pyghidra is not installed. Please install it using `pip install pyghidra`.")
        return ""

    try:
        # Start PyGhidra
        pyghidra.start()
        
        # We need to import Ghidra classes AFTER starting PyGhidra
        from ghidra.app.decompiler import DecompInterface
        from ghidra.util.task import ConsoleTaskMonitor
        from ghidra.program.model.address import AddressFormatException

        print(f"[*] PyGhidra analyzing {binary_path}...")
        with pyghidra.open_program(binary_path, analyze=True) as flat_api:
            program = flat_api.getCurrentProgram()
            monitor = ConsoleTaskMonitor()
            
            # Find the function
            function_manager = program.getFunctionManager()
            target_func = None
            
            # Try to parse as address first
            try:
                addr_factory = program.getAddressFactory()
                # Remove 0x if present
                clean_addr = address_or_name.lower().replace("0x", "")
                addr = addr_factory.getAddress(clean_addr)
                if addr:
                    target_func = function_manager.getFunctionContaining(addr)
                    if not target_func:
                        target_func = function_manager.getFunctionAt(addr)
            except Exception:
                pass
            
            # If not found by address, try by name
            if not target_func:
                functions = list(function_manager.getFunctions(True))
                for func in functions:
                    if func.getName() == address_or_name or func.getName() == f"_{address_or_name}":
                        target_func = func
                        break

            if not target_func:
                print(f"[-] Could not find function {address_or_name} in {binary_path} via Ghidra.")
                return ""

            print(f"[*] Found function: {target_func.getName()} at {target_func.getEntryPoint()}")
            
            # Initialize Decompiler
            decomp_interface = DecompInterface()
            decomp_interface.openProgram(program)
            
            # Decompile
            results = decomp_interface.decompileFunction(target_func, 60, monitor)
            if results and results.decompileCompleted():
                c_code = results.getDecompiledFunction().getC()
                return c_code
            else:
                print(f"[-] Failed to decompile {target_func.getName()} using Ghidra.")
                if results:
                    print(results.getErrorMessage())
                return ""

    except Exception as e:
        print(f"[-] PyGhidra Error: {e}")
        return ""
