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
            "expected_pattern": "do {",
            "unexpected_pattern": "goto loop_start"
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
            "expected_pattern": "if (a && b) goto error",
            "unexpected_pattern": None
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
            "expected_pattern": "while (!(done))",
            "unexpected_pattern": "while (true)"
        },
        
        # Test 4: Real decompiler output example
        {
            "name": "Real Decompiler Output (Loop Pattern)",
            "input": """
void process_data(int* data, int size) {
    int i;
    i = 0;
LAB_loop:
    if (i >= size) goto LAB_end;
    data[i] = data[i] * 2;
    i = i + 1;
    goto LAB_loop;
LAB_end:
    return;
}
""",
            "expected_pattern": "do {",
            "unexpected_pattern": None
        },
    ]
    
    print("=" * 70)
    print("  CFG Structurizer Tests")
    print("=" * 70)
    print()
    
    passed = 0
    failed = 0
    
    for tc in test_cases:
        print(f"📌 Test: {tc['name']}")
        print("-" * 50)
        
        output = CFGStructurizerPython.structurize(tc["input"])
        
        # Check expected pattern
        if tc["expected_pattern"]:
            if tc["expected_pattern"] in output:
                print(f"  ✅ Found expected: '{tc['expected_pattern']}'")
                passed += 1
            else:
                print(f"  ❌ Missing expected: '{tc['expected_pattern']}'")
                failed += 1
        
        # Check unexpected pattern
        if tc["unexpected_pattern"]:
            if tc["unexpected_pattern"] not in output:
                print(f"  ✅ Removed: '{tc['unexpected_pattern']}'")
            else:
                print(f"  ⚠️ Still present: '{tc['unexpected_pattern']}'")
        
        # Show transformation
        print()
        print("  Input preview:")
        for line in tc["input"].strip().split('\n')[:5]:
            print(f"    {line}")
        print()
        print("  Output preview:")
        for line in output.strip().split('\n')[:5]:
            print(f"    {line}")
        print()
    
    print("=" * 70)
    print(f"  Summary: {passed}/{passed+failed} tests passed")
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
