use std::env;

use anyhow::{anyhow, Result};
use fission_sleigh::compiler::{
    discover_all_entry_specs, generated_root, write_all_generated_artifacts,
    write_generated_artifacts_for_entry_spec, write_ghidra_language_manifest,
};

fn main() -> Result<()> {
    let output_root = generated_root();
    let mut entry_filter = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--entry" => {
                entry_filter = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("--entry requires an entry id"))?,
                );
            }
            "--help" | "-h" => {
                println!(
                    "usage: generate_sleigh_frontends [--entry <entry-id>]\n\n\
                     Without --entry, regenerate all frontends. With --entry, only write that \
                     entry under the normal target/fission-sleigh/generated cache."
                );
                return Ok(());
            }
            other => return Err(anyhow!("unknown argument {other}")),
        }
    }

    let spec_manifest = write_ghidra_language_manifest()?;
    if let Some(entry_id) = entry_filter {
        let entry = discover_all_entry_specs()?
            .into_iter()
            .find(|entry| entry.entry_id == entry_id)
            .ok_or_else(|| anyhow!("unknown SLEIGH entry id {entry_id}"))?;
        write_generated_artifacts_for_entry_spec(&entry.path, &output_root)?;
        println!(
            "{} {} -> {}",
            entry.arch,
            entry.entry_id,
            output_root.display()
        );
        return Ok(());
    }

    let manifest = write_all_generated_artifacts(&output_root)?;
    println!(
        "{} processors / {} variants -> {}",
        spec_manifest.processor_count,
        manifest.entries.len(),
        output_root.display()
    );
    Ok(())
}
