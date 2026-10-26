//! IBM physical OpenQASM projection of a deliberately small canonical FIR subset.
//! This is a source projection, not a scheduler, device transport or calibration.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entangler {
    Cx,
    Cz,
}
/// Explicit caller-supplied physical layout. The submission adapter must verify
/// rz/sx/x/reset/measure support and these coupling tuples against its snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub physical_qubits: Vec<u16>,
    pub entangler: Entangler,
    pub couplings: Vec<(u16, u16)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Circuit {
    pub qasm: String,
    /// Measurement record bit i corresponds to this canonical SSA definition.
    pub trace_values: Vec<ValueId>,
}
#[derive(Clone, Copy)]
enum Bit {
    Constant(bool),
    Measurement(usize),
}
fn h(text: &mut String, q: u16) {
    // H up to a global phase. Every condition in this projection is classical.
    writeln!(text, "rz(pi/2) ${q};\nsx ${q};\nrz(pi/2) ${q};").unwrap();
}

/// Prepare an explicit computational-basis input with reset/X, execute admitted
/// FIR, rotate each terminal observation basis (logical order XYZ), then measure.
/// Native gate availability remains an explicit checked submission requirement.
pub fn emit(
    body: &CompiledInstruction,
    target: &Target,
    initial_basis: usize,
    observation: &str,
) -> Result<Circuit, FslError> {
    let qubits = u8::try_from(target.physical_qubits.len())
        .map_err(|_| error("IBM layout exceeds reference qubit extent"))?;
    QuantumContract::for_instruction(body)?.admit_qubits(qubits)?;
    if !body.blocks.is_empty()
        || initial_basis >= 1usize << qubits
        || observation.len() != usize::from(qubits)
        || !observation.bytes().all(|b| matches!(b, b'X' | b'Y' | b'Z'))
    {
        return Err(error("IBM projection requires linear FIR, explicit basis input and one X/Y/Z observation per logical qubit"));
    }
    let unique: std::collections::HashSet<_> = target.physical_qubits.iter().collect();
    if unique.len() != target.physical_qubits.len() || target.couplings.iter().any(|(a, b)| a == b)
    {
        return Err(error(
            "IBM physical layout must be injective with distinct coupling endpoints",
        ));
    }
    for value in &body.values {
        require_bit(body, value.id)?;
    }
    let mut bits = vec![None; body.values.len()];
    let mut trace_values = Vec::new();
    let mut operations = String::new();
    let physical = |q: u8| target.physical_qubits[usize::from(q)];
    for (index, op) in body.ops.iter().enumerate() {
        writeln!(operations, "// FIR operation {index}").unwrap();
        match *op {
            FirOp::IntConstant { output, value } => {
                bits[usize::from(output.0)] = Some(Bit::Constant(value != 0));
            }
            FirOp::Quantum(QuantumOp::H { qubit }) => h(&mut operations, physical(qubit)),
            FirOp::Quantum(QuantumOp::X { qubit }) => {
                writeln!(operations, "x ${};", physical(qubit)).unwrap();
            }
            FirOp::Quantum(QuantumOp::Z { qubit }) => {
                writeln!(operations, "rz(pi) ${};", physical(qubit)).unwrap();
            }
            FirOp::Quantum(QuantumOp::Reset { qubit }) => {
                writeln!(operations, "reset ${};", physical(qubit)).unwrap();
            }
            FirOp::Quantum(QuantumOp::MeasureZ { qubit, output }) => {
                let slot = trace_values.len();
                trace_values.push(output);
                bits[usize::from(output.0)] = Some(Bit::Measurement(slot));
                writeln!(operations, "trace[{slot}] = measure ${};", physical(qubit)).unwrap();
            }
            FirOp::Quantum(QuantumOp::XIf { qubit, condition }) => {
                match bits[usize::from(condition.0)]
                    .ok_or_else(|| error("undefined IBM projection condition"))?
                {
                    Bit::Constant(false) => {}
                    Bit::Constant(true) => {
                        writeln!(operations, "x ${};", physical(qubit)).unwrap();
                    }
                    Bit::Measurement(slot) => {
                        writeln!(
                            operations,
                            "if (trace[{slot}] == 1) {{ x ${}; }}",
                            physical(qubit)
                        )
                        .unwrap();
                    }
                }
            }
            FirOp::Quantum(QuantumOp::Cx { control, target: q }) => {
                let (a, b) = (physical(control), physical(q));
                match target.entangler {
                    Entangler::Cx if target.couplings.contains(&(a, b)) => {
                        writeln!(operations, "cx ${a}, ${b};").unwrap();
                    }
                    Entangler::Cz => {
                        let pair = if target.couplings.contains(&(a, b)) {
                            (a, b)
                        } else if target.couplings.contains(&(b, a)) {
                            (b, a)
                        } else {
                            return Err(error("missing IBM CZ coupling; no routing fallback"));
                        };
                        h(&mut operations, b);
                        writeln!(operations, "cz ${}, ${};", pair.0, pair.1).unwrap();
                        h(&mut operations, b);
                    }
                    _ => {
                        return Err(error(
                            "missing directed IBM CX coupling; no routing fallback",
                        ))
                    }
                }
            }
            _ => {
                return Err(error(
                    "IBM projection refuses noise, arithmetic and foreign effects",
                ))
            }
        }
    }
    let mut qasm = String::from("OPENQASM 3.0;\ninclude \"stdgates.inc\";\n");
    if !trace_values.is_empty() {
        writeln!(qasm, "bit[{}] trace;", trace_values.len()).unwrap();
    }
    writeln!(
        qasm,
        "bit[{qubits}] out;\n// Explicit computational-basis preparation"
    )
    .unwrap();
    for (i, &q) in target.physical_qubits.iter().enumerate() {
        writeln!(qasm, "reset ${q};").unwrap();
        if initial_basis & (1usize << i) != 0 {
            writeln!(qasm, "x ${q};").unwrap();
        }
    }
    qasm.push_str(&operations);
    qasm.push_str("// Terminal observation; this wrapper is not a FIR effect\n");
    for (i, (axis, &q)) in observation.bytes().zip(&target.physical_qubits).enumerate() {
        if axis == b'Y' {
            writeln!(qasm, "rz(-pi/2) ${q};").unwrap();
        }
        if axis != b'Z' {
            h(&mut qasm, q);
        }
        writeln!(qasm, "out[{i}] = measure ${q};").unwrap();
    }
    Ok(Circuit { qasm, trace_values })
}
