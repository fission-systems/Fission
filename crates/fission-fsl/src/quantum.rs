//! Logical quantum effects in the canonical FIR, with a bounded numerical
//! reference interpreter. Qubit indices name subsystems of ONE joint state.
//! Simulator vector copies are host bookkeeping, never a guest cloning operation.
use crate::{CompiledInstruction, FirOp, FirTerminator, FslError, IntegerSign, ValueId};
use std::fmt::Write;

pub use crate::quantum_qasm::{import_openqasm, ImportedQuantum};
#[path = "quantum_run.rs"]
mod bounded;
pub use bounded::{simulate_bounded, QuantumCursor, QuantumLimit, QuantumLimits, QuantumRun};

/// Reference-host bound; canonical packed indices address up to 256 subsystems.
pub const MAX_QUBITS: u8 = 8;
pub const MAX_SPLITS: usize = 12;
fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}

/// Explicit model parameters, not inferred device calibration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseChannel {
    BitFlip,
    PhaseFlip,
    AmplitudeDamping,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantumOp {
    /// Zero-temperature Markovian idle model. Integer times use picoseconds.
    Relaxation {
        qubit: u8,
        duration_ps: u64,
        t1_ps: u64,
        t2_ps: u64,
    },
    /// Rational p = numerator/denominator, with 0 <= p <= 1.
    Noise {
        qubit: u8,
        channel: NoiseChannel,
        numerator: u8,
        denominator: u8,
    },
    H {
        qubit: u8,
    },
    X {
        qubit: u8,
    },
    Z {
        qubit: u8,
    },
    Cx {
        control: u8,
        target: u8,
    },
    MeasureZ {
        qubit: u8,
        output: ValueId,
    },
    Reset {
        qubit: u8,
    },
    XIf {
        qubit: u8,
        condition: ValueId,
    },
}
impl QuantumOp {
    pub(crate) fn encode(self) -> (u8, u8, u16) {
        match self {
            Self::Relaxation { qubit, .. } => (10, qubit, 0),
            Self::Noise {
                qubit,
                channel,
                numerator,
                denominator,
            } => (
                match channel {
                    NoiseChannel::BitFlip => 7,
                    NoiseChannel::PhaseFlip => 8,
                    NoiseChannel::AmplitudeDamping => 9,
                },
                qubit,
                u16::from_be_bytes([numerator, denominator]),
            ),
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
            7..=9 => {
                let [numerator, denominator] = payload.to_be_bytes();
                Self::Noise {
                    qubit,
                    numerator,
                    denominator,
                    channel: match tag {
                        7 => NoiseChannel::BitFlip,
                        8 => NoiseChannel::PhaseFlip,
                        _ => NoiseChannel::AmplitudeDamping,
                    },
                }
            }
            _ => return Err(error("unknown or noncanonical quantum encoding")),
        };
        op.validate()?;
        Ok(op)
    }
    fn qubit(self) -> u8 {
        match self {
            Self::Relaxation { qubit, .. }
            | Self::Noise { qubit, .. }
            | Self::H { qubit }
            | Self::X { qubit }
            | Self::Z { qubit }
            | Self::Reset { qubit }
            | Self::MeasureZ { qubit, .. }
            | Self::XIf { qubit, .. } => qubit,
            Self::Cx { control, .. } => control,
        }
    }
    pub(crate) fn split_cost(self) -> usize {
        match self {
            Self::Relaxation { .. } => 2,
            Self::MeasureZ { .. } | Self::Reset { .. } | Self::Noise { .. } => 1,
            _ => 0,
        }
    }
    pub(crate) fn validate(self) -> Result<(), FslError> {
        if let Self::Relaxation { t1_ps, t2_ps, .. } = self {
            if t1_ps == 0 || t2_ps == 0 || u128::from(t2_ps) > 2 * u128::from(t1_ps) {
                return Err(error("relaxation requires T1>0, T2>0 and T2<=2*T1"));
            }
        }
        if let Self::Noise {
            numerator,
            denominator,
            ..
        } = self
        {
            if denominator == 0 || numerator > denominator {
                return Err(error("noise probability requires 0 <= numerator <= denominator <= 255 and nonzero denominator"));
            }
        }
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

/// Bounded acyclic canonical FIR with quantum effects and classical u1 values.
/// No allocator, cyclic execution, CPU/GPU effects or pulse timing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantumContract {
    pub minimum_qubits: u16,
    pub splits: usize,
}
impl QuantumContract {
    pub fn for_instruction(body: &CompiledInstruction) -> Result<Self, FslError> {
        Self::inspect(body, false)
    }
    pub fn for_bounded_instruction(body: &CompiledInstruction) -> Result<Self, FslError> {
        Self::inspect(body, true)
    }
    fn inspect(body: &CompiledInstruction, cyclic: bool) -> Result<Self, FslError> {
        body.validate()?;
        if body.ops.len() > 256 || body.values.len() > 64 {
            return Err(error(
                "quantum reference requires <=256 ops / <=64 classical values",
            ));
        }
        // Check all syntactic paths before touching state, including untaken cycles.
        let blocks = crate::control::blocks(body);
        let mut incoming = vec![0usize; blocks.len()];
        for block in &blocks {
            for edge in crate::control::edges(&block.terminator) {
                incoming[usize::from(edge.target)] += 1;
            }
        }
        let mut queue: std::collections::VecDeque<_> = incoming
            .iter()
            .enumerate()
            .filter_map(|(i, &n)| (n == 0).then_some(i))
            .collect();
        let mut visited = 0;
        while let Some(i) = queue.pop_front() {
            visited += 1;
            for edge in crate::control::edges(&blocks[i].terminator) {
                let n = &mut incoming[usize::from(edge.target)];
                *n -= 1;
                if *n == 0 {
                    queue.push_back(usize::from(edge.target));
                }
            }
        }
        if !cyclic && visited != blocks.len() {
            return Err(error(
                "quantum reference refuses cyclic CFG before execution",
            ));
        }
        for value in &body.values {
            require_bit(body, value.id)?;
        }
        let mut extent = 0;
        let mut splits = 0;
        for op in &body.ops {
            match *op {
                FirOp::Quantum(q) => {
                    extent = extent.max(u16::from(q.qubit()) + 1);
                    if let QuantumOp::Cx { target, .. } = q {
                        extent = extent.max(u16::from(target) + 1);
                    }
                    splits += q.split_cost();
                }
                FirOp::IntConstant { output, .. } => {
                    require_bit(body, output)?;
                }
                _ => return Err(error("quantum reference does not support this FIR effect")),
            }
        }
        if extent == 0 || (!cyclic && splits > MAX_SPLITS) {
            return Err(error(
                "quantum profile requires quantum effects and <=12 measurement/reset/noise operations",
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
    noisy: bool,
}
fn branch_json(branch: &QuantumBranch) -> String {
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
    let amplitudes = branch
        .state
        .amplitudes
        .iter()
        .map(|a| format!("[{},{}]", a.re, a.im))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"probability\":{},\"values\":[{values}],\"measurements\":[{measurements}],\"amplitudes\":[{amplitudes}]}}",branch.probability)
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
        if self.noisy {
            text = text.replace(
                "ideal-logical-f64-enumerated",
                "declared-noise-logical-f64-enumerated",
            );
        }
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
/// Apply the two Kraus alternatives locally to a JOINT state. Environment
/// outcomes are host decomposition details and never classical FIR definitions.
fn noise(branch: QuantumBranch, qubit: u8, channel: NoiseChannel, p: f64) -> Vec<QuantumBranch> {
    let mut out = Vec::new();
    for alternative in 0..2 {
        let mut child = branch.clone();
        let mask = 1usize << qubit;
        match channel {
            NoiseChannel::BitFlip | NoiseChannel::PhaseFlip => {
                let weight = if alternative == 0 { 1.0 - p } else { p };
                if weight == 0.0 {
                    continue;
                }
                child.probability *= weight;
                if alternative == 1 {
                    if channel == NoiseChannel::BitFlip {
                        x(&mut child.state, qubit, None);
                    } else {
                        for (i, a) in child.state.amplitudes.iter_mut().enumerate() {
                            if i & mask != 0 {
                                *a = a.scale(-1.0);
                            }
                        }
                    }
                }
            }
            NoiseChannel::AmplitudeDamping => {
                for i in 0..child.state.amplitudes.len() {
                    if i & mask == 0 {
                        if alternative == 1 {
                            child.state.amplitudes[i] =
                                branch.state.amplitudes[i | mask].scale(p.sqrt());
                        }
                    } else {
                        child.state.amplitudes[i] = if alternative == 0 {
                            branch.state.amplitudes[i].scale((1.0 - p).sqrt())
                        } else {
                            Complex::default()
                        };
                    }
                }
                let weight: f64 = child.state.amplitudes.iter().map(|a| a.norm()).sum();
                if weight == 0.0 {
                    continue;
                }
                child.probability *= weight;
                for a in &mut child.state.amplitudes {
                    *a = a.scale(1.0 / weight.sqrt());
                }
            }
        }
        out.push(child);
    }
    out
}
fn apply_op(mut branch: QuantumBranch, op: FirOp) -> Result<Vec<QuantumBranch>, FslError> {
    match op {
        FirOp::Quantum(QuantumOp::Relaxation {
            qubit,
            duration_ps,
            t1_ps,
            t2_ps,
        }) => {
            let amp = -(-(duration_ps as f64) / (t1_ps as f64)).exp_m1();
            let difference = 2 * u128::from(t1_ps) - u128::from(t2_ps);
            let exponent = ((duration_ps as f64) / (t2_ps as f64))
                * ((difference as f64) / (2.0 * t1_ps as f64));
            let phase = -(-exponent).exp_m1() / 2.0;
            return Ok(noise(branch, qubit, NoiseChannel::AmplitudeDamping, amp)
                .into_iter()
                .flat_map(|b| noise(b, qubit, NoiseChannel::PhaseFlip, phase))
                .collect());
        }
        FirOp::Quantum(QuantumOp::Noise {
            qubit,
            channel,
            numerator,
            denominator,
        }) => {
            return Ok(noise(
                branch,
                qubit,
                channel,
                f64::from(numerator) / f64::from(denominator),
            ))
        }
        FirOp::Quantum(QuantumOp::MeasureZ { qubit, output }) => {
            return Ok(split(branch, qubit, Some(output)))
        }
        FirOp::Quantum(QuantumOp::Reset { qubit }) => return Ok(split(branch, qubit, None)),
        _ => {}
    }

    match op {
        FirOp::IntConstant { output, value } => {
            branch.values[usize::from(output.0)] = Some(value as u8)
        }
        FirOp::Quantum(q) => match q {
            QuantumOp::X { qubit } => x(&mut branch.state, qubit, None),
            QuantumOp::Cx { control, target } => x(&mut branch.state, target, Some(control)),
            QuantumOp::XIf { qubit, condition } => {
                if branch.values[usize::from(condition.0)]
                    .ok_or_else(|| error("undefined quantum condition"))?
                    == 1
                {
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
    Ok(vec![branch])
}
/// Enumerate weighted measurement branches, preserving each joint post-state.
/// f64 reference evidence with explicit model channels; no calibrated hardware model.
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
    let blocks = crate::control::blocks(body);
    let mut pending = vec![(0usize, branches.remove(0))];
    let mut finished = Vec::new();
    while let Some((pc, branch)) = pending.pop() {
        branches = vec![branch];
        let block = &blocks[pc];
        for op in &body.ops[usize::from(block.start)..usize::from(block.end)] {
            let mut next = Vec::new();
            for branch in branches {
                next.extend(apply_op(branch, *op)?);
            }
            branches = next;
        }

        for mut branch in branches {
            let edge = match &block.terminator {
                FirTerminator::Return => {
                    finished.push(branch);
                    continue;
                }
                FirTerminator::Branch(edge) => edge,
                FirTerminator::CondBranch {
                    condition,
                    on_true,
                    on_false,
                } => {
                    if branch.values[usize::from(condition.0)]
                        .ok_or_else(|| error("undefined branch condition"))?
                        == 1
                    {
                        on_true
                    } else {
                        on_false
                    }
                }
            };
            // Snapshot all arguments before binding parameters (parallel transfer).
            let arguments: Vec<_> = edge
                .arguments
                .iter()
                .map(|id| branch.values[usize::from(id.0)])
                .collect();
            for (&param, value) in blocks[usize::from(edge.target)]
                .parameters
                .iter()
                .zip(arguments)
            {
                branch.values[usize::from(param.0)] =
                    Some(value.ok_or_else(|| error("undefined edge argument"))?);
            }
            pending.push((usize::from(edge.target), branch));
        }
    }
    Ok(QuantumResult {
        branches: finished,
        noisy: body.ops.iter().any(|op| {
            matches!(
                op,
                FirOp::Quantum(QuantumOp::Noise { .. } | QuantumOp::Relaxation { .. })
            )
        }),
    })
}

/// Logical OpenQASM 3.0 projection. No hardware mapping, calibration or timing.
pub fn emit_openqasm(body: &CompiledInstruction, qubits: u8) -> Result<String, FslError> {
    QuantumContract::for_instruction(body)?.admit_qubits(qubits)?;
    if !body.blocks.is_empty()
        || body.ops.iter().any(|op| {
            matches!(
                op,
                FirOp::Quantum(QuantumOp::Noise { .. } | QuantumOp::Relaxation { .. })
            )
        })
    {
        return Err(error(
            "OpenQASM projection does not support CFG or declared noise channels",
        ));
    }
    let mut next = 0;
    for op in &body.ops {
        let id = match *op {
            FirOp::IntConstant { output, .. }
            | FirOp::Quantum(QuantumOp::MeasureZ { output, .. }) => Some(output),
            _ => None,
        };
        if let Some(id) = id {
            if usize::from(id.0) != next {
                return Err(error(
                    "OpenQASM projection requires dense classical definition order",
                ));
            }
            next += 1;
        }
    }
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
                QuantumOp::Noise { .. } | QuantumOp::Relaxation { .. } => {
                    unreachable!("noise projection rejected")
                }
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
