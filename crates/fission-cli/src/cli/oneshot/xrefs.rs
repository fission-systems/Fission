//! Canonical xref index emission (`fission-static::xref_index`).

use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::{build_xref_index, FunctionXrefsSummary};
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
            Some(fs) => print_function_slice_text(&mut stdout, fa, &fs)?,
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

fn print_function_slice_text(
    w: &mut std::io::StdoutLock<'_>,
    entry: u64,
    fs: &FunctionXrefsSummary,
) -> Result<()> {
    writeln!(
        w,
        "function 0x{:x}: calls_out={} callers={} jumps_out={} strings={} globals_read={} globals_written={} imports_used={}",
        entry,
        fs.calls_out.len(),
        fs.callers.len(),
        fs.jumps_out.len(),
        fs.strings.len(),
        fs.globals_read.len(),
        fs.globals_written.len(),
        fs.imports_used.len(),
    )
    .context("write function xref slice")?;
    Ok(())
}
