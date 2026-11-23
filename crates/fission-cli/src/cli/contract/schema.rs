//! `fission_cli schema`: machine-readable description of the contract.
//!
//! The generic envelope and the per-command payload contracts are versioned
//! separately. Publishing a schema for an existing payload preserves its version.

use super::CONTRACT_ID;
use super::errors::registry_json;
use crate::cli::args::{CANONICAL_SUBCOMMANDS, SchemaArgs};
use anyhow::{Result, bail};
use serde_json::{Value, json};

/// Commands the contract covers (everything except `schema` itself).
fn contract_commands() -> Vec<&'static str> {
    CANONICAL_SUBCOMMANDS
        .iter()
        .copied()
        .filter(|name| !matches!(*name, "schema" | "skill"))
        .collect()
}

fn payload_schema(command: &str) -> Option<Value> {
    let text = match command {
        "info" => include_str!("../../../../../docs/schemas/cli-info-data.v1.schema.json"),
        "list" => include_str!("../../../../../docs/schemas/cli-list-data.v1.schema.json"),
        "decomp" => include_str!("../../../../../docs/schemas/cli-decomp-data.v1.schema.json"),
        "quotient" => {
            include_str!("../../../../../docs/schemas/cli-quotient-data.v1.schema.json")
        }
        _ => return None,
    };
    let mut value: Value =
        serde_json::from_str(text).expect("committed payload schema is valid JSON");
    value
        .as_object_mut()
        .expect("payload schema is an object")
        .remove("$schema");
    Some(value)
}

pub(crate) fn envelope_schema(command: Option<&str>) -> Value {
    let command_property = match command {
        Some(name) => json!({ "const": name }),
        None => json!({ "type": "string" }),
    };
    let mut schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "fission_cli --contract v1 response envelope",
        "description": "One JSON document on stdout for every contract-mode invocation, success or failure.",
        "type": "object",
        "required": ["contract", "command", "ok", "warnings", "tool", "elapsed_ms"],
        "properties": {
            "contract": { "const": CONTRACT_ID },
            "command": command_property,
            "ok": { "type": "boolean" },
            "schema_version": {
                "type": "integer",
                "minimum": 1,
                "description": "Version of the `data` payload for this command."
            },
            "data": {
                "description": "The command's own JSON output, unchanged.",
                "x-data-schema": "untyped"
            },
            "summary": {
                "type": "object",
                "description": "Present for batch commands whose rows report per-item errors.",
                "required": ["total", "succeeded", "failed"],
                "properties": {
                    "total": { "type": "integer", "minimum": 0 },
                    "succeeded": { "type": "integer", "minimum": 0 },
                    "failed": { "type": "integer", "minimum": 0 }
                }
            },
            "warnings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["code", "message"],
                    "properties": {
                        "code": { "type": "string" },
                        "message": { "type": "string" }
                    }
                }
            },
            "error": {
                "type": "object",
                "required": ["code", "kind", "message", "retryable"],
                "properties": {
                    "code": { "type": "string", "description": "See `fission_cli schema --error-codes`." },
                    "kind": {
                        "enum": [
                            "failed", "usage", "input", "partial",
                            "timeout", "environment", "internal"
                        ]
                    },
                    "message": { "type": "string" },
                    "hint": { "type": "string" },
                    "retryable": { "type": "boolean" },
                    "context": { "type": "object" }
                }
            },
            "tool": {
                "type": "object",
                "required": ["name", "version"],
                "properties": {
                    "name": { "type": "string" },
                    "version": { "type": "string" },
                    "commit": { "type": ["string", "null"] }
                }
            },
            "elapsed_ms": { "type": "integer", "minimum": 0 }
        },
        "allOf": [
            {
                "if": { "properties": { "ok": { "const": true } }, "required": ["ok"] },
                "then": { "required": ["schema_version", "data"] }
            },
            {
                "if": { "properties": { "ok": { "const": false } }, "required": ["ok"] },
                "then": { "required": ["error"] }
            }
        ]
    });
    if let Some(payload) = command.and_then(payload_schema) {
        schema["properties"]["data"] = payload;
        schema["properties"]["schema_version"]["const"] = json!(1);
    }
    schema
}

fn commands_json() -> Value {
    json!({
        "contract": CONTRACT_ID,
        "commands": contract_commands(),
        "note": "typed_data_commands lists the commands whose v1 data schemas are available",
        "typed_data_commands": contract_commands().into_iter().filter(|name| payload_schema(name).is_some()).collect::<Vec<_>>()
    })
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub fn run_schema(args: &SchemaArgs) -> Result<()> {
    if args.list {
        return print_json(&commands_json());
    }
    if args.error_codes {
        return print_json(&registry_json());
    }
    match args.command.as_deref() {
        None => print_json(&envelope_schema(None)),
        Some(name) if contract_commands().contains(&name) => {
            print_json(&envelope_schema(Some(name)))
        }
        Some(name) => bail!("unknown command {name:?}; see `fission_cli schema --list`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed(text: &str) -> String {
        text.replace("\r\n", "\n")
    }

    fn pretty(value: &Value) -> String {
        format!(
            "{}\n",
            serde_json::to_string_pretty(value).expect("serializes")
        )
    }

    #[test]
    fn envelope_schema_matches_the_committed_file() {
        let committed_text = committed(include_str!(
            "../../../../../docs/schemas/cli-envelope.v1.schema.json"
        ));
        assert_eq!(
            pretty(&envelope_schema(None)),
            committed_text,
            "regenerate with `fission_cli schema > docs/schemas/cli-envelope.v1.schema.json`"
        );
    }

    #[test]
    fn error_code_registry_matches_the_committed_file() {
        let committed_text = committed(include_str!(
            "../../../../../docs/schemas/cli-error-codes.v1.json"
        ));
        assert_eq!(
            pretty(&registry_json()),
            committed_text,
            "regenerate with `fission_cli schema --error-codes > docs/schemas/cli-error-codes.v1.json`"
        );
    }

    /// A payload schema is embedded under the envelope's `data`, so a `$ref`
    /// in it would resolve against the envelope, not against itself.
    #[test]
    fn payload_schemas_are_self_contained() {
        for command in contract_commands() {
            let Some(schema) = payload_schema(command) else {
                continue;
            };
            let text = schema.to_string();
            assert!(
                !text.contains("\"$ref\""),
                "{command}: payload schema uses $ref"
            );
            assert!(
                !text.contains("\"$defs\""),
                "{command}: payload schema uses $defs"
            );
        }
        assert!(payload_schema("quotient").is_some());
    }

    #[test]
    fn command_schema_pins_the_command_name() {
        let schema = envelope_schema(Some("decomp"));
        assert_eq!(schema["properties"]["command"]["const"], "decomp");
        assert!(!contract_commands().contains(&"schema"));
    }
}
