#!/usr/bin/env python3
import os
import re

def extract_signatures(file_path):
    print(f"[*] Parsing ntzwapi header: {file_path}")
    if not os.path.exists(file_path):
        print(f"[!] Error: File does not exist: {file_path}")
        return []
    
    with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
        content = f.read()

    # Matches NTSYSCALLAPI return_type NTAPI ZwName(params);
    pattern = re.compile(
        r'NTSYSCALLAPI\s+([A-Za-z0-9_]+)\s+NTAPI\s+(Zw[A-Za-z0-9_]+)\s*\(\s*(.*?)\s*\)\s*;', 
        re.DOTALL
    )
    
    signatures = []
    matches = list(pattern.finditer(content))
    
    for m in matches:
        return_type = m.group(1).strip()
        func_name = m.group(2).strip()
        raw_params = m.group(3).strip()
        
        # Parse parameters while respecting balanced parentheses for SAL annotations
        parsed_params = []
        if raw_params and raw_params.upper() != "VOID":
            param_lines = []
            current_param = []
            paren_depth = 0
            for char in raw_params:
                if char == '(':
                    paren_depth += 1
                elif char == ')':
                    paren_depth -= 1
                
                if char == ',' and paren_depth == 0:
                    param_lines.append("".join(current_param).strip())
                    current_param = []
                else:
                    current_param.append(char)
            if current_param:
                param_lines.append("".join(current_param).strip())
                
            for param in param_lines:
                param = param.strip()
                if not param:
                    continue
                
                # Strip SAL annotations like _Out_writes_bytes_opt_(x) or _In_
                param = re.sub(r'_[A-Za-z0-9_]+\([^\)]*\)', '', param)
                param = re.sub(r'_[A-Za-z0-9_]+', '', param)
                param = param.strip()
                
                # Clean asterisks and duplicate spaces
                param = re.sub(r'\s*\*\s*', '*', param)
                param = re.sub(r'\s+', ' ', param)
                
                # Split type and name
                parts = param.split(' ')
                if len(parts) >= 2:
                    name = parts[-1].strip()
                    param_type = " ".join(parts[:-1]).strip()
                    
                    # Fix pointer asterisks attached to parameter name
                    if name.startswith('*'):
                        stars = len(name) - len(name.lstrip('*'))
                        param_type += '*' * stars
                        name = name.lstrip('*')
                        
                    parsed_params.append(f"{name}:{param_type}")
                elif len(parts) == 1 and parts[0]:
                    parsed_params.append(f"param:{parts[0]}")

        params_str = ",".join(parsed_params)
        
        # 1. Add Zw version
        signatures.append((func_name, return_type, params_str))
        
        # 2. Add Nt version (shares identical signature)
        nt_name = "Nt" + func_name[2:]
        signatures.append((nt_name, return_type, params_str))
        
    print(f"[✓] Extracted {len(signatures)} Native API prototypes (Nt and Zw variants)")
    return signatures

def main():
    print("[*] Starting System Informer phnt Native API Extractor...")
    
    workspace_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
    ntzwapi_path = os.path.join(workspace_root, "vendor", "systeminformer-3.2.25011.2103", "phnt", "include", "ntzwapi.h")
    
    extracted = extract_signatures(ntzwapi_path)
    if not extracted:
        print("[!] Error: No Native APIs were extracted. Check paths.")
        return

    # Load existing win_api_signatures.txt to merge and deduplicate
    signatures_path = os.path.join(workspace_root, "utils", "signatures", "typeinfo", "win32", "win_api_signatures.txt")
    
    existing_sigs = {}
    if os.path.exists(signatures_path):
        print(f"[*] Loading existing signatures from {signatures_path}...")
        with open(signatures_path, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                line_trimmed = line.strip()
                if not line_trimmed or line_trimmed.startswith('#'):
                    continue
                parts = line_trimmed.split('|')
                if len(parts) >= 2:
                    name = parts[0].strip()
                    existing_sigs[name] = line_trimmed

    # Merge and update
    merged_count = 0
    added_count = 0
    
    for func_name, return_type, params_str in extracted:
        new_line = f"{func_name}|{return_type}|{params_str}"
        if func_name in existing_sigs:
            # Overwrite/update if it was a simpler mock or placeholder
            existing_sigs[func_name] = new_line
            merged_count += 1
        else:
            existing_sigs[func_name] = new_line
            added_count += 1

    # Write back to file in sorted order
    # Header comment
    header = "# name|return_type|param_name:type,param_name:type\n"
    
    sorted_names = sorted(existing_sigs.keys())
    with open(signatures_path, 'w', encoding='utf-8') as f:
        f.write(header)
        for name in sorted_names:
            f.write(existing_sigs[name] + "\n")
            
    print(f"[✓] Successfully merged Native APIs!")
    print(f"    Updated: {merged_count} existing signatures")
    print(f"    Added:   {added_count} new signatures")
    print(f"    Total signatures in database: {len(existing_sigs)}")

if __name__ == "__main__":
    main()
