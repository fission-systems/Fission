//! Focused alloc_meta / mallocng global integrity probes.
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use std::path::PathBuf;

fn make_emu(max_inst: u64) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).unwrap();
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).unwrap();
    let load_spec = binary.load_spec().unwrap().clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let arch =
        ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary)).unwrap();
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .unwrap()
        .with_max_inst(Some(max_inst));
    emu.apply_linux_image(info).unwrap();
    emu
}

fn r64(emu: &mut Emulator, a: u64) -> u64 {
    let b = emu
        .state
        .read_space(emu.state.ram_space(), a, 8)
        .unwrap_or_else(|_| vec![0; 8]);
    u64::from_le_bytes(b.try_into().unwrap())
}
fn r32(emu: &mut Emulator, a: u64) -> u32 {
    let b = emu
        .state
        .read_space(emu.state.ram_space(), a, 4)
        .unwrap_or_else(|_| vec![0; 4]);
    u32::from_le_bytes(b.try_into().unwrap())
}

/// Walk budgets through first alloc_meta and dump key BSS fields.
#[test]
fn probe_alloc_meta_globals() {
    // Globals from disasm of 0x1002BE0:
    // 0x1007EB0 __auxv, 0x1007F18 secret, 0x1007F20 init_done (dword),
    // 0x1007F28 freelist, 0x1007F30 ctx, 0x1007F38 avail_slots,
    // 0x1007F40 page_mask, 0x1007F48 sc, 0x1007F50 head, 0x1007F58 tail,
    // 0x1007F60 next_page, 0x10082B0 brk_cur
    for budget in [700u64, 850, 900, 950, 1000, 1050, 1100, 1200] {
        let mut emu = make_emu(budget);
        emu.run().unwrap();
        let pc = emu.pc;
        let init = r32(&mut emu, 0x1007F20);
        let secret = r64(&mut emu, 0x1007F18);
        let freelist = r64(&mut emu, 0x1007F28);
        let ctx = r64(&mut emu, 0x1007F30);
        let avail = r64(&mut emu, 0x1007F38);
        let mask = r64(&mut emu, 0x1007F40);
        let sc = r64(&mut emu, 0x1007F48);
        let head = r64(&mut emu, 0x1007F50);
        let tail = r64(&mut emu, 0x1007F58);
        let page = r64(&mut emu, 0x1007F60);
        let brk_cur = r64(&mut emu, 0x10082B0);
        let bin0 = r64(&mut emu, 0x1007F68);
        let auxv = r64(&mut emu, 0x1007EB0);
        let r14 = emu.read_register_u64("R14").unwrap_or(0);
        let rax = emu.read_register_u64("RAX").unwrap_or(0);
        let rsp = emu.read_register_u64("RSP").unwrap_or(0);
        // meta at head+0x18 (first object) if head set
        let mbase = if head != 0 { head + 0x18 } else { 0 };
        let freeable = if mbase != 0 { r32(&mut emu, mbase + 0x18) } else { 0 };
        let last_idx = if mbase != 0 { r32(&mut emu, mbase + 0x1c) } else { 0 };
        let amask = if mbase != 0 { r64(&mut emu, mbase + 0x20) } else { 0 };
        let map_b = emu.state.page_map.is_mapped(0x100B000);
        let prot_b = emu.state.page_map.page_flags(0x100B000);
        eprintln!(
            "b={budget} pc=0x{pc:X} init={init} secret=0x{secret:X} freelist=0x{freelist:X} ctx=0x{ctx:X} avail=0x{avail:X} mask=0x{mask:X} sc=0x{sc:X} head=0x{head:X} tail=0x{tail:X} page=0x{page:X} brk_cur=0x{brk_cur:X} bin0=0x{bin0:X} auxv=0x{auxv:X} r14=0x{r14:X} rax=0x{rax:X} rsp=0x{rsp:X} meta+18 freeable={freeable} last={last_idx} amask=0x{amask:X} mapB={map_b} flagsB={prot_b:?} sys={:?}",
            emu.metrics.syscalls
        );
    }
}

/// After first malloc attempt, dump meta object at R14 if in heap range.
#[test]
fn probe_meta_object_fields() {
    let mut emu = make_emu(1500);
    emu.run().unwrap();
    let r14 = emu.read_register_u64("R14").unwrap_or(0);
    eprintln!("pc=0x{:X} r14=0x{r14:X}", emu.pc);
    if r14 >= 0x1009000 && r14 < 0x1010000 {
        for off in [0u64, 8, 0x10, 0x18, 0x1c, 0x20] {
            if off == 0x18 || off == 0x1c {
                eprintln!("  meta+0x{off:X} = 0x{:X}", r32(&mut emu, r14 + off));
            } else {
                eprintln!("  meta+0x{off:X} = 0x{:X}", r64(&mut emu, r14 + off));
            }
        }
        // page header at 0x100A000
        eprintln!("  page_hdr+0x10 = 0x{:X}", r32(&mut emu, 0x100A010));
        eprintln!("  page_hdr+0x00 = 0x{:X}", r64(&mut emu, 0x100A000));
    }
    // sizeclass usage row
    for i in 0..8u64 {
        let v = r64(&mut emu, 0x10080E8 + i * 8);
        if v != 0 {
            eprintln!("  sc_usage[{i}]=0x{v:X}");
        }
    }
    // bins
    for i in 0..8u64 {
        let v = r64(&mut emu, 0x1007F68 + i * 8);
        if v != 0 {
            eprintln!("  bin[{i}]=0x{v:X}");
        }
    }
}
