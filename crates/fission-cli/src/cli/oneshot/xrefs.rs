//! Canonical xref index emission (`fission-static::xref_index`).

use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::{FunctionXrefsSummary, build_xref_index};
use serde_json::json;
use std::io::Write;

use crate::cli::args::OneShotArgs;

pub(super) fn run_xrefs(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    let include_disasm = !cli.xref_no_disassembly;
    let idx = build_xref_index(binary, include_disasm);
    let summary = idx.summary();

    let mut stdout = std::io::stdout().lock();

    if cli.json {
        let mut payload = json!({
            "summary": summary,
            "refs": idx.refs,
        });

        if let Some(fa) = cli.xref_function {
            match idx.function_summary_for(binary, fa, 0x100) {
                Some(fs) => {
                    payload["function"] =
                        serde_json::to_value(&fs).context("serialize function xref slice")?;
                }
                None => {
                    payload["function"] = serde_json::Value::Null;
                    payload["function_note"] = json!(
                        "no discovered function entry contains this VA for aggregation buckets"
                    );
                }
            }
        }

        let text = serde_json::to_string_pretty(&payload).context("serialize xrefs JSON")?;
        println!("{}", text);
        return Ok(());
    }

    writeln!(
        stdout,
        "xref_index: total={} calls={} jumps={} data={} imports={} exports={} strings={} globals={}; relocations={}",
        summary.total,
        summary.calls,
        summary.jumps,
        summary.data,
        summary.imports,
        summary.exports,
        summary.strings,
        summary.globals,
        summary.relocations,
    )
    .context("write xref summary")?;

    if let Some(note) = &summary.relocation_note {
        writeln!(stdout, "note: {}", note).context("write xref note")?;
    }

    if let Some(fa) = cli.xref_function {
        match idx.function_summary_for(binary, fa, 0x100) {
            Some(fs) => print_function_slice_text(&mut stdout, binary, &idx, fa, &fs)?,
            None => writeln!(
                stdout,
                "(no function bucket for --function 0x{:x}; discovery profile may omit this entry)",
                fa
            )
            .context("write function xref miss")?,
        }
    }

    writeln!(
        stdout,
        "hint: pass --json for full records (`refs`) and optional `function` slice"
    )
    .context("write xref hint")?;

    Ok(())
}

/// The per-function slice, spelled out.
///
/// This used to print counts -- `callers=0 strings=8` -- which answers
/// "how many" for a question that is always "which". Getting the answer meant
/// dumping `--json` and joining ids to records by hand.
fn print_function_slice_text(
    w: &mut std::io::StdoutLock<'_>,
    binary: &LoadedBinary,
    idx: &fission_static::analysis::XrefIndex,
    entry: u64,
    fs: &FunctionXrefsSummary,
) -> Result<()> {
    writeln!(
        w,
        "function 0x{:x}  {}",
        entry,
        function_label(binary, entry)
    )
    .context("write function xref slice")?;
    let groups: [(&str, &Vec<u32>); 7] = [
        ("callers", &fs.callers),
        ("calls", &fs.calls_out),
        ("jumps", &fs.jumps_out),
        ("strings", &fs.strings),
        ("globals read", &fs.globals_read),
        ("globals written", &fs.globals_written),
        ("imports used", &fs.imports_used),
    ];
    for (title, ids) in groups {
        if ids.is_empty() {
            continue;
        }
        writeln!(w, "  {title} ({}):", ids.len()).context("write xref group")?;
        for id in ids {
            let Some(record) = idx.refs.get(*id as usize) else {
                continue;
            };
            writeln!(
                w,
                "    0x{:012x} -> {}",
                record.source.address,
                describe_target(binary, record)
            )
            .context("write xref row")?;
        }
    }
    Ok(())
}

/// A function's name, or its address when discovery never named it.
fn function_label(binary: &LoadedBinary, address: u64) -> String {
    match binary.function_at_exact(address) {
        Some(function) if !function.name.is_empty() => function.name.clone(),
        _ => format!("0x{address:x}"),
    }
}

/// What one record points at, in the most specific terms available.
fn describe_target(binary: &LoadedBinary, record: &fission_static::analysis::XrefRecord) -> String {
    // An import carries its own symbol; nothing the loader knows beats it.
    if let Some(symbol) = &record.target.symbol {
        return symbol.clone();
    }
    let Some(address) = record.target.address else {
        return "?".to_string();
    };
    if let Some(function) = binary.function_at_exact(address)
        && !function.name.is_empty()
    {
        return format!("{} (0x{address:x})", function.name);
    }
    // A loader-layer string record carries its text in the evidence note.
    if let Some(preview) = record
        .evidence
        .note
        .as_deref()
        .and_then(|note| note.split_once("preview="))
        .map(|(_, preview)| preview)
    {
        return format!("0x{address:x}  {preview}");
    }
    // A disassembly-layer read has no note, but the loader scanned the same
    // bytes and kept the text. Without this the strings group of a function
    // slice is a column of addresses -- the one group whose whole value is
    // being able to read it.
    if let Some(text) = binary.string_map.get(&address) {
        return format!("0x{address:x}  \"{}\"", text.escape_default());
    }
    format!("0x{address:x}")
}
