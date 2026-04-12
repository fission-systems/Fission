/// Tests for ABI-aware register parameter naming.
///
/// `register_name_with_param` maps Ghidra REGISTER-space offsets to either
/// `("param_N", Some(N-1))` for parameter registers or `(hw_name, None)` for others.
/// The distinction depends on the active `CallingConvention`.
use super::*;
use crate::nir::AbiState;

// ── Windows x64 ────────────────────────────────────────────────────────────────

#[test]
fn win64_rcx_is_param_1() {
    let (name, idx) = register_name_with_param(0x08, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
}

#[test]
fn win64_rdx_is_param_2() {
    let (name, idx) = register_name_with_param(0x10, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_2");
    assert_eq!(idx, Some(1));
}

#[test]
fn win64_r8_is_param_3() {
    let (name, idx) = register_name_with_param(0x80, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_3");
    assert_eq!(idx, Some(2));
}

#[test]
fn win64_r9_is_param_4() {
    let (name, idx) = register_name_with_param(0x88, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_4");
    assert_eq!(idx, Some(3));
}

#[test]
fn win64_rdi_is_not_a_param() {
    let (name, idx) = register_name_with_param(0x38, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "rdi");
    assert_eq!(idx, None);
}

#[test]
fn win64_rsi_is_not_a_param() {
    let (name, idx) = register_name_with_param(0x30, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "rsi");
    assert_eq!(idx, None);
}

// ── System V AMD64 ─────────────────────────────────────────────────────────────

#[test]
fn sysv_rdi_is_param_1() {
    let (name, idx) = register_name_with_param(0x38, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
}

#[test]
fn sysv_rsi_is_param_2() {
    let (name, idx) = register_name_with_param(0x30, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_2");
    assert_eq!(idx, Some(1));
}

#[test]
fn sysv_rdx_is_param_3() {
    let (name, idx) = register_name_with_param(0x10, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_3");
    assert_eq!(idx, Some(2));
}

#[test]
fn sysv_rcx_is_param_4() {
    let (name, idx) = register_name_with_param(0x08, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_4");
    assert_eq!(idx, Some(3));
}

#[test]
fn sysv_r8_is_param_5() {
    let (name, idx) = register_name_with_param(0x80, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_5");
    assert_eq!(idx, Some(4));
}

#[test]
fn sysv_r9_is_param_6() {
    let (name, idx) = register_name_with_param(0x88, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_6");
    assert_eq!(idx, Some(5));
}

// ── Non-param registers must always use hardware names ─────────────────────────

#[test]
fn rax_is_never_a_param() {
    for abi in [
        CallingConvention::WindowsX64,
        CallingConvention::SystemVAmd64,
    ] {
        let (name, idx) = register_name_with_param(0x00, 8, abi).unwrap();
        assert_eq!(name, "rax", "rax should stay 'rax' in {abi:?}");
        assert_eq!(idx, None, "rax must not be a param in {abi:?}");
    }
}

#[test]
fn rsp_is_never_a_param() {
    for abi in [
        CallingConvention::WindowsX64,
        CallingConvention::SystemVAmd64,
    ] {
        let (name, idx) = register_name_with_param(0x20, 8, abi).unwrap();
        assert_eq!(name, "rsp");
        assert_eq!(idx, None);
    }
}

#[test]
fn unknown_offset_returns_none() {
    for abi in [
        CallingConvention::WindowsX64,
        CallingConvention::SystemVAmd64,
    ] {
        assert!(register_name_with_param(0xDEAD, 8, abi).is_none());
    }
}

// ── x64_ghidra_reg_name is always ABI-independent ─────────────────────────────

#[test]
fn ghidra_reg_name_is_hardware_canonical() {
    assert_eq!(x64_ghidra_reg_name(0x00), Some("rax"));
    assert_eq!(x64_ghidra_reg_name(0x08), Some("rcx"));
    assert_eq!(x64_ghidra_reg_name(0x10), Some("rdx"));
    assert_eq!(x64_ghidra_reg_name(0x30), Some("rsi"));
    assert_eq!(x64_ghidra_reg_name(0x38), Some("rdi"));
    assert_eq!(x64_ghidra_reg_name(0x80), Some("r8"));
    assert_eq!(x64_ghidra_reg_name(0x88), Some("r9"));
    assert_eq!(x64_ghidra_reg_name(0xDEAD), None);
}

#[test]
fn abi_state_classifies_win64_home_slot() {
    let abi = AbiState::new(CallingConvention::WindowsX64, true, 8, 0x40);
    assert_eq!(
        abi.classify_stack_slot_origin(StackBase::Rsp, 0x40),
        NirBindingOrigin::HomeSlot(0x40)
    );
    assert_eq!(
        abi.classify_stack_slot_origin(StackBase::Rsp, 0x20),
        NirBindingOrigin::StackOffset(0x20)
    );
}

#[test]
fn abi_state_recovers_win64_stack_arg_index() {
    let abi = AbiState::new(CallingConvention::WindowsX64, true, 8, 0x40);
    assert_eq!(abi.stack_argument_index(0x20), Some(0));
    assert_eq!(abi.stack_argument_index(0x28), Some(1));
    assert_eq!(abi.stack_argument_index(0x18), None);
}
