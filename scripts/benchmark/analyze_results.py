#!/usr/bin/env python3
"""Analyze benchmark results to find improvement areas."""
import json, collections, sys, glob

dirs = sorted(glob.glob("benchmark_results/macos_arm64_*"))
if not dirs:
    print("No results found"); sys.exit(1)
arm64_dir = dirs[-1]
results = json.load(open(f"{arm64_dir}/results.json"))

# 1. Failed patterns (O0 only — no optimizer interference)
failed_patterns = collections.Counter()
failed_by_cat = collections.defaultdict(lambda: collections.Counter())
low_funcs = []

for r in results:
    if r.get("opt_level") != "O0":
        continue
    chk = r.get("fission_checklist", {})
    if not chk:
        continue
    patterns = chk.get("patterns", {})
    cat = r.get("category", "?")
    for pat, hit in patterns.items():
        if not hit:
            failed_patterns[pat] += 1
            failed_by_cat[cat][pat] += 1
    if chk.get("ratio", 1) < 1.0:
        low_funcs.append((chk["ratio"], r["function"], cat,
                         [p for p, h in patterns.items() if not h]))

print("=== Most Failed Patterns (O0) ===")
for pat, cnt in failed_patterns.most_common(25):
    print(f"  {cnt:3d}x  {pat}")

print("\n=== Failed Patterns by Category ===")
for cat in sorted(failed_by_cat):
    print(f"  [{cat}]")
    for pat, cnt in failed_by_cat[cat].most_common(8):
        print(f"    {cnt:2d}x  {pat}")

print("\n=== Worst Functions (O0) ===")
for ratio, func, cat, missing in sorted(low_funcs)[:20]:
    print(f"  {ratio:5.1%}  {func:30s} [{cat:10s}]  missing: {missing}")

# 2. Sample failing output for worst arithmetic functions
print("\n=== Sample Fission Output for Worst Functions ===")
for r in results:
    if r.get("opt_level") != "O0":
        continue
    if r["function"] in ("divide_by_3", "mod_2", "multiply_by_2", "signed_div_3",
                         "murmur3_mix", "next_token", "rect_area"):
        chk = r.get("fission_checklist", {})
        ratio = chk.get("ratio", 1) if chk else 1
        code = r.get("fission_code", "")[:400]
        print(f"\n--- {r['function']} [{r['category']}] chk={ratio:.0%} ---")
        print(code)
