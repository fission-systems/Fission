#!/usr/bin/env python3
"""Compare two benchmark result sets to find regressions/improvements."""
import json, sys

v2_path = sys.argv[1]
v3_path = sys.argv[2]

with open(v2_path) as f:
    v2 = json.load(f)
with open(v3_path) as f:
    v3 = json.load(f)

v2map = {(r['function'], r['opt_level']): r for r in v2}
v3map = {(r['function'], r['opt_level']): r for r in v3}

regressions = []
improvements = []
for key in v2map:
    r2 = v2map[key]['fission_checklist']['ratio']
    r3 = v3map.get(key, {}).get('fission_checklist', {}).get('ratio', 0)
    if r3 < r2:
        regressions.append((key, r2, r3))
    elif r3 > r2:
        improvements.append((key, r2, r3))

print(f'Regressions: {len(regressions)}, Improvements: {len(improvements)}')
for key, r2, r3 in sorted(regressions):
    v3r = v3map[key]
    missing = [p for p, v in v3r['fission_checklist']['patterns'].items() if not v]
    print(f'  REGRESS {key[0]} [{key[1]}]: {r2:.0%} -> {r3:.0%}  missing={missing}')
    # Show code snippet
    code = v3r.get('fission_code', '')[:200]
    print(f'    code: {repr(code)}')
print()
for key, r2, r3 in sorted(improvements):
    print(f'  IMPROVE {key[0]} [{key[1]}]: {r2:.0%} -> {r3:.0%}')
