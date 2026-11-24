//! `resources` subcommand — bundle root probes and resolved [`fission_core::PATHS`] locations.

use crate::cli::build_identity::{build_identity, long_version};
use anyhow::Result;
use fission_core::resource_roots::resource_status_snapshot;

pub fn print_resources_status(json: bool) -> Result<()> {
    let snap = resource_status_snapshot(build_identity());
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(());
    }

    println!("Build: fission_cli {}", long_version());
    let manifest = &snap.resource_manifest;
    let state = serde_json::to_value(manifest.state)?;
    println!(
        "Resource manifest: {}: {}",
        state.as_str().unwrap_or("?"),
        manifest.detail
    );
    if let Some(path) = &manifest.path {
        println!("  {path}");
    }
    println!();

    println!("Resource bundle candidates:");
    for r in &snap.resource_roots {
        let state = if r.exists { "exists" } else { "missing" };
        println!("  [{}] {} ({})", r.kind, r.path, state);
    }

    if !snap.signature_paths.is_empty() {
        println!("\nUser signature paths:");
        for p in &snap.signature_paths {
            let state = if p.exists { "exists" } else { "MISSING" };
            println!(
                "  {} ({}, {} FID databases)",
                p.path, state, p.fid_databases
            );
        }
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
        "  fidb_java_dir: {:?} (present={})",
        res.fidb_java_dir, res.fidb_java_present
    );
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
    println!(
        "  generic_clib_pipe_text: {:?} (present={})",
        res.generic_clib_pipe_text, res.generic_clib_pipe_text_present
    );
    println!(
        "  generic_clib_64_pipe_text: {:?} (present={})",
        res.generic_clib_64_pipe_text, res.generic_clib_64_pipe_text_present
    );
    println!(
        "  mac_osx_pipe_text: {:?} (present={})",
        res.mac_osx_pipe_text, res.mac_osx_pipe_text_present
    );

    Ok(())
}
