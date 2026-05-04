//! Prints [`fission_dynamic::runtime_status`] as deterministic JSON (no OS debugger APIs).

fn main() {
    let json =
        serde_json::to_string_pretty(&fission_dynamic::runtime_status()).expect("serialize status");
    println!("{json}");
}
