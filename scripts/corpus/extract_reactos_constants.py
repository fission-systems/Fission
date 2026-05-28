#!/usr/bin/env python3
import os
import re
import json

def clean_and_parse_value(value_str, raw_constants):
    value_str = value_str.strip()
    
    # Strip comments
    value_str = re.sub(r'/\*.*?\*/', '', value_str)
    value_str = re.sub(r'//.*$', '', value_str)
    value_str = value_str.strip()

    # Recursively remove castings and wrapper macros
    while True:
        prev = value_str
        # Remove typecasts
        value_str = re.sub(r'\(\s*(NTSTATUS|HRESULT|DWORD|LONG|ULONG|USHORT|SHORT|BYTE|CHAR|WCHAR)\s*\)', '', value_str, flags=re.IGNORECASE)
        # Remove wrappers
        value_str = re.sub(r'_HRESULT_TYPEDEF_\s*\(\s*(.*?)\s*\)', r'\1', value_str, flags=re.IGNORECASE)
        # Remove outer parentheses
        if value_str.startswith('(') and value_str.endswith(')'):
            value_str = value_str[1:-1].strip()
        if value_str == prev:
            break

    # Strip standard numeric suffixes (L, U, UL, LL, ULL)
    value_str = re.sub(r'(?i)(?<=\d|[\da-f])[ul]{1,3}$', '', value_str)
    
    # Attempt parsing as integer
    try:
        return int(value_str, 0)
    except ValueError:
        # If it's a known identifier in our dictionary, return it as reference
        if value_str in raw_constants:
            return value_str
        # Otherwise, check if it's a hex or simple numeric without prefix
        try:
            return int(value_str)
        except ValueError:
            return value_str

def parse_header(file_path):
    print(f"[*] Parsing header: {file_path}")
    if not os.path.exists(file_path):
        print(f"[!] Error: File does not exist: {file_path}")
        return {}

    # Matches: #define SYMBOL VALUE or #define SYMBOL (VALUE)
    define_pattern = re.compile(r'^\s*#\s*define\s+([A-Za-z0-9_]+)\s+(.+)$')
    
    raw_constants = {}
    with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
        for line in f:
            match = define_pattern.match(line)
            if match:
                symbol = match.group(1)
                value_raw = match.group(2).strip()
                
                # Exclude macros that have parameters (e.g. #define FOO(x) ...)
                if '(' in symbol:
                    continue
                
                # Check target prefix to filter out non-status/non-error constants
                # This ensures we get high-quality error code mappings
                is_target_prefix = any(symbol.startswith(pref) for pref in [
                    "STATUS_", "ERROR_", "E_", "S_", "RPC_S_", "RPC_X_",
                    "SEC_E_", "CRYPT_E_", "DNS_ERROR_", "CO_E_", "DISP_E_",
                    "TYPE_E_", "STG_E_"
                ])
                if not is_target_prefix:
                    continue

                parsed_val = clean_and_parse_value(value_raw, raw_constants)
                raw_constants[symbol] = parsed_val

    # Resolve references recursively
    resolved = {}
    def resolve_val(v, visited):
        if isinstance(v, int):
            return v
        if isinstance(v, str):
            if v in visited:
                return None
            visited.add(v)
            if v in raw_constants:
                return resolve_val(raw_constants[v], visited)
        return None

    for name, raw_v in raw_constants.items():
        val = resolve_val(raw_v, set())
        if val is not None:
            resolved[name] = val

    print(f"[✓] Extracted {len(resolved)} constants from {os.path.basename(file_path)}")
    return resolved

def main():
    print("[*] Starting ReactOS Win32 Constants Extractor...")
    
    workspace_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
    sdk_include = os.path.join(workspace_root, "vendor", "reactos-0.4.15-release", "sdk", "include")
    
    ntstatus_path = os.path.join(sdk_include, "psdk", "ntstatus.h")
    winerror_path = os.path.join(sdk_include, "psdk", "winerror.h")
    
    # Parse files
    constants = {}
    constants.update(parse_header(ntstatus_path))
    constants.update(parse_header(winerror_path))
    
    if not constants:
        print("[!] Error: No constants were extracted. Check paths.")
        return

    # Structure target mapping
    # We map both decimal string and hex string to the symbolic name for maximum lookup speed in Rust
    output_map = {}
    for name, val in constants.items():
        # Represent signed integer properly or keep unsigned representation
        # NTSTATUS is technically a signed 32-bit integer, but often printed as unsigned hex or signed decimal.
        # We can store:
        # 1. Decimal string: str(val)
        # 2. Signed decimal string (if negative or large unsigned): str(val - (1 << 32)) if val >= 0x80000000 else str(val)
        # 3. Lowercase hex string: "0x{:08x}".format(val)
        
        dec_str = str(val)
        hex_str = "0x{:08x}".format(val)
        
        output_map[dec_str] = name
        output_map[hex_str] = name
        
        # If val is negative (e.g. standard HRESULT/NTSTATUS), also register signed representation
        if val >= 0x80000000:
            signed_val = val - (1 << 32)
            output_map[str(signed_val)] = name

    # Ensure target directory exists
    output_dir = os.path.join(workspace_root, "utils", "signatures", "typeinfo", "win32")
    os.makedirs(output_dir, exist_ok=True)
    
    output_path = os.path.join(output_dir, "win_api_constants.json")
    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(output_map, f, indent=2, sort_keys=True)
        
    print(f"[✓] Successfully wrote {len(output_map)} constant mappings to {output_path}")

if __name__ == "__main__":
    main()
