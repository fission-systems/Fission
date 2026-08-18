//! Precompute the name-keyed half of `NirCallParamRule`.
//!
//! `build_nir_call_param_rules` walks all 151,408 signatures and their 835,245
//! parameters to arrive at ~716 rules. Those rules depend only on the signature
//! tables and the struct layouts -- both static -- so nothing about them needs
//! recomputing per binary, and that walk is the only thing forcing the whole
//! signature table into memory at startup.
//!
//! Emitting them offline is what lets the signature database be read lazily;
//! addresses are attached at analysis time from the binary's own call targets.
//!
//! Usage: cargo run -p fission-decompiler --bin export_call_param_rules -- <out.json>

fn main() {
    let Some(out) = std::env::args().nth(1) else {
        eprintln!("usage: export_call_param_rules <out.json>");
        std::process::exit(2);
    };
    let rules = fission_decompiler::facts::name_keyed_call_param_rules();
    let json = serde_json::to_string(&rules).expect("serialize rules");
    std::fs::write(&out, json).expect("write rules");
    eprintln!("[+] {} rules -> {out}", rules.len());
}
