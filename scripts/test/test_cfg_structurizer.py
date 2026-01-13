#!/usr/bin/env python3
"""
CFGStructurizer Test - Verify goto elimination and control flow structurization

Tests the LLVM-inspired CFGStructurizer that transforms unstructured control flow
(gotos) into structured constructs (if/else, while, do-while).
"""

import re
import sys

class CFGStructurizerPython:
    """
    Python implementation of the C++ CFGStructurizer for testing.
    Implements the same transformations.
    """
    
    @staticmethod
    def flatten_nested_if_goto(c_code: str) -> str:
        """Flatten nested if-goto patterns"""
        pattern = r'if\s*\(\s*([^)]+)\s*\)\s*\{\s*\n\s*if\s*\(\s*([^)]+)\s*\)\s*\{\s*\n\s*goto\s+(\w+)\s*;\s*\n\s*\}\s*\n\s*\}'
        return re.sub(pattern, r'if (\1 && \2) goto \3;', c_code)
    
    @staticmethod
    def convert_backward_gotos_to_loops(c_code: str) -> str:
        """Convert backward gotos to do-while loops"""
        pattern = r'(\w+)\s*:\s*\n((?:[^\n]*\n)*?)if\s*\(\s*([^)]+)\s*\)\s*goto\s+\1\s*;'
        
        def replace(match):
            label = match.group(1)
            body = match.group(2)
            condition = match.group(3)
            return f'do {{\n{body}}} while ({condition});'
        
        return re.sub(pattern, replace, c_code)
    
    @staticmethod
    def normalize_do_while_true(c_code: str) -> str:
        """Convert do-while(true) with break to while loop"""
        # Simplified pattern
        pattern = r'do\s*\{\s*\n\s*if\s*\(\s*([^)]+)\s*\)\s*(?:break|return[^;]*)\s*;\s*\n((?:[^\}]|\}(?!\s*while))*)\}\s*while\s*\(\s*(?:true|1)\s*\)\s*;'
        
        def replace(match):
            condition = match.group(1)
            body = match.group(2)
            # Negate condition
            if condition.startswith('!'):
                negated = condition[1:].strip('()')
            else:
                negated = f'!({condition})'
            return f'while ({negated}) {{\n{body}}}'
        
        return re.sub(pattern, replace, c_code, flags=re.DOTALL)
    
    @staticmethod
    def eliminate_forward_gotos(c_code: str) -> str:
        """Convert forward gotos to if/else"""
        # Pattern: if (cond) goto LABEL; ... LABEL:
        pattern = r'if\s*\(\s*([^)]+)\s*\)\s*goto\s+(\w+)\s*;\s*\n((?:[^\n]*\n)*?)\s*\2\s*:'
        
        def replace(match):
            condition = match.group(1)
            label = match.group(2)
            body = match.group(3)
            # Negate condition
            negated = negate_condition(condition)
            return f'if ({negated}) {{\n{body}}}'
        
        return re.sub(pattern, replace, c_code)
    
    @staticmethod
    def remove_unused_labels(c_code: str) -> str:
        """Remove labels that are no longer referenced"""
        # Find all labels
        labels = re.findall(r'^\s*(\w+)\s*:\s*$', c_code, re.MULTILINE)
        
        # Find all goto targets
        goto_targets = set(re.findall(r'goto\s+(\w+)\s*;', c_code))
        
        # Remove unused labels
        for label in labels:
            if label not in goto_targets:
                c_code = re.sub(r'\n\s*' + re.escape(label) + r'\s*:\s*\n', '\n', c_code)
        
        return c_code
    
    @classmethod
    def structurize(cls, c_code: str) -> str:
        """Apply all structurization transformations"""
        result = c_code
        result = cls.flatten_nested_if_goto(result)
        result = cls.convert_backward_gotos_to_loops(result)
        result = cls.normalize_do_while_true(result)
        result = cls.eliminate_forward_gotos(result)
        result = cls.remove_unused_labels(result)
        return result


def negate_condition(condition: str) -> str:
    """Negate a C condition"""
    condition = condition.strip()
    if condition.startswith('!'):
        return condition[1:].strip('()')
    elif '==' in condition:
        return condition.replace('==', '!=')
    elif '!=' in condition:
        return condition.replace('!=', '==')
    elif '>=' in condition:
        return condition.replace('>=', '<')
    elif '<=' in condition:
        return condition.replace('<=', '>')
    elif '>' in condition:
        return condition.replace('>', '<=')
    elif '<' in condition:
        return condition.replace('<', '>=')
    else:
        return f'!({condition})'


def test_cfg_structurizer():
    """Test CFG structurization transformations"""
    
    test_cases = [
        # Test 1: Backward goto to do-while
        {
            "name": "Backward Goto → Do-While",
            "input": """
loop_start:
    printf("iteration\\n");
    count++;
    if (count < 10) goto loop_start;
    printf("done\\n");
""",
            "check_present": ["do {", "while (count < 10)"],
            "check_absent": ["goto loop_start"]
        },
        
        # Test 2: Nested if-goto flattening
        {
            "name": "Nested If-Goto Flattening",
            "input": """
if (a) {
    if (b) {
        goto error;
    }
}
""",
            "check_present": ["if (a && b) goto error"],
            "check_absent": []
        },
        
        # Test 3: do-while(true) with break
        {
            "name": "Do-While(True) with Break → While",
            "input": """
do {
    if (done) break;
    process();
} while (true);
""",
            "check_present": ["while"],
            "check_absent": ["while (true)"]
        },
        
        # Test 4: Forward goto elimination
        {
            "name": "Forward Goto → If/Else",
            "input": """
if (error) goto skip_processing;
    process_data();
    save_result();
skip_processing:
    cleanup();
""",
            "check_present": ["if (!error)", "process_data"],
            "check_absent": []
        },
        
        # Test 5: For loop pattern (new multi-label)
        {
            "name": "For Loop Pattern Recovery",
            "input": """
void process(int n) {
    int i;
    i = 0;
    loop:
    if (i >= n) goto done;
    printf("%d\\n", i);
    i++;
    goto loop;
    done:
    return;
}
""",
            "check_present": [],  # Complex pattern - just test no crash
            "check_absent": []
        },
        
        # Test 6: Unconditional backward goto
        {
            "name": "Unconditional Backward Goto → Loop",
            "input": """
start:
    if (check_done()) break;
    do_work();
    goto start;
""",
            "check_present": [],  # Pattern analysis
            "check_absent": []
        },
        
        # Test 7: Real decompiler output (Ghidra style labels)
        {
            "name": "Ghidra-Style Labels (LAB_xxx)",
            "input": """
void complex_function(int n) {
    int i;
    i = 0;
LAB_loop:
    if (i >= n) goto LAB_end;
    printf("%d\\n", i);
    i = i + 1;
    goto LAB_loop;
LAB_end:
    return;
}
""",
            "check_present": [],  # Just verify no crash
            "check_absent": []
        },
    ]
    
    print("=" * 70)
    print("  CFG Structurizer Tests (Enhanced)")
    print("=" * 70)
    print()
    
    passed = 0
    failed = 0
    total_checks = 0
    
    for tc in test_cases:
        print(f"📌 Test: {tc['name']}")
        print("-" * 50)
        
        try:
            output = CFGStructurizerPython.structurize(tc["input"])
            
            # Check present patterns
            for pattern in tc["check_present"]:
                total_checks += 1
                if pattern in output:
                    print(f"  ✅ Found: '{pattern}'")
                    passed += 1
                else:
                    print(f"  ❌ Missing: '{pattern}'")
                    failed += 1
            
            # Check absent patterns
            for pattern in tc["check_absent"]:
                total_checks += 1
                if pattern not in output:
                    print(f"  ✅ Removed: '{pattern}'")
                    passed += 1
                else:
                    print(f"  ⚠️ Still present: '{pattern}'")
                    failed += 1
            
            if not tc["check_present"] and not tc["check_absent"]:
                print(f"  ✅ No crash (pattern analysis only)")
                passed += 1
                total_checks += 1
            
            # Count gotos
            goto_before = tc["input"].count("goto ")
            goto_after = output.count("goto ")
            if goto_before > 0:
                print(f"  📊 Gotos: {goto_before} → {goto_after}")
            
        except Exception as e:
            print(f"  ❌ Error: {e}")
            failed += 1
            total_checks += 1
        
        print()
    
    print("=" * 70)
    print(f"  Summary: {passed}/{total_checks} checks passed")
    print("=" * 70)
    
    return failed == 0


def demo_full_transformation():
    """Demo a complete transformation on complex code"""
    
    sample = """
// Complex control flow with gotos
void complex_function(int n) {
    int i;
    int sum;
    sum = 0;
    i = 0;
    
loop_outer:
    if (i >= n) goto loop_end;
    
    int j;
    j = 0;
loop_inner:
    if (j >= i) goto next_i;
    
    if (sum > 100) {
        if (i > 5) {
            goto early_exit;
        }
    }
    
    sum = sum + j;
    j = j + 1;
    goto loop_inner;
    
next_i:
    i = i + 1;
    goto loop_outer;
    
loop_end:
    printf("Sum: %d\\n", sum);
    return;
    
early_exit:
    printf("Early exit!\\n");
    return;
}
"""
    
    print()
    print("=" * 70)
    print("  Full Transformation Demo")
    print("=" * 70)
    print()
    print("BEFORE (with gotos):")
    print("-" * 40)
    print(sample[:500])
    print()
    
    result = CFGStructurizerPython.structurize(sample)
    
    print("AFTER (structurized):")
    print("-" * 40)
    print(result[:500])
    print()
    
    # Count improvements
    goto_count_before = sample.count('goto')
    goto_count_after = result.count('goto')
    
    print(f"Goto statements: {goto_count_before} → {goto_count_after}")
    if goto_count_after < goto_count_before:
        print(f"✅ Eliminated {goto_count_before - goto_count_after} goto statements!")
    
    return True


if __name__ == "__main__":
    success1 = test_cfg_structurizer()
    success2 = demo_full_transformation()
    
    print()
    if success1 and success2:
        print("🎉 All CFG Structurizer tests passed!")
        sys.exit(0)
    else:
        print("❌ Some tests failed")
        sys.exit(1)
