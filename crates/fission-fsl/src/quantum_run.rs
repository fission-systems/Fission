//! Resource-bounded execution of cyclic canonical blocks. A stopped ensemble is
//! an incomplete instrument, never a normalized terminal output distribution.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuantumLimits {
    /// Global count of executed operations plus terminators, across all paths.
    pub steps: usize,
    /// Count of measurement/reset/noise splits along each individual path.
    pub splits: usize,
    /// Total retained completed and pending pure-state alternatives.
    pub branches: usize,
}
impl Default for QuantumLimits {
    fn default() -> Self {
        Self {
            steps: 65_536,
            splits: MAX_SPLITS,
            branches: 4096,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantumLimit {
    Steps,
    Splits,
    Branches,
}
#[derive(Clone, Debug, PartialEq)]
pub struct QuantumCursor {
    pub branch: QuantumBranch,
    pub block: u16,
    /// Next operation in the sole canonical operation table; end means terminator.
    pub operation: u16,
    pub splits: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct QuantumRun {
    completed: Vec<QuantumBranch>,
    pending: Vec<QuantumCursor>,
    steps: usize,
    limit: Option<QuantumLimit>,
    noisy: bool,
}
impl QuantumRun {
    pub fn completed(&self) -> &[QuantumBranch] {
        &self.completed
    }
    pub fn pending(&self) -> &[QuantumCursor] {
        &self.pending
    }
    pub fn steps(&self) -> usize {
        self.steps
    }
    pub fn limit(&self) -> Option<QuantumLimit> {
        self.limit
    }
    pub fn is_complete(&self) -> bool {
        self.pending.is_empty() && self.limit.is_none()
    }
    pub fn into_complete(self) -> Result<QuantumResult, FslError> {
        if !self.is_complete() {
            return Err(error(
                "incomplete quantum run cannot become a terminal result",
            ));
        }
        Ok(QuantumResult {
            branches: self.completed,
            noisy: self.noisy,
        })
    }
    pub fn diagnostic_json(&self) -> String {
        let complete = self.is_complete();
        let reason = match self.limit {
            None => "null",
            Some(QuantumLimit::Steps) => "\"steps\"",
            Some(QuantumLimit::Splits) => "\"splits\"",
            Some(QuantumLimit::Branches) => "\"branches\"",
        };
        let completed = self
            .completed
            .iter()
            .map(branch_json)
            .collect::<Vec<_>>()
            .join(",");
        let pending = self
            .pending
            .iter()
            .map(|c| {
                format!(
                    "{{\"block\":{},\"operation\":{},\"splits\":{},\"branch\":{}}}",
                    c.block,
                    c.operation,
                    c.splits,
                    branch_json(&c.branch)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let result = if complete {
            QuantumResult {
                branches: self.completed.clone(),
                noisy: self.noisy,
            }
            .diagnostic_json()
        } else {
            "null".into()
        };
        format!("{{\"status\":\"{}\",\"limit\":{reason},\"steps\":{},\"completed_probability\":{},\"frontier_probability\":{},\"completed\":[{completed}],\"frontier\":[{pending}],\"result\":{result}}}\n",if complete {"complete"}else{"incomplete"},self.steps,self.completed.iter().map(|b|b.probability).sum::<f64>(),self.pending.iter().map(|c|c.branch.probability).sum::<f64>())
    }
}

pub fn simulate_bounded(
    body: &CompiledInstruction,
    initial: &PureState,
    limits: QuantumLimits,
) -> Result<QuantumRun, FslError> {
    if limits.steps > 1_000_000
        || limits.splits > MAX_SPLITS
        || !(1..=4096).contains(&limits.branches)
    {
        return Err(error(
            "quantum bounds require steps <=1000000, splits <=12 and branches 1..4096",
        ));
    }
    QuantumContract::for_bounded_instruction(body)?.admit_qubits(initial.qubits)?;
    let blocks = crate::control::blocks(body);
    let mut run = QuantumRun {
        completed: vec![],
        pending: vec![QuantumCursor {
            branch: QuantumBranch {
                probability: 1.0,
                state: initial.clone(),
                values: vec![None; body.values.len()],
                measurements: vec![],
            },
            block: 0,
            operation: blocks[0].start,
            splits: 0,
        }],
        steps: 0,
        limit: None,
        noisy: body.ops.iter().any(|op| {
            matches!(
                op,
                FirOp::Quantum(QuantumOp::Noise { .. } | QuantumOp::Relaxation { .. })
            )
        }),
    };
    while let Some(mut cursor) = run.pending.pop() {
        let block = &blocks[usize::from(cursor.block)];
        let stop = if run.steps == limits.steps {
            Some(QuantumLimit::Steps)
        } else if cursor.operation < block.end {
            let op = body.ops[usize::from(cursor.operation)];
            let split_cost = match op {
                FirOp::Quantum(q) => q.split_cost(),
                _ => 0,
            };
            if cursor.splits + split_cost > limits.splits {
                Some(QuantumLimit::Splits)
            } else {
                // Keep the pre-effect state until exact branch-capacity admission.
                let children = apply_op(body, cursor.branch.clone(), op)?;
                if run.pending.len() + run.completed.len() + children.len() > limits.branches {
                    Some(QuantumLimit::Branches)
                } else {
                    run.steps += 1;
                    for child in children.into_iter().rev() {
                        run.pending.push(QuantumCursor {
                            branch: child,
                            operation: cursor.operation + 1,
                            splits: cursor.splits + split_cost,
                            ..cursor.clone()
                        });
                    }
                    continue;
                }
            }
        } else {
            run.steps += 1;
            let edge = match &block.terminator {
                FirTerminator::Return => {
                    run.completed.push(cursor.branch);
                    continue;
                }
                FirTerminator::Branch(edge) => edge,
                FirTerminator::CondBranch {
                    condition,
                    on_true,
                    on_false,
                } => {
                    if cursor.branch.values[usize::from(condition.0)]
                        .ok_or_else(|| error("undefined bounded branch condition"))?
                        == 1
                    {
                        on_true
                    } else {
                        on_false
                    }
                }
            };
            let args: Vec<_> = edge
                .arguments
                .iter()
                .map(|id| cursor.branch.values[usize::from(id.0)])
                .collect();
            for (&id, value) in blocks[usize::from(edge.target)].parameters.iter().zip(args) {
                cursor.branch.values[usize::from(id.0)] =
                    Some(value.ok_or_else(|| error("undefined bounded edge argument"))?);
            }
            cursor.block = edge.target;
            cursor.operation = blocks[usize::from(cursor.block)].start;
            run.pending.push(cursor);
            continue;
        };
        run.limit = stop;
        run.pending.push(cursor);
        break;
    }
    Ok(run)
}
