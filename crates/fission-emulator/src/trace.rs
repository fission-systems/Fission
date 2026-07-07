use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single entry in the emulator execution trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TraceEntry {
    /// A successfully decoded and lifted instruction.
    Instruction {
        pc: u64,
        bytes_hex: String,
        mnemonic: String,
        /// P-Code opcode names in execution order.
        pcode_ops: Vec<String>,
        /// Non-null only when the instruction decoded but had partial lift issues.
        decode_error: Option<String>,
    },
    /// A `CallOther` (USEROP) dispatch event.
    UseropeDispatch {
        pc: u64,
        userop_name: String,
        /// Input values resolved at dispatch time.
        input_vals: Vec<u64>,
    },
    /// An HLE function interception.
    HleDispatch {
        pc: u64,
        func_name: String,
    },
    /// Decode or lift completely failed; instruction was skipped.
    DecodeError {
        pc: u64,
        reason: String,
    },
}

/// Accumulates trace entries during sandbox execution.
pub struct ExecutionTrace {
    pub entries: Vec<TraceEntry>,
    pub enabled: bool,
}

impl ExecutionTrace {
    pub fn enabled() -> Self {
        Self { entries: Vec::new(), enabled: true }
    }

    pub fn disabled() -> Self {
        Self { entries: Vec::new(), enabled: false }
    }

    /// Record a trace entry if tracing is enabled.
    #[inline]
    pub fn push(&mut self, entry: TraceEntry) {
        if self.enabled {
            self.entries.push(entry);
        }
    }

    /// Write all trace entries as newline-delimited JSON (NDJSON) to a file.
    pub fn write_ndjson(&self, path: &std::path::Path) -> anyhow::Result<()> {
        use std::io::Write;
        use anyhow::Context;
        let file = std::fs::File::create(path)
            .with_context(|| format!("Failed to create trace file: {}", path.display()))?;
        let mut writer = std::io::BufWriter::new(file);
        for entry in &self.entries {
            serde_json::to_writer(&mut writer, entry)?;
            writeln!(writer)?;
        }
        Ok(())
    }

    /// Print a brief summary of the trace to stderr.
    pub fn print_summary(&self) {
        let total = self.entries.len();
        let mut decode_errors = 0usize;
        let mut userop_dispatches = 0usize;
        let mut hle_dispatches = 0usize;
        let mut opcode_freq: HashMap<String, usize> = HashMap::new();

        for entry in &self.entries {
            match entry {
                TraceEntry::Instruction { pcode_ops, .. } => {
                    for op in pcode_ops {
                        *opcode_freq.entry(op.clone()).or_default() += 1;
                    }
                }
                TraceEntry::DecodeError { .. } => decode_errors += 1,
                TraceEntry::UseropeDispatch { .. } => userop_dispatches += 1,
                TraceEntry::HleDispatch { .. } => hle_dispatches += 1,
            }
        }

        eprintln!("╔══════════════════════════════════════════╗");
        eprintln!("║       Fission Sandbox Trace Summary      ║");
        eprintln!("╠══════════════════════════════════════════╣");
        eprintln!("║  Total trace entries : {:>6}             ║", total);
        eprintln!("║  Decode errors       : {:>6}             ║", decode_errors);
        eprintln!("║  USEROP dispatches   : {:>6}             ║", userop_dispatches);
        eprintln!("║  HLE dispatches      : {:>6}             ║", hle_dispatches);
        eprintln!("╠══════════════════════════════════════════╣");
        eprintln!("║  Top P-Code opcodes:                     ║");

        let mut freq_vec: Vec<_> = opcode_freq.into_iter().collect();
        freq_vec.sort_by(|a, b| b.1.cmp(&a.1));
        for (opcode, count) in freq_vec.iter().take(10) {
            eprintln!("║  {:>8}x  {:<28} ║", count, opcode);
        }
        eprintln!("╚══════════════════════════════════════════╝");
    }
}
