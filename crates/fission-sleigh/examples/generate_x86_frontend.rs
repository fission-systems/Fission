use anyhow::Result;
use fission_sleigh::compiler::{generated_root_for_arch, write_x86_64_generated_artifacts};

fn main() -> Result<()> {
    let output_root = generated_root_for_arch("x86");
    write_x86_64_generated_artifacts(&output_root)?;
    println!("{}", output_root.display());
    Ok(())
}
