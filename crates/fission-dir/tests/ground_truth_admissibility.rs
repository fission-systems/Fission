//! Audits initial-state admissibility, observable-frame exclusions,
//! termination/fault mismatches, and report denominators in `fission-dir`.

use fission_dir::report::VerifyOutcome;
use fission_dir::{
    AdmissibleCase, ConstraintKind, EmulatorHarness, EscapedFramePointer, FoundationIdentity,
    FrameExclusionJustification, Grounding, InitialStateDomain, MachineFlow, RunPlan,
    TerminationKind, WitnessKind, call_sites, check_admissible_case, check_ground_truth_report,
    decompile_one,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_midend_core::ir::{
    HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, NirBinding, NirType,
};
use fission_midend_prehir::{
    PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt,
};
use fission_static::analysis::decomp::facts::FactStore;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

const TEST_CODE_VADDR: u64 = 0x40_1000;

struct TempElf {
    path: PathBuf,
}

impl Drop for TempElf {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Build a minimal valid x86-64 static ELF executable mapping `.text` at `0x401000`.
fn build_x86_64_elf(tag: &str, code: &[u8]) -> TempElf {
    let shstrtab: &[u8] = b"\0.text\0.shstrtab\0";
    let code_off = 0x1000u64;
    let shstr_off = code_off + code.len() as u64;
    let shoff = (shstr_off + shstrtab.len() as u64 + 7) & !7;
    let total_len = shoff as usize + 3 * 64;
    let mut buf = vec![0u8; total_len];

    // ELF64 header
    buf[0..4].copy_from_slice(b"\x7fELF");
    buf[4] = 2; // ELFCLASS64
    buf[5] = 1; // ELFDATA2LSB
    buf[6] = 1; // EV_CURRENT
    buf[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    buf[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    buf[20..24].copy_from_slice(&1u32.to_le_bytes()); // EV_CURRENT
    buf[24..32].copy_from_slice(&TEST_CODE_VADDR.to_le_bytes()); // e_entry
    buf[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    buf[40..48].copy_from_slice(&shoff.to_le_bytes()); // e_shoff
    buf[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    buf[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    buf[56..58].copy_from_slice(&1u16.to_le_bytes()); // e_phnum
    buf[58..60].copy_from_slice(&64u16.to_le_bytes()); // e_shentsize
    buf[60..62].copy_from_slice(&3u16.to_le_bytes()); // e_shnum
    buf[62..64].copy_from_slice(&2u16.to_le_bytes()); // e_shstrndx

    // Single PT_LOAD RX segment covering [0x400000 .. 0x401000 + code.len())
    let p_off = 64usize;
    let filesz = code_off + code.len() as u64;
    buf[p_off..p_off + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    buf[p_off + 4..p_off + 8].copy_from_slice(&5u32.to_le_bytes()); // PF_R | PF_X
    buf[p_off + 8..p_off + 16].copy_from_slice(&0u64.to_le_bytes()); // p_offset
    buf[p_off + 16..p_off + 24].copy_from_slice(&0x40_0000u64.to_le_bytes()); // p_vaddr
    buf[p_off + 24..p_off + 32].copy_from_slice(&0x40_0000u64.to_le_bytes()); // p_paddr
    buf[p_off + 32..p_off + 40].copy_from_slice(&filesz.to_le_bytes());
    buf[p_off + 40..p_off + 48].copy_from_slice(&filesz.to_le_bytes());
    buf[p_off + 48..p_off + 56].copy_from_slice(&0x1000u64.to_le_bytes());

    buf[code_off as usize..code_off as usize + code.len()].copy_from_slice(code);
    buf[shstr_off as usize..shstr_off as usize + shstrtab.len()].copy_from_slice(shstrtab);

    // Section 1: .text
    let s1 = shoff as usize + 64;
    buf[s1..s1 + 4].copy_from_slice(&1u32.to_le_bytes()); // sh_name -> ".text"
    buf[s1 + 4..s1 + 8].copy_from_slice(&1u32.to_le_bytes()); // SHT_PROGBITS
    buf[s1 + 8..s1 + 16].copy_from_slice(&6u64.to_le_bytes()); // SHF_ALLOC | SHF_EXECINSTR
    buf[s1 + 16..s1 + 24].copy_from_slice(&TEST_CODE_VADDR.to_le_bytes());
    buf[s1 + 24..s1 + 32].copy_from_slice(&code_off.to_le_bytes());
    buf[s1 + 32..s1 + 40].copy_from_slice(&(code.len() as u64).to_le_bytes());
    buf[s1 + 48..s1 + 56].copy_from_slice(&16u64.to_le_bytes());

    // Section 2: .shstrtab
    let s2 = shoff as usize + 128;
    buf[s2..s2 + 4].copy_from_slice(&7u32.to_le_bytes()); // sh_name -> ".shstrtab"
    buf[s2 + 4..s2 + 8].copy_from_slice(&3u32.to_le_bytes()); // SHT_STRTAB
    buf[s2 + 24..s2 + 32].copy_from_slice(&shstr_off.to_le_bytes());
    buf[s2 + 32..s2 + 40].copy_from_slice(&(shstrtab.len() as u64).to_le_bytes());

    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "fission_dir_admissibility_{}_{}_{tag}.elf",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, &buf).expect("write temp ELF64 fixture");
    TempElf { path }
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/ground_truth_fixture.elf")
}

fn win64_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-emulator/testdata/x64_mingw_control_flow.exe")
}

fn symbol_address(binary: &LoadedBinary, name: &str) -> u64 {
    let inner = binary.inner();
    if let Some(&idx) = inner.function_name_index.get(name) {
        return inner.functions[idx].address;
    }
    binary
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("symbol {name} not found"))
        .address
}

fn int_ty(bits: u32, signed: bool) -> NirType {
    NirType::Int { bits, signed }
}

fn ptr_ty() -> NirType {
    NirType::Ptr(Box::new(int_ty(8, false)))
}

fn binding(name: &str, ty: NirType) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin: None,
        initializer: None,
    }
}

fn make_hir(
    name: &str,
    params: Vec<NirBinding>,
    locals: Vec<NirBinding>,
    return_type: NirType,
    body: Vec<HirStmt>,
) -> HirFunction {
    HirFunction {
        name: name.to_string(),
        params,
        locals,
        return_type,
        body,
        ..HirFunction::default()
    }
}

fn leaf_grounding(
    path: &std::path::Path,
    address: u64,
    frame_offsets: BTreeMap<String, i64>,
) -> Grounding {
    let flow = LoadedBinary::from_file(path)
        .ok()
        .and_then(|bin| call_sites(&bin, address).ok())
        .unwrap_or_else(|| MachineFlow {
            instructions: [address].into_iter().collect(),
            ..MachineFlow::default()
        });
    Grounding {
        flow: Some(flow),
        frame_offsets,
        ..Grounding::default()
    }
}

fn to_prehir_expr(expr: &HirExpr) -> PreHirExpr {
    match expr {
        HirExpr::Var(name) => PreHirExpr::Var(name.clone()),
        HirExpr::Const(v, ty) => PreHirExpr::Const(*v, ty.clone()),
        HirExpr::Cast { ty, expr } => PreHirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(to_prehir_expr(expr)),
        },
        HirExpr::Load { ptr, ty } => PreHirExpr::Load {
            ptr: Box::new(to_prehir_expr(ptr)),
            ty: ty.clone(),
        },
        HirExpr::PtrOffset { base, offset } => PreHirExpr::PtrOffset {
            base: Box::new(to_prehir_expr(base)),
            offset: *offset,
        },
        HirExpr::AddressOfLocal(name) => PreHirExpr::AddressOfLocal(name.clone()),
        HirExpr::Binary { op, lhs, rhs, ty } => PreHirExpr::Binary {
            op: match op {
                HirBinaryOp::Div => PreHirBinaryOp::Div,
                other => panic!("unsupported binary op in test helper: {other:?}"),
            },
            lhs: Box::new(to_prehir_expr(lhs)),
            rhs: Box::new(to_prehir_expr(rhs)),
            ty: ty.clone(),
        },
        other => panic!("unsupported expr in test helper: {other:?}"),
    }
}

fn to_prehir_stmt(stmt: &HirStmt) -> PreHirStmt {
    match stmt {
        HirStmt::Assign { lhs, rhs } => PreHirStmt::Assign {
            lhs: match lhs {
                HirLValue::Var(name) => PreHirLValue::Var(name.clone()),
                HirLValue::Deref { ptr, ty } => PreHirLValue::Deref {
                    ptr: Box::new(to_prehir_expr(ptr)),
                    ty: ty.clone(),
                },
                other => panic!("unsupported lvalue in test helper: {other:?}"),
            },
            rhs: to_prehir_expr(rhs),
        },
        HirStmt::Return(expr) => PreHirStmt::Return(expr.as_ref().map(to_prehir_expr)),
        other => panic!("unsupported stmt in test helper: {other:?}"),
    }
}

fn to_prehir_binding(b: &NirBinding) -> PreHirBinding {
    PreHirBinding {
        name: b.name.clone(),
        ty: b.ty.clone(),
        surface_type_name: b.surface_type_name.clone(),
        origin: b.origin,
        initializer: b.initializer.as_ref().map(to_prehir_expr),
    }
}

fn to_prehir(hir: &HirFunction) -> PreHirFunction {
    PreHirFunction {
        name: hir.name.clone(),
        params: hir.params.iter().map(to_prehir_binding).collect(),
        locals: hir.locals.iter().map(to_prehir_binding).collect(),
        return_type: hir.return_type.clone(),
        body: hir.body.iter().map(to_prehir_stmt).collect(),
        ..PreHirFunction::default()
    }
}

fn dummy_foundation(addr: u64) -> FoundationIdentity {
    FoundationIdentity {
        model_version: 1,
        function_address: addr,
        block_count: 1,
        op_count: 1,
        digest: 0xdead_beef,
    }
}

/// 1. Escaping frame pointer:
/// `mov dword ptr [rsp - 4], edi; lea rax, [rsp - 4]; ret`
/// Returns `&local` inside the private frame.
/// - Corrupted HIR (omits storing `a` into `local`) MUST be caught as `Diverged`.
/// - Matching HIR MUST flag `EscapedFramePointer::Returned` and `Unjustified` frame
///   exclusion (never silently reporting clean `Equivalent`).
#[test]
fn escaping_frame_pointer_catches_write_loss_and_refuses_unjustified_pass() {
    // 89 7c 24 fc        mov [rsp - 4], edi
    // 48 8d 44 24 fc     lea rax, [rsp - 4]
    // c3                 ret
    let elf = build_x86_64_elf(
        "escape_frame_ptr",
        &[0x89, 0x7c, 0x24, 0xfc, 0x48, 0x8d, 0x44, 0x24, 0xfc, 0xc3],
    );
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    let frame_offsets: BTreeMap<String, i64> = [("local_x".to_string(), -12)].into_iter().collect();
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, frame_offsets);

    let buggy_hir = make_hir(
        "escape_buggy",
        vec![binding("param_1", int_ty(32, true))],
        vec![binding("local_x", int_ty(32, true))],
        ptr_ty(),
        vec![HirStmt::Return(Some(HirExpr::AddressOfLocal(
            "local_x".to_string(),
        )))],
    );
    let buggy_report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&buggy_hir),
        &buggy_hir,
        &AdmissibleCase {
            args: &[0x1234_5678],
            initial_memory: &[],
        },
        &grounding,
    );
    assert!(
        matches!(buggy_report.outcome, VerifyOutcome::Diverged(_)),
        "corrupted escaped frame store must diverge, got {:?}",
        buggy_report.outcome
    );

    let matching_hir = make_hir(
        "escape_match",
        vec![binding("param_1", int_ty(32, true))],
        vec![binding("local_x", int_ty(32, true))],
        ptr_ty(),
        vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("local_x".to_string()),
                rhs: HirExpr::Var("param_1".to_string()),
            },
            HirStmt::Return(Some(HirExpr::AddressOfLocal("local_x".to_string()))),
        ],
    );
    let matching_report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&matching_hir),
        &matching_hir,
        &AdmissibleCase {
            args: &[0x1234_5678],
            initial_memory: &[],
        },
        &grounding,
    );
    assert!(
        matches!(matching_report.outcome, VerifyOutcome::Inconclusive { .. }),
        "returning an escaped frame pointer must not claim Equivalent; got {:?}",
        matching_report.outcome
    );
    assert_eq!(matching_report.denominators.unjustified_frame_runs, 1);
    assert_eq!(matching_report.denominators.unknown_runs, 1);
    let witness = &matching_report.witnesses[0];
    let audit = witness.frame_audit.as_ref().expect("frame audit");
    assert!(
        audit
            .escaped_frame_pointers
            .iter()
            .any(|e| matches!(e, EscapedFramePointer::Returned(_))),
        "expected Returned escape in {audit:?}"
    );
    assert!(matches!(
        audit.justification,
        FrameExclusionJustification::Unjustified { .. }
    ));
}

/// 2. Caller stack writes above `entry_sp` (previously swallowed by `FRAME_ABOVE = 64`):
/// `mov [rsp + 16], edi; xor eax, eax; ret`
#[test]
fn caller_stack_write_above_entry_sp_is_not_excluded_by_frame() {
    // 89 7c 24 10    mov [rsp + 0x10], edi
    // 31 c0          xor eax, eax
    // c3             ret
    let elf = build_x86_64_elf(
        "caller_stack_write",
        &[0x89, 0x7c, 0x24, 0x10, 0x31, 0xc0, 0xc3],
    );
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    let hir = make_hir(
        "missing_caller_stack_store",
        vec![binding("param_1", int_ty(32, true))],
        Vec::new(),
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
    );
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, BTreeMap::new());
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &PreHirFunction::default(),
        &hir,
        &AdmissibleCase {
            args: &[0x5566_7788],
            initial_memory: &[],
        },
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Diverged(_)),
        "caller stack store above entry SP must not be silently dropped: {report:?}"
    );
    let native_divs = report.native_admissible_divergences();
    assert_eq!(native_divs.len(), 1);
    let audit = native_divs[0].frame_audit.as_ref().expect("frame audit");
    assert!(
        !audit.caller_stack_write_addrs.is_empty(),
        "expected caller_stack_write_addrs recorded: {audit:?}"
    );
}

/// 3. Out-param pointer aliasing into the stack frame (`entry_sp - 0x200`):
/// Writes through the aliased frame pointer remain observable instead of being
/// discarded as private locals.
#[test]
fn out_param_aliasing_private_frame_remains_observable() {
    // 89 37    mov dword ptr [rdi], esi
    // 31 c0    xor eax, eax
    // c3       ret
    let elf = build_x86_64_elf("out_param_alias", &[0x89, 0x37, 0x31, 0xc0, 0xc3]);
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    harness
        .begin_native_run(TEST_CODE_VADDR, &[0, 0])
        .expect("probe sp");
    let cfa = harness.cfa().expect("cfa");
    let alias_addr = cfa - 8 - 0x200;
    let initial_memory = [
        (alias_addr, 0u8),
        (alias_addr + 1, 0),
        (alias_addr + 2, 0),
        (alias_addr + 3, 0),
    ];

    let matching_hir = make_hir(
        "write_alias_ok",
        vec![
            binding("param_1", ptr_ty()),
            binding("param_2", int_ty(32, true)),
        ],
        Vec::new(),
        int_ty(32, true),
        vec![
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("param_1".to_string())),
                    ty: int_ty(32, true),
                },
                rhs: HirExpr::Var("param_2".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true)))),
        ],
    );
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, BTreeMap::new());
    let ok_report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&matching_hir),
        &matching_hir,
        &AdmissibleCase {
            args: &[alias_addr, 0x1122_3344],
            initial_memory: &initial_memory,
        },
        &grounding,
    );
    assert_eq!(ok_report.outcome, VerifyOutcome::Equivalent { checked: 1 });
    assert_eq!(ok_report.native_admissible_agreements(), 1);
    assert!(
        ok_report.witnesses[0]
            .frame_audit
            .as_ref()
            .expect("audit")
            .aliased_input_addrs
            .contains(&alias_addr)
    );

    let buggy_hir = make_hir(
        "write_alias_dropped",
        vec![
            binding("param_1", ptr_ty()),
            binding("param_2", int_ty(32, true)),
        ],
        Vec::new(),
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
    );
    let div_report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &PreHirFunction::default(),
        &buggy_hir,
        &AdmissibleCase {
            args: &[alias_addr, 0x1122_3344],
            initial_memory: &initial_memory,
        },
        &grounding,
    );
    assert!(
        matches!(div_report.outcome, VerifyOutcome::Diverged(_)),
        "dropped write through aliased frame pointer must diverge: {div_report:?}"
    );
}

/// 4. Partial byte store on explicit scratch buffer:
/// `mov byte ptr [rdi + 1], sil; mov eax, esi; ret`
/// Narrow 8-bit store agrees (`NativeAdmissible`); a wider 32-bit overwrite is caught.
#[test]
fn partial_byte_write_agrees_as_native_admissible_and_catches_wide_overwrite() {
    // 40 88 77 01    mov byte ptr [rdi + 1], sil
    // 89 f0          mov eax, esi
    // c3             ret
    let elf = build_x86_64_elf("partial_write", &[0x40, 0x88, 0x77, 0x01, 0x89, 0xf0, 0xc3]);
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    let scratch_bytes = [0x10u8, 0x20, 0x30, 0x40, 0x50];
    let scratch = harness.install_scratch(&scratch_bytes).expect("scratch");
    let initial_memory: Vec<(u64, u8)> = scratch_bytes
        .iter()
        .enumerate()
        .map(|(i, &b)| (scratch + i as u64, b))
        .collect();

    let matching_hir = make_hir(
        "partial_ok",
        vec![
            binding("param_1", ptr_ty()),
            binding("param_2", int_ty(32, true)),
        ],
        Vec::new(),
        int_ty(32, true),
        vec![
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 1,
                    }),
                    ty: int_ty(8, false),
                },
                rhs: HirExpr::Cast {
                    ty: int_ty(8, false),
                    expr: Box::new(HirExpr::Var("param_2".to_string())),
                },
            },
            HirStmt::Return(Some(HirExpr::Var("param_2".to_string()))),
        ],
    );
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, BTreeMap::new());
    let ok = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&matching_hir),
        &matching_hir,
        &AdmissibleCase {
            args: &[scratch, 0x1122_33a6],
            initial_memory: &initial_memory,
        },
        &grounding,
    );
    assert_eq!(ok.outcome, VerifyOutcome::Equivalent { checked: 1 });
    assert_eq!(ok.native_admissible_agreements(), 1);
    assert_eq!(ok.synthetic_agreements(), 0);

    let wide_hir = make_hir(
        "partial_overwide",
        vec![
            binding("param_1", ptr_ty()),
            binding("param_2", int_ty(32, true)),
        ],
        Vec::new(),
        int_ty(32, true),
        vec![
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 1,
                    }),
                    ty: int_ty(32, false),
                },
                rhs: HirExpr::Var("param_2".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Var("param_2".to_string()))),
        ],
    );
    let wide = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &PreHirFunction::default(),
        &wide_hir,
        &AdmissibleCase {
            args: &[scratch, 0x1122_33a6],
            initial_memory: &initial_memory,
        },
        &grounding,
    );
    assert!(
        matches!(wide.outcome, VerifyOutcome::Diverged(_)),
        "32-bit overwrite over 8-bit store must diverge: {wide:?}"
    );
    assert_eq!(wide.native_admissible_divergences().len(), 1);
}

/// 5. Invalid/unmapped pointer distinction between synthetic lazy-memory mode and
/// strict native-admissible mode.
#[test]
fn invalid_pointer_separates_synthetic_lazy_memory_from_native_fault() {
    // 8b 07    mov eax, dword ptr [rdi]
    // c3       ret
    let elf = build_x86_64_elf("deref_ptr", &[0x8b, 0x07, 0xc3]);
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");

    let deref_hir = make_hir(
        "deref_ok",
        vec![binding("param_1", ptr_ty())],
        Vec::new(),
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Load {
            ptr: Box::new(HirExpr::Var("param_1".to_string())),
            ty: int_ty(32, true),
        }))],
    );

    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, BTreeMap::new());
    // Synthetic mode materializes memory behind unmapped pointers, classifying
    // those agreements strictly as `SyntheticModel`.
    let synthetic = check_ground_truth_report(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&deref_hir),
        &deref_hir,
        &RunPlan {
            runs: 32,
            ..RunPlan::default()
        },
        &grounding,
    );
    assert!(matches!(
        synthetic.outcome,
        VerifyOutcome::Equivalent { .. }
    ));
    assert!(synthetic.synthetic_agreements() > 0);
    assert_eq!(
        synthetic.native_admissible_agreements(),
        0,
        "random pointer dereferences must not claim native admissibility"
    );
    assert_eq!(synthetic.policy.items.len(), InitialStateDomain::ALL.len());
    assert!(
        synthetic
            .policy
            .items
            .iter()
            .any(|item| item.kind == ConstraintKind::SyntheticAssumption)
    );

    // Strict native mode refuses unmapped `0xdead_beef_0000`:
    // (a) buggy HIR returning 0 catches an asymmetric fault divergence;
    // (b) faithful deref HIR faults on both sides -> `Inconclusive`.
    let buggy_const_hir = make_hir(
        "deref_elided",
        vec![binding("param_1", ptr_ty())],
        Vec::new(),
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
    );
    let asym = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&buggy_const_hir),
        &buggy_const_hir,
        &AdmissibleCase {
            args: &[0xdead_beef_0000],
            initial_memory: &[],
        },
        &grounding,
    );
    assert!(
        matches!(asym.outcome, VerifyOutcome::Diverged(_)),
        "eliding fault-inducing dereference must diverge: {asym:?}"
    );
    assert_eq!(asym.denominators.asymmetric_fault_runs, 1);
    assert!(
        asym.witnesses
            .iter()
            .any(|w| w.kind == WitnessKind::AsymmetricFault)
    );

    let both_fault = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&deref_hir),
        &deref_hir,
        &AdmissibleCase {
            args: &[0xdead_beef_0000],
            initial_memory: &[],
        },
        &grounding,
    );
    assert!(
        matches!(both_fault.outcome, VerifyOutcome::Inconclusive { .. }),
        "both sides faulting on unmapped pointer must report Inconclusive: {both_fault:?}"
    );
    assert_eq!(both_fault.denominators.unknown_runs, 1);
}

/// 6. Modeled vs. unmodeled call handling:
/// Modeled stubbed calls classify as `SyntheticModel` (never `NativeAdmissible`);
/// missing call flow reports `Unsupported` and counts skipped runs accurately.
#[test]
fn modeled_calls_stay_synthetic_and_unmodeled_calls_report_skipped_denominator() {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let address = symbol_address(&binary, "reload_after_call");
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: "reload_after_call".to_string(),
        address,
        ..Default::default()
    };
    let pair = decompile_one(&binary, &facts, &func).expect("decompile");
    let grounding = Grounding::for_function(&binary, address, &pair);
    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("harness");

    let modeled = check_ground_truth_report(
        &mut harness,
        address,
        &pair.prehir,
        &pair.hir,
        &RunPlan {
            runs: 16,
            ..RunPlan::default()
        },
        &grounding,
    );
    assert_eq!(modeled.outcome, VerifyOutcome::Equivalent { checked: 16 });
    assert_eq!(modeled.synthetic_agreements(), 16);
    assert_eq!(modeled.native_admissible_agreements(), 0);
    assert!(
        modeled
            .witnesses
            .iter()
            .all(|w| w.stub_summary.calls_intercepted > 0)
    );

    let unmodeled_grounding = Grounding {
        flow: None,
        ..grounding.clone()
    };
    let unmodeled = check_ground_truth_report(
        &mut harness,
        address,
        &pair.prehir,
        &pair.hir,
        &RunPlan {
            runs: 8,
            ..RunPlan::default()
        },
        &unmodeled_grounding,
    );
    assert!(matches!(unmodeled.outcome, VerifyOutcome::Unsupported(_)));
    assert_eq!(unmodeled.denominators.planned_runs, 8);
    assert_eq!(unmodeled.denominators.checked_runs, 0);
    assert_eq!(unmodeled.denominators.skipped_interpreter_errors, 8);
    assert!(!unmodeled.unresolved_effects.is_empty());
}

/// 7. Termination / fault mismatch:
/// Non-terminating loop (`HitBudget`) and integer divide-by-zero vs. returning HIR.
#[test]
fn termination_and_arithmetic_fault_mismatches_diverge_never_silent_pass() {
    // 7a. Infinite loop `_start` (`HitBudget`) vs corrupted HIR returning 0.
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let start_addr = symbol_address(&binary, "_start");
    let mut harness = EmulatorHarness::build(&path, Some(5_000)).expect("harness");
    let fake_hir = make_hir(
        "_start",
        Vec::new(),
        Vec::new(),
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
    );
    let start_grounding = leaf_grounding(&path, start_addr, BTreeMap::new());
    let loop_report = check_admissible_case(
        &mut harness,
        start_addr,
        &PreHirFunction::default(),
        &fake_hir,
        &AdmissibleCase {
            args: &[],
            initial_memory: &[],
        },
        &start_grounding,
    );
    assert!(
        matches!(loop_report.outcome, VerifyOutcome::Diverged(_)),
        "HitBudget vs returning HIR must diverge: {loop_report:?}"
    );
    assert_eq!(loop_report.denominators.asymmetric_fault_runs, 1);
    assert!(
        loop_report
            .witnesses
            .iter()
            .any(|w| w.kind == WitnessKind::AsymmetricFault
                && w.machine_termination == TerminationKind::HitBudget)
    );

    // 7b. `idiv esi`: divide by zero in HIR traps (`TerminationKind::Trap`),
    // while `fission-emulator`'s `IntSDiv` returns 0 without raising SIGFPE --
    // the verifier explicitly catches this asymmetric trap rather than skipping it.
    // 89 f8    mov eax, edi
    // 99       cdq
    // f7 fe    idiv esi
    // c3       ret
    let div_elf = build_x86_64_elf("idiv_zero", &[0x89, 0xf8, 0x99, 0xf7, 0xfe, 0xc3]);
    let div_grounding = leaf_grounding(&div_elf.path, TEST_CODE_VADDR, BTreeMap::new());
    let mut div_harness = EmulatorHarness::build(&div_elf.path, Some(50_000)).expect("harness");

    let faithful_div_hir = make_hir(
        "div_faithful",
        vec![
            binding("param_1", int_ty(32, true)),
            binding("param_2", int_ty(32, true)),
        ],
        Vec::new(),
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Div,
            lhs: Box::new(HirExpr::Var("param_1".to_string())),
            rhs: Box::new(HirExpr::Var("param_2".to_string())),
            ty: int_ty(32, true),
        }))],
    );
    let div_prehir = to_prehir(&faithful_div_hir);
    let div_trap = check_admissible_case(
        &mut div_harness,
        TEST_CODE_VADDR,
        &div_prehir,
        &faithful_div_hir,
        &AdmissibleCase {
            args: &[10, 0],
            initial_memory: &[],
        },
        &div_grounding,
    );
    assert!(
        matches!(div_trap.outcome, VerifyOutcome::Diverged(_)),
        "HIR division-by-zero trap vs machine return must diverge: {div_trap:?}"
    );
    assert_eq!(div_trap.denominators.asymmetric_fault_runs, 1);
    assert!(
        div_trap
            .witnesses
            .iter()
            .any(|w| w.kind == WitnessKind::AsymmetricFault
                && matches!(w.hir_termination, TerminationKind::Trap(_)))
    );

    let nonzero_div = check_admissible_case(
        &mut div_harness,
        TEST_CODE_VADDR,
        &div_prehir,
        &faithful_div_hir,
        &AdmissibleCase {
            args: &[42, 6],
            initial_memory: &[],
        },
        &div_grounding,
    );
    assert_eq!(
        nonzero_div.outcome,
        VerifyOutcome::Equivalent { checked: 1 }
    );
    assert_eq!(nonzero_div.native_admissible_agreements(), 1);
}

/// 8. Independent native replay witness on pure fixture function + AssuranceReport
/// non-overstatement contract + skipped/unknown denominators.
#[test]
fn native_replay_assurance_contract_and_denominators_are_preserved() {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let facts = FactStore::from_binary(&binary);

    // 8a. Native replay of `pick(true, 42)` and synthetic sweep of `pick`.
    let pick_addr = symbol_address(&binary, "pick");
    let pick_pair = decompile_one(
        &binary,
        &facts,
        &FunctionInfo {
            name: "pick".to_string(),
            address: pick_addr,
            ..Default::default()
        },
    )
    .expect("decompile pick");
    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("harness");
    let native_pick = check_admissible_case(
        &mut harness,
        pick_addr,
        &pick_pair.prehir,
        &pick_pair.hir,
        &AdmissibleCase {
            args: &[0, 42],
            initial_memory: &[],
        },
        &Grounding::for_function(&binary, pick_addr, &pick_pair),
    );
    assert_eq!(
        native_pick.outcome,
        VerifyOutcome::Equivalent { checked: 1 }
    );
    assert_eq!(native_pick.native_admissible_agreements(), 1);
    let assurance = native_pick
        .to_assurance_report(dummy_foundation(pick_addr))
        .expect("assurance");
    assert!(!assurance.is_proven());
    assert!(!assurance.is_unconditionally_proven());
    assert!(assurance.is_sampled_only());
    assert!(assurance.is_trace_only());

    // 8b. Denominator accounting on `first_plus_fourth` (top-page skipped runs)
    // and `read_garbage` (`unknown_runs` on undefined reads).
    let fpf_addr = symbol_address(&binary, "first_plus_fourth");
    let fpf_pair = decompile_one(
        &binary,
        &facts,
        &FunctionInfo {
            name: "first_plus_fourth".to_string(),
            address: fpf_addr,
            ..Default::default()
        },
    )
    .expect("decompile first_plus_fourth");
    let fpf_report = check_ground_truth_report(
        &mut harness,
        fpf_addr,
        &fpf_pair.prehir,
        &fpf_pair.hir,
        &RunPlan {
            runs: 32,
            ..RunPlan::default()
        },
        &Grounding::for_function(&binary, fpf_addr, &fpf_pair),
    );
    assert!(matches!(
        fpf_report.outcome,
        VerifyOutcome::Equivalent { .. }
    ));
    assert_eq!(fpf_report.denominators.planned_runs, 32);
    assert!(fpf_report.denominators.skipped_emulator_errors > 0);
    assert_eq!(
        fpf_report.denominators.agreed_runs + fpf_report.denominators.skipped_emulator_errors,
        32
    );
    assert!(!fpf_report.unresolved_effects.is_empty());

    let rg_addr = symbol_address(&binary, "read_garbage");
    let rg_pair = decompile_one(
        &binary,
        &facts,
        &FunctionInfo {
            name: "read_garbage".to_string(),
            address: rg_addr,
            ..Default::default()
        },
    )
    .expect("decompile read_garbage");
    let rg_report = check_ground_truth_report(
        &mut harness,
        rg_addr,
        &rg_pair.prehir,
        &rg_pair.hir,
        &RunPlan {
            runs: 16,
            ..RunPlan::default()
        },
        &Grounding::for_function(&binary, rg_addr, &rg_pair),
    );
    assert!(matches!(
        rg_report.outcome,
        VerifyOutcome::Inconclusive { .. }
    ));
    assert_eq!(rg_report.denominators.checked_runs, 0);
    assert_eq!(rg_report.denominators.unknown_runs, 16);

    // 8c. Win64 `clamp` preserves 32-byte shadow-space exclusion while keeping
    // the return address slot and caller stack args unexcluded.
    let win_path = win64_fixture();
    let win_bin = LoadedBinary::from_file(&win_path).expect("load pe");
    let win_facts = FactStore::from_binary(&win_bin);
    let clamp_addr = symbol_address(&win_bin, "clamp");
    let clamp_pair = decompile_one(
        &win_bin,
        &win_facts,
        &FunctionInfo {
            name: "clamp".to_string(),
            address: clamp_addr,
            ..Default::default()
        },
    )
    .expect("decompile clamp");
    let mut win_harness = EmulatorHarness::build(&win_path, Some(200_000)).expect("pe harness");
    let clamp_report = check_ground_truth_report(
        &mut win_harness,
        clamp_addr,
        &clamp_pair.prehir,
        &clamp_pair.hir,
        &RunPlan {
            runs: 32,
            ..RunPlan::default()
        },
        &Grounding::for_function(&win_bin, clamp_addr, &clamp_pair),
    );
    assert_eq!(
        clamp_report.outcome,
        VerifyOutcome::Equivalent { checked: 32 }
    );
    let clamp_audit = clamp_report.witnesses[0]
        .frame_audit
        .as_ref()
        .expect("clamp frame audit");
    assert_eq!(
        clamp_audit.spec.abi_home_space.end - clamp_audit.spec.abi_home_space.start,
        32
    );
    assert!(!clamp_audit.spec.is_excluded(clamp_audit.spec.entry_sp));
}
