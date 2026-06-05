use anyhow::Result;
use fission_sleigh::compiler::{
    generated_root_for_arch, write_generated_artifacts_for_entry_spec, x86_64_entry_spec_path,
};

fn main() -> Result<()> {
    let output_root = generated_root_for_arch("x86");
    let entry_spec = x86_64_entry_spec_path();
    write_generated_artifacts_for_entry_spec(&entry_spec, &output_root)?;
    println!("{}", output_root.display());
    Ok(())
}
