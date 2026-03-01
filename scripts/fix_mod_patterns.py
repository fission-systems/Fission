#!/usr/bin/env python3
"""Fix mod function expected patterns from & to % in benchmark suites."""
import re
import glob

suite_files = glob.glob('scripts/benchmark/suites/suite_*.yaml')
for sf in suite_files:
    with open(sf) as f:
        lines = f.read().split('\n')
    modified = False
    i = 0
    while i < len(lines):
        line = lines[i].strip()
        if re.match(r"name:\s+mod_(2|4|8|16|256)\s*$", line):
            for j in range(i+1, min(i+5, len(lines))):
                stripped = lines[j].strip()
                if stripped == "- '&'":
                    lines[j] = lines[j].replace("- '&'", "- '%'")
                    modified = True
                    break
        i += 1
    if modified:
        with open(sf, 'w') as f:
            f.write('\n'.join(lines))
        print(f'Fixed: {sf}')
    else:
        print(f'No changes: {sf}')
