use fission_fsl::{
    compile_source, execute_decoded_memory, ExecutionStatus, FslcPackage, MachineState,
};
const SOURCE: &str = r#"language memory.test {
 byte_order little; address_unit byte;
 instruction store {
 opcode 0x01; mnemonic "store";
 evidence "self" "https://example.invalid" "synthetic" "Generic ordered byte effect";
 semantics {
 %address: u64 = int.const 4096;
 %value: u32 = int.const 0x12345678;
 memory.store.le %address, %value;
 %after: u32 = memory.load.le %address;
 register.write destination, %after;
 }
 }
}"#;
// Supply a selector field using fixed-width encoding; no ISA-specific semantics.
fn source() -> String {
    SOURCE.replace(
        "opcode 0x01;",
        "encoding 32 mask 0xffffff00 value 0x01000000 { field destination offset 0 bits 8; }",
    )
}
#[test]
fn store_version_roundtrip_typed_read_after_write_and_transaction_failure() {
    let p = compile_source(&source()).unwrap();
    assert_eq!(p.version, 11);
    let p = FslcPackage::decode_binary(&p.encode_binary().unwrap()).unwrap();
    let d = p
        .decode_bytes("memory.test", &[0, 0, 0, 1])
        .unwrap()
        .unwrap();
    let mut s = MachineState {
        registers: vec![9],
        flags: vec![],
    };
    let mut memory = [0xaa; 8];
    assert_eq!(
        execute_decoded_memory(&p, &d, &mut s, &mut memory, 4096, None)
            .unwrap()
            .0,
        ExecutionStatus::Success
    );
    assert_eq!(s.registers, [0x12345678]);
    assert_eq!(memory, [0x78, 0x56, 0x34, 0x12, 0xaa, 0xaa, 0xaa, 0xaa]);
    let mut short = [0xaa; 3];
    let before = s.clone();
    assert_eq!(
        execute_decoded_memory(&p, &d, &mut s, &mut short, 4096, None)
            .unwrap()
            .0,
        ExecutionStatus::InvalidState
    );
    assert_eq!(short, [0xaa; 3]);
    assert_eq!(s, before);
    // A valid store precedes a wider out-of-bounds load: full transaction rollback.
    let late = compile_source(&source().replace("%after: u32", "%after: u64")).unwrap();
    let observation = late
        .decode_bytes("memory.test", &[0, 0, 0, 1])
        .unwrap()
        .unwrap();
    let mut bytes = [0xaa; 4];
    assert_eq!(
        execute_decoded_memory(&late, &observation, &mut s, &mut bytes, 4096, None)
            .unwrap()
            .0,
        ExecutionStatus::InvalidState
    );
    assert_eq!(bytes, [0xaa; 4]);
    assert_eq!(s, before);
    let mut old = p.clone();
    old.version = 10;
    assert!(old.encode_binary().is_err());
    assert!(compile_source(&source().replace("%address: u64", "%address: u32")).is_err());
    assert!(compile_source(&source().replace("%value: u32", "%value: i32")).is_err());
    assert!(fission_fsl::execute_decoded(&p, &d, &mut s).is_err());
}
