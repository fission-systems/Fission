import os
import re

roots = ["crates/fission-pcode/src", "crates/fission-static/src", "crates/fission-decompiler/src"]

def process_file(path):
    with open(path, 'r') as f:
        content = f.read()

    original = content

    # Replace absolute path from pcode
    content = content.replace("fission_pcode::nir::CallingConvention", "fission_core::CallingConvention")
    content = content.replace("crate::nir::support::CallingConvention", "fission_core::CallingConvention")
    content = content.replace("crate::nir::CallingConvention", "fission_core::CallingConvention")
    
    # Imports inside pcode files
    content = re.sub(r'use crate::nir::support::CallingConvention;', 'use fission_core::CallingConvention;', content)
    content = re.sub(r'use super::CallingConvention;', 'use fission_core::CallingConvention;', content)
    
    # If the file uses CallingConvention and doesn't import fission_core::CallingConvention, 
    # and we replaced something but missed the import, we'll rely on `cargo check` to tell us.
    # We can also do a quick hack: if `CallingConvention` is in the file but not imported and not `fission_core::CallingConvention`, add it.
    
    # This is a bit risky but mostly there's a few places that just use `CallingConvention`
    if 'CallingConvention' in content and 'use fission_core::CallingConvention;' not in content and 'fission_core::CallingConvention' not in content:
        # Check if we removed the old import
        content = re.sub(r'(use\s+.*::)CallingConvention([,;])', r'\1/* removed CallingConvention */\2\nuse fission_core::CallingConvention;', content)

    if original != content:
        with open(path, 'w') as f:
            f.write(content)
        print(f"Fixed {path}")

for root in roots:
    for dirpath, _, filenames in os.walk(root):
        for filename in filenames:
            if filename.endswith(".rs"):
                process_file(os.path.join(dirpath, filename))

