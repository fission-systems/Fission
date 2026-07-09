//! Semantic Replay Diff (SRD) — structured multi-run emulator regression.
//!
//! Capture owner-native facts from a finished [`crate::core::Emulator`] run, then
//! diff two captures into a layered delta (SLEIGH/JIT vs page/memory vs HLE/OS
//! vs control-flow vs path constraints). This is the emulator analogue of
//! decompiler quality-loop language: compare **contracts**, not raw register dumps.
//!
//! Layout-specific mallocng probes are optional and fixture-oriented (same
//! spirit as `diag_*` CRT tests). They must not be treated as universal heap ABI.

use crate::core::Emulator;
use crate::metrics::EmulatorMetrics;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which Fission layer is the best first owner for a given delta field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerLayer {
    /// Decode/lift / ConstructTpl / inst_next issues.
    SleighLift,
    /// Cranelift TB, relative BRANCH remap, fuse exits.
    JitRuntime,
    /// Page map, mmap/brk/mprotect, silent store faults.
    PageMem,
    /// Syscall / libc HLE, unknown syscall, LOCK/UNLOCK userops.
    HleOs,
    /// stop_pc / halt / instruction budget / control progress.
    ControlFlow,
    /// Solver assertions / concolic path summary.
    PathConstraints,
    /// Catch-all when multiple layers are implicated equally.
    Mixed,
}

/// Optional musl mallocng BSS probes (static x86-64 fixture layout).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MallocngProbe {
    pub init_done: u32,
    pub secret: u64,
    pub freelist: u64,
    pub avail_slots: u64,
    pub page_mask: u64,
    pub brk_cur: u64,
    /// First 8 bin heads at the fixture `bins` base.
    pub bin_heads: Vec<u64>,
    /// freeable dword on bin[5] meta when bin head non-zero; else 0.
    pub freeable_bin5: u32,
    pub probe_base_bins: u64,
}

/// One finished run, serializable for disk comparison across commits/policies.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticReplaySnapshot {
    pub schema_version: u32,
    pub label: String,
    pub binary: String,
    pub format: String,
    pub max_inst: Option<u64>,
    pub halt_requested: bool,
    pub pc: u64,
    pub stop_pc: u64,
    pub inst_count: u64,
    pub pcode_ops: u64,
    pub fs_base: u64,
    pub brk: u64,
    pub brk_base: u64,
    pub exit_reason: Option<String>,
    pub metrics: EmulatorMetrics,
    /// Syscall counts (copy of metrics.syscalls for stable top-level access).
    pub syscalls: BTreeMap<u64, u64>,
    pub unknown_syscalls: BTreeMap<u64, u64>,
    pub hle_misses: BTreeMap<String, u64>,
    pub userops: BTreeMap<String, u64>,
    /// Solver path assertions (pretty-printed; empty on pure concrete runs).
    pub path_assertions: Vec<String>,
    pub path_assertion_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mallocng: Option<MallocngProbe>,
}

/// Options for [`SemanticReplaySnapshot::capture`].
#[derive(Clone, Debug)]
pub struct CaptureOpts {
    pub label: String,
    pub binary: String,
    /// When true, read static musl mallocng BSS probes (fixture addresses).
    pub probe_mallocng: bool,
    /// Override bins base (default: static CRT fixture `0x1007f68`).
    pub mallocng_bins_base: u64,
}

impl Default for CaptureOpts {
    fn default() -> Self {
        Self {
            label: "run".into(),
            binary: String::new(),
            probe_mallocng: false,
            mallocng_bins_base: 0x1007_f68,
        }
    }
}

/// One field-level change with owner hint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDelta {
    pub field: String,
    pub left: String,
    pub right: String,
    pub owner: OwnerLayer,
}

/// Structured delta between two snapshots.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticReplayDelta {
    pub schema_version: u32,
    pub left_label: String,
    pub right_label: String,
    pub identical: bool,
    pub field_deltas: Vec<FieldDelta>,
    /// Distinct owners that appear in `field_deltas` (sorted).
    pub owners_touched: Vec<OwnerLayer>,
    /// Best single primary owner for triage (heuristic).
    pub primary_owner: OwnerLayer,
    pub summary: String,
}

impl SemanticReplaySnapshot {
    pub const SCHEMA_VERSION: u32 = 1;

    /// Capture owner-native facts from a finished emulator.
    pub fn capture(emu: &mut Emulator, opts: CaptureOpts) -> Self {
        let path_assertions: Vec<String> = emu
            .solver
            .assertions
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        let path_assertion_count = path_assertions.len();
        let mallocng = if opts.probe_mallocng {
            Some(probe_mallocng(emu, opts.mallocng_bins_base))
        } else {
            None
        };
        Self {
            schema_version: Self::SCHEMA_VERSION,
            label: opts.label,
            binary: opts.binary,
            format: emu.binary.format.clone(),
            max_inst: emu.max_inst,
            halt_requested: emu.halt_requested,
            pc: emu.pc,
            stop_pc: emu.metrics.stop_pc,
            inst_count: emu.inst_count,
            pcode_ops: emu.pcode_ops,
            fs_base: emu.fs_base,
            brk: emu.state.page_map.brk,
            brk_base: emu.state.page_map.brk_base,
            exit_reason: emu.metrics.exit_reason.clone(),
            metrics: emu.metrics.clone(),
            syscalls: emu.metrics.syscalls.clone(),
            unknown_syscalls: emu.metrics.unknown_syscalls.clone(),
            hle_misses: emu.metrics.hle_misses.clone(),
            userops: emu.metrics.userops.clone(),
            path_assertions,
            path_assertion_count,
            mallocng,
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn write_json_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = self
            .to_json_pretty()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn read_json_file(path: &std::path::Path) -> std::io::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Self::from_json(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl SemanticReplayDelta {
    pub const SCHEMA_VERSION: u32 = 1;

    /// Diff two captures. Field order is deterministic for golden-ish dumps.
    pub fn diff(left: &SemanticReplaySnapshot, right: &SemanticReplaySnapshot) -> Self {
        let mut field_deltas = Vec::new();

        push_scalar(
            &mut field_deltas,
            "stop_pc",
            &format!("0x{:X}", left.stop_pc),
            &format!("0x{:X}", right.stop_pc),
            OwnerLayer::ControlFlow,
        );
        push_scalar(
            &mut field_deltas,
            "pc",
            &format!("0x{:X}", left.pc),
            &format!("0x{:X}", right.pc),
            OwnerLayer::ControlFlow,
        );
        push_scalar(
            &mut field_deltas,
            "halt_requested",
            &left.halt_requested.to_string(),
            &right.halt_requested.to_string(),
            OwnerLayer::ControlFlow,
        );
        push_scalar(
            &mut field_deltas,
            "inst_count",
            &left.inst_count.to_string(),
            &right.inst_count.to_string(),
            OwnerLayer::ControlFlow,
        );
        push_scalar(
            &mut field_deltas,
            "pcode_ops",
            &left.pcode_ops.to_string(),
            &right.pcode_ops.to_string(),
            OwnerLayer::JitRuntime,
        );
        push_opt_str(
            &mut field_deltas,
            "exit_reason",
            left.exit_reason.as_deref(),
            right.exit_reason.as_deref(),
            OwnerLayer::ControlFlow,
        );
        push_scalar(
            &mut field_deltas,
            "fs_base",
            &format!("0x{:X}", left.fs_base),
            &format!("0x{:X}", right.fs_base),
            OwnerLayer::HleOs,
        );
        push_scalar(
            &mut field_deltas,
            "brk",
            &format!("0x{:X}", left.brk),
            &format!("0x{:X}", right.brk),
            OwnerLayer::PageMem,
        );
        push_scalar(
            &mut field_deltas,
            "brk_base",
            &format!("0x{:X}", left.brk_base),
            &format!("0x{:X}", right.brk_base),
            OwnerLayer::PageMem,
        );

        // Metrics core
        push_scalar(
            &mut field_deltas,
            "metrics.decode_errors",
            &left.metrics.decode_errors.to_string(),
            &right.metrics.decode_errors.to_string(),
            OwnerLayer::SleighLift,
        );
        push_scalar(
            &mut field_deltas,
            "metrics.memory_faults",
            &left.metrics.memory_faults.to_string(),
            &right.metrics.memory_faults.to_string(),
            OwnerLayer::PageMem,
        );
        push_scalar(
            &mut field_deltas,
            "metrics.tbs_compiled",
            &left.metrics.tbs_compiled.to_string(),
            &right.metrics.tbs_compiled.to_string(),
            OwnerLayer::JitRuntime,
        );
        push_scalar(
            &mut field_deltas,
            "metrics.hard_chains",
            &left.metrics.hard_chains.to_string(),
            &right.metrics.hard_chains.to_string(),
            OwnerLayer::JitRuntime,
        );

        map_u64_delta(
            &mut field_deltas,
            "syscalls",
            &left.syscalls,
            &right.syscalls,
            OwnerLayer::HleOs,
        );
        map_u64_delta(
            &mut field_deltas,
            "unknown_syscalls",
            &left.unknown_syscalls,
            &right.unknown_syscalls,
            OwnerLayer::HleOs,
        );
        map_string_delta(
            &mut field_deltas,
            "hle_misses",
            &left.hle_misses,
            &right.hle_misses,
            OwnerLayer::HleOs,
        );
        map_string_delta(
            &mut field_deltas,
            "userops",
            &left.userops,
            &right.userops,
            OwnerLayer::HleOs,
        );
        map_string_delta(
            &mut field_deltas,
            "unimplemented_opcodes",
            &left.metrics.unimplemented_opcodes,
            &right.metrics.unimplemented_opcodes,
            OwnerLayer::JitRuntime,
        );

        push_scalar(
            &mut field_deltas,
            "path_assertion_count",
            &left.path_assertion_count.to_string(),
            &right.path_assertion_count.to_string(),
            OwnerLayer::PathConstraints,
        );
        if left.path_assertions != right.path_assertions {
            field_deltas.push(FieldDelta {
                field: "path_assertions".into(),
                left: format!("{} assertion(s)", left.path_assertion_count),
                right: format!("{} assertion(s)", right.path_assertion_count),
                owner: OwnerLayer::PathConstraints,
            });
        }

        match (&left.mallocng, &right.mallocng) {
            (Some(a), Some(b)) => {
                push_scalar(
                    &mut field_deltas,
                    "mallocng.init_done",
                    &a.init_done.to_string(),
                    &b.init_done.to_string(),
                    OwnerLayer::PageMem,
                );
                push_scalar(
                    &mut field_deltas,
                    "mallocng.page_mask",
                    &format!("0x{:X}", a.page_mask),
                    &format!("0x{:X}", b.page_mask),
                    OwnerLayer::PageMem,
                );
                push_scalar(
                    &mut field_deltas,
                    "mallocng.freeable_bin5",
                    &a.freeable_bin5.to_string(),
                    &b.freeable_bin5.to_string(),
                    OwnerLayer::PageMem,
                );
                push_scalar(
                    &mut field_deltas,
                    "mallocng.brk_cur",
                    &format!("0x{:X}", a.brk_cur),
                    &format!("0x{:X}", b.brk_cur),
                    OwnerLayer::PageMem,
                );
                let la: BTreeMap<u64, u64> = a
                    .bin_heads
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| **h != 0)
                    .map(|(i, h)| (i as u64, *h))
                    .collect();
                let ra: BTreeMap<u64, u64> = b
                    .bin_heads
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| **h != 0)
                    .map(|(i, h)| (i as u64, *h))
                    .collect();
                map_u64_delta(
                    &mut field_deltas,
                    "mallocng.bin_heads",
                    &la,
                    &ra,
                    OwnerLayer::PageMem,
                );
            }
            (None, Some(_)) => field_deltas.push(FieldDelta {
                field: "mallocng".into(),
                left: "absent".into(),
                right: "present".into(),
                owner: OwnerLayer::PageMem,
            }),
            (Some(_), None) => field_deltas.push(FieldDelta {
                field: "mallocng".into(),
                left: "present".into(),
                right: "absent".into(),
                owner: OwnerLayer::PageMem,
            }),
            (None, None) => {}
        }

        let identical = field_deltas.is_empty();
        let mut owners_touched: Vec<OwnerLayer> =
            field_deltas.iter().map(|d| d.owner).collect();
        owners_touched.sort();
        owners_touched.dedup();
        let primary_owner = pick_primary_owner(&field_deltas);
        let summary = if identical {
            format!(
                "identical runs ({} vs {})",
                left.label, right.label
            )
        } else {
            format!(
                "{} field delta(s); primary_owner={:?}; owners={:?}",
                field_deltas.len(),
                primary_owner,
                owners_touched
            )
        };

        Self {
            schema_version: Self::SCHEMA_VERSION,
            left_label: left.label.clone(),
            right_label: right.label.clone(),
            identical,
            field_deltas,
            owners_touched,
            primary_owner,
            summary,
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

fn probe_mallocng(emu: &mut Emulator, bins_base: u64) -> MallocngProbe {
    let rd8 = |emu: &mut Emulator, a: u64| -> u64 {
        let b = emu
            .state
            .read_space(emu.state.ram_space(), a, 8)
            .unwrap_or_else(|_| vec![0; 8]);
        u64::from_le_bytes(b.try_into().unwrap_or([0; 8]))
    };
    let rd4 = |emu: &mut Emulator, a: u64| -> u32 {
        let b = emu
            .state
            .read_space(emu.state.ram_space(), a, 4)
            .unwrap_or_else(|_| vec![0; 4]);
        u32::from_le_bytes(b.try_into().unwrap_or([0; 4]))
    };
    // Layout anchors used by static CRT diag (x64_static_printf_malloc.elf).
    let init_done = rd4(emu, 0x1007_f20);
    let secret = rd8(emu, 0x1007_f18);
    let freelist = rd8(emu, 0x1007_f28);
    let avail_slots = rd8(emu, 0x1007_f38);
    let page_mask = rd8(emu, 0x1007_f40);
    let brk_cur = rd8(emu, 0x1008_2b0);
    let mut bin_heads = Vec::with_capacity(8);
    for i in 0..8u64 {
        bin_heads.push(rd8(emu, bins_base + i * 8));
    }
    let freeable_bin5 = if bin_heads.get(5).copied().unwrap_or(0) != 0 {
        rd4(emu, bin_heads[5] + 0x18)
    } else {
        0
    };
    MallocngProbe {
        init_done,
        secret,
        freelist,
        avail_slots,
        page_mask,
        brk_cur,
        bin_heads,
        freeable_bin5,
        probe_base_bins: bins_base,
    }
}

fn push_scalar(
    out: &mut Vec<FieldDelta>,
    field: &str,
    left: &str,
    right: &str,
    owner: OwnerLayer,
) {
    if left != right {
        out.push(FieldDelta {
            field: field.into(),
            left: left.into(),
            right: right.into(),
            owner,
        });
    }
}

fn push_opt_str(
    out: &mut Vec<FieldDelta>,
    field: &str,
    left: Option<&str>,
    right: Option<&str>,
    owner: OwnerLayer,
) {
    let l = left.unwrap_or("");
    let r = right.unwrap_or("");
    push_scalar(out, field, l, r, owner);
}

fn map_u64_delta(
    out: &mut Vec<FieldDelta>,
    prefix: &str,
    left: &BTreeMap<u64, u64>,
    right: &BTreeMap<u64, u64>,
    owner: OwnerLayer,
) {
    let mut keys: Vec<u64> = left.keys().chain(right.keys()).copied().collect();
    keys.sort_unstable();
    keys.dedup();
    for k in keys {
        let lv = left.get(&k).copied().unwrap_or(0);
        let rv = right.get(&k).copied().unwrap_or(0);
        if lv != rv {
            out.push(FieldDelta {
                field: format!("{prefix}[{k}]"),
                left: lv.to_string(),
                right: rv.to_string(),
                owner,
            });
        }
    }
}

fn map_string_delta(
    out: &mut Vec<FieldDelta>,
    prefix: &str,
    left: &BTreeMap<String, u64>,
    right: &BTreeMap<String, u64>,
    owner: OwnerLayer,
) {
    let mut keys: Vec<&String> = left.keys().chain(right.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let lv = left.get(k).copied().unwrap_or(0);
        let rv = right.get(k).copied().unwrap_or(0);
        if lv != rv {
            out.push(FieldDelta {
                field: format!("{prefix}[{k}]"),
                left: lv.to_string(),
                right: rv.to_string(),
                owner,
            });
        }
    }
}

/// Prefer HLE/page/sleigh over pure control-flow noise when multiple layers move.
fn pick_primary_owner(deltas: &[FieldDelta]) -> OwnerLayer {
    if deltas.is_empty() {
        return OwnerLayer::Mixed;
    }
    let priority = [
        OwnerLayer::SleighLift,
        OwnerLayer::JitRuntime,
        OwnerLayer::PageMem,
        OwnerLayer::HleOs,
        OwnerLayer::PathConstraints,
        OwnerLayer::ControlFlow,
        OwnerLayer::Mixed,
    ];
    let mut counts: BTreeMap<OwnerLayer, usize> = BTreeMap::new();
    for d in deltas {
        *counts.entry(d.owner).or_insert(0) += 1;
    }
    let mut best = OwnerLayer::Mixed;
    let mut best_score = -1i32;
    for (owner, n) in &counts {
        let pri = priority
            .iter()
            .position(|p| p == owner)
            .map(|i| 100 - i as i32)
            .unwrap_or(0);
        let score = (*n as i32) * 10 + pri;
        if score > best_score {
            best_score = score;
            best = *owner;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_classifies_stop_pc_as_control_flow() {
        let mut left = minimal_snap("a");
        let mut right = minimal_snap("b");
        left.stop_pc = 0x10035A3;
        right.stop_pc = 0x10060D9;
        let d = SemanticReplayDelta::diff(&left, &right);
        assert!(!d.identical);
        assert!(
            d.field_deltas
                .iter()
                .any(|f| f.field == "stop_pc" && f.owner == OwnerLayer::ControlFlow)
        );
        assert_eq!(d.primary_owner, OwnerLayer::ControlFlow);
    }

    #[test]
    fn diff_classifies_unknown_syscall_as_hle() {
        let left = minimal_snap("a");
        let mut right = minimal_snap("b");
        right.unknown_syscalls.insert(999, 1);
        let d = SemanticReplayDelta::diff(&left, &right);
        assert!(d
            .field_deltas
            .iter()
            .any(|f| f.field.starts_with("unknown_syscalls") && f.owner == OwnerLayer::HleOs));
        assert_eq!(d.primary_owner, OwnerLayer::HleOs);
    }

    #[test]
    fn diff_classifies_mallocng_freeable_as_page_mem() {
        let mut left = minimal_snap("a");
        let mut right = minimal_snap("b");
        left.mallocng = Some(MallocngProbe {
            freeable_bin5: 0,
            ..Default::default()
        });
        right.mallocng = Some(MallocngProbe {
            freeable_bin5: 30,
            bin_heads: vec![0, 0, 0, 0, 0, 0x100A018, 0, 0],
            ..Default::default()
        });
        let d = SemanticReplayDelta::diff(&left, &right);
        assert!(d.field_deltas.iter().any(|f| {
            f.field == "mallocng.freeable_bin5" && f.owner == OwnerLayer::PageMem
        }));
    }

    fn minimal_snap(label: &str) -> SemanticReplaySnapshot {
        SemanticReplaySnapshot {
            schema_version: 1,
            label: label.into(),
            binary: "t".into(),
            format: "ELF64".into(),
            max_inst: Some(1000),
            halt_requested: false,
            pc: 0,
            stop_pc: 0,
            inst_count: 0,
            pcode_ops: 0,
            fs_base: 0,
            brk: 0,
            brk_base: 0,
            exit_reason: None,
            metrics: EmulatorMetrics::default(),
            syscalls: BTreeMap::new(),
            unknown_syscalls: BTreeMap::new(),
            hle_misses: BTreeMap::new(),
            userops: BTreeMap::new(),
            path_assertions: Vec::new(),
            path_assertion_count: 0,
            mallocng: None,
        }
    }
}
