//! Raw instruction throughput, on a workload another engine can be given
//! byte for byte.
//!
//! Ignored by default: it is a measurement, not an assertion, and it takes
//! seconds. Run it with
//!
//! ```text
//! cargo test --release -p fission-emulator --test throughput_bench -- --ignored --nocapture
//! ```
//!
//! The point is comparability. It executes a fixed instruction sequence for a
//! fixed instruction count with no loader, no OS layer and no observers, so
//! the number is this emulator's execution core and nothing else -- which is
//! the only thing a comparison against Unicorn (QEMU's TCG as a library) or
//! Ghidra's `PcodeEmulator` can be about.

use std::path::PathBuf;
use std::time::Instant;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::BareMetalEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

/// Where the loop is placed, and how much room it gets.
const CODE_BASE: u64 = 0x1000_0000;
const STACK_TOP: u64 = 0x2000_0000;

/// A three-instruction loop, x86-64:
///
/// ```text
/// 0: 83 c0 01     add eax, 1
/// 3: 83 e9 01     sub ecx, 1
/// 6: eb f8        jmp 0
/// ```
///
/// Deliberately dull: no memory traffic, no flags anyone reads, nothing an
/// optimiser on either side can fold away. What it measures is the cost of
/// *getting to* the next instruction.
const LOOP_CODE: &[u8] = &[0x83, 0xC0, 0x01, 0x83, 0xE9, 0x01, 0xEB, 0xF8];

/// The same shape with a memory round trip in it, which is where the two
/// engines differ structurally -- ours calls out to the host for every access.
///
/// ```text
/// 0: 48 89 45 00  mov [rbp], rax
/// 4: 48 8b 45 00  mov rax, [rbp]
/// 8: 83 e9 01     sub ecx, 1
/// b: eb f3        jmp 0
/// ```
const MEM_CODE: &[u8] = &[
    0x48, 0x89, 0x45, 0x00, 0x48, 0x8B, 0x45, 0x00, 0x83, 0xE9, 0x01, 0xEB, 0xF3,
];

fn host_binary() -> Option<PathBuf> {
    // Any x86-64 image will do: it is only here to give the frontend a load
    // spec to resolve a language from. The code that runs is written in below.
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    path.is_file().then_some(path)
}

fn run(code: &[u8], instructions: u64) -> Option<(u64, f64)> {
    let path = host_binary()?;
    let binary = LoadedBinary::from_file(&path).ok()?;
    let load_spec = binary.load_spec()?.clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .ok()?
        .into_iter()
        .next()?;
    let arch =
        ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary)).ok()?;

    let mut state = MachineState::new();
    let ram = state.ram_space();
    use fission_emulator::pcode::page_map::prot;
    state.page_map.map_region(
        CODE_BASE,
        0x1000,
        prot::VALID | prot::READ | prot::EXEC,
        true,
    );
    state.page_map.map_region(
        STACK_TOP - 0x10000,
        0x10000,
        prot::VALID | prot::READ | prot::WRITE,
        true,
    );
    state.write_space(ram, CODE_BASE, code).ok()?;

    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(BareMetalEnv::new()))
        .ok()?
        .with_max_inst(Some(instructions));
    emu.pc = CODE_BASE;
    emu.write_register_u64("RCX", instructions).ok()?;
    emu.write_register_u64("RBP", STACK_TOP - 0x1000).ok()?;
    emu.write_register_u64("RSP", STACK_TOP - 0x2000).ok()?;

    // One short run first, so translation is not being timed.
    let started = Instant::now();
    let _ = emu.run();
    let elapsed = started.elapsed().as_secs_f64();
    Some((emu.inst_count, elapsed))
}

#[test]
#[ignore = "a measurement, not an assertion"]
fn instruction_throughput() {
    for (name, code) in [("register loop", LOOP_CODE), ("memory loop", MEM_CODE)] {
        // Two points, so translation and set-up fall out of the difference.
        let small = 2_000_000u64;
        let large = 20_000_000u64;
        let Some((n1, t1)) = run(code, small) else {
            eprintln!("skipping: fixture not present");
            return;
        };
        let Some((n2, t2)) = run(code, large) else {
            return;
        };
        let delta_n = n2.saturating_sub(n1) as f64;
        let delta_t = t2 - t1;
        eprintln!(
            "{name:<16} {n1} in {t1:.3}s, {n2} in {t2:.3}s  ->  marginal {:.2}M inst/s",
            delta_n / delta_t / 1e6
        );
    }
}
