use fission_fsl::abi::compile_abi_source;
use fission_fsl::registers::{
    compile_layout_source, execute_bound, link_abi, RegisterBinding, RegisterFile,
};
use fission_fsl::{compile_source, ExecutionStatus, FslcPackage};

const SOURCE_ADD: &str = include_str!("../specs/ebpf-add64-register.fsl");
const SLA_ADD: &str = include_str!("../specs/ebpf-sla-add64-register.fsl");
const SOURCE_LAYOUT: &str = include_str!("../specs/ebpf.le.registers.fslregs");
const SLA_LAYOUT: &str = include_str!("../specs/ebpf.le.sla.registers.fslregs");

#[test]
fn sla_derived_layout_and_canonical_fir_match_source_candidate() {
    let layout = compile_layout_source(SLA_LAYOUT).unwrap();
    let source = compile_layout_source(SOURCE_LAYOUT).unwrap();
    assert_eq!(layout.default_space, source.default_space);
    assert_eq!(layout.spaces, source.spaces);
    assert_eq!(layout.registers, source.registers);
    assert!(layout.evidence[0]
        .url
        .ends_with("compiled/eBPF/eBPF_le.sla"));
    let abi = compile_abi_source(include_str!("../specs/ebpf.fslabi")).unwrap();
    let linked = link_abi(&abi, &layout).unwrap();
    assert_eq!(layout.view(linked.stack_register).unwrap().offset, 80);
    let sla = compile_source(SLA_ADD).unwrap();
    let source = compile_source(SOURCE_ADD).unwrap();
    assert_eq!(sla.version, 3);
    let binary = sla.encode_binary().unwrap();
    assert_eq!(FslcPackage::decode_binary(&binary).unwrap(), sla);
    let a = &sla.instructions[0];
    let b = &source.instructions[0];
    assert_eq!(a.encoding, b.encoding);
    assert_eq!(a.values, b.values);
    assert_eq!(a.ops, b.ops);
    assert_eq!(a.mnemonic, b.mnemonic);
    let mut accepted = 0;
    for prefix in 0..=u16::MAX {
        let raw = [
            prefix as u8,
            (prefix >> 8) as u8,
            0x34,
            0x12,
            0x78,
            0x56,
            0x34,
            0x12,
        ];
        let da = sla.decode_bytes(&sla.language, &raw).unwrap();
        let db = source.decode_bytes(&source.language, &raw).unwrap();
        let oracle = raw[0] == 0x0f && raw[1] & 15 <= 10 && raw[1] >> 4 <= 10;
        assert_eq!(da.is_some(), oracle);
        assert_eq!(db.is_some(), oracle);
        if let (Some(da), Some(db)) = (da, db) {
            assert_eq!(da.fields, db.fields);
            assert_eq!(sla.reencode(&da, &[]).unwrap(), raw);
            assert_eq!(source.reencode(&db, &[]).unwrap(), raw);
            accepted += 1;
        }
    }
    assert_eq!(accepted, 121);
    println!("source/SLA candidate prefix comparisons=65536; admitted=121; unchanged unused suffix bits retained");
}

#[test]
fn sla_derived_fir_executes_through_own_byte_storage_and_abi_link() {
    let package = compile_source(SLA_ADD).unwrap();
    let layout = compile_layout_source(SLA_LAYOUT).unwrap();
    let abi = compile_abi_source(include_str!("../specs/ebpf.fslabi")).unwrap();
    link_abi(&abi, &layout).unwrap();
    let binding = RegisterBinding {
        registers: (0..11).map(|i| format!("R{i}")).collect(),
        flags: vec![],
    };
    let mut count = 0;
    for dst in 0..11u8 {
        for src in 0..11u8 {
            for (left, right) in [
                (0, 0),
                (u64::MAX, 1),
                (u64::MAX, u64::MAX),
                (123456789, 987654321),
            ] {
                let raw = [0x0f, dst | src << 4, 0, 0, 0, 0, 0, 0];
                let decoded = package
                    .decode_bytes(&package.language, &raw)
                    .unwrap()
                    .unwrap();
                let mut file = RegisterFile::new(layout.clone()).unwrap();
                let mut expected: Vec<u64> = (0..11).map(|i| i + 77).collect();
                expected[dst as usize] = left;
                expected[src as usize] = right;
                for (i, &v) in expected.iter().enumerate() {
                    file.write_u64(&format!("R{i}"), v).unwrap();
                }
                file.write_u64("PC", 0xabcdef).unwrap();
                expected[dst as usize] = ((u128::from(expected[dst as usize])
                    + u128::from(expected[src as usize]))
                    % (1u128 << 64)) as u64;
                assert_eq!(
                    execute_bound(&package, &decoded, &binding, &mut file).unwrap(),
                    ExecutionStatus::Success
                );
                for (i, &v) in expected.iter().enumerate() {
                    assert_eq!(file.read_bytes(&format!("R{i}")).unwrap(), v.to_le_bytes());
                }
                assert_eq!(file.read_u64("PC").unwrap(), 0xabcdef);
                count += 1;
            }
        }
    }
    assert_eq!(count, 484);
    println!("SLA-derived bound FIR state comparisons={count}; not live kernel/VM execution");
}
