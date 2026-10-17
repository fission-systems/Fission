//! Ideal logical quantum effects in the canonical FIR, with a bounded numerical
//! reference interpreter. Qubit indices name subsystems of ONE joint state.
//! Simulator vector copies are host bookkeeping, never a guest cloning operation.
use crate::{CompiledInstruction, FirOp, FslError, IntegerSign, ValueId};
use std::fmt::Write;

pub use crate::quantum_qasm::{import_openqasm, ImportedQuantum};

/// Reference-host bound; canonical packed indices address up to 256 subsystems.
pub const MAX_QUBITS: u8 = 8;
pub const MAX_SPLITS: usize = 12;
fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantumOp {
    H { qubit: u8 },
    X { qubit: u8 },
    Z { qubit: u8 },
    Cx { control: u8, target: u8 },
    MeasureZ { qubit: u8, output: ValueId },
    Reset { qubit: u8 },
    XIf { qubit: u8, condition: ValueId },
}
impl QuantumOp {
    pub(crate) fn encode(self) -> (u8, u8, u16) {
        match self {
            Self::H { qubit } => (0, qubit, 0),
            Self::X { qubit } => (1, qubit, 0),
            Self::Z { qubit } => (2, qubit, 0),
            Self::Cx { control, target } => (3, control, u16::from(target)),
            Self::MeasureZ { qubit, output } => (4, qubit, output.0),
            Self::Reset { qubit } => (5, qubit, 0),
            Self::XIf { qubit, condition } => (6, qubit, condition.0),
        }
    }
    pub(crate) fn decode(tag: u8, qubit: u8, payload: u16) -> Result<Self, FslError> {
        let op = match tag {
            0 if payload == 0 => Self::H { qubit },
            1 if payload == 0 => Self::X { qubit },
            2 if payload == 0 => Self::Z { qubit },
            3 => Self::Cx {
                control: qubit,
                target: u8::try_from(payload).map_err(|_| error("bad quantum target"))?,
            },
            4 => Self::MeasureZ {
                qubit,
                output: ValueId(payload),
            },
            5 if payload == 0 => Self::Reset { qubit },
            6 => Self::XIf {
                qubit,
                condition: ValueId(payload),
            },
            _ => return Err(error("unknown or noncanonical quantum encoding")),
        };
        op.validate()?;
        Ok(op)
    }
    fn qubit(self) -> u8 {
        match self {
            Self::H { qubit }
            | Self::X { qubit }
            | Self::Z { qubit }
            | Self::Reset { qubit }
            | Self::MeasureZ { qubit, .. }
            | Self::XIf { qubit, .. } => qubit,
            Self::Cx { control, .. } => control,
        }
    }
    pub(crate) fn validate(self) -> Result<(), FslError> {
        if let Self::Cx { control, target } = self {
            if control == target {
                return Err(error("CX requires distinct qubits"));
            }
        }
        Ok(())
    }
}
pub(crate) fn require_bit(body: &CompiledInstruction, id: ValueId) -> Result<(), FslError> {
    if !body
        .values
        .get(usize::from(id.0))
        .is_some_and(|v| v.ty.bits == 1 && v.ty.sign == IntegerSign::Unsigned)
    {
        return Err(error("quantum measurement/condition requires classical u1"));
    }
    Ok(())
}

/// This slice admits linear ordered quantum effects plus classical u1 constants.
/// No allocator, general CFG, CPU/GPU effects, noisy channels or pulse timing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantumContract {
    pub minimum_qubits: u16,
    pub splits: usize,
}
impl QuantumContract {
    pub fn for_instruction(body: &CompiledInstruction) -> Result<Self, FslError> {
        body.validate()?;
        if !body.blocks.is_empty() || body.ops.len() > 256 || body.values.len() > 64 {
            return Err(error(
                "quantum reference requires linear <=256 ops / <=64 classical values",
            ));
        }
        let mut extent = 0;
        let mut splits = 0;
        let mut next = 0;
        for op in &body.ops {
            let output = match *op {
                FirOp::Quantum(q) => {
                    extent = extent.max(u16::from(q.qubit()) + 1);
                    if let QuantumOp::Cx { target, .. } = q {
                        extent = extent.max(u16::from(target) + 1);
                    }
                    match q {
                        QuantumOp::MeasureZ { output, .. } => {
                            splits += 1;
                            Some(output)
                        }
                        QuantumOp::Reset { .. } => {
                            splits += 1;
                            None
                        }
                        _ => None,
                    }
                }
                FirOp::IntConstant { output, .. } => {
                    require_bit(body, output)?;
                    Some(output)
                }
                _ => return Err(error("quantum reference does not support this FIR effect")),
            };
            if let Some(id) = output {
                if usize::from(id.0) != next {
                    return Err(error(
                        "quantum projection requires classical definitions in dense SSA order",
                    ));
                }
                next += 1;
            }
        }
        if extent == 0 || splits > MAX_SPLITS {
            return Err(error(
                "quantum profile requires quantum effects and <=12 measurement/reset operations",
            ));
        }
        Ok(Self {
            minimum_qubits: extent,
            splits,
        })
    }
    fn admit_qubits(&self, qubits: u8) -> Result<(), FslError> {
        if !(1..=MAX_QUBITS).contains(&qubits) || u16::from(qubits) < self.minimum_qubits {
            return Err(error("quantum state extent does not satisfy FIR contract"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}
impl Complex {
    fn norm(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    fn scale(self, x: f64) -> Self {
        Self {
            re: self.re * x,
            im: self.im * x,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PureState {
    qubits: u8,
    amplitudes: Vec<Complex>,
}
impl PureState {
    pub fn new(qubits: u8, amplitudes: Vec<Complex>) -> Result<Self, FslError> {
        if !(1..=MAX_QUBITS).contains(&qubits) || amplitudes.len() != 1usize << qubits {
            return Err(error(
                "pure state requires 1..8 qubits and exactly 2^n amplitudes",
            ));
        }
        let norm: f64 = amplitudes.iter().map(|v| v.norm()).sum();
        if amplitudes
            .iter()
            .any(|v| !v.re.is_finite() || !v.im.is_finite())
            || !norm.is_finite()
            || (norm - 1.0).abs() > 1e-12
        {
            return Err(error(
                "initial amplitudes must be finite and normalized within 1e-12",
            ));
        }
        Ok(Self { qubits, amplitudes })
    }
    /// Little-endian logical qubits: q[0] is basis-index bit 0.
    pub fn basis(qubits: u8, index: usize) -> Result<Self, FslError> {
        if !(1..=MAX_QUBITS).contains(&qubits) || index >= 1usize << qubits {
            return Err(error("basis state out of range"));
        }
        let mut amplitudes = vec![Complex::default(); 1usize << qubits];
        amplitudes[index].re = 1.0;
        Self::new(qubits, amplitudes)
    }
    pub fn amplitudes(&self) -> &[Complex] {
        &self.amplitudes
    }
    pub fn qubits(&self) -> u8 {
        self.qubits
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct QuantumBranch {
    pub probability: f64,
    pub state: PureState,
    /// Canonical classical SSA slots; unexecuted definitions are None.
    pub values: Vec<Option<u8>>,
    /// Observable measurement events only. Reset's internal outcomes are hidden.
    pub measurements: Vec<(ValueId, u8)>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct QuantumResult {
    branches: Vec<QuantumBranch>,
}
impl QuantumResult {
    pub fn branches(&self) -> &[QuantumBranch] {
        &self.branches
    }
    /// Debug/evidence output only; this is not a portable quantum package format.
    pub fn diagnostic_json(&self) -> String {
        let complex = |v: &[Complex]| {
            v.iter()
                .map(|a| format!("[{},{}]", a.re, a.im))
                .collect::<Vec<_>>()
                .join(",")
        };
        let mut text = format!("{{\"simulation\":\"ideal-logical-f64-enumerated\",\"hardware_execution\":false,\"initial_state\":\"caller-supplied\",\"qubits\":{},\"total_probability\":{},\"branches\":[",self.branches[0].state.qubits,self.total_probability());
        for (i, branch) in self.branches.iter().enumerate() {
            if i > 0 {
                text.push(',');
            }
            let values = branch
                .values
                .iter()
                .map(|v| v.map_or("null".into(), |v| v.to_string()))
                .collect::<Vec<_>>()
                .join(",");
            let measurements = branch
                .measurements
                .iter()
                .map(|(id, bit)| format!("[{},{}]", id.0, bit))
                .collect::<Vec<_>>()
                .join(",");
            write!(text,"{{\"probability\":{},\"values\":[{values}],\"measurements\":[{measurements}],\"amplitudes\":[{}]}}",branch.probability,complex(branch.state.amplitudes())).unwrap();
        }
        writeln!(
            text,
            "],\"density_matrix\":[{}]}}",
            complex(&self.density_matrix())
        )
        .unwrap();
        text
    }
    pub fn total_probability(&self) -> f64 {
        self.branches.iter().map(|b| b.probability).sum()
    }
    /// Ensemble density operator, row-major; retains coherence and reset mixtures.
    pub fn density_matrix(&self) -> Vec<Complex> {
        let n = self.branches[0].state.amplitudes.len();
        let mut out = vec![Complex::default(); n * n];
        for branch in &self.branches {
            for (i, a) in branch.state.amplitudes.iter().enumerate() {
                for (j, b) in branch.state.amplitudes.iter().enumerate() {
                    out[i * n + j].re += branch.probability * (a.re * b.re + a.im * b.im);
                    out[i * n + j].im += branch.probability * (a.im * b.re - a.re * b.im);
                }
            }
        }
        out
    }
}
fn x(state: &mut PureState, qubit: u8, control: Option<u8>) {
    let mask = 1usize << qubit;
    for i in 0..state.amplitudes.len() {
        if i & mask == 0 && control.is_none_or(|q| i & (1usize << q) != 0) {
            state.amplitudes.swap(i, i | mask);
        }
    }
}
fn split(branch: QuantumBranch, qubit: u8, output: Option<ValueId>) -> Vec<QuantumBranch> {
    let mask = 1usize << qubit;
    let mut probabilities = [0.0; 2];
    for (i, a) in branch.state.amplitudes.iter().enumerate() {
        probabilities[usize::from(i & mask != 0)] += a.norm();
    }
    let mut out = Vec::new();
    for (bit, p) in probabilities.into_iter().enumerate() {
        if p == 0.0 {
            continue;
        } // No epsilon pruning of physically possible outcomes.
        let mut child = branch.clone();
        child.probability *= p;
        for (i, a) in child.state.amplitudes.iter_mut().enumerate() {
            *a = if usize::from(i & mask != 0) == bit {
                a.scale(1.0 / p.sqrt())
            } else {
                Complex::default()
            };
        }
        if let Some(id) = output {
            child.values[usize::from(id.0)] = Some(bit as u8);
            child.measurements.push((id, bit as u8));
        } else if bit == 1 {
            x(&mut child.state, qubit, None);
        }
        out.push(child);
    }
    out
}
/// Enumerate weighted measurement branches, preserving each joint post-state.
/// f64 reference evidence, not symbolic equality or a hardware/noise model.
pub fn simulate(
    body: &CompiledInstruction,
    initial: &PureState,
) -> Result<QuantumResult, FslError> {
    let contract = QuantumContract::for_instruction(body)?;
    contract.admit_qubits(initial.qubits)?;
    let mut branches = vec![QuantumBranch {
        probability: 1.0,
        state: initial.clone(),
        values: vec![None; body.values.len()],
        measurements: Vec::new(),
    }];
    for op in &body.ops {
        if let FirOp::Quantum(QuantumOp::MeasureZ { qubit, output }) = *op {
            branches = branches
                .into_iter()
                .flat_map(|b| split(b, qubit, Some(output)))
                .collect();
            continue;
        }
        if let FirOp::Quantum(QuantumOp::Reset { qubit }) = *op {
            branches = branches
                .into_iter()
                .flat_map(|b| split(b, qubit, None))
                .collect();
            continue;
        }
        for branch in &mut branches {
            match *op {
                FirOp::IntConstant { output, value } => {
                    branch.values[usize::from(output.0)] = Some(value as u8)
                }
                FirOp::Quantum(q) => match q {
                    QuantumOp::X { qubit } => x(&mut branch.state, qubit, None),
                    QuantumOp::Cx { control, target } => {
                        x(&mut branch.state, target, Some(control))
                    }
                    QuantumOp::XIf { qubit, condition } => {
                        if branch.values[usize::from(condition.0)] == Some(1) {
                            x(&mut branch.state, qubit, None);
                        }
                    }
                    QuantumOp::Z { qubit } => {
                        for (i, a) in branch.state.amplitudes.iter_mut().enumerate() {
                            if i & (1usize << qubit) != 0 {
                                *a = a.scale(-1.0);
                            }
                        }
                    }
                    QuantumOp::H { qubit } => {
                        let mask = 1usize << qubit;
                        for i in 0..branch.state.amplitudes.len() {
                            if i & mask == 0 {
                                let a = branch.state.amplitudes[i];
                                let b = branch.state.amplitudes[i | mask];
                                branch.state.amplitudes[i] = Complex {
                                    re: a.re + b.re,
                                    im: a.im + b.im,
                                }
                                .scale(std::f64::consts::FRAC_1_SQRT_2);
                                branch.state.amplitudes[i | mask] = Complex {
                                    re: a.re - b.re,
                                    im: a.im - b.im,
                                }
                                .scale(std::f64::consts::FRAC_1_SQRT_2);
                            }
                        }
                    }
                    _ => unreachable!("measurement/reset handled above"),
                },
                _ => unreachable!("quantum admission rejects foreign effects"),
            }
        }
    }
    Ok(QuantumResult { branches })
}

/// Logical OpenQASM 3.0 projection. No hardware mapping, calibration or timing.
pub fn emit_openqasm(body: &CompiledInstruction, qubits: u8) -> Result<String, FslError> {
    QuantumContract::for_instruction(body)?.admit_qubits(qubits)?;
    let mut text = format!(
        "OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[{qubits}] q;\nbit[{}] m;\n",
        body.values.len().max(1)
    );
    for op in &body.ops {
        match *op {
            FirOp::IntConstant { output, value } => {
                writeln!(text, "m[{}] = {value};", output.0).unwrap()
            }
            FirOp::Quantum(q) => match q {
                QuantumOp::H { qubit } => writeln!(text, "h q[{qubit}];").unwrap(),
                QuantumOp::X { qubit } => writeln!(text, "x q[{qubit}];").unwrap(),
                QuantumOp::Z { qubit } => writeln!(text, "z q[{qubit}];").unwrap(),
                QuantumOp::Cx { control, target } => {
                    writeln!(text, "cx q[{control}], q[{target}];").unwrap()
                }
                QuantumOp::Reset { qubit } => writeln!(text, "reset q[{qubit}];").unwrap(),
                QuantumOp::MeasureZ { qubit, output } => {
                    writeln!(text, "m[{}] = measure q[{qubit}];", output.0).unwrap()
                }
                QuantumOp::XIf { qubit, condition } => {
                    writeln!(text, "if (m[{}] == 1) {{ x q[{qubit}]; }}", condition.0).unwrap()
                }
            },
            _ => unreachable!("quantum admission rejects foreign effects"),
        }
    }
    Ok(text)
}
