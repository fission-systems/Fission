//! Register host bindings (`binary`, `emit`) on the Rhai [`Engine`].

use crate::api::binary;
use crate::error::ScriptError;
use crate::limits::ScriptLimits;
use crate::result::{ScriptFinding, ScriptMeta};
use fission_loader::loader::LoadedBinary;
use rhai::{Dynamic, Engine, Map};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct BinaryHost(pub Arc<LoadedBinary>);

impl BinaryHost {
    pub fn path(&mut self) -> String {
        self.0.path.clone()
    }

    pub fn format(&mut self) -> String {
        self.0.format.clone()
    }

    pub fn image_base(&mut self) -> String {
        format!("0x{:x}", self.0.image_base)
    }

    pub fn is_64bit(&mut self) -> bool {
        self.0.is_64bit
    }

    pub fn functions(&mut self) -> rhai::Array {
        binary::functions_array(&self.0)
    }

    pub fn imports(&mut self) -> rhai::Array {
        binary::imports_array(&self.0)
    }

    pub fn exports(&mut self) -> rhai::Array {
        binary::exports_array(&self.0)
    }

    pub fn sections(&mut self) -> rhai::Array {
        binary::sections_array(&self.0)
    }

    pub fn strings_with_min_len(&mut self, min_len: i64) -> rhai::Array {
        let n = min_len.max(1) as usize;
        binary::strings_array(&self.0, n)
    }

    pub fn strings_default(&mut self) -> rhai::Array {
        binary::strings_array(&self.0, 4)
    }
}

pub fn register_binary(engine: &mut Engine) -> Result<(), ScriptError> {
    engine
        .register_type::<BinaryHost>()
        .register_fn("path", BinaryHost::path)
        .register_fn("format", BinaryHost::format)
        .register_fn("image_base", BinaryHost::image_base)
        .register_fn("is_64bit", BinaryHost::is_64bit)
        .register_fn("functions", BinaryHost::functions)
        .register_fn("imports", BinaryHost::imports)
        .register_fn("exports", BinaryHost::exports)
        .register_fn("sections", BinaryHost::sections)
        .register_fn("strings", BinaryHost::strings_default)
        .register_fn("strings", BinaryHost::strings_with_min_len);

    Ok(())
}

pub(crate) fn dynamic_to_json_value(d: Dynamic) -> serde_json::Value {
    if d.is_unit() {
        return serde_json::Value::Null;
    }
    if let Some(v) = d.clone().try_cast::<bool>() {
        return serde_json::Value::Bool(v);
    }
    if let Some(v) = d.clone().try_cast::<i64>() {
        return serde_json::Value::Number(v.into());
    }
    if let Some(v) = d.clone().try_cast::<u64>() {
        return serde_json::Number::from(v).into();
    }
    if let Some(v) = d.clone().try_cast::<rhai::ImmutableString>() {
        return serde_json::Value::String(v.to_string());
    }
    if let Some(arr) = d.clone().try_cast::<rhai::Array>() {
        let vec: Vec<serde_json::Value> = arr.into_iter().map(dynamic_to_json_value).collect();
        return serde_json::Value::Array(vec);
    }
    if let Some(map) = d.clone().try_cast::<Map>() {
        let mut obj = serde_json::Map::new();
        for (k, v) in map {
            obj.insert(k.to_string(), dynamic_to_json_value(v));
        }
        return serde_json::Value::Object(obj);
    }
    serde_json::Value::String(d.to_string())
}

fn finding_from_map(m: &Map) -> ScriptFinding {
    let kind = m
        .get("kind")
        .and_then(|d| d.clone().try_cast::<rhai::ImmutableString>())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "finding".to_string());

    let message = m
        .get("message")
        .and_then(|d| d.clone().try_cast::<rhai::ImmutableString>())
        .map(|s| s.to_string());

    let severity = m
        .get("severity")
        .and_then(|d| d.clone().try_cast::<rhai::ImmutableString>())
        .map(|s| s.to_string());

    let address = m
        .get("address")
        .and_then(|d| d.clone().try_cast::<rhai::ImmutableString>())
        .map(|s| s.to_string());

    let data = m
        .get("data")
        .map(|d| dynamic_to_json_value(d.clone()));

    ScriptFinding {
        kind,
        message,
        severity,
        address,
        data,
    }
}

fn approximate_findings_payload_bytes(findings: &[ScriptFinding]) -> usize {
    serde_json::to_string(findings).map(|s| s.len()).unwrap_or(findings.len() * 64)
}

pub fn register_emit(
    engine: &mut Engine,
    findings: Arc<Mutex<Vec<ScriptFinding>>>,
    limits: ScriptLimits,
    halted: Arc<Mutex<Option<String>>>,
) -> Result<(), ScriptError> {
    engine.register_fn("emit", move |m: Map| {
        let stopped = halted.lock().map(|h| h.is_some()).unwrap_or(true);
        if stopped {
            return;
        }

        let finding = finding_from_map(&m);

        let mut fs = match findings.lock() {
            Ok(f) => f,
            Err(_) => return,
        };

        if fs.len() >= limits.max_findings {
            if let Ok(mut h) = halted.lock() {
                *h = Some("script exceeded max_findings limit".into());
            }
            return;
        }

        fs.push(finding);

        if approximate_findings_payload_bytes(&fs) > limits.max_output_bytes {
            fs.pop();
            if let Ok(mut h) = halted.lock() {
                *h = Some("script exceeded max_output_bytes limit".into());
            }
        }
    });

    Ok(())
}

pub fn script_meta(path: impl Into<String>) -> ScriptMeta {
    ScriptMeta {
        path: path.into(),
        engine: "rhai".into(),
    }
}
