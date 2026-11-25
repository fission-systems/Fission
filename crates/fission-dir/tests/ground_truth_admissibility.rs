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
        HirExpr::Call {
            origin,
            target,
            args,
            ty,
        } => PreHirExpr::Call {
            origin: *origin,
            target: target.clone(),
            args: args.iter().map(to_prehir_expr).collect(),
            ty: ty.clone(),
        },
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
        HirStmt::Expr(expr) => PreHirStmt::Expr(to_prehir_expr(expr)),
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
    // The tree drops the store into the escaped local. Only the machine
    // stored there, and the local's extent is the decompiler's inference, so
    // the byte could as well be machine-private state: never agreement, and
    // the object difference is named in the reason.
    match &buggy_report.outcome {
        VerifyOutcome::Inconclusive { reason } => assert!(
            reason.contains("escaped frame object byte"),
            "reason must name the escaped object difference: {reason}"
        ),
        other => panic!("dropped escaped-frame store must be inconclusive, got {other:?}"),
    }

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
    // No live caller object can sit below the callee's entry SP natively.
    assert_eq!(ok_report.native_admissible_agreements(), 0);
    assert_eq!(ok_report.synthetic_agreements(), 1);
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
/// Neither is agreement; neither is a sound counterexample either.
#[test]
fn termination_and_arithmetic_fault_mismatches_never_silently_pass() {
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
    // The emulator's step budget is a resource limit, not evidence that the
    // machine never returns: the run is neither agreement nor a divergence.
    assert!(
        matches!(loop_report.outcome, VerifyOutcome::Inconclusive { .. }),
        "HitBudget vs returning HIR must be inconclusive, never Equivalent: {loop_report:?}"
    );
    assert_eq!(loop_report.denominators.asymmetric_fault_runs, 0);
    assert_eq!(loop_report.denominators.unknown_runs, 1);
    assert!(
        loop_report
            .witnesses
            .iter()
            .any(|w| w.kind == WitnessKind::Unknown
                && w.machine_termination == TerminationKind::HitBudget)
    );
    assert_denominators_consistent(&loop_report);

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
    // The tree is a faithful translation: natively `idiv` by zero raises #DE
    // too. The emulator does not model that trap, so the machine side's
    // "return" is not evidence against the tree.
    assert!(
        matches!(div_trap.outcome, VerifyOutcome::Inconclusive { .. }),
        "HIR division-by-zero trap vs unmodeled machine trap must be inconclusive: {div_trap:?}"
    );
    assert_eq!(div_trap.denominators.asymmetric_fault_runs, 0);
    assert!(
        div_trap
            .witnesses
            .iter()
            .any(|w| w.kind == WitnessKind::Unknown
                && matches!(w.hir_termination, TerminationKind::Trap(_)))
    );
    assert_denominators_consistent(&div_trap);

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

// ---------------------------------------------------------------------------
// Escaped / aliased frame objects: the observable region is the whole object
// with known bounds, never a fixed window after the pointer (#1332 review).
// ---------------------------------------------------------------------------

/// Every attempted run lands in exactly one terminal bucket, and every checked
/// run in exactly one verdict bucket.
fn assert_denominators_consistent(report: &fission_dir::GroundTruthReport) {
    let d = &report.denominators;
    assert_eq!(
        d.attempted_runs,
        d.checked_runs + d.unknown_runs + d.skipped_interpreter_errors + d.skipped_emulator_errors,
        "attempted runs must partition into checked/unknown/skipped: {d:?}"
    );
    assert_eq!(
        d.checked_runs,
        d.agreed_runs + d.diverged_runs + d.suspect_runs + d.asymmetric_fault_runs,
        "checked runs must partition into agreed/diverged/suspect/asymmetric: {d:?}"
    );
    assert_eq!(
        d.agreed_runs,
        d.native_admissible_runs + d.synthetic_model_runs,
        "agreed runs must be classified native or synthetic: {d:?}"
    );
    assert!(d.unjustified_frame_runs <= d.unknown_runs, "{d:?}");
    assert!(d.attempted_runs <= d.planned_runs, "{d:?}");
}

fn aggregate_ty(size: u32) -> NirType {
    NirType::Aggregate {
        size,
        fields: Vec::new(),
    }
}

fn store_i32_at(base: HirExpr, offset: i64, value: HirExpr) -> HirStmt {
    HirStmt::Assign {
        lhs: HirLValue::Deref {
            ptr: Box::new(HirExpr::PtrOffset {
                base: Box::new(base),
                offset,
            }),
            ty: int_ty(32, true),
        },
        rhs: value,
    }
}

fn call_stmt(site: u64, args: Vec<HirExpr>) -> HirStmt {
    HirStmt::Expr(HirExpr::Call {
        origin: Some(fission_midend_core::ir::CallOrigin {
            instruction_address: site,
            pcode_sequence: 0,
        }),
        target: "callee".to_string(),
        args,
        ty: int_ty(32, true),
    })
}

fn local(name: &str) -> HirExpr {
    HirExpr::AddressOfLocal(name.to_string())
}

/// A returned pointer to the start of a 128-byte local; the machine stores at
/// `ptr + 64` -- the first byte past the old fixed 64-byte check window.
/// `mov [rsp-0x40], edi; lea rax, [rsp-0x80]; ret`
#[test]
fn returned_frame_pointer_difference_at_plus_64_is_caught() {
    let elf = build_x86_64_elf(
        "escape_plus_64",
        &[
            0x89, 0x7c, 0x24, 0xc0, // mov [rsp-0x40], edi
            0x48, 0x8d, 0x44, 0x24, 0x80, // lea rax, [rsp-0x80]
            0xc3, // ret
        ],
    );
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    // buf = [entry_sp - 0x80, entry_sp) = cfa - 0x88 on SysV x86-64.
    let frame_offsets: BTreeMap<String, i64> = [("buf".to_string(), -0x88)].into_iter().collect();
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, frame_offsets);
    let params = vec![binding("param_1", int_ty(32, true))];
    let locals = vec![binding("buf", aggregate_ty(128))];
    let case = AdmissibleCase {
        args: &[0x1234_5678],
        initial_memory: &[],
    };

    // The tree stores the wrong value at +64.
    let wrong = make_hir(
        "escape_plus_64_wrong",
        params.clone(),
        locals.clone(),
        ptr_ty(),
        vec![
            store_i32_at(local("buf"), 64, HirExpr::Const(7, int_ty(32, true))),
            HirStmt::Return(Some(local("buf"))),
        ],
    );
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&wrong),
        &wrong,
        &case,
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Diverged(_)),
        "a wrong store at +64 inside the escaped object must be compared: {:?}",
        report.outcome
    );
    assert_denominators_consistent(&report);

    // The tree drops the store at +64.
    let dropped = make_hir(
        "escape_plus_64_dropped",
        params.clone(),
        locals.clone(),
        ptr_ty(),
        vec![HirStmt::Return(Some(local("buf")))],
    );
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&dropped),
        &dropped,
        &case,
        &grounding,
    );
    match &report.outcome {
        VerifyOutcome::Inconclusive { reason } => assert!(
            reason.contains("escaped frame object byte"),
            "the +64 difference must be seen: {reason}"
        ),
        other => panic!("dropped store at +64 must be inconclusive: {other:?}"),
    }
    assert_denominators_consistent(&report);

    let matching = make_hir(
        "escape_plus_64_ok",
        params,
        locals,
        ptr_ty(),
        vec![
            store_i32_at(local("buf"), 64, HirExpr::Var("param_1".to_string())),
            HirStmt::Return(Some(local("buf"))),
        ],
    );
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&matching),
        &matching,
        &case,
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Inconclusive { .. }),
        "a returned frame pointer never yields Equivalent: {:?}",
        report.outcome
    );
    assert_eq!(report.denominators.unjustified_frame_runs, 1);
    assert_denominators_consistent(&report);
}

/// A returned interior pointer (`buf + 0x40`); the machine stores at
/// `ptr - 8`, a negative offset inside the same object.
/// `mov [rsp-0x48], edi; lea rax, [rsp-0x40]; ret`
#[test]
fn returned_interior_frame_pointer_negative_offset_difference_is_caught() {
    let elf = build_x86_64_elf(
        "escape_negative",
        &[
            0x89, 0x7c, 0x24, 0xb8, // mov [rsp-0x48], edi
            0x48, 0x8d, 0x44, 0x24, 0xc0, // lea rax, [rsp-0x40]
            0xc3, // ret
        ],
    );
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    let frame_offsets: BTreeMap<String, i64> = [("buf".to_string(), -0x88)].into_iter().collect();
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, frame_offsets);
    // The tree stores the wrong value at `ptr - 8`.
    let buggy = make_hir(
        "escape_negative_buggy",
        vec![binding("param_1", int_ty(32, true))],
        vec![binding("buf", aggregate_ty(128))],
        ptr_ty(),
        vec![
            store_i32_at(local("buf"), 0x38, HirExpr::Const(7, int_ty(32, true))),
            HirStmt::Return(Some(HirExpr::PtrOffset {
                base: Box::new(local("buf")),
                offset: 0x40,
            })),
        ],
    );
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&buggy),
        &buggy,
        &AdmissibleCase {
            args: &[0x1234_5678],
            initial_memory: &[],
        },
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Diverged(_)),
        "a store below an escaped interior pointer must be compared: {:?}",
        report.outcome
    );
    assert_denominators_consistent(&report);
}

/// A frame object passed to a callee is observable by that callee; a store
/// into it after the call (at +64) that the tree drops must not pass as
/// `Equivalent`.
#[test]
fn frame_object_passed_to_callee_is_compared_in_full() {
    // 0x00 sub rsp, 0x88 / 0x07 lea rdi, [rsp+8] / 0x0c call 0x23
    // 0x11 mov dword [rsp+0x48], 42 / 0x19 xor eax, eax / 0x1b add rsp, 0x88
    // 0x22 ret / 0x23 callee: ret
    let code = [
        0x48, 0x81, 0xec, 0x88, 0x00, 0x00, 0x00, // sub rsp, 0x88
        0x48, 0x8d, 0x7c, 0x24, 0x08, // lea rdi, [rsp+8]
        0xe8, 0x12, 0x00, 0x00, 0x00, // call +0x12 -> 0x23
        0xc7, 0x44, 0x24, 0x48, 0x2a, 0x00, 0x00, 0x00, // mov dword [rsp+0x48], 42
        0x31, 0xc0, // xor eax, eax
        0x48, 0x81, 0xc4, 0x88, 0x00, 0x00, 0x00, // add rsp, 0x88
        0xc3, // ret
        0xc3, // callee: ret
    ];
    let elf = build_x86_64_elf("passed_to_callee", &code);
    let call_site = TEST_CODE_VADDR + 0x0c;
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    // buf = [rsp+8, rsp+0x88) = [entry_sp - 0x80, entry_sp) = cfa - 0x88.
    let frame_offsets: BTreeMap<String, i64> = [("buf".to_string(), -0x88)].into_iter().collect();
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, frame_offsets);
    assert!(
        grounding
            .flow
            .as_ref()
            .is_some_and(|flow| flow.calls.contains_key(&call_site)),
        "fixture call must be decoded: {:?}",
        grounding.flow
    );
    let plan = RunPlan {
        runs: 4,
        ..RunPlan::default()
    };

    let buggy = make_hir(
        "passed_buggy",
        Vec::new(),
        vec![binding("buf", aggregate_ty(128))],
        int_ty(32, true),
        vec![
            call_stmt(call_site, vec![local("buf")]),
            HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true)))),
        ],
    );
    let report = check_ground_truth_report(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&buggy),
        &buggy,
        &plan,
        &grounding,
    );
    match &report.outcome {
        VerifyOutcome::Inconclusive { reason } => assert!(
            reason.contains("escaped frame object byte"),
            "the dropped store must be seen: {reason}"
        ),
        other => panic!(
            "a dropped store into a frame object a callee received must never pass: {other:?}"
        ),
    }
    assert_denominators_consistent(&report);

    let wrong = make_hir(
        "passed_wrong",
        Vec::new(),
        vec![binding("buf", aggregate_ty(128))],
        int_ty(32, true),
        vec![
            call_stmt(call_site, vec![local("buf")]),
            store_i32_at(local("buf"), 64, HirExpr::Const(43, int_ty(32, true))),
            HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true)))),
        ],
    );
    let report = check_ground_truth_report(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&wrong),
        &wrong,
        &plan,
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Diverged(_)),
        "a wrong store into a frame object a callee received must diverge: {:?}",
        report.outcome
    );
    assert_denominators_consistent(&report);

    let matching = make_hir(
        "passed_ok",
        Vec::new(),
        vec![binding("buf", aggregate_ty(128))],
        int_ty(32, true),
        vec![
            call_stmt(call_site, vec![local("buf")]),
            store_i32_at(local("buf"), 64, HirExpr::Const(42, int_ty(32, true))),
            HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true)))),
        ],
    );
    let report = check_ground_truth_report(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&matching),
        &matching,
        &plan,
        &grounding,
    );
    assert_eq!(report.outcome, VerifyOutcome::Equivalent { checked: 4 });
    assert_denominators_consistent(&report);
}

/// A frame pointer passed to 17 callees and then stored to caller-visible
/// memory. The store is an unjustified escape no matter how many callee
/// escapes were seen first; a sample cap must not hide it.
#[test]
fn stored_frame_pointer_is_detected_after_many_callee_escapes() {
    const CALLS: usize = 17;
    let mut code = vec![
        0x53, // push rbx
        0x48, 0x89, 0xf3, // mov rbx, rsi
        0x48, 0x81, 0xec, 0x80, 0x00, 0x00, 0x00, // sub rsp, 0x80
    ];
    let call_block_start = code.len();
    let tail_len = 4 + 3 + 2 + 7 + 1 + 1;
    let callee_off = call_block_start + CALLS * 9 + tail_len;
    let mut sites = Vec::new();
    for _ in 0..CALLS {
        code.extend_from_slice(&[0x48, 0x8d, 0x3c, 0x24]); // lea rdi, [rsp]
        let site = code.len();
        let rel = (callee_off as i64 - (site as i64 + 5)) as i32;
        code.push(0xe8);
        code.extend_from_slice(&rel.to_le_bytes());
        sites.push(TEST_CODE_VADDR + site as u64);
    }
    code.extend_from_slice(&[0x48, 0x8d, 0x3c, 0x24]); // lea rdi, [rsp]
    code.extend_from_slice(&[0x48, 0x89, 0x3b]); // mov [rbx], rdi
    code.extend_from_slice(&[0x31, 0xc0]); // xor eax, eax
    code.extend_from_slice(&[0x48, 0x81, 0xc4, 0x80, 0x00, 0x00, 0x00]); // add rsp, 0x80
    code.push(0x5b); // pop rbx
    code.push(0xc3); // ret
    assert_eq!(code.len(), callee_off);
    code.push(0xc3); // callee: ret

    let elf = build_x86_64_elf("stored_after_many_calls", &code);
    let mut harness = EmulatorHarness::build(&elf.path, Some(200_000)).expect("harness");
    // buf = [entry_sp - 0x88, entry_sp - 0x08) = cfa - 0x90.
    let frame_offsets: BTreeMap<String, i64> = [("buf".to_string(), -0x90)].into_iter().collect();
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, frame_offsets);
    let mut body: Vec<HirStmt> = sites
        .iter()
        .map(|&site| call_stmt(site, vec![local("buf")]))
        .collect();
    body.push(HirStmt::Assign {
        lhs: HirLValue::Deref {
            ptr: Box::new(HirExpr::Var("param_2".to_string())),
            ty: ptr_ty(),
        },
        rhs: local("buf"),
    });
    body.push(HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true)))));
    let hir = make_hir(
        "stored_after_many_calls",
        vec![binding("param_2", ptr_ty())],
        vec![binding("buf", aggregate_ty(128))],
        int_ty(32, true),
        body,
    );
    let report = check_ground_truth_report(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&hir),
        &hir,
        &RunPlan {
            runs: 4,
            ..RunPlan::default()
        },
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Inconclusive { .. }),
        "a frame pointer stored to caller-visible memory must never pass: {:?}",
        report.outcome
    );
    // Runs whose synthetic pointer lands on an unmappable page are skipped;
    // every run that executed is unjustified, none agreed.
    assert!(report.denominators.unjustified_frame_runs > 0);
    assert_eq!(
        report.denominators.unjustified_frame_runs,
        report.denominators.unknown_runs
    );
    assert_eq!(report.denominators.agreed_runs, 0);
    let audit = report.witnesses[0].frame_audit.as_ref().expect("audit");
    assert!(
        audit
            .escaped_frame_pointers
            .iter()
            .any(|e| matches!(e, EscapedFramePointer::StoredExternally { .. })),
        "stored escape must be recorded: {audit:?}"
    );
    assert_denominators_consistent(&report);
}

/// An input pointer into the callee's private frame (not a state a native
/// caller can produce) with a declared 0x130-byte caller object; the machine
/// stores at `ptr + 0x12c`, past the old fixed 256-byte alias window.
/// `mov [rdi+0x12c], esi; xor eax, eax; ret`
#[test]
fn aliased_input_object_is_observable_past_a_fixed_window() {
    let elf = build_x86_64_elf(
        "alias_past_window",
        &[
            0x89, 0xb7, 0x2c, 0x01, 0x00, 0x00, // mov [rdi+0x12c], esi
            0x31, 0xc0, // xor eax, eax
            0xc3, // ret
        ],
    );
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    harness
        .begin_native_run(TEST_CODE_VADDR, &[0, 0])
        .expect("probe sp");
    let cfa = harness.cfa().expect("cfa");
    let alias_addr = cfa - 8 - 0x400;
    let initial_memory: Vec<(u64, u8)> = (0..0x130).map(|i| (alias_addr + i, 0u8)).collect();
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, BTreeMap::new());
    let params = vec![
        binding("param_1", ptr_ty()),
        binding("param_2", int_ty(32, true)),
    ];
    let case = AdmissibleCase {
        args: &[alias_addr, 0x1122_3344],
        initial_memory: &initial_memory,
    };

    let buggy = make_hir(
        "alias_past_window_buggy",
        params.clone(),
        Vec::new(),
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
    );
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&buggy),
        &buggy,
        &case,
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Diverged(_)),
        "a dropped store at +0x12c into the declared aliased object must diverge: {:?}",
        report.outcome
    );
    assert_denominators_consistent(&report);

    let matching = make_hir(
        "alias_past_window_ok",
        params,
        Vec::new(),
        int_ty(32, true),
        vec![
            store_i32_at(
                HirExpr::Var("param_1".to_string()),
                0x12c,
                HirExpr::Var("param_2".to_string()),
            ),
            HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true)))),
        ],
    );
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&matching),
        &matching,
        &case,
        &grounding,
    );
    assert_eq!(report.outcome, VerifyOutcome::Equivalent { checked: 1 });
    assert_eq!(
        report.native_admissible_agreements(),
        0,
        "a caller object below the callee's entry SP is not a native state"
    );
    assert_eq!(report.synthetic_agreements(), 1);
    assert_denominators_consistent(&report);
}

/// The machine clobbers callee-saved `rbx`; the scope says callee-saved state
/// is checked, so the run cannot count as agreement.
/// `mov ebx, edi; mov eax, edi; ret`
#[test]
fn callee_saved_clobber_is_never_counted_as_agreement() {
    let elf = build_x86_64_elf("clobber_rbx", &[0x89, 0xfb, 0x89, 0xf8, 0xc3]);
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, BTreeMap::new());
    let hir = make_hir(
        "clobber_rbx",
        vec![binding("param_1", int_ty(32, true))],
        Vec::new(),
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Var("param_1".to_string())))],
    );
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&hir),
        &hir,
        &AdmissibleCase {
            args: &[0x55],
            initial_memory: &[],
        },
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Inconclusive { .. }),
        "an ABI violation must not be reported as agreement: {:?}",
        report.outcome
    );
    assert_eq!(report.denominators.agreed_runs, 0);
    assert_denominators_consistent(&report);
}

/// The tree reads a frame byte nothing wrote while the machine does not: a
/// divergence, and it must be counted as one.
/// `mov eax, edi; ret`
#[test]
fn undefined_read_divergence_is_counted_as_diverged() {
    let elf = build_x86_64_elf("tree_undefined_read", &[0x89, 0xf8, 0xc3]);
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    let frame_offsets: BTreeMap<String, i64> = [("local_x".to_string(), -12)].into_iter().collect();
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, frame_offsets);
    let hir = make_hir(
        "tree_undefined_read",
        vec![binding("param_1", int_ty(32, true))],
        vec![binding("local_x", int_ty(32, true))],
        int_ty(32, true),
        vec![HirStmt::Return(Some(HirExpr::Load {
            ptr: Box::new(local("local_x")),
            ty: int_ty(32, true),
        }))],
    );
    let report = check_admissible_case(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&hir),
        &hir,
        &AdmissibleCase {
            args: &[7],
            initial_memory: &[],
        },
        &grounding,
    );
    assert!(
        matches!(report.outcome, VerifyOutcome::Diverged(_)),
        "{:?}",
        report.outcome
    );
    assert_eq!(report.denominators.diverged_runs, 1);
    assert_denominators_consistent(&report);
}

/// The decompiler infers a 64-byte local where the machine's buffer is 128
/// bytes; the buffer escapes to a callee and the machine then stores at
/// `buf + 64`, one byte past the inferred extent, which the tree drops. An
/// extent only the decompiler vouches for cannot justify agreement.
#[test]
fn inferred_extent_too_small_with_callee_escape_is_never_equivalent() {
    // Same machine code as `frame_object_passed_to_callee_is_compared_in_full`.
    let code = [
        0x48, 0x81, 0xec, 0x88, 0x00, 0x00, 0x00, // sub rsp, 0x88
        0x48, 0x8d, 0x7c, 0x24, 0x08, // lea rdi, [rsp+8]
        0xe8, 0x12, 0x00, 0x00, 0x00, // call +0x12 -> 0x23
        0xc7, 0x44, 0x24, 0x48, 0x2a, 0x00, 0x00, 0x00, // mov dword [rsp+0x48], 42
        0x31, 0xc0, // xor eax, eax
        0x48, 0x81, 0xc4, 0x88, 0x00, 0x00, 0x00, // add rsp, 0x88
        0xc3, // ret
        0xc3, // callee: ret
    ];
    let elf = build_x86_64_elf("inferred_too_small", &code);
    let call_site = TEST_CODE_VADDR + 0x0c;
    let mut harness = EmulatorHarness::build(&elf.path, Some(50_000)).expect("harness");
    let frame_offsets: BTreeMap<String, i64> = [("buf".to_string(), -0x88)].into_iter().collect();
    let grounding = leaf_grounding(&elf.path, TEST_CODE_VADDR, frame_offsets);
    let hir = make_hir(
        "inferred_too_small",
        Vec::new(),
        vec![binding("buf", aggregate_ty(64))],
        int_ty(32, true),
        vec![
            call_stmt(call_site, vec![local("buf")]),
            HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true)))),
        ],
    );
    let report = check_ground_truth_report(
        &mut harness,
        TEST_CODE_VADDR,
        &to_prehir(&hir),
        &hir,
        &RunPlan {
            runs: 4,
            ..RunPlan::default()
        },
        &grounding,
    );
    match &report.outcome {
        VerifyOutcome::Inconclusive { reason } => assert!(
            reason.contains("inferred extent"),
            "the reason must name the unjustified inferred extent: {reason}"
        ),
        other => panic!(
            "a callee escape bounded only by a too-small inferred extent must not pass: {other:?}"
        ),
    }
    assert_eq!(report.denominators.agreed_runs, 0);
    assert_denominators_consistent(&report);
}
