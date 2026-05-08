//! Optional JSON hints tying symbol text to bundled Win API rows (observation-only).

use fission_signatures::{SIGNATURE_RESOURCES, symbol_for_win_api_database_lookup};
use serde_json::{Value, json};

/// Observation-only payload for debug bundles / benchmarks (does not drive semantics).
#[must_use]
pub fn win_api_prototype_hint_json(symbol_or_import_name: &str) -> Option<Value> {
    let flat = symbol_for_win_api_database_lookup(symbol_or_import_name)?;
    let sig = SIGNATURE_RESOURCES
        .api_signatures()
        .ok()?
        .find(|s| s.name == flat)?;
    Some(json!({
        "win_api_flat": flat,
        "param_count": sig.params.len(),
        "return_type": sig.return_type,
        "source": "signatures_win_api_catalog",
    }))
}
