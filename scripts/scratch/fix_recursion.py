import re
with open("crates/fission-pcode/src/nir/structuring/linear/mod.rs", "r") as f:
    content = f.read()

# We need to change the call at line 466 back to lower_linear_body_with_depth_detailed
# The code around line 466 should be:
target = """        let result = self.lower_linear_body_cached(
            start_idx,
            exit,
            depth,
            budget.as_deref_mut(),
            region_recovery,
        )?;"""

replacement = """        let result = self.lower_linear_body_with_depth_detailed(
            start_idx,
            exit,
            depth,
            budget.as_deref_mut(),
            region_recovery,
        )?;"""

content = content.replace(target, replacement)
with open("crates/fission-pcode/src/nir/structuring/linear/mod.rs", "w") as f:
    f.write(content)
print("Done")
