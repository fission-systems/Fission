import re

# Fix init.rs variable assignment
with open('crates/fission-pcode/src/nir/builder/init.rs', 'r') as f:
    init = f.read()

init = init.replace('        Self {\n            pcode,', '        let mut b = Self {\n            pcode,')
with open('crates/fission-pcode/src/nir/builder/init.rs', 'w') as f:
    f.write(init)

