//! `resources` subcommand — bundle root probes and resolved [`fission_core::PATHS`] locations.

use anyhow::Result;
use fission_core::resource_roots::resource_status_snapshot;

pub fn print_resources_status(json: bool) -> Result<()> {
    let snap = resource_status_snapshot();
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(());
    }

    println!("Resource bundle candidates:");
    for r in &snap.resource_roots {
        let state = if r.exists { "exists" } else { "missing" };
        println!("  [{}] {} ({})", r.kind, r.path, state);
    }

    println!("\nResolved resources:");
    let res = &snap.resources;
    println!("  signatures_base: {:?}", res.signatures_base);
    println!("  workspace_root: {:?}", res.workspace_root);
    println!(
        "  win32_typeinfo: {:?} (present={})",
        res.win32_typeinfo_dir, res.win32_typeinfo_present
    );
    println!("  fid_dir: {:?} (present={})", res.fid_dir, res.fid_present);
    println!(
        "  die_dir: {:?} (die_corpus_present={})",
        res.die_dir, res.die_corpus_present
    );
    println!(
        "  patterns_dir: {:?} (present={})",
        res.patterns_dir, res.patterns_present
    );
    println!(
        "  die_pe_signatures_json: {:?} (present={})",
        res.die_pe_signatures_json, res.die_pe_json_present
    );
    println!(
        "  win_api_pipe_text: {:?} (present={})",
        res.win_api_pipe_text, res.win_api_pipe_text_present
    );

    Ok(())
}
