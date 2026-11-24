//! Taint through memory, the same in both engines.
//!
//! Two defects CSIC found (PR #1106), each in one engine:
//!
//! 1. **The interpreter panicked on a store through a tainted pointer.** The
//!    evaluator kept a symbolic-execution path of its own that read every
//!    shadow id as a solver node; under taint a shadow id is a *set* id, and
//!    the lookup in `memory_mixin.rs` unwrapped a `None`. The same path also
//!    skipped the concrete store, and loaded zero through a shadowed pointer.
//! 2. **The JIT lost direct-data taint through a store to an absolute
//!    address.** x86 lifts `mov [abs], eax` to a `COPY` into a RAM varnode,
//!    not a `STORE`. The JIT held that in a variable and wrote it at block
//!    exit -- a concrete write, which clears the shadow the op had just set.
//!    The deferral was a concrete bug too: a load through a pointer to the
//!    same bytes later in the block read the old value.
//!
//! The programs run after the fixture's `read(0, rsp, 1)` of one stdin byte
//! (`'A'`), so `[rsp]` is the taint source. Built from this repository's own
//! fixture, never a corpus binary.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::ShadowMode;
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

/// A page the programs below address absolutely.
const SCRATCH: u64 = 0x1000_0000;

fn build(code: &[u8], interpret: bool) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_concolic_branch_sys.elf");
    let mut bytes = std::fs::read(&path).expect("read fixture");
    // Right after the fixture's `read` syscall; see `taint_flow.rs`.
    bytes.splice(0x96..0x96, code.iter().copied());
    for field in [0x60, 0x68] {
        let old = u64::from_le_bytes(bytes[field..field + 8].try_into().unwrap());
        bytes[field..field + 8].copy_from_slice(&(old + code.len() as u64).to_le_bytes());
    }
    let binary =
        LoadedBinary::from_bytes(bytes, format!("{}-taint-engines", path.display())).unwrap();
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(4096));
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");
    emu.seed_stdin(b"A");
    emu.set_shadow_mode(ShadowMode::Taint);
    emu.state
        .page_map
        .map_region(SCRATCH, 0x1000, prot::RW, true);
    emu
}

/// Per byte from `addr`: the concrete value and the data-taint labels.
type Bytes = Vec<(u8, Vec<String>)>;

fn run(code: &[u8], interpret: bool, at: impl Fn(&mut Emulator) -> u64, len: u64) -> Bytes {
    let mut emu = build(code, interpret);
    emu.run().expect("run");
    let addr = at(&mut emu);
    let ram = emu.state.ram_space();
    let values = emu.state.read_space(ram, addr, len as usize).unwrap();
    (0..len)
        .map(|i| {
            let set = emu.state.get_shadow_memory(ram, addr + i);
            let data = set.and_then(|set| emu.taint.data_projection(set));
            let labels = data.map(|set| emu.taint.labels(set)).unwrap_or_default();
            (values[i as usize], labels)
        })
        .collect()
}

fn both(code: &[u8], at: impl Fn(&mut Emulator) -> u64 + Copy, len: u64) -> Bytes {
    let interpreted = run(code, true, at, len);
    let jitted = run(code, false, at, len);
    assert_eq!(jitted, interpreted, "the engines disagree");
    jitted
}

fn read() -> Vec<String> {
    vec!["read".to_string()]
}

/// `movzx eax, [rsp]; and eax, 3; mov byte [rsp+rax+8], 0x41`: the stored
/// value is a constant and the *pointer* carries the input. The address
/// dependence is data provenance in both engines' taint layer.
#[test]
fn a_store_through_a_tainted_pointer_agrees_in_both_engines() {
    let code = [
        0x0F, 0xB6, 0x04, 0x24, // movzx eax, byte ptr [rsp]
        0x83, 0xE0, 0x03, // and eax, 3              ; 'A' & 3 = 1
        0xC6, 0x44, 0x04, 0x08, 0x41, // mov byte ptr [rsp+rax+8], 0x41
    ];
    let bytes = both(&code, |emu| emu.read_register_u64("RSP").unwrap() + 8, 4);
    assert_eq!(bytes[1], (0x41, read()), "the byte the pointer chose");
    assert_eq!(bytes[0].1, Vec::<String>::new());
    assert_eq!(bytes[2].1, Vec::<String>::new());
}

/// The same with the stored value tainted too: the pointer-and-value path,
/// where the interpreter's panic was.
#[test]
fn a_tainted_store_through_a_tainted_pointer_agrees_in_both_engines() {
    let code = [
        0x0F, 0xB6, 0x04, 0x24, // movzx eax, byte ptr [rsp]
        0x89, 0xC1, // mov ecx, eax
        0x83, 0xE0, 0x03, // and eax, 3              ; 'A' & 3 = 1
        0x88, 0x4C, 0x04, 0x08, // mov byte ptr [rsp+rax+8], cl
    ];
    let bytes = both(&code, |emu| emu.read_register_u64("RSP").unwrap() + 8, 4);
    assert_eq!(bytes[1], (0x41, read()), "the byte the pointer chose");
    assert_eq!(bytes[0].1, Vec::<String>::new());
    assert_eq!(bytes[2].1, Vec::<String>::new());
}

/// `mov [abs], eax` with a tainted `eax`, then a load of the same bytes
/// through a register in the same block, stored next to it.
#[test]
fn an_absolute_store_keeps_its_value_and_taint_in_both_engines() {
    let code = [
        0x0F, 0xB6, 0x04, 0x24, // movzx eax, byte ptr [rsp]
        0xEB, 0x00, // jmp +0: what follows is one block
        0xA3, 0, 0, 0, 0x10, 0, 0, 0, 0, // mov [0x10000000], eax
        0x48, 0xBB, 0, 0, 0, 0x10, 0, 0, 0, 0, // mov rbx, 0x10000000
        0x8B, 0x0B, // mov ecx, [rbx]
        0x89, 0x4B, 0x04, // mov [rbx+4], ecx
    ];
    let bytes = both(&code, |_| SCRATCH, 8);
    assert_eq!(bytes[0], (0x41, read()), "the absolute store");
    assert_eq!(
        bytes[4],
        (0x41, read()),
        "the load of it, through a pointer, in the same block"
    );
}
