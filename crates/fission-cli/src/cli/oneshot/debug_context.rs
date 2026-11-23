//! Evidence-backed debugger context: current machine state plus optional static analysis.
//!
//! A runtime PC is not a file VA. Never associate a native PC with decompiled
//! code unless the selected module's mapping proves a translation to the exact
//! binary supplied for analysis.

use crate::cli::args::DebugContextArgs;
use anyhow::{Result, bail};
use fission_dynamic::debug::traits::ExecutionBackend;
use fission_dynamic::debug::types::{
    DebugStatus, ModuleAddressTransform, ModuleListReport, ProcessMemoryMapping, ProcessModule,
    RegisterState,
};
use fission_loader::loader::LoadedBinary;
use serde_json::{Value, json};
use std::path::Path;

fn hex(address: u64) -> String {
    format!("0x{address:x}")
}

fn registers_json(regs: &RegisterState) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("pc".to_owned(), json!(hex(regs.pc)));
    for (name, value) in regs.iter() {
        map.insert(name.to_ascii_lowercase(), json!(hex(value)));
    }
    Value::Object(map)
}

fn same_file(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            #[cfg(windows)]
            {
                a.eq_ignore_ascii_case(b)
            }
            #[cfg(not(windows))]
            {
                a == b
            }
        }
    }
}

fn mapping_at_pc(report: &ModuleListReport, pc: u64) -> Option<(&ProcessModule, &ProcessMemoryMapping)> {
    report.modules.iter().find_map(|module| {
        module
            .mappings
            .iter()
            .find(|mapping| mapping.runtime_start <= pc && pc < mapping.runtime_end)
            .map(|mapping| (module, mapping))
    })
}

fn translated_address(pc: u64, load_bias: i64) -> Option<u64> {
    // Signed arithmetic is necessary for binaries mapped below their ELF VA,
    // and checked conversion avoids wrapping an invalid address into code.
    u64::try_from(i128::from(pc) - i128::from(load_bias)).ok()
}

fn native_location(report: &ModuleListReport, pc: u64, binary: Option<&str>) -> (Value, Value, Option<u64>) {
    let Some((module, mapping)) = mapping_at_pc(report, pc) else {
        return (
            Value::Null,
            json!({"status": "unavailable", "reason": "PC is not inside a reported file-backed mapping"}),
            None,
        );
    };
    let location = json!({
        "name": module.name,
        "path": module.path,
        "mapping_start": hex(mapping.runtime_start),
        "mapping_end": hex(mapping.runtime_end),
        "permissions": mapping.permissions,
    });
    let Some(binary) = binary else {
        return (location, json!({
            "status": "unavailable",
            "reason": "supply --binary or launch a session with a matching binary for static analysis"
        }), None);
    };
    if !same_file(&module.path, binary) {
        return (location, json!({
            "status": "unavailable",
            "reason": "the selected PC belongs to a different module than the supplied analysis binary",
            "analysis_binary": binary,
        }), None);
    }
    match &module.address_transform {
        ModuleAddressTransform::Resolved { load_bias, formula, analysis_address_domain, .. } => {
            if let Some(va) = translated_address(pc, *load_bias) {
                (location, json!({
                    "status": "resolved",
                    "analysis_va": hex(va),
                    "load_bias": load_bias,
                    "formula": formula,
                    "analysis_address_domain": analysis_address_domain,
                    "source": "debugger_module_mapping",
                }), Some(va))
            } else {
                (location, json!({
                    "status": "unavailable",
                    "reason": "runtime PC and reported load bias cannot produce a valid analysis address"
                }), None)
            }
        }
        ModuleAddressTransform::NotApplicable { reason }
        | ModuleAddressTransform::Unavailable { reason } => {
            (location, json!({ "status": "unavailable", "reason": reason }), None)
        }
    }
}

fn stack_json(backend: &dyn ExecutionBackend, regs: &RegisterState, size: usize) -> Value {
    if size == 0 {
        return json!({ "status": "not_requested" });
    }
    let sp = regs.get("rsp").or_else(|| regs.get("esp")).or_else(|| regs.get("sp"));
    let Some(sp) = sp else {
        return json!({ "status": "unavailable", "reason": "no scalar stack pointer in the register snapshot" });
    };
    match backend.read_memory_partial(sp, size) {
        Ok(bytes) => json!({
            "status": if bytes.len() == size { "ok" } else { "partial" },
            "address": hex(sp),
            "requested_size": size,
            "size": bytes.len(),
            "hex": bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        }),
        Err(err) => json!({ "status": "unavailable", "address": hex(sp), "reason": err.to_string() }),
    }
}

/// Read exactly one stopped machine state. A static decompile never claims to
/// be a rendering of the live memory image (which might be patched/unpacked).
pub(super) fn run_context(
    backend: &mut dyn ExecutionBackend,
    thread_id: u32,
    args: &DebugContextArgs,
    session_binary: Option<&str>,
    emulator: bool,
) -> Result<Value> {
    if args.stack > 4096 {
        bail!("context --stack is limited to 4096 bytes");
    }
    let state = backend.get_state();
    if state.status != DebugStatus::Suspended {
        bail!("context requires a stopped target; wait for an event or breakpoint first");
    }
    // A preceding switch-thread command can select a thread other than the
    // session's original main thread. Snapshot the selected one.
    let thread_id = state.current_thread_id.unwrap_or(thread_id);
    let regs = backend.fetch_registers(thread_id)?;
    let pc = regs.pc;
    let binary_path = args.binary.as_deref().or(session_binary);
    let stack = stack_json(backend, &regs, args.stack);

    let (module, mut analysis, analysis_pc) = if emulator {
        (
            Value::Null,
            json!({
                "status": "resolved",
                "analysis_va": hex(pc),
                "source": "emulated_guest_pc",
                "note": "the emulator PC is in the loaded guest image coordinate system",
            }),
            Some(pc),
        )
    } else {
        match backend.list_modules() {
            Ok(report) => native_location(&report, pc, binary_path),
            Err(err) => (
                Value::Null,
                json!({ "status": "unavailable", "reason": format!("module query failed: {err}") }),
                None,
            ),
        }
    };

    let mut function = Value::Null;
    let mut decompilation = if args.decomp {
        json!({ "status": "unavailable", "reason": "a trustworthy analysis binary and address are required" })
    } else {
        json!({ "status": "not_requested" })
    };

    if let (Some(path), Some(va)) = (binary_path, analysis_pc) {
        match LoadedBinary::from_file(Path::new(path)) {
            Ok(mut binary) => {
                if emulator && binary.view_executable_bytes(va, 1).is_none() {
                    analysis = json!({
                        "status": "unavailable",
                        "reason": "guest PC is outside executable bytes in the supplied binary",
                    });
                } else {
                    let _discovery = fission_static::analysis::discover_functions_with_runtime(&mut binary);
                    if let Some(func) = binary.function_at(va) {
                        function = json!({
                            "name": func.name,
                            "entry": hex(func.address),
                            "offset": hex(va - func.address),
                            "size": func.size,
                            "origin": func.origin,
                            "source": "on_disk_static_analysis",
                        });
                        if args.decomp {
                            let config = fission_decompiler::RustSleighDecompileConfig::cli_defaults();
                            decompilation = match fission_decompiler::decompile_with_rust_sleigh(
                                &binary, func.address, &func.name, &config, None, None,
                            ) {
                                Ok(result) => json!({
                                    "status": "ok",
                                    "code": result.code,
                                    "fell_back": result.fell_back,
                                    "fallback_reason": result.fallback_reason,
                                    "source": "on_disk_static_analysis",
                                    "runtime_memory_verified": false,
                                }),
                                Err(err) => json!({ "status": "failed", "reason": err }),
                            };
                        }
                    } else if args.decomp {
                        decompilation = json!({
                            "status": "unavailable",
                            "reason": "no discovered function contains the analysis address",
                        });
                    }
                }
            }
            Err(err) => {
                analysis = json!({
                    "status": "unavailable",
                    "reason": format!("cannot load analysis binary {path}: {err}"),
                });
            }
        }
    }

    if args.decomp && decompilation["status"] == "unavailable"
        && analysis["status"] == "unavailable"
    {
        decompilation["reason"] = analysis["reason"].clone();
    }

    Ok(json!({
        "schema_version": 1,
        "action": "context",
        "thread_id": thread_id,
        "pc": hex(pc),
        "stop": backend.stop_reason(),
        "registers": registers_json(&regs),
        "stack": stack,
        "module": module,
        "analysis": analysis,
        "function": function,
        "decompilation": decompilation,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_dynamic::debug::types::{
        ModuleListCompleteness, ProcessMemoryMapping, ProcessModule,
    };

    fn report() -> ModuleListReport {
        ModuleListReport {
            schema_version: 1,
            pid: 123,
            completeness: ModuleListCompleteness::Complete,
            modules: vec![ProcessModule {
                path: "/tmp/test-guest".into(),
                name: "test-guest".into(),
                device: None,
                inode: None,
                mappings: vec![ProcessMemoryMapping {
                    runtime_start: 0x5555_0000,
                    runtime_end: 0x5555_2000,
                    file_offset: Some(0),
                    permissions: Some("r-xp".into()),
                    path: Some("/tmp/test-guest".into()),
                }],
                address_transform: ModuleAddressTransform::Resolved {
                    format: "elf".into(),
                    analysis_address_domain: "elf_virtual_address".into(),
                    runtime_address_domain: "process_virtual_address".into(),
                    formula: "runtime_va = analysis_va + load_bias".into(),
                    load_bias: 0x5554_0000,
                },
            }],
            other_mappings: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn translated_pc_handles_positive_and_negative_bias_without_wrapping() {
        assert_eq!(translated_address(0x5555_1020, 0x5554_0000), Some(0x11020));
        assert_eq!(translated_address(0x1000, -0x1000), Some(0x2000));
        assert_eq!(translated_address(0x1000, 0x2000), None);
        assert_eq!(translated_address(u64::MAX, -1), None);
    }

    #[test]
    fn native_context_does_not_guess_module_or_address() {
        let report = report();
        assert!(mapping_at_pc(&report, 0x5555_0000).is_some());
        assert!(mapping_at_pc(&report, 0x5555_1fff).is_some());
        assert!(mapping_at_pc(&report, 0x5555_2000).is_none());

        let (_module, address, va) = native_location(&report, 0x5555_1020, Some("/tmp/test-guest"));
        assert_eq!(va, Some(0x11020));
        assert_eq!(address["source"], "debugger_module_mapping");

        let (_module, address, va) = native_location(&report, 0x5555_1020, Some("/tmp/other"));
        assert!(va.is_none());
        assert_eq!(address["status"], "unavailable");

        let (_module, address, va) = native_location(&report, 0x5555_2000, Some("/tmp/test-guest"));
        assert!(va.is_none());
        assert_eq!(address["status"], "unavailable");
    }

    #[test]
    fn unresolved_transform_is_not_silently_treated_as_identity() {
        let mut report = report();
        report.modules[0].address_transform = ModuleAddressTransform::Unavailable {
            reason: "untrusted slide".into(),
        };
        let (_, location, va) = native_location(&report, 0x5555_1020, Some("/tmp/test-guest"));
        assert_eq!(location["status"], "unavailable");
        assert_eq!(location["reason"], "untrusted slide");
        assert!(va.is_none());
    }
}
