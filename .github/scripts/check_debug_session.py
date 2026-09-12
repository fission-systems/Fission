"""Assert a released binary can actually debug.

Every release before this one answered `fission debug` with "Debugger support
is not compiled into this build": the feature existed, no build enabled it,
and no smoke test asked. This reads the JSON report of a session driven
against a Windows executable on Linux -- the thing only the emulator backend
can do -- and fails if any of it is missing.
"""

import json
import sys


def main() -> int:
    report = json.load(sys.stdin)

    results = report.get("results")
    if not isinstance(results, list) or len(results) != 3:
        print(f"expected three command results, got: {report}", file=sys.stderr)
        return 1

    for result in results:
        if result.get("status") != "ok":
            print(f"command failed: {result}", file=sys.stderr)
            return 1

    registers = results[2].get("registers")
    if not isinstance(registers, dict):
        print(f"no registers in the third result: {results[2]}", file=sys.stderr)
        return 1

    pc = registers.get("pc", "")
    if not pc.startswith("0x") or int(pc, 16) == 0:
        print(f"the machine reported no program counter: {registers}", file=sys.stderr)
        return 1

    # A stack pointer the loader set up: zero here would mean the session
    # reported a machine that was never actually started. Under whichever name
    # this machine uses -- `rsp` on x86-64, `esp` on a 32-bit target, `sp`
    # elsewhere -- because naming one was how the register state got this
    # wrong in the first place.
    stack = next(
        (registers[name] for name in ("rsp", "esp", "sp") if name in registers),
        None,
    )
    if stack is None or int(stack, 16) == 0:
        print(f"the machine has no stack: {registers}", file=sys.stderr)
        return 1

    print(f"debug session ok: stepped twice, pc={pc} rsp={stack}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
