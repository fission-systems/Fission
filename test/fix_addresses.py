#!/usr/bin/env python3
"""
Fix addresses by adding ImageBase to RVAs
"""

import os
import subprocess
import sys
from pathlib import Path

def get_imagebase(exe_path):
    """Get ImageBase from PE header"""
    result = subprocess.run(
        ['x86_64-w64-mingw32-objdump', '-x', exe_path],
        capture_output=True,
        text=True
    )
    
    for line in result.stdout.splitlines():
        if line.startswith('ImageBase'):
            return int(line.split()[1], 16)
    
    return None

def extract_functions_from_fission(exe_path):
    """Extract function addresses directly from Fission"""
    env = os.environ.copy()
    lib_path = Path(__file__).parent.parent / 'ghidra_decompiler' / 'build'
    env['DYLD_LIBRARY_PATH'] = f"{lib_path}:{env.get('DYLD_LIBRARY_PATH', '')}"
    
    fission_bin = Path(__file__).parent.parent / 'target' / 'debug' / 'fission_cli'
    if not fission_bin.exists():
        fission_bin = Path(__file__).parent.parent / 'target' / 'release' / 'fission_cli'
    
    result = subprocess.run(
        [str(fission_bin), str(exe_path), '-l'],
        capture_output=True,
        text=True,
        env=env
    )
    
    addresses = []
    for line in result.stdout.splitlines():
        line = line.strip()
        if not line or not line.startswith('0x'):
            continue
        
        # Parse line: "0x000140001450       0  find_pair"
        parts = line.split()
        if len(parts) >= 3:
            addr = parts[0]
            name = parts[2]
            
            # Skip runtime functions
            skip_patterns = [
                '__mingw', '__gcc', '__do_global', '__main',
                'CRTStartup', '_setargv', '__dyn_tls', '__tlregdtor',
                '_matherr', 'atexit', '__report_error', 'mark_section_writable',
                '_pei386_runtime_relocator', '__mingwthr', '_ValidateImageBase',
                '_GetPEImageBase', '__mingw_raise_matherr', '__mingw_setusermatherr',
                '_gnu_exception_handler'
            ]
            
            if any(pattern in name for pattern in skip_patterns):
                continue
            
            addresses.append(addr)
    
    return addresses

def main():
    bin_dir = Path('bin_x64')
    addr_dir = Path('addresses')
    addr_dir.mkdir(exist_ok=True)
    
    test_files = [
        ('nested_loops_x64.exe', 'nested_loops_addrs.txt'),
        ('switch_case_x64.exe', 'switch_case_addrs.txt'),
        ('recursion_x64.exe', 'recursion_addrs.txt'),
        ('complex_structs_x64.exe', 'complex_structs_addrs.txt'),
        ('function_pointers_x64.exe', 'function_pointers_addrs.txt'),
        ('virtual_functions_x64.exe', 'virtual_functions_addrs.txt'),
    ]
    
    for exe_name, addr_file in test_files:
        exe_path = bin_dir / exe_name
        if not exe_path.exists():
            print(f"Warning: {exe_path} not found")
            continue
        
        print(f"Processing: {exe_name}")
        
        addresses = extract_functions_from_fission(exe_path)
        
        output_path = addr_dir / addr_file
        with open(output_path, 'w') as f:
            for addr in addresses:
                f.write(addr + '\n')
        
        print(f"  Extracted {len(addresses)} functions to {output_path}")
    
    print("\n✓ Address extraction complete")

if __name__ == '__main__':
    main()
