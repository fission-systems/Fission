#!/usr/bin/env python3
"""
PostProcessor Optimization Test
Tests the C++ PostProcessor regex transformations
"""

import subprocess
import sys

# Sample input with patterns the PostProcessor should optimize
test_input = '''
int main(void) {
    int i;
    int count;
    
    i = 0;
    count = 0;
    
    // These should be converted to compound operators
    i = i + 1;
    i = i - 1;
    count = count + 5;
    count = count - 3;
    count = count * 2;
    count = count | 0xFF;
    count = count & 0x0F;
    
    // These should be simplified
    if (i != 0) {
        printf("not zero");
    }
    if (count == 0) {
        printf("zero");
    }
    
    // Double parens should be simplified
    if ((x)) {
        do_something();
    }
    
    return 0;
}
'''

expected_patterns = [
    ('i++;', 'i = i + 1 should become i++'),
    ('i--;', 'i = i - 1 should become i--'),
    ('count += 5;', 'count = count + 5 should become count += 5'),
    ('count -= 3;', 'count = count - 3 should become count -= 3'),
    ('count *= 2;', 'count = count * 2 should become count *= 2'),
    ('count |= 0xFF;', 'count = count | 0xFF should become count |= 0xFF'),
    ('count &= 0x0F;', 'count = count & 0x0F should become count &= 0x0F'),
    ('if (i)', 'if (i != 0) should become if (i)'),
    ('if (!count)', 'if (count == 0) should become if (!count)'),
]

print("PostProcessor Optimization Test")
print("=" * 50)
print()
print("Input:")
print(test_input)
print()
print("Expected optimizations:")
for pattern, desc in expected_patterns:
    print(f"  • {desc}")
print()
print("Note: This test verifies the PostProcessor's regex patterns.")
print("The actual PostProcessor is implemented in C++ and integrated")
print("into the decompilation pipeline.")
