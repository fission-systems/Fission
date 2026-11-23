//! Backward dynamic slicing over a recorded trace, and the candidate
//! interventions it yields.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::{CandidateOrigin, Location};
use crate::machine::Program;
use crate::trace::{Access, ByteLoc, Trace};

/// The steps the sink depends on, and through which uses.
#[derive(Debug, Clone, Default)]
pub struct Slice {
    pub steps: BTreeSet<u64>,
    /// Steps that entered only as control parents.
    pub control_only: BTreeSet<u64>,
    /// The uses of each slice step (all of its uses: a step in the slice
    /// depends on everything it read).
    pub uses: BTreeMap<u64, Vec<Access>>,
    /// Input-stream offsets the slice reaches.
    pub input_offsets: BTreeSet<usize>,
    /// Syscall steps whose outputs the slice reaches.
    pub syscall_steps: BTreeSet<u64>,
}

/// Backward dynamic slice of `trace` from `criterion` just before
/// `from_step`. With `control`, dynamic control parents (from static
/// postdominators) are followed as well as data.
pub fn backward(trace: &Trace, from_step: u64, criterion: &[Access], control: bool) -> Slice {
    let mut slice = Slice::default();
    let mut live: BTreeSet<ByteLoc> = criterion.iter().flat_map(Access::bytes).collect();
    let mut pending: BTreeSet<u64> = BTreeSet::new();
    if control
        && let Some(sink) = trace.step(from_step)
        && let Some(parent) = sink.cd_parent
    {
        pending.insert(parent);
    }
    let end = from_step.min(trace.steps.len() as u64);
    for i in (0..end).rev() {
        let Some(step) = trace.step(i) else { continue };
        let hit: BTreeSet<ByteLoc> = step
            .defs
            .iter()
            .flat_map(Access::bytes)
            .filter(|b| live.contains(b))
            .collect();
        let by_control = pending.remove(&i);
        if hit.is_empty() && !by_control {
            continue;
        }
        slice.steps.insert(i);
        if hit.is_empty() {
            slice.control_only.insert(i);
        }
        for input in &step.input {
            if hit.contains(&ByteLoc::Mem(input.addr)) {
                slice.input_offsets.insert(input.offset);
            }
        }
        if !step.syscalls.is_empty() && !hit.is_empty() {
            slice.syscall_steps.insert(i);
        }
        for b in step.defs.iter().flat_map(Access::bytes) {
            live.remove(&b);
        }
        for u in &step.uses {
            live.extend(u.bytes());
        }
        slice.uses.insert(i, step.uses.clone());
        if control && let Some(parent) = step.cd_parent {
            pending.insert(parent);
        }
    }
    slice
}

/// A time-indexed location to intervene on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Candidate {
    pub step: u64,
    pub location: Location,
    pub origin: CandidateOrigin,
    pub input_offset: Option<usize>,
}

/// The input offset `addr` holds just before `step`, if its last writer was
/// the read that delivered it.
pub fn input_offset_at(trace: &Trace, addr: u64, step: u64) -> Option<usize> {
    for i in (0..step.min(trace.steps.len() as u64)).rev() {
        let s = trace.step(i)?;
        if s.defs
            .iter()
            .flat_map(Access::bytes)
            .any(|b| b == ByteLoc::Mem(addr))
        {
            return s.input.iter().find(|b| b.addr == addr).map(|b| b.offset);
        }
    }
    None
}

/// Name a register access, preferring an exact register and falling back
/// to the smallest containing one.
pub fn register_location(program: &Program, offset: u64, size: u32) -> Option<Location> {
    if let Some(name) = program.register_named(offset, size) {
        return Some(Location::Register {
            name: name.to_string(),
        });
    }
    program
        .register_containing(offset, size)
        .map(|(name, _, _)| Location::Register {
            name: name.to_string(),
        })
}

/// Candidates from a slice: every input byte it reaches (intervened on
/// right after its read), then every location a slice step read, nearest
/// the sink first. Program counter and stack pointer are left out: changing
/// them changes control or frame structure, not a value.
pub fn candidates(program: &Program, trace: &Trace, slice: &Slice) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut seen: BTreeSet<(u64, Location)> = BTreeSet::new();
    let excluded: BTreeSet<String> = [&program.pc_register, &program.sp_register]
        .iter()
        .map(|s| s.to_ascii_uppercase())
        .collect();

    // Input bytes, by offset.
    let mut inputs: Vec<(usize, u64, u64)> = Vec::new();
    for s in &trace.steps {
        for b in &s.input {
            if slice.input_offsets.contains(&b.offset) {
                inputs.push((b.offset, b.addr, s.step + 1));
            }
        }
    }
    inputs.sort_unstable();
    for (offset, addr, step) in inputs {
        let location = Location::Memory { addr, len: 1 };
        if seen.insert((step, location.clone())) {
            out.push(Candidate {
                step,
                location,
                origin: CandidateOrigin::InputByte,
                input_offset: Some(offset),
            });
        }
    }

    for (&step, uses) in slice.uses.iter().rev() {
        let origin = if slice.control_only.contains(&step) {
            CandidateOrigin::ControlDependence
        } else {
            CandidateOrigin::DynamicData
        };
        for u in uses {
            let location = match *u {
                Access::Reg { offset, size } => match register_location(program, offset, size) {
                    Some(l) => l,
                    None => continue,
                },
                Access::Mem { addr, len } => Location::Memory { addr, len },
            };
            if let Location::Register { name } = &location
                && excluded.contains(&name.to_ascii_uppercase())
            {
                continue;
            }
            let input_offset = match location {
                Location::Memory { addr, len: 1 } => input_offset_at(trace, addr, step),
                _ => None,
            };
            if seen.insert((step, location.clone())) {
                out.push(Candidate {
                    step,
                    location,
                    origin,
                    input_offset,
                });
            }
        }
    }
    out
}

/// Deterministic alternative values for a location holding `original`:
/// low-bit flip, zero (or one), top-bit flip, complement, +1, -1, then the
/// comparison constants in `pool`.
pub fn alternatives(original: &[u8], pool: &[u64], max: usize) -> Vec<Vec<u8>> {
    let width = original.len();
    if width == 0 {
        return Vec::new();
    }
    let narrow = width.min(8);
    let mut v = 0u64;
    for (i, b) in original.iter().take(narrow).enumerate() {
        v |= u64::from(*b) << (8 * i);
    }
    let mask = if narrow == 8 {
        u64::MAX
    } else {
        (1u64 << (8 * narrow)) - 1
    };
    let mut values = vec![
        v ^ 1,
        if v != 0 { 0 } else { 1 },
        v ^ 0x80,
        !v,
        v.wrapping_add(1),
        v.wrapping_sub(1),
    ];
    for c in pool {
        values.push(*c);
        values.push(c.wrapping_add(1));
    }
    let mut out: Vec<Vec<u8>> = Vec::new();
    for value in values {
        let value = value & mask;
        if value == v {
            continue;
        }
        let mut bytes = original.to_vec();
        for (i, b) in bytes.iter_mut().take(narrow).enumerate() {
            *b = (value >> (8 * i)) as u8;
        }
        if !out.contains(&bytes) {
            out.push(bytes);
        }
        if out.len() >= max {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::alternatives;

    #[test]
    fn alternatives_never_include_the_original_and_are_deterministic() {
        let a = alternatives(&[0x7f], &[0x7f, 0x41], 8);
        assert!(!a.contains(&vec![0x7f]));
        assert_eq!(a, alternatives(&[0x7f], &[0x7f, 0x41], 8));
        // Positive control: the first alternative is the low-bit flip.
        assert_eq!(a[0], vec![0x7e]);
        // A pool constant other than the original is offered.
        assert!(a.contains(&vec![0x41]));
    }

    #[test]
    fn alternatives_respect_width_and_budget() {
        let a = alternatives(&[1, 0, 0, 0], &[], 3);
        assert_eq!(a.len(), 3);
        assert!(a.iter().all(|v| v.len() == 4));
    }
}
