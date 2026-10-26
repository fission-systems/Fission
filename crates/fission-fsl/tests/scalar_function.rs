#![cfg(feature = "x86")]
use fission_fsl::{pipeline::owned_x86_package, x86::X86Program};
fn project(bytes: &[u8]) -> Result<(String, String), fission_fsl::FslError> {
    X86Program::lift(&owned_x86_package().unwrap(), 64, 0x1000, bytes)?
        .emit_scalar_function("sysv-amd64-u64-leaf-v1", "recovered_test")
}
#[test]
fn leaf_recovers_observable_register_prefix() {
    let (c, report) = project(&[0x48, 0x89, 0xf8, 0xc3]).unwrap(); // RDI -> RAX
    assert!(c.contains("uint64_t recovered_test(uint64_t arg0)"));
    assert!(report.contains("parameter-prefix=1 input-slots={7}"));
    assert!(report.contains("return-paths=1"));
    assert!(!c.contains("uint32_t fsl_x86("));
    let (_, report) = project(&[0x48, 0x89, 0xd0, 0xc3]).unwrap(); // RDX -> RAX
    assert!(report.contains("parameter-prefix=3 input-slots={2}"));
}
#[test]
fn branches_recover_both_return_paths_and_control_inputs() {
    // cmp rdi,0; je +4; mov rax,rsi; ret; mov rax,rdx; ret
    let bytes = [
        0x48, 0x83, 0xff, 0, 0x74, 4, 0x48, 0x89, 0xf0, 0xc3, 0x48, 0x89, 0xd0, 0xc3,
    ];
    let (_, report) = project(&bytes).unwrap();
    assert!(
        report.contains("parameter-prefix=3 input-slots={2, 6, 7}"),
        "{report}"
    );
    assert!(report.contains("return-paths=2"));
    assert!(report.contains("cfg-edge 0x1004 -> 0x100a"));
    assert!(report.contains("cfg-edge 0x1004 -> 0x1006"));
}
#[test]
fn refuses_unproved_function_contracts() {
    for (bytes, reason) in [
        (vec![0xc3], "non-argument"), // unknown entry RAX
        (vec![0x48, 0x89, 0xf8, 0x48, 0x89, 0xfb, 0xc3], "preserved"), // clobber RBX
        (vec![0x48, 0x89, 0xe0, 0xc3], "non-argument"), // return stack pointer
        (vec![0x48, 0x8b, 0x07, 0xc3], "non-return memory"), // dead/faulting load too
        (vec![0x74, 0x00, 0x48, 0x89, 0xf8, 0xc3], "entry flags"),
        (
            vec![0x48, 0x83, 0xff, 0, 0x74, 0xfa, 0x48, 0x89, 0xf8, 0xc3],
            "cycle",
        ),
        (
            vec![0x48, 0x89, 0xf8, 0x48, 0x83, 0xc4, 8, 0xc3],
            "entry-stack",
        ),
        (
            vec![0x48, 0x89, 0xf8, 0x48, 0x89, 0x07, 0xc3],
            "memory writes",
        ),
    ] {
        let err = project(&bytes).unwrap_err();
        assert!(err.message.contains(reason), "{bytes:x?}: {err}");
    }
}
#[test]
fn c_symbol_and_abi_are_explicit_and_bounded() {
    let p = X86Program::lift(
        &owned_x86_package().unwrap(),
        64,
        0x1000,
        &[0x48, 0x89, 0xf8, 0xc3],
    )
    .unwrap();
    for symbol in [
        "return",
        "NULL",
        "recovered_x;bad",
        "recovered_é",
        "fsl_internal_x",
    ] {
        assert!(p
            .emit_scalar_function("sysv-amd64-u64-leaf-v1", symbol)
            .is_err());
    }
    assert!(p.emit_scalar_function("auto", "recovered_x").is_err());
    assert!(X86Program::lift(
        &owned_x86_package().unwrap(),
        32,
        0x1000,
        &[0x89, 0xf8, 0xc3]
    )
    .unwrap()
    .emit_scalar_function("sysv-amd64-u64-leaf-v1", "recovered_x")
    .is_err());
}
