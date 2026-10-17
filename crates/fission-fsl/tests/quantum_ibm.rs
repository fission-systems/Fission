use fission_fsl::{compile_source, quantum::ibm::*};
fn body(text: &str) -> fission_fsl::CompiledInstruction {
    compile_source(&format!("language ibm.test {{ byte_order little; address_unit byte; instruction test {{ opcode 0; mnemonic \"test\"; evidence \"authored\" \"synthetic\" \"v1\" \"IBM projection fixture\"; semantics {{ {text} }} }} }}")).unwrap().instructions.remove(0)
}
fn target() -> Target {
    Target {
        physical_qubits: vec![7, 3],
        entangler: Entangler::Cz,
        couplings: vec![(3, 7)],
    }
}
#[test]
fn physical_layout_preparation_observation_and_origins_are_explicit() {
    let body = body("quantum.h 0; quantum.cx 0,1; quantum.z 1;");
    let circuit = emit(&body, &target(), 1, "XY").unwrap();
    assert!(circuit.qasm.contains("reset $7;\nx $7;\nreset $3;"));
    assert!(circuit.qasm.contains("cz $3, $7;"));
    assert!(circuit.qasm.contains("rz(-pi/2) $3;"));
    assert!(circuit.qasm.contains("out[0] = measure $7;"));
    assert!(circuit.qasm.contains("// FIR operation 2"));
    assert!(!circuit.qasm.contains("qubit["));
    assert!(!circuit.qasm.contains("bit[0]"));
    assert!(circuit.trace_values.is_empty());
}
#[test]
fn measurement_slots_follow_canonical_ids_and_constant_conditions_are_folded() {
    let body=body("%off:u1=int.const 0; %on:u1=int.const 1; %m:u1=quantum.measure_z 0; quantum.x_if 1,%off; quantum.x_if 1,%on; quantum.x_if 1,%m;");
    let circuit = emit(&body, &target(), 0, "ZZ").unwrap();
    assert_eq!(circuit.trace_values, vec![fission_fsl::ValueId(2)]);
    assert!(circuit.qasm.contains("if (trace[0] == 1) { x $3; }"));
    assert_eq!(circuit.qasm.matches("x $3;").count(), 2);
}
#[test]
fn directed_cx_never_uses_a_reverse_edge_as_a_forward_gate() {
    let body = body("quantum.cx 0,1;");
    let mut t = target();
    t.entangler = Entangler::Cx;
    assert!(emit(&body, &t, 0, "ZZ").is_err());
    t.couplings = vec![(7, 3)];
    assert!(emit(&body, &t, 0, "ZZ")
        .unwrap()
        .qasm
        .contains("cx $7, $3;"));
    t.couplings.clear();
    assert!(emit(&body, &t, 0, "ZZ").is_err());
}
#[test]
fn unsupported_semantics_or_ambiguous_preconditions_refuse() {
    for source in [
        "quantum.bit_flip 0,1,2;",
        "%a:u1=int.const 1; %b:u1=u1.xor %a,%a; quantum.x_if 0,%b;",
        "block entry() { quantum.x 0; return; }",
    ] {
        assert!(emit(&body(source), &target(), 0, "ZZ").is_err());
    }
    let body = body("quantum.x 1;");
    for basis in ["Z", "ZA", "ZZZ"] {
        assert!(emit(&body, &target(), 0, basis).is_err());
    }
    assert!(emit(&body, &target(), 4, "ZZ").is_err());
    let mut t = target();
    t.physical_qubits = vec![7, 7];
    assert!(emit(&body, &t, 0, "ZZ").is_err());
    t.physical_qubits = vec![7];
    assert!(emit(&body, &t, 0, "Z").is_err());
}
