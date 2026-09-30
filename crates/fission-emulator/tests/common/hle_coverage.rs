use fission_emulator::arch::ArchInfo;
use fission_emulator::metrics::EmulatorMetrics;
use fission_emulator::os::linux::{syscall_abi, syscall_conv::SyscallAbi};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HleCoverageReport {
    pub schema_version: u32,
    pub binaries: Vec<HleCoverageRecord>,
}

#[derive(Debug, Serialize)]
pub struct HleCoverageRecord {
    pub binary: String,
    pub guest_os: String,
    pub guest_abi: String,
    /// Process termination is recorded independently of unknown behavior.
    pub process_status: String,
    pub exit_reason: Option<String>,
    pub exit_code: Option<u32>,
    pub instructions: u64,
    pub syscalls: Vec<SyscallCoverage>,
    pub api_misses: Vec<NamedCount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyscallCoverage {
    pub guest_number: u64,
    pub name: String,
    pub count: u64,
    pub unhandled_count: u64,
}

#[derive(Debug, Serialize)]
pub struct NamedCount {
    pub name: String,
    pub count: u64,
}

#[allow(dead_code)]
pub fn failed_record(binary: &str, error: String) -> HleCoverageRecord {
    HleCoverageRecord {
        binary: binary.to_string(),
        guest_os: "unknown".to_string(),
        guest_abi: "unknown".to_string(),
        process_status: "setup_error".to_string(),
        exit_reason: None,
        exit_code: None,
        instructions: 0,
        syscalls: Vec::new(),
        api_misses: Vec::new(),
        error: Some(error),
    }
}

pub fn record_from_run(
    binary: &str,
    guest_os: &str,
    arch: &ArchInfo,
    halted: bool,
    exit_code: Option<u32>,
    error: Option<String>,
    metrics: &EmulatorMetrics,
) -> HleCoverageRecord {
    let syscall_abi = SyscallAbi::for_arch(arch);
    let unknown = &metrics.unknown_syscalls;
    let syscalls = metrics
        .guest_syscalls
        .iter()
        .map(|(&guest_number, &count)| {
            let canonical = syscall_abi.canonical_number(guest_number);
            let unknown_key = canonical.unwrap_or(guest_number);
            let unhandled_count = unknown.get(&unknown_key).copied().unwrap_or(0).min(count);
            SyscallCoverage {
                guest_number,
                name: syscall_name(arch, guest_number, canonical),
                count,
                unhandled_count,
            }
        })
        .collect();
    let api_misses = metrics
        .hle_misses
        .iter()
        .map(|(name, &count)| NamedCount {
            name: name.clone(),
            count,
        })
        .collect();
    let exit_reason = metrics.exit_reason.clone();
    let process_status = if error.is_some() {
        "error"
    } else if halted && exit_code.is_some() {
        "process_exit"
    } else if halted {
        "halted"
    } else if exit_reason.as_deref() == Some("max_inst") {
        "instruction_budget"
    } else {
        "stopped"
    };

    HleCoverageRecord {
        binary: binary.to_string(),
        guest_os: guest_os.to_string(),
        guest_abi: abi_name(guest_os, arch),
        process_status: process_status.to_string(),
        exit_reason,
        exit_code,
        instructions: metrics.instructions,
        syscalls,
        api_misses,
        error,
    }
}

fn abi_name(os: &str, arch: &ArchInfo) -> String {
    if os == "linux" {
        let table = if arch.name.starts_with("AARCH64") {
            "asm-generic"
        } else if arch.name.starts_with("x86:LE:64") {
            "x86-64"
        } else if arch.name.starts_with("x86:LE:32") {
            "i386"
        } else if arch.name.starts_with("ARM:") {
            "arm-eabi"
        } else {
            "legacy"
        };
        format!("linux/{table}/{}", arch.name)
    } else {
        format!("{os}/{}", arch.name)
    }
}

fn syscall_name(arch: &ArchInfo, guest_number: u64, canonical: Option<u64>) -> String {
    if arch.name.starts_with("AARCH64") {
        if let Some(spec) = canonical.and_then(syscall_abi::spec) {
            return spec.name.to_string();
        }
        if let Some(name) = syscall_abi::generic_syscall_name(guest_number) {
            return name.to_string();
        }
    } else if arch.name.starts_with("x86:LE:64") {
        if let Some(spec) = syscall_abi::spec(guest_number) {
            return spec.name.to_string();
        }
    }
    format!("syscall_{guest_number}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_keeps_guest_number_name_and_unknown_behavior_separate_from_exit() {
        let arch = ArchInfo::x86_64_sysv();
        let mut metrics = EmulatorMetrics::default();
        metrics.note_guest_syscall(78);
        metrics.note_unknown_syscall(78);
        let row = record_from_run("sample.elf", "linux", &arch, true, Some(0), None, &metrics);
        assert_eq!(row.process_status, "process_exit");
        assert_eq!(row.syscalls.len(), 1);
        assert_eq!(row.syscalls[0].guest_number, 78);
        assert_eq!(row.syscalls[0].name, "getdents");
        assert_eq!(row.syscalls[0].count, 1);
        assert_eq!(row.syscalls[0].unhandled_count, 1);
    }

    #[test]
    fn report_preserves_windows_api_names() {
        let arch = ArchInfo::x86_64_win();
        let mut metrics = EmulatorMetrics::default();
        metrics.note_hle_miss("RegCloseKey");
        let row = record_from_run(
            "sample.exe",
            "windows",
            &arch,
            true,
            Some(0),
            None,
            &metrics,
        );
        assert_eq!(row.api_misses.len(), 1);
        assert_eq!(row.api_misses[0].name, "RegCloseKey");
        assert_eq!(row.api_misses[0].count, 1);
    }

    #[test]
    fn report_names_unhandled_aarch64_generic_calls_by_guest_number() {
        let arch = ArchInfo::aarch64();
        let mut metrics = EmulatorMetrics::default();
        for number in [99, 293] {
            metrics.note_guest_syscall(number);
            metrics.note_unknown_syscall(number);
        }
        let row = record_from_run("sample", "linux", &arch, true, Some(0), None, &metrics);
        assert_eq!(row.syscalls[0].name, "set_robust_list");
        assert_eq!(row.syscalls[0].guest_number, 99);
        assert_eq!(row.syscalls[0].unhandled_count, 1);
        assert_eq!(row.syscalls[1].name, "rseq");
        assert_eq!(row.syscalls[1].guest_number, 293);
        assert_eq!(row.syscalls[1].unhandled_count, 1);
    }

    #[test]
    fn report_does_not_label_arm32_eabi_numbers_as_asm_generic() {
        let arch = ArchInfo::arm32();
        let mut metrics = EmulatorMetrics::default();
        metrics.note_guest_syscall(78);
        metrics.note_unknown_syscall(78);
        let row = record_from_run("sample", "linux", &arch, true, Some(0), None, &metrics);
        assert_eq!(row.guest_abi, "linux/arm-eabi/ARM:LE:32:v7");
        assert_eq!(row.syscalls[0].name, "syscall_78");
        assert_eq!(row.syscalls[0].unhandled_count, 1);
    }
}
