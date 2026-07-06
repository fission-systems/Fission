import json
import subprocess
import sys

def run_check():
    result = subprocess.run(
        ["cargo", "check", "-p", "fission-pcode", "--message-format=json"],
        capture_output=True,
        text=True
    )
    errors = []
    for line in result.stdout.splitlines():
        try:
            msg = json.loads(line)
            if msg.get("reason") == "compiler-message" and msg["message"]["level"] == "error":
                errors.append(msg["message"])
        except:
            pass
    return errors

def patch_file(file_path, line_idx, col_idx, is_pattern):
    with open(file_path, "r") as f:
        lines = f.readlines()
    
    line = lines[line_idx]
    # We need to insert `address: None, ` or `.. `
    # Since rustfmt varies, usually it's `HirStmt::Assign { lhs, rhs }`
    # We can just append `..` if it's a pattern, or `address: None` if it's a struct expression.
    # Actually, if we just find the closing brace `}` of the struct and insert before it.
    pass

