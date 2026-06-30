import os
import glob

for filename in glob.glob(".github/workflows/*.yml"):
    with open(filename, "r") as f:
        lines = f.readlines()
    
    new_lines = []
    i = 0
    while i < len(lines):
        line = lines[i]
        new_lines.append(line)
        if "uses: actions/checkout" in line:
            # find where the checkout block ends
            j = i + 1
            while j < len(lines) and (lines[j].strip() == "" or lines[j].startswith(" ") and not lines[j].strip().startswith("-")):
                new_lines.append(lines[j])
                j += 1
            
            # Check if setup-utils is already there
            if j < len(lines) and "Setup Utils" not in lines[j] and "setup-utils" not in "\n".join(lines):
                # indent the same as checkout
                indent = len(line) - len(line.lstrip())
                prefix = " " * indent
                new_lines.append("\n")
                new_lines.append(f"{prefix}- name: Setup Utils\n")
                new_lines.append(f"{prefix}  uses: ./.github/actions/setup-utils\n")
            i = j - 1
        i += 1
        
    with open(filename, "w") as f:
        f.writelines(new_lines)

