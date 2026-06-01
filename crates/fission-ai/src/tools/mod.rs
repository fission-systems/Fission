pub mod execution;
pub mod registry;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

pub const MAX_TOOL_NAME_LENGTH: usize = 64;
pub const TOOL_NAME_HASH_LEN: usize = 8;
pub const FISSION_TOOL_PREFIX: &str = "fission__";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Original raw name of the tool (e.g., "disasm")
    pub raw_name: String,
    /// Model-visible tool name (e.g., "fission__disasm" or truncated+hash)
    pub callable_name: String,
    /// Description of what the tool does
    pub description: String,
    /// JSON schema for the tool parameters
    pub parameters: JsonValue,
}

impl ToolDefinition {
    pub fn new(raw_name: &str, description: &str, mut parameters: JsonValue) -> Self {
        let callable_name = normalize_tool_name(raw_name);
        inject_schema_guidance(&mut parameters);
        Self {
            raw_name: raw_name.to_string(),
            callable_name,
            description: description.to_string(),
            parameters,
        }
    }
}

pub fn normalize_tool_name(raw_name: &str) -> String {
    let prefixed = format!("{}{}", FISSION_TOOL_PREFIX, raw_name);
    if prefixed.len() <= MAX_TOOL_NAME_LENGTH {
        return prefixed;
    }

    // Name is too long, we must truncate and append hash.
    let mut hasher = Sha256::new();
    hasher.update(raw_name.as_bytes());
    let mut hash = String::new();
    for byte in hasher.finalize() {
        use std::fmt::Write;
        write!(&mut hash, "{:02x}", byte).unwrap();
    }
    let suffix = format!("_{}", &hash[..TOOL_NAME_HASH_LEN]);

    let prefix_len = MAX_TOOL_NAME_LENGTH.saturating_sub(suffix.len());
    let truncated: String = prefixed.chars().take(prefix_len).collect();
    
    format!("{}{}", truncated, suffix)
}

pub fn inject_schema_guidance(schema: &mut JsonValue) {
    let Some(properties) = schema
        .as_object_mut()
        .and_then(|obj| obj.get_mut("properties"))
        .and_then(JsonValue::as_object_mut)
    else {
        return;
    };

    let target_fields = ["addr", "address"];
    for field_name in target_fields {
        if let Some(property_schema) = properties.get_mut(field_name) {
            mask_address_property_schema(property_schema);
        }
    }
}

fn mask_address_property_schema(schema: &mut JsonValue) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };

    let mut description = object
        .get("description")
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    
    let guidance = "This parameter expects a 64-bit hex address string prefixed with '0x' (e.g., '0x14000000'). Do NOT use decimal.";
    if description.is_empty() {
        description = guidance.to_string();
    } else if !description.contains(guidance) {
        description = format!("{} {}", description, guidance);
    }

    object.insert("description".to_string(), JsonValue::String(description));
    // Ensure the type is marked as a string so the model sends a hex string
    object.insert("type".to_string(), JsonValue::String("string".to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_tool_name_short() {
        let name = normalize_tool_name("disasm");
        assert_eq!(name, "fission__disasm");
        assert!(name.len() <= MAX_TOOL_NAME_LENGTH);
    }

    #[test]
    fn test_normalize_tool_name_long() {
        let long_raw = "a".repeat(100);
        let name = normalize_tool_name(&long_raw);
        assert_eq!(name.len(), MAX_TOOL_NAME_LENGTH);
        assert!(name.starts_with("fission__a"));
    }

    #[test]
    fn test_inject_schema_guidance() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "addr": {
                    "type": "integer",
                    "description": "Memory address"
                },
                "other": {
                    "type": "string"
                }
            }
        });

        inject_schema_guidance(&mut schema);
        
        let addr_prop = &schema["properties"]["addr"];
        assert_eq!(addr_prop["type"], "string");
        assert!(addr_prop["description"].as_str().unwrap().contains("Do NOT use decimal"));
    }
}
