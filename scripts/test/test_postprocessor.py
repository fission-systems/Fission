#!/usr/bin/env python3
"""
PostProcessor Regex Transformation Tester

This script tests the PostProcessor's regex transformations by
applying them to sample decompiled code and measuring improvements.
"""

import re
import sys
from typing import Tuple, List

class PostProcessorPython:
    """
    Python implementation of the C++ PostProcessor for testing.
    Implements the same regex transformations.
    """
    
    @staticmethod
    def convert_while_to_for(c_code: str) -> str:
        """Convert compound assignments to shorter forms"""
        # i = i + 1 -> i++
        c_code = re.sub(r'(\w+)\s*=\s*\1\s*\+\s*1\s*;', r'\1++;', c_code)
        # i = i - 1 -> i--
        c_code = re.sub(r'(\w+)\s*=\s*\1\s*-\s*1\s*;', r'\1--;', c_code)
        # i = i + N -> i += N
        c_code = re.sub(r'(\w+)\s*=\s*\1\s*\+\s*([^;]+);', r'\1 += \2;', c_code)
        # i = i - N -> i -= N  
        c_code = re.sub(r'(\w+)\s*=\s*\1\s*-\s*([^;]+);', r'\1 -= \2;', c_code)
        # i = i * N -> i *= N
        c_code = re.sub(r'(\w+)\s*=\s*\1\s*\*\s*([^;]+);', r'\1 *= \2;', c_code)
        # i = i | N -> i |= N
        c_code = re.sub(r'(\w+)\s*=\s*\1\s*\|\s*([^;]+);', r'\1 |= \2;', c_code)
        # i = i & N -> i &= N
        c_code = re.sub(r'(\w+)\s*=\s*\1\s*&\s*([^;]+);', r'\1 &= \2;', c_code)
        return c_code
    
    @staticmethod
    def simplify_nested_if(c_code: str) -> str:
        """Simplify if conditions"""
        # Remove double parentheses
        c_code = re.sub(r'\(\(([^()]+)\)\)', r'(\1)', c_code)
        # if (x != 0) -> if (x)
        c_code = re.sub(r'if\s*\(\s*(\w+)\s*!=\s*0\s*\)', r'if (\1)', c_code)
        # if (x == 0) -> if (!x)
        c_code = re.sub(r'if\s*\(\s*(\w+)\s*==\s*0\s*\)', r'if (!\1)', c_code)
        return c_code
    
    @staticmethod
    def improve_variable_names(c_code: str) -> str:
        """Rename return variables to 'result'"""
        match = re.search(r'return\s+(local_\w+)\s*;', c_code)
        if match:
            var_name = match.group(1)
            # Count occurrences
            count = len(re.findall(re.escape(var_name), c_code))
            if 2 <= count <= 10:
                c_code = re.sub(re.escape(var_name), 'result', c_code)
        return c_code
    
    @classmethod
    def process(cls, c_code: str) -> str:
        """Apply all transformations"""
        result = c_code
        result = cls.convert_while_to_for(result)
        result = cls.simplify_nested_if(result)
        result = cls.improve_variable_names(result)
        return result


def test_transformations():
    """Test each transformation with sample code"""
    
    test_cases = [
        # Test case 1: Compound operators
        {
            "name": "Compound Assignment Operators",
            "input": """
int main(void) {
    int i;
    int count;
    
    i = i + 1;
    i = i - 1;
    count = count + 5;
    count = count - 3;
    count = count * 2;
    count = count | 0xFF;
    count = count & 0x0F;
    
    return 0;
}
""",
            "expected_patterns": [
                ("i++;", "i = i + 1"),
                ("i--;", "i = i - 1"),
                ("count += 5;", "count = count + 5"),
                ("count -= 3;", "count = count - 3"),
                ("count *= 2;", "count = count * 2"),
                ("count |= 0xFF;", "count = count | 0xFF"),
                ("count &= 0x0F;", "count = count & 0x0F"),
            ]
        },
        # Test case 2: If simplification
        {
            "name": "If Statement Simplification",
            "input": """
void check(int x, int y) {
    if (x != 0) {
        printf("x is non-zero");
    }
    if (y == 0) {
        printf("y is zero");
    }
    if ((value)) {
        do_something();
    }
}
""",
            "expected_patterns": [
                ("if (x)", "if (x != 0)"),
                ("if (!y)", "if (y == 0)"),
                ("if (value)", "if ((value))"),
            ]
        },
        # Test case 3: Variable renaming
        {
            "name": "Variable Renaming (return value)",
            "input": """
int calculate(int a, int b) {
    int local_c;
    local_c = a + b;
    local_c = local_c * 2;
    return local_c;
}
""",
            "expected_patterns": [
                ("result = a + b;", "return variable renamed"),
                ("return result;", "return uses result"),
            ]
        },
        # Test case 4: Real Fission output sample
        {
            "name": "Real Fission Output Sample (main function)",
            "input": """
// ============================================
// Function: main @ 0x1400016d8
// ============================================

int __cdecl main(int _Argc,char **_Argv,char **_Env)

{
  undefined4 local_68;
  undefined4 local_64;
  undefined4 local_60;
  
  __main();
  puts("=== Nested Loops Test ===\\n");
  local_68 = 1;
  local_64 = 2;
  local_60 = 3;
  find_pair(&local_68,6,0xb);
  print_3d_matrix(2,3,4);
  return 0;
}
""",
            "expected_patterns": []  # Just show the transformation
        },
    ]
    
    print("=" * 70)
    print("  PostProcessor Transformation Tests")
    print("=" * 70)
    print()
    
    total_passed = 0
    total_tests = 0
    
    for tc in test_cases:
        print(f"📌 Test: {tc['name']}")
        print("-" * 50)
        
        input_code = tc["input"]
        output_code = PostProcessorPython.process(input_code)
        
        # Calculate improvement metrics
        input_chars = len(input_code)
        output_chars = len(output_code)
        reduction = input_chars - output_chars
        reduction_pct = (reduction / input_chars * 100) if input_chars > 0 else 0
        
        print(f"  Input:  {input_chars} chars")
        print(f"  Output: {output_chars} chars")
        print(f"  Reduction: {reduction} chars ({reduction_pct:.1f}%)")
        print()
        
        if tc["expected_patterns"]:
            print("  Pattern checks:")
            for pattern, desc in tc["expected_patterns"]:
                total_tests += 1
                if pattern in output_code:
                    print(f"    ✅ '{pattern}' found ({desc})")
                    total_passed += 1
                else:
                    print(f"    ❌ '{pattern}' NOT found ({desc})")
        
        print()
        print("  Output preview (first 400 chars):")
        preview = output_code[:400].replace('\n', '\n    ')
        print(f"    {preview}")
        print()
        print()
    
    print("=" * 70)
    print(f"  Summary: {total_passed}/{total_tests} pattern checks passed")
    print("=" * 70)
    
    return total_passed == total_tests


def test_real_fission_output():
    """Test with actual Fission decompilation output"""
    
    # This would typically read from a file
    sample_output = """
// ============================================
// Function: complex_iteration @ 0x140001678
// ============================================

void complex_iteration(int n)

{
  int local_14;
  int local_10;
  int local_c;
  
  local_c = 0;
  local_10 = 0;
  do {
    if (n <= local_10) {
      printf("Final count: %d\\n",local_c);
      return;
    }
    local_14 = 0;
    while (local_14 < local_10) {
      if (local_14 != 0) {
        if (local_10 != 0) {
          local_c = local_c + 1;
        }
      }
      local_14 = local_14 + 1;
    }
    local_10 = local_10 + 1;
  } while( true );
}
"""
    
    print("=" * 70)
    print("  Real Fission Output Transformation Demo")
    print("=" * 70)
    print()
    print("BEFORE (Original Fission Output):")
    print("-" * 40)
    print(sample_output)
    print()
    
    processed = PostProcessorPython.process(sample_output)
    
    print("AFTER (PostProcessor Applied):")
    print("-" * 40)
    print(processed)
    print()
    
    # Count specific improvements
    improvements = [
        ("++", "Increment operators"),
        ("+=", "Add-assign operators"),
        ("if (local", "Simplified if conditions"),
    ]
    
    print("Improvements detected:")
    for pattern, desc in improvements:
        before_count = sample_output.count(pattern)
        after_count = processed.count(pattern)
        if after_count > before_count:
            print(f"  ✅ {desc}: {before_count} → {after_count}")
    
    return True


if __name__ == "__main__":
    print()
    success1 = test_transformations()
    print()
    success2 = test_real_fission_output()
    print()
    
    if success1 and success2:
        print("🎉 All tests passed!")
        sys.exit(0)
    else:
        print("❌ Some tests failed")
        sys.exit(1)
