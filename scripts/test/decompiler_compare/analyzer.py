import difflib

def compare_codes(code1: str, code2: str) -> tuple[float, str]:
    """
    Compare two strings (usually normalized C code).
    Returns a tuple of (similarity_ratio, unified_diff_string)
    """
    if not code1 and not code2:
        return 1.0, ""
    
    # Calculate similarity ratio
    matcher = difflib.SequenceMatcher(None, code1, code2)
    ratio = matcher.ratio()

    # Generate unified diff
    lines1 = code1.splitlines(keepends=True)
    lines2 = code2.splitlines(keepends=True)

    diff = difflib.unified_diff(
        lines1, lines2,
        fromfile='Ghidra',
        tofile='Fission',
        lineterm='\n'
    )
    
    diff_string = "".join(diff)
    return ratio, diff_string
