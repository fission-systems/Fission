import os
import time

def run_ghidra_batch(binary_path: str, functions: list[tuple[str, str]], ghidra_install_dir: str = None):
    """
    Runs PyGhidra and batch decompiles the given list of functions.
    Measures the initial engine load time and the individual function decompilation time.
    """
    if ghidra_install_dir:
        os.environ["GHIDRA_INSTALL_DIR"] = ghidra_install_dir

    try:
        import pyghidra
    except ImportError:
        print("[-] pyghidra is not installed. Please install it using `pip install pyghidra`.")
        return 0, []

    times = []
    
    # 1. Measure Engine Init Time (Program load + analysis)
    load_start = time.perf_counter()
    pyghidra.start()
    
    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor
    
    try:
        with pyghidra.open_program(binary_path, analyze=True) as flat_api:
            program = flat_api.getCurrentProgram()
            monitor = ConsoleTaskMonitor()
            
            decomp_interface = DecompInterface()
            decomp_interface.openProgram(program)
            
            engine_load_time = time.perf_counter() - load_start
            
            # 2. Sequential function decompilation
            function_manager = program.getFunctionManager()
            addr_factory = program.getAddressFactory()
            
            for addr_str, name in functions:
                func_start = time.perf_counter()
                success = False
                
                try:
                    # Parse address
                    clean_addr = addr_str.lower().replace("0x", "")
                    addr = addr_factory.getAddress(clean_addr)
                    
                    target_func = None
                    if addr:
                        target_func = function_manager.getFunctionContaining(addr)
                        if not target_func:
                            target_func = function_manager.getFunctionAt(addr)
                    
                    # Fallback to name search
                    if not target_func:
                        for f in list(function_manager.getFunctions(True)):
                            if f.getName() == name or f.getName() == f"_{name}":
                                target_func = f
                                break
                                
                    if target_func:
                        results = decomp_interface.decompileFunction(target_func, 60, monitor)
                        if results and results.decompileCompleted() and results.getDecompiledFunction():
                            success = True
                except Exception as e:
                    pass
                
                elapsed = time.perf_counter() - func_start
                times.append({
                    "address": addr_str,
                    "name": name,
                    "time_s": elapsed,
                    "success": success
                })
                
        return engine_load_time, times
        
    except Exception as e:
        print(f"[-] PyGhidra Error: {e}")
        return 0, []
