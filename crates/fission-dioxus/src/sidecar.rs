//! Native-only read/write of a binary's `<name>.fission.json` sidecar
//! project file: user function renames + comments, keyed by address.
//!
//! `fission-static`'s `FactStore::from_binary` already reads
//! `user_function_names`/`user_structs` from this same file (one-way, into
//! name resolution) -- this module closes the loop for the GUI's own
//! `AppState.rename_map`/`comments`, which were previously pure in-memory
//! state with no persistence at all. Structs are read generically via
//! `serde_json::Value` rather than modelled here, so a save never disturbs
//! `user_structs` (or any other key) a user may have hand-edited.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn sidecar_path_for(binary_path: &str) -> PathBuf {
    Path::new(binary_path).with_extension("fission.json")
}

fn parse_addr(s: &str) -> Option<u64> {
    s.parse::<u64>()
        .ok()
        .or_else(|| u64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok())
}

fn string_map_from(value: &serde_json::Value, key: &str) -> HashMap<u64, String> {
    value
        .get(key)
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| Some((parse_addr(k)?, v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Best-effort load of user renames + comments from `binary_path`'s sidecar
/// file. Returns empty maps if the file doesn't exist or fails to parse --
/// this is a convenience preload, not a hard requirement.
pub fn load_sidecar(binary_path: &str) -> (HashMap<u64, String>, HashMap<u64, String>) {
    let Ok(content) = std::fs::read_to_string(sidecar_path_for(binary_path)) else {
        return (HashMap::new(), HashMap::new());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return (HashMap::new(), HashMap::new());
    };
    (
        string_map_from(&value, "user_function_names"),
        string_map_from(&value, "user_comments"),
    )
}

/// Read-modify-write: overlays `renames`/`comments` onto any existing
/// sidecar file at `binary_path`'s `.fission.json`, leaving unrelated keys
/// (`user_structs`, anything else) untouched.
pub fn save_sidecar(
    binary_path: &str,
    renames: &HashMap<u64, String>,
    comments: &HashMap<u64, String>,
) -> std::io::Result<()> {
    let path = sidecar_path_for(binary_path);
    let mut root: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let names_obj: serde_json::Map<String, serde_json::Value> = renames
        .iter()
        .map(|(addr, name)| (format!("0x{addr:x}"), serde_json::Value::String(name.clone())))
        .collect();
    let comments_obj: serde_json::Map<String, serde_json::Value> = comments
        .iter()
        .map(|(addr, text)| (format!("0x{addr:x}"), serde_json::Value::String(text.clone())))
        .collect();

    let obj = root.as_object_mut().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "sidecar root is not a JSON object")
    })?;
    obj.insert("user_function_names".to_string(), serde_json::Value::Object(names_obj));
    obj.insert("user_comments".to_string(), serde_json::Value::Object(comments_obj));

    std::fs::write(path, serde_json::to_string_pretty(&root)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_binary_path(name: &str) -> String {
        std::env::temp_dir().join(name).display().to_string()
    }

    #[test]
    fn save_then_load_round_trips_renames_and_comments() {
        let binary_path = temp_binary_path("fission_sidecar_roundtrip_test.exe");
        let sidecar_path = sidecar_path_for(&binary_path);
        let _ = std::fs::remove_file(&sidecar_path);

        let mut renames = HashMap::new();
        renames.insert(0x140001000u64, "my_renamed_fn".to_string());
        let mut comments = HashMap::new();
        comments.insert(0x140001010u64, "sets up the stack frame".to_string());

        save_sidecar(&binary_path, &renames, &comments).expect("save should succeed");
        let (loaded_renames, loaded_comments) = load_sidecar(&binary_path);

        assert_eq!(loaded_renames, renames);
        assert_eq!(loaded_comments, comments);

        let _ = std::fs::remove_file(&sidecar_path);
    }

    #[test]
    fn save_preserves_unrelated_hand_edited_keys() {
        let binary_path = temp_binary_path("fission_sidecar_preserve_test.exe");
        let sidecar_path = sidecar_path_for(&binary_path);
        std::fs::write(
            &sidecar_path,
            serde_json::json!({
                "user_structs": [{"name": "MyStruct", "size": 8, "fields": []}],
                "user_function_names": {"0x1000": "old_name"},
            })
            .to_string(),
        )
        .unwrap();

        let mut renames = HashMap::new();
        renames.insert(0x1000u64, "new_name".to_string());
        save_sidecar(&binary_path, &renames, &HashMap::new()).expect("save should succeed");

        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&sidecar_path).unwrap()).unwrap();
        assert_eq!(saved["user_structs"][0]["name"], "MyStruct");
        assert_eq!(saved["user_function_names"]["0x1000"], "new_name");

        let _ = std::fs::remove_file(&sidecar_path);
    }

    #[test]
    fn load_missing_sidecar_returns_empty_maps() {
        let binary_path = temp_binary_path("fission_sidecar_missing_test.exe");
        let _ = std::fs::remove_file(sidecar_path_for(&binary_path));
        let (renames, comments) = load_sidecar(&binary_path);
        assert!(renames.is_empty());
        assert!(comments.is_empty());
    }
}
