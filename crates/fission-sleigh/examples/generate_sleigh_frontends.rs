use anyhow::Result;
use fission_sleigh::compiler::{
    generated_root, write_all_generated_artifacts, write_ghidra_language_manifest,
};

fn main() -> Result<()> {
    let spec_manifest = write_ghidra_language_manifest()?;
    let output_root = generated_root();
    let manifest = write_all_generated_artifacts(&output_root)?;
    println!(
        "{} processors / {} variants -> {}",
        spec_manifest.processor_count,
        manifest.entries.len(),
        output_root.display()
    );
    Ok(())
}
