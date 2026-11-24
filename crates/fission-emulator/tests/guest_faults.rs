//! A guest memory fault is a signal to the guest, delivered precisely.
//!
//! It used to come back from the run as an emulator `Err` (CSIC, PR #1106),
//! and only the JIT's error named the instruction. What the guest sees
//! instead is what x86-64 Linux does, checked natively with a static musl
//! build run under Docker (`linux/amd64`):
//!
//! ```text
//! store: signo=11 code=1 addr=0x1234 rip_is_insn=1
//! push: signo=11 code=1 addr=0x1ffffff8 rip_is_insn=1 rsp=0x20000000
//! no handler: signaled=1 termsig=11
//! ```
//!
//! - `SIGSEGV`, `si_code` `SEGV_MAPERR` (1), `si_addr` the byte that faulted;
//! - the saved PC is the faulting instruction, which did not retire: a `push`
//!   to an unmapped stack leaves `rsp` where it was;
//! - with no handler, the process is killed by signal 11.
//!
//! Every program runs in both engines, which must agree. They are spliced into
//! the emulator's own fixture after its `read` syscall, never a corpus binary.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::{Emulator, GuestFault};
use fission_emulator::os::LinuxEnv;
use fission_emulator::os::linux::signal::{SA_SIGINFO, SigAction, sig};
use fission_emulator::pcode::page_map::AccessKind;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

/// Where the spliced code lands: the fixture maps file offset 0 at 0x400000.
const CODE: u64 = 0x40_0096;

fn build(code: &[u8], interpret: bool) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_concolic_branch_sys.elf");
    let mut bytes = std::fs::read(&path).expect("read fixture");
    bytes.splice(0x96..0x96, code.iter().copied());
    for field in [0x60, 0x68] {
        let old = u64::from_le_bytes(bytes[field..field + 8].try_into().unwrap());
        bytes[field..field + 8].copy_from_slice(&(old + code.len() as u64).to_le_bytes());
    }
    let binary = LoadedBinary::from_bytes(bytes, format!("{}-faults", path.display())).unwrap();
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
    emu
}

/// What a run left behind that the guest, or its parent, could see.
#[derive(Debug, PartialEq)]
struct End {
    outcome: String,
    exit_signal: Option<i32>,
    exit_code: Option<u32>,
    pc: u64,
    inst_count: u64,
    fault: Option<GuestFault>,
    rax: u64,
    rsp: u64,
}

fn run(emu: &mut Emulator) -> End {
    let outcome = emu
        .resume()
        .expect("a guest fault is not an emulator error");
    End {
        outcome: format!("{outcome:?}"),
        exit_signal: emu.exit_signal,
        exit_code: emu.exit_code,
        pc: emu.pc,
        inst_count: emu.inst_count,
        fault: emu.last_fault,
        rax: emu.read_register_u64("RAX").unwrap(),
        rsp: emu.read_register_u64("RSP").unwrap(),
    }
}

fn both(code: &[u8], setup: impl Fn(&mut Emulator)) -> End {
    let mut interpreted = build(code, true);
    setup(&mut interpreted);
    let interpreted = run(&mut interpreted);
    let mut jitted = build(code, false);
    setup(&mut jitted);
    let jitted = run(&mut jitted);
    assert_eq!(jitted, interpreted, "the engines disagree");
    jitted
}

/// `movabs rax, M; xor ecx, ecx; mov [rcx+0x10], eax` with no handler.
#[test]
fn an_unhandled_store_to_an_unmapped_page_kills_the_guest_with_sigsegv() {
    let code = [
        0x48, 0xB8, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, // movabs rax, M
        0x31, 0xC9, // xor ecx, ecx
        0x89, 0x41, 0x10, // mov [rcx+0x10], eax
    ];
    let store = CODE + 12;
    let end = both(&code, |_| {});
    assert_eq!(end.outcome, "ProcessExited");
    assert_eq!(end.exit_signal, Some(sig::SIGSEGV));
    assert_eq!(end.exit_code, None, "killed by a signal, not exited");
    assert_eq!(end.pc, store, "stopped on the faulting instruction");
    assert_eq!(
        end.fault,
        Some(GuestFault {
            pc: store,
            addr: 0x10,
            access: AccessKind::Write,
            mapped: false,
        })
    );
    assert_eq!(end.fault.unwrap().si_code(), 1, "SEGV_MAPERR");
    assert_eq!(end.rax, 0x1122_3344_5566_7788);
}

/// `movabs rsp, 0x20000000; push rax`: the push's own `rsp` update is undone.
#[test]
fn a_faulting_push_does_not_retire() {
    let code = [
        0x48, 0xBC, 0, 0, 0, 0x20, 0, 0, 0, 0,    // movabs rsp, 0x20000000
        0x50, // push rax
    ];
    let push = CODE + 10;
    let end = both(&code, |_| {});
    assert_eq!(end.exit_signal, Some(sig::SIGSEGV));
    assert_eq!(end.pc, push);
    assert_eq!(end.rsp, 0x2000_0000, "rsp as it was before the push");
    let fault = end.fault.expect("fault");
    assert_eq!((fault.pc, fault.addr), (push, 0x1FFF_FFF8));
    // The push did not retire: one fewer than the instructions it started.
    let mut counted = build(&code, true);
    counted.set_breakpoint(push);
    counted.resume().unwrap();
    assert_eq!(end.inst_count, counted.inst_count);
}

/// An `SA_SIGINFO` handler runs with the fault's `siginfo_t`. It checks
/// `signo`, `si_signo`, `si_code`, `si_addr` and a null `ucontext_t *`, and
/// exits 42 if all hold, 7 if not.
#[test]
fn a_siginfo_handler_receives_the_fault() {
    let code = [
        0x31, 0xC9, // xor ecx, ecx
        0x89, 0x81, 0x34, 0x12, 0x00, 0x00, // mov [rcx+0x1234], eax
        // handler:
        0x83, 0xFF, 0x0B, // cmp edi, 11
        0x75, 0x21, // jne bad
        0x48, 0x81, 0x7E, 0x10, 0x34, 0x12, 0x00, 0x00, // cmp qword [rsi+16], 0x1234
        0x75, 0x17, // jne bad
        0x83, 0x3E, 0x0B, // cmp dword [rsi], 11
        0x75, 0x12, // jne bad
        0x83, 0x7E, 0x08, 0x01, // cmp dword [rsi+8], 1
        0x75, 0x0C, // jne bad
        0x48, 0x85, 0xD2, // test rdx, rdx
        0x75, 0x07, // jne bad
        0xBF, 0x2A, 0x00, 0x00, 0x00, // mov edi, 42
        0xEB, 0x05, // jmp done
        0xBF, 0x07, 0x00, 0x00, 0x00, // bad: mov edi, 7
        0xB8, 0x3C, 0x00, 0x00, 0x00, // done: mov eax, 60
        0x0F, 0x05, // syscall
    ];
    let handler = CODE + 8;
    let end = both(&code, |emu| {
        emu.signals
            .set_action(sig::SIGSEGV, SigAction::Handler(handler), SA_SIGINFO);
    });
    assert_eq!(end.outcome, "ProcessExited");
    assert_eq!(end.exit_signal, None);
    assert_eq!(end.exit_code, Some(42), "the handler saw the fault");
    assert_eq!(end.fault.map(|f| (f.pc, f.addr)), Some((CODE + 2, 0x1234)));
}

/// A fault while `SIGSEGV` is ignored still kills: Linux resets a forced
/// signal to its default action.
#[test]
fn an_ignored_sigsegv_still_kills_on_a_fault() {
    let code = [0x31, 0xC9, 0x89, 0x41, 0x10]; // xor ecx, ecx; mov [rcx+0x10], eax
    let end = both(&code, |emu| {
        emu.signals.set_action(sig::SIGSEGV, SigAction::Ignore, 0);
    });
    assert_eq!(end.exit_signal, Some(sig::SIGSEGV));
    assert_eq!(end.pc, CODE + 2);
}
