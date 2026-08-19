//! Does `.sla` + `.pspec` reproduce the `.slaspec`-derived default context?
//!
//! `default_context` decides, among other things, ARM vs Thumb decoding. If the
//! two paths agree on all 133 languages, the last thing the runtime needed the
//! SLEIGH source for is gone.

use std::path::Path;

use fission_sleigh::compiler::{
    compile_frontend_for_entry_spec, default_context_from_sla_and_pspec, discover_all_entry_specs,
    packaged_sla_for_entry_spec, processor_spec_for_entry_spec,
    sla::load_construct_templates_from_sla,
};

fn main() {
    let specs = discover_all_entry_specs().expect("entry specs");
    let mut compared = 0usize;
    let mut agree = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    let mut skipped = 0usize;

    for entry in &specs {
        let Ok(Some(sla_path)) = packaged_sla_for_entry_spec(&entry.path) else {
            skipped += 1;
            continue;
        };
        let Ok(library) = load_construct_templates_from_sla(&sla_path) else {
            skipped += 1;
            continue;
        };
        let Ok(compiled) = compile_frontend_for_entry_spec(&entry.path) else {
            skipped += 1;
            continue;
        };

        let pspec = processor_spec_for_entry_spec(&entry.path).unwrap_or(None);
        let from_sla =
            match default_context_from_sla_and_pspec(&entry.path, pspec.as_deref(), &library) {
                Ok(v) => v,
                Err(e) => {
                    mismatches.push(format!("{}: sla path errored: {e:#}", entry.entry_id));
                    continue;
                }
            };

        compared += 1;
        let want = (
            compiled.default_context,
            compiled.default_context_known_mask,
        );
        let got = (from_sla.context_bits(), from_sla.mask_bits());
        if want == got {
            agree += 1;
        } else {
            mismatches.push(format!(
                "{}: spec=(0x{:016x},0x{:016x}) sla=(0x{:016x},0x{:016x})",
                entry.entry_id, want.0, want.1, got.0, got.1
            ));
        }
    }

    println!("entry specs      : {}", specs.len());
    println!("  skipped        : {skipped}");
    println!("  compared       : {compared}");
    println!("  agree          : {agree}");
    println!("  disagree       : {}", mismatches.len());
    for m in mismatches.iter().take(20) {
        println!("      {m}");
    }
    let _ = Path::new("");
}
