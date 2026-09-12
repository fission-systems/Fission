#!/usr/bin/env python3
"""Which functions that look like tests does no runner ever run?

`#[test]` had been written twice on two functions in `fission-dir` -- once
above the doc comment, once below it -- and the two functions that followed
them had none at all. Both compiled. Both passed review. Neither had ever
executed. A compile-surface audit cannot see this: the configuration *was*
built, and only the harness skipped the function.

The rule with teeth is not "a function in a test module without `#[test]`" --
helpers are exactly that, and flagging them would bury the real thing. It is
that a helper is *called*. A function inside a test scope that takes no
arguments, returns nothing, carries no attribute, and is named nowhere else
in its file is not a helper; nothing can reach it.

The duplicated attribute is reported too, because it is the shape that
produces the first defect: a copied function takes the attribute with it and
leaves the original with two.

Usage:  scripts/audit/unrun_tests_scan.py [path ...]      # default: crates/
Exit:   1 if anything was found.
"""

import re
import sys
from pathlib import Path

FN = re.compile(r"^(?P<indent>\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(?P<name>\w+)\s*\((?P<args>[^)]*)\)(?P<rest>[^{;]*)")
ATTR = re.compile(r"^\s*#\[(?P<body>[^\]]*)\]")
TEST_ATTRS = ("test", "bench", "ignore", "should_panic", "rstest", "case", "proptest")


def test_scopes(lines, path):
    """Line ranges (start, end) that a test runner is responsible for."""
    # An integration test file is entirely one.
    if f"{path.parent.name}" == "tests" or "/tests/" in str(path):
        return [(0, len(lines))]
    scopes = []
    for i, line in enumerate(lines):
        if "cfg(test)" not in line:
            continue
        # The module this gates, if it is one.
        j = i + 1
        while j < len(lines) and (ATTR.match(lines[j]) or lines[j].strip().startswith("//")):
            j += 1
        if j >= len(lines) or not re.match(r"\s*(?:pub\s+)?mod\s+\w+", lines[j]):
            continue
        depth = 0
        started = False
        for k in range(j, len(lines)):
            depth += lines[k].count("{") - lines[k].count("}")
            started = started or "{" in lines[k]
            if started and depth <= 0:
                scopes.append((j, k + 1))
                break
    return scopes


def attribute_block(lines, i):
    """Every attribute attached to the item declared at line `i`."""
    attrs = []
    k = i - 1
    while k >= 0:
        stripped = lines[k].strip()
        if not stripped or stripped.startswith("//"):
            k -= 1
            continue
        m = ATTR.match(lines[k])
        if not m:
            break
        attrs.append(m.group("body").strip())
        k -= 1
    return attrs


def scan(path):
    text = path.read_text(errors="replace")
    lines = text.split("\n")
    findings = []
    for start, end in test_scopes(lines, path):
        for i in range(start, end):
            m = FN.match(lines[i])
            if not m:
                continue
            attrs = attribute_block(lines, i)
            test_like = [a for a in attrs if a.split("(")[0].split("::")[-1] in TEST_ATTRS]

            duplicates = [a for a in set(test_like) if test_like.count(a) > 1]
            for a in duplicates:
                findings.append((i + 1, m.group("name"), f"#[{a}] written twice"))

            if test_like or attrs:
                continue
            # A helper takes arguments, or returns something, or is called.
            if m.group("args").strip() or "->" in m.group("rest"):
                continue
            name = m.group("name")
            uses = len(re.findall(rf"\b{re.escape(name)}\b", text)) - 1
            if uses == 0:
                findings.append((i + 1, name, "no #[test], and nothing calls it"))
    return findings


def main(argv):
    roots = [Path(a) for a in argv[1:]] or [Path("crates")]
    total = 0
    for root in roots:
        files = sorted(root.rglob("*.rs")) if root.is_dir() else [root]
        for path in files:
            if "target/" in str(path):
                continue
            for line, name, why in scan(path):
                print(f"{path}:{line}  {name}  -- {why}")
                total += 1
    print(f"\n{total} function(s) no runner runs")
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
