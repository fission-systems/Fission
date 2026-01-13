#!/usr/bin/env python3
"""
Real Binary PostProcessor Test

Tests the PostProcessor and CFGStructurizer on actual Fission decompilation output.
"""

import subprocess
import sys
import os
import re

def run_fission_decompile(binary: str, address: str) -> str:
    """Run Fission decompilation and return the output"""
    cmd = [
        'python3', 'scripts/compare/compare_decompilers_v2.py',
        binary, address, f'/tmp/pp_test_{address.replace("0x", "")}'
    ]
    
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=120, cwd='/Users/sjkim1127/Fission')
    
    output_file = f'/tmp/pp_test_{address.replace("0x", "")}_fission_decomp.txt'
    if os.path.exists(output_file):
        with open(output_file, 'r') as f:
            return f.read()
    return ""

def analyze_postprocessor_effects(code: str) -> dict:
    """Analyze what PostProcessor optimizations are visible in the code"""
    analysis = {
        "has_increment_operators": bool(re.search(r'\w+\+\+', code)),
        "has_decrement_operators": bool(re.search(r'\w+--', code)),
        "has_compound_assignment": bool(re.search(r'\w+\s*[+\-*/|&]=\s*\w+', code)),
        "has_simplified_conditions": not bool(re.search(r'if\s*\([^)]+\s*!=\s*0\s*\)', code)),
        "has_gotos": bool(re.search(r'\bgoto\s+\w+', code)),
        "has_labels": bool(re.search(r'^\s*\w+\s*:\s*$', code, re.MULTILINE)),
        "has_do_while": bool(re.search(r'do\s*\{', code)),
        "has_while_loop": bool(re.search(r'while\s*\([^)]+\)\s*\{', code)),
        "has_for_loop": bool(re.search(r'for\s*\(', code)),
        "line_count": len(code.strip().split('\n')),
        "char_count": len(code),
    }
    return analysis

def test_real_binary():
    """Test PostProcessor on real binary functions"""
    
    binary = 'examples/binaries/bin_x64/nested_loops_x64.exe'
    
    # Test multiple functions
    test_cases = [
        ('0x140001400', 'mainCRTStartup'),
        ('0x1400016d8', 'main'),
        ('0x140001000', '__mingw_invalidParameterHandler'),
    ]
    
    print("=" * 70)
    print("  Real Binary PostProcessor Test")
    print("=" * 70)
    print(f"\n  Binary: {binary}\n")
    
    results = []
    
    for address, expected_name in test_cases:
        print(f"📌 Testing {address} ({expected_name})")
        print("-" * 50)
        
        try:
            code = run_fission_decompile(binary, address)
            
            if not code:
                print("  ❌ No output generated")
                continue
            
            analysis = analyze_postprocessor_effects(code)
            
            print(f"  📊 Lines: {analysis['line_count']}, Chars: {analysis['char_count']}")
            print(f"  Optimizations detected:")
            
            if analysis['has_increment_operators']:
                print(f"    ✅ Increment operators (++)")
            if analysis['has_compound_assignment']:
                print(f"    ✅ Compound assignment (+=, -=, etc)")
            if analysis['has_simplified_conditions']:
                print(f"    ✅ Simplified conditions (no x != 0)")
            if not analysis['has_gotos']:
                print(f"    ✅ No gotos")
            else:
                print(f"    ⚠️ Contains gotos")
            if analysis['has_for_loop']:
                print(f"    ✅ For loops")
            if analysis['has_while_loop']:
                print(f"    ✅ While loops")
            if analysis['has_do_while']:
                print(f"    ✅ Do-while loops")
            
            results.append({
                'address': address,
                'name': expected_name,
                'analysis': analysis
            })
            
            # Show first few lines
            print("\n  Preview:")
            for line in code.strip().split('\n')[4:10]:
                print(f"    {line}")
                
        except Exception as e:
            print(f"  ❌ Error: {e}")
        
        print()
    
    # Summary
    print("=" * 70)
    print("  Summary")
    print("=" * 70)
    
    total_inc = sum(1 for r in results if r['analysis']['has_increment_operators'])
    total_compound = sum(1 for r in results if r['analysis']['has_compound_assignment'])
    total_no_goto = sum(1 for r in results if not r['analysis']['has_gotos'])
    
    print(f"  Functions tested: {len(results)}")
    print(f"  With increment operators: {total_inc}")
    print(f"  With compound assignments: {total_compound}")
    print(f"  Without gotos: {total_no_goto}")
    
    return True


if __name__ == "__main__":
    success = test_real_binary()
    sys.exit(0 if success else 1)
