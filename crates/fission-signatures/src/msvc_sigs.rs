//! MSVC and CRT Function Signatures
//!
//! Collection of binary patterns for identifying MSVC CRT functions,
//! standard library functions, and common patterns in Windows binaries.
//! Data is loaded from JSON at compile time via `include_str!`.

use serde::Deserialize;

use super::signature::FunctionSignature;

#[derive(Deserialize)]
struct JsonMsvcSignature {
    name: String,
    pattern: String,
}

/// Load all MSVC/CRT signatures into the provided vector
pub fn load_msvc_signatures(signatures: &mut Vec<FunctionSignature>) {
    let json_str = include_str!("../data/signatures/msvc.json");
    let items: Vec<JsonMsvcSignature> = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!(
            "Failed to parse msvc.json - this is a compile-time embedded file, please check data/signatures/msvc.json syntax: {}",
            e
        ));

    for item in items {
        signatures.push(FunctionSignature::from_hex(&item.name, &item.pattern));
    }
}
