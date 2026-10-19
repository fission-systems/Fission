//! Experimental cross-project probe: compare an FSL FIR effect candidate with
//! the checked-in JVM `.sla` runtime's real P-code lift.
//!
//! This is a parity probe, not an alternate decoder or production FSL path.
//! Pass the generated JSON package from `fslc_probe.py` as the sole argument.
//! The checked-in fixture under `examples/fixtures` makes this probe runnable
//! without a separate `fission-research` checkout.

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, ensure, Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode};
use fission_sleigh::compiler::CompiledTemplateSource;
use fission_sleigh::runtime::{RuntimeFrontendStatus, RuntimeSleighFrontend};
use serde_json::{json, Value};

const PATTERN_ID: &str = "jvm.se26.iadd";
const BYTECODE_ADDRESS: u64 = 0;

struct FslPattern {
    profile_id: String,
    source_path: String,
    source_sha256: String,
    pattern_id: String,
    mnemonic: String,
    opcode: u8,
    effect: Value,
    provenance: Value,
    sources: Value,
}

fn required_str<'a>(value: &'a Value, path: &str) -> Result<&'a str> {
    value
        .pointer(path)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("FSL pattern package is missing string {path}"))
}

fn required_u64(value: &Value, path: &str) -> Result<u64> {
    let raw = value
        .pointer(path)
        .ok_or_else(|| anyhow!("FSL pattern package is missing {path}"))?;
    if let Some(number) = raw.as_u64() {
        return Ok(number);
    }
    let text = raw
        .as_str()
        .ok_or_else(|| anyhow!("FSL package field {path} must be an integer or string"))?;
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).with_context(|| format!("parse {path}={text:?}"))
    } else {
        text.parse::<u64>()
            .with_context(|| format!("parse {path}={text:?}"))
    }
}

fn read_fsl_pattern(path: &PathBuf) -> Result<FslPattern> {
    let bytes =
        fs::read(path).with_context(|| format!("read pattern package {}", path.display()))?;
    let package: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse pattern package {}", path.display()))?;

    ensure!(
        required_str(&package, "/format")? == "fission_research_isa_pattern_db",
        "unsupported FSL pattern package format"
    );
    ensure!(
        required_u64(&package, "/schema_version")? == 1,
        "unsupported pattern schema"
    );
    ensure!(
        required_str(&package, "/compiled_from/format")? == "fission_fsl_experimental",
        "package was not compiled from an experimental FSL source"
    );
    ensure!(
        required_str(&package, "/architecture/endianness")? == "big",
        "JVM bytecode must be big-endian"
    );
    ensure!(
        required_u64(&package, "/architecture/instruction_word_bits")? == 8,
        "this probe accepts one-byte JVM opcodes only"
    );

    let patterns = package
        .get("patterns")
        .and_then(Value::as_array)
        .context("pattern package has no patterns array")?;
    let mut matching_patterns = patterns
        .iter()
        .filter(|item| item.get("pattern_id").and_then(Value::as_str) == Some(PATTERN_ID));
    let pattern = matching_patterns
        .next()
        .context("FSL package has no JVM iadd pattern")?;
    ensure!(
        matching_patterns.next().is_none(),
        "FSL package duplicates JVM iadd pattern"
    );

    let pattern_id = required_str(pattern, "/pattern_id")?;
    let mnemonic = required_str(pattern, "/mnemonic")?;
    let mask = required_u64(pattern, "/match/mask")?;
    let opcode = required_u64(pattern, "/match/value")?;
    ensure!(
        mask == 0xff && opcode <= u8::MAX as u64,
        "FSL iadd must use an 8-bit exact match"
    );
    ensure!(
        mnemonic == "iadd",
        "FSL pattern identity and mnemonic disagree"
    );

    let effect = pattern
        .get("fir_effect")
        .context("FSL iadd has no FIR effect candidate")?;
    ensure!(
        required_str(effect, "/operation")? == "arith.add",
        "unexpected FIR operation"
    );
    ensure!(
        required_str(effect, "/domain")? == "vm_operand_stack",
        "unexpected FIR operand domain"
    );
    ensure!(
        required_str(effect, "/input_type")? == "int32",
        "unexpected FIR input type"
    );
    ensure!(
        required_str(effect, "/output_type")? == "int32",
        "unexpected FIR output type"
    );
    ensure!(
        required_str(effect, "/overflow")? == "wrap_low_32_bits",
        "unexpected overflow rule"
    );
    ensure!(
        required_str(effect, "/runtime_exception")? == "none",
        "unexpected runtime exception effect"
    );
    ensure!(
        effect.get("pops") == Some(&json!(["int32", "int32"])),
        "FSL iadd must pop two int32 values"
    );
    ensure!(
        effect.get("pushes") == Some(&json!(["int32"])),
        "FSL iadd must push one int32 value"
    );

    let provenance = pattern
        .get("provenance")
        .and_then(Value::as_array)
        .context("FSL iadd has no provenance")?;
    ensure!(!provenance.is_empty(), "FSL iadd provenance is empty");
    let sources = package
        .get("sources")
        .and_then(Value::as_array)
        .context("FSL package has no source catalog")?;
    let mut resolved_sources = Vec::new();
    for evidence in provenance {
        let source_id = evidence
            .get("source_id")
            .and_then(Value::as_str)
            .context("FSL provenance entry has no source_id")?;
        let source = sources
            .iter()
            .find(|source| source.get("source_id").and_then(Value::as_str) == Some(source_id))
            .with_context(|| format!("FSL provenance references unknown source {source_id:?}"))?;
        ensure!(
            source.get("kind").and_then(Value::as_str) == Some("primary_specification"),
            "FSL iadd parity probe requires primary-specification provenance"
        );
        ensure!(
            source
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|url| url.starts_with("https://")),
            "FSL provenance source must use HTTPS"
        );
        resolved_sources.push(source.clone());
    }

    let profile_id = required_str(&package, "/profile_id")?.to_owned();
    let source_path = required_str(&package, "/compiled_from/path")?.to_owned();
    let source_sha256 = required_str(&package, "/compiled_from/sha256")?.to_owned();
    ensure!(
        source_sha256.len() == 64 && source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "FSL source SHA-256 must be a 64-character hex digest"
    );

    let pattern = FslPattern {
        profile_id,
        source_path,
        source_sha256,
        pattern_id: pattern_id.to_owned(),
        mnemonic: mnemonic.to_owned(),
        opcode: opcode as u8,
        effect: effect.clone(),
        provenance: Value::Array(provenance.clone()),
        sources: Value::Array(resolved_sources),
    };
    Ok(pattern)
}

fn verify_fission_lift(pattern: &FslPattern) -> Result<(Value, Vec<PcodeOp>)> {
    let frontend = RuntimeSleighFrontend::new_for_language("JVM")?;
    ensure!(
        frontend.status() == RuntimeFrontendStatus::ExecutableCandidate,
        "Fission JVM frontend is not an executable candidate"
    );

    let (instruction, pcode, length, details) = frontend
        .decode_instruction_and_lift_with_context_override(
            &[pattern.opcode],
            BYTECODE_ADDRESS,
            None,
        )
        .context("decode and lift the FSL opcode through Fission's JVM SLA runtime")?;
    ensure!(
        instruction.mnemonic == pattern.mnemonic,
        "FSL and Fission mnemonics differ"
    );
    ensure!(
        instruction.bytes == [pattern.opcode],
        "Fission decoded different input bytes"
    );
    ensure!(
        length == 1 && instruction.length == 1,
        "Fission decoded an unexpected instruction length"
    );
    ensure!(
        details.template_source == Some(CompiledTemplateSource::SpecDerived),
        "Fission lift did not use its checked-in SLA ConstructTpl"
    );

    let loads = pcode
        .iter()
        .filter(|op| op.opcode == PcodeOpcode::Load)
        .collect::<Vec<_>>();
    let load_outputs = loads
        .iter()
        .filter_map(|op| op.output.as_ref())
        .filter(|vn| vn.size == 4)
        .collect::<Vec<_>>();
    ensure!(
        loads.len() == 2 && load_outputs.len() == 2,
        "Fission iadd did not load two 32-bit stack values"
    );

    let value_adds = pcode
        .iter()
        .filter(|op| {
            op.opcode == PcodeOpcode::IntAdd
                && op.output.as_ref().is_some_and(|output| output.size == 4)
                && op.inputs.len() == 2
                && op.inputs.iter().all(|input| {
                    input.size == 4
                        && !input.is_constant
                        && load_outputs.iter().any(|loaded| *loaded == input)
                })
        })
        .collect::<Vec<_>>();
    ensure!(
        value_adds.len() == 1,
        "Fission P-code has no unique iadd over the two loaded values"
    );
    let sum = value_adds[0]
        .output
        .as_ref()
        .context("Fission integer add has no output")?;

    let stores = pcode
        .iter()
        .filter(|op| op.opcode == PcodeOpcode::Store)
        .collect::<Vec<_>>();
    ensure!(
        stores.len() == 1,
        "Fission iadd did not perform one stack store"
    );
    ensure!(
        stores[0].inputs.last() == Some(sum),
        "Fission stack store does not consume the iadd result"
    );

    let output = json!({
        "runtime_status": frontend.status().as_str(),
        "decoded_instruction": instruction,
        "length": length,
        "template_source": "SpecDerived",
        "checks": {
            "two_32_bit_stack_loads": true,
            "one_32_bit_add_consuming_both_loads": true,
            "one_stack_store_consuming_add_result": true
        }
    });
    Ok((output, pcode))
}

fn usage() -> ! {
    eprintln!("usage: fsl_jvm_iadd_parity PATTERN_PACKAGE.json");
    std::process::exit(2);
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let Some(package_path) = args.next().map(PathBuf::from) else {
        usage();
    };
    if args.next().is_some() {
        bail!("expected one FSL pattern package path");
    }

    let pattern = read_fsl_pattern(&package_path)?;
    let (fission_runtime, pcode) = verify_fission_lift(&pattern)?;
    let report = json!({
        "format": "fission_fsl_jvm_iadd_parity_probe",
        "status": "parity_verified",
        "fsl": {
            "profile_id": pattern.profile_id,
            "source_path": pattern.source_path,
            "source_sha256": pattern.source_sha256,
            "pattern_id": pattern.pattern_id,
            "provenance": pattern.provenance,
            "sources": pattern.sources,
            "fir_effect_candidate": pattern.effect
        },
        "fission_runtime": fission_runtime,
        "pcode": pcode,
        "scope": "One synthetic JVM opcode byte. This does not exercise class-file parsing, verifier frames, CFG construction, or whole-method decompilation."
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
