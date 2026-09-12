//! `CPUID` answers, and answers a processor this emulator can honour.
//!
//! SLEIGH does not emit a bare `cpuid`. The x86 specification branches on
//! `EAX` and calls a userop named for that leaf -- `cpuid_basic_info`,
//! `cpuid_Version_info`, `cpuid_brand_part1_info` -- so a handler matching
//! `"cpuid"` caught none of them. A sweep of six real programs found exactly
//! one unanswered CALLOTHER, and it was `cpuid_basic_info`: the C runtime of
//! a real program asks on startup and none of ours ever had.
//!
//! The failure mode is worse than a missing answer. The userop returns a
//! *pointer* -- the spec does `tmpptr = cpuid_<leaf>(EAX)` and then reads
//! four registers from `tmpptr + 0/4/8/12` -- so answering zero does not mean
//! "no features", it means the guest reads its feature words out of address
//! zero.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::{BareMetalEnv, env::answer_processor_userop};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn emulator() -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    Emulator::new(
        MachineState::new(),
        binary,
        sleigh,
        arch,
        Box::new(BareMetalEnv::new()),
    )
    .expect("emulator")
}

/// The four registers the instruction reads back out of what the userop
/// returned, in the order the spec reads them: EAX, EBX, EDX, ECX.
fn cpuid(emu: &mut Emulator, name: &str, leaf: u64) -> (u32, u32, u32, u32) {
    assert!(
        answer_processor_userop(emu, name, &[leaf]),
        "{name} was not answered at all"
    );
    let pointer = emu.callother_result;
    assert_ne!(
        pointer, 0,
        "{name} answered with a null pointer; the guest would read its \
         feature words out of address zero"
    );
    let ram = emu.state.ram_space();
    let bytes = emu
        .state
        .read_space(ram, pointer, 16)
        .expect("the answer is readable guest memory");
    let word = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().expect("four bytes"));
    (word(0), word(4), word(8), word(12))
}

/// Every leaf SLEIGH names is answered, not just the one a sweep happened to
/// reach.
#[test]
fn every_leaf_sleigh_names_is_answered() {
    let mut emu = emulator();
    for name in [
        "cpuid",
        "cpuid_basic_info",
        "cpuid_Version_info",
        "cpuid_cache_tlb_info",
        "cpuid_serial_info",
        "cpuid_brand_part1_info",
        "cpuid_brand_part2_info",
        "cpuid_brand_part3_info",
        "cpuid_Extended_Feature_Enumeration_info",
    ] {
        assert!(
            answer_processor_userop(&mut emu, name, &[0]),
            "{name} was not answered"
        );
        assert_ne!(emu.callother_result, 0, "{name} answered with null");
    }
}

#[test]
fn leaf_zero_reports_a_vendor_and_a_highest_leaf() {
    let mut emu = emulator();
    let (eax, ebx, edx, ecx) = cpuid(&mut emu, "cpuid_basic_info", 0);

    // The vendor string is EBX:EDX:ECX, twelve bytes, little-endian per word.
    let mut vendor = Vec::new();
    for word in [ebx, edx, ecx] {
        vendor.extend_from_slice(&word.to_le_bytes());
    }
    assert_eq!(
        String::from_utf8_lossy(&vendor),
        "GenuineIntel",
        "an unknown vendor sends runtime libraries down paths nobody tests"
    );
    assert!(
        eax >= 1,
        "the highest leaf must cover the feature leaf, got {eax}"
    );
}

/// What is claimed has to be what this emulator can honour: a program told it
/// has AVX-512 will use registers this emulator decodes to an empty handle.
#[test]
fn the_features_claimed_are_the_x86_64_baseline_and_no_more() {
    let mut emu = emulator();
    let (_, _, edx, ecx) = cpuid(&mut emu, "cpuid_Version_info", 1);

    const FPU: u32 = 1 << 0;
    const TSC: u32 = 1 << 4;
    const CMOV: u32 = 1 << 15;
    const MMX: u32 = 1 << 23;
    const FXSR: u32 = 1 << 24;
    const SSE: u32 = 1 << 25;
    const SSE2: u32 = 1 << 26;
    for (bit, what) in [
        (FPU, "FPU"),
        (TSC, "TSC"),
        (CMOV, "CMOV"),
        (MMX, "MMX"),
        (FXSR, "FXSR"),
        (SSE, "SSE"),
        (SSE2, "SSE2"),
    ] {
        assert_ne!(
            edx & bit,
            0,
            "{what} is guaranteed on x86-64 and every program assumes it"
        );
    }

    // ECX is where SSE3, SSSE3, SSE4.1, SSE4.2, AVX and the rest live.
    assert_eq!(
        ecx, 0,
        "claiming a feature beyond the baseline invites instructions this \
         emulator may not implement"
    );
}

/// A leaf above the reported maximum reads as zero, which is what a real
/// processor does -- and it still answers with a pointer rather than null.
#[test]
fn an_unknown_leaf_reads_as_zero_through_a_real_pointer() {
    let mut emu = emulator();
    let (eax, ebx, edx, ecx) = cpuid(&mut emu, "cpuid_brand_part1_info", 0x8000_0002);
    assert_eq!((eax, ebx, edx, ecx), (0, 0, 0, 0));
}

/// Each answer gets its own bytes: two calls must not hand back one buffer
/// that the second overwrote.
#[test]
fn two_answers_do_not_share_one_buffer() {
    let mut emu = emulator();

    assert!(answer_processor_userop(&mut emu, "cpuid_basic_info", &[0]));
    let first = emu.callother_result;
    assert!(answer_processor_userop(
        &mut emu,
        "cpuid_Version_info",
        &[1]
    ));
    let second = emu.callother_result;

    assert_ne!(first, second, "the second answer landed on the first");

    // And the first is still what it was.
    let ram = emu.state.ram_space();
    let bytes = emu.state.read_space(ram, first, 4).expect("still readable");
    assert_eq!(
        u32::from_le_bytes(bytes[..4].try_into().expect("four bytes")),
        1,
        "the highest-leaf answer was overwritten by the next call"
    );
}
