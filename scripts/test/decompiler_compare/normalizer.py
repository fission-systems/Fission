import re

def normalize_c_code(code: str, aggressive: bool = False) -> str:
    """
    Normalize C code to minimize non-structural diffs (e.g. whitespace, 
    specific Ghidra variable names vs Fission variable names, etc).
    """
    if not code:
        return ""

    # Basic normalization
    lines = code.splitlines()
    norm_lines = []
    
    for line in lines:
        # Strip trailing/leading spaces
        line = line.strip()
        
        if not line:
            continue
            
        # Optional: remove single line comments
        line = re.sub(r'//.*', '', line).strip()
        if not line:
            continue
            
        norm_lines.append(line)
        
    normalized_code = "\n".join(norm_lines)
    
    if aggressive:
        # Aggressive normalization: Strip all variable names to generic
        # e.g., local_x or uVarX -> VAR
        normalized_code = re.sub(r'\blocal_[0-9a-zA-Z_]+\b', 'VAR', normalized_code)
        normalized_code = re.sub(r'\b[ui]Var[0-9]+\b', 'VAR', normalized_code)
        normalized_code = re.sub(r'\bpcVar[0-9]+\b', 'VAR', normalized_code)
        normalized_code = re.sub(r'\bpuVar[0-9]+\b', 'VAR', normalized_code)
        normalized_code = re.sub(r'\bpiVar[0-9]+\b', 'VAR', normalized_code)
        
        # Normalize simple hexadecimal literals to lower/upper case uniformly
        # Pointers and types could also be stripped or matched
        # (void *) -> (PVOID)
        normalized_code = normalized_code.replace("void *", "PVOID")
        
    return normalized_code
