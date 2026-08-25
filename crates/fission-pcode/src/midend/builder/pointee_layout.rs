//! What a pointer points at, read from where it is dereferenced.
//!
//! Every other type mechanism here pulls a type out of a *known signature* --
//! a callee parameter, a return type, a format specifier. That dictionary can
//! only name types someone shipped a declaration for, and a program's own
//! `struct Node` is not in it.
//!
//! p-code states the shape anyway. Each `LOAD`/`STORE` names its address, and
//! an address is a base plus a constant, so the set of offsets a value is
//! dereferenced at *is* the layout of the thing it points to. `list_sum`
//! walking `struct Node { int value; struct Node *next; }` dereferences one
//! value at offset 0 for four bytes and offset 8 for eight; that is the struct.
//!
//! This recovers *shape*, not *name*. It yields "four bytes at 0, eight at 8",
//! never `Node`. What it buys is that two pointers into the same shape stop
//! being printed as different types -- `list_sum` currently declares one of
//! them `fission_agg16 *` and the other `int *` for the same source `Node *`.
//!
//! **Deliberately abstract: one walk, no execution, no solver.** Cost is
//! constant per op against a median 556 ops per function, so roughly 0.2ms
//! against a 0.77s decompile. Questions this cannot answer from syntax --
//! whether two bases alias, whether a stride is an induction variable, whether
//! a self-reference makes the shape a list -- belong to DIR, which owns
//! anything needing execution or satisfiability. Answering them here is what
//! would make this a hundred times slower.

use super::*;
use fission_midend_core::ir::StructField;

/// A dereference of some base: how far in, and how wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Access {
    pub(super) offset: i64,
    pub(super) size: u32,
}

/// Accesses observed against each base value.
pub(super) type LayoutMap = HashMap<VarnodeKey, Vec<Access>>;

fn const_offset(vn: &Varnode) -> Option<i64> {
    if !vn.is_constant {
        return None;
    }
    // A negative displacement is stored as the unsigned two's complement of
    // the operand's own width. Sign-extending at 64 bits regardless turns a
    // four-byte `-8` into 18446744073709551608.
    let bits = u32::from(vn.size).saturating_mul(8).min(64);
    if bits == 0 || bits >= 64 {
        return Some(vn.constant_val);
    }
    let masked = (vn.constant_val as u64) & ((1u64 << bits) - 1);
    Some(if masked >= (1u64 << (bits - 1)) {
        (masked as i64) - (1i64 << bits)
    } else {
        masked as i64
    })
}

/// Observe every `(base, offset, width)` a function dereferences.
pub(super) fn collect(pcode: &PcodeFunction) -> LayoutMap {
    // value -> (base it was derived from, constant distance from that base)
    let mut derived: HashMap<VarnodeKey, (VarnodeKey, i64)> = HashMap::default();
    let mut out: LayoutMap = HashMap::default();

    for block in &pcode.blocks {
        for op in &block.ops {
            let out_key = op.output.as_ref().map(VarnodeKey::from);

            match op.opcode {
                PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
                    let pair = const_offset(&op.inputs[1])
                        .map(|k| (&op.inputs[0], k))
                        .or_else(|| const_offset(&op.inputs[0]).map(|k| (&op.inputs[1], k)));
                    if let (Some(out_key), Some((base_vn, delta))) = (out_key.clone(), pair) {
                        let base_key = VarnodeKey::from(base_vn);
                        let (root, at) = derived.get(&base_key).cloned().unwrap_or((base_key, 0));
                        derived.insert(out_key, (root, at.saturating_add(delta)));
                        continue;
                    }
                }
                PcodeOpcode::Copy if op.inputs.len() == 1 => {
                    if let Some(out_key) = out_key.clone() {
                        let src = VarnodeKey::from(&op.inputs[0]);
                        let entry = derived.get(&src).cloned().unwrap_or((src, 0));
                        derived.insert(out_key, entry);
                        continue;
                    }
                }
                // `LOAD out <- space, addr` and `STORE - <- space, addr, value`.
                PcodeOpcode::Load | PcodeOpcode::Store => {
                    let Some(addr) = op.inputs.get(1) else {
                        continue;
                    };
                    let width = match op.opcode {
                        PcodeOpcode::Load => op.output.as_ref().map(|v| v.size),
                        _ => op.inputs.get(2).map(|v| v.size),
                    };
                    let Some(size) = width else { continue };
                    let addr_key = VarnodeKey::from(addr);
                    let (root, offset) = derived.get(&addr_key).cloned().unwrap_or((addr_key, 0));
                    let slot = out.entry(root).or_default();
                    let access = Access { offset, size };
                    if !slot.contains(&access) {
                        slot.push(access);
                    }
                    continue;
                }
                _ => {}
            }

            // Any other definition invalidates what the value used to mean.
            if let Some(out_key) = out_key {
                derived.remove(&out_key);
            }
        }
    }
    out
}

/// The aggregate a set of accesses describes, if they describe one.
///
/// Two conditions, both about honesty rather than tuning. A single access is a
/// scalar dereference and says nothing about a surrounding struct. And a
/// negative offset means the base points into the middle of something, so the
/// extent measured from it is not the object's size.
pub(super) fn aggregate_for(accesses: &[Access]) -> Option<NirType> {
    if accesses.len() < 2 {
        return None;
    }
    if accesses.iter().any(|a| a.offset < 0 || a.size == 0) {
        return None;
    }
    let extent = accesses
        .iter()
        .map(|a| a.offset.saturating_add(i64::from(a.size)))
        .max()?;
    let size = u32::try_from(extent).ok()?;
    if size == 0 {
        return None;
    }
    let mut sorted = accesses.to_vec();
    sorted.sort();
    let fields = sorted
        .iter()
        .map(|a| StructField {
            offset: u32::try_from(a.offset).unwrap_or(0),
            ty: NirType::Int {
                bits: a.size.saturating_mul(8),
                signed: false,
            },
            name: format!("field_{:x}", a.offset),
        })
        .collect();
    Some(NirType::Aggregate { size, fields })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PcodeOp;

    fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }
    fn uniq(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }
    fn cst(value: i64, size: u32) -> Varnode {
        Varnode {
            space_id: 0,
            offset: 0,
            size,
            is_constant: true,
            constant_val: value,
        }
    }
    fn op(seq: u32, opcode: PcodeOpcode, out: Option<Varnode>, ins: Vec<Varnode>) -> PcodeOp {
        PcodeOp {
            seq_num: seq,
            opcode,
            address: 0x1000 + u64::from(seq),
            output: out,
            inputs: ins,
            asm_mnemonic: Some(format!("{opcode:?}").to_ascii_uppercase()),
        }
    }
    fn func(ops: Vec<PcodeOp>) -> PcodeFunction {
        PcodeFunction {
            blocks: vec![crate::pcode::PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: Vec::new(),
                ops,
            }],
        }
    }
    /// `LOAD out <- space, addr`; the space id is a constant operand.
    fn load(seq: u32, out: Varnode, addr: Varnode) -> PcodeOp {
        op(seq, PcodeOpcode::Load, Some(out), vec![cst(3, 4), addr])
    }

    fn sorted(map: &LayoutMap, base: &Varnode) -> Vec<Access> {
        let mut v = map
            .get(&VarnodeKey::from(base))
            .cloned()
            .unwrap_or_default();
        v.sort();
        v
    }

    /// `struct Node { int value; struct Node *next; }` walked by `list_sum`:
    /// four bytes at 0 and eight at 8. Taken from the shape
    /// `advanced_patterns_gcc_O0.exe` actually lifts to.
    #[test]
    fn a_pointer_walked_at_two_offsets_describes_its_struct() {
        let cur = reg(0x0, 8);
        let addr = uniq(0x8f00, 8);
        let map = collect(&func(vec![
            // cur->value : LOAD 4 bytes at offset 0
            load(0, uniq(0x23d00, 4), cur.clone()),
            // cur->next  : IntAdd cur,8 then LOAD 8 bytes
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(addr.clone()),
                vec![cur.clone(), cst(8, 8)],
            ),
            load(2, uniq(0x23e00, 8), addr),
        ]));
        assert_eq!(
            sorted(&map, &cur),
            vec![Access { offset: 0, size: 4 }, Access { offset: 8, size: 8 }]
        );
        assert_eq!(
            aggregate_for(&sorted(&map, &cur)),
            Some(NirType::Aggregate {
                size: 16,
                fields: vec![
                    StructField {
                        offset: 0,
                        ty: NirType::Int {
                            bits: 32,
                            signed: false
                        },
                        name: "field_0".to_string(),
                    },
                    StructField {
                        offset: 8,
                        ty: NirType::Int {
                            bits: 64,
                            signed: false
                        },
                        name: "field_8".to_string(),
                    },
                ],
            })
        );
    }

    /// The same source struct on a 32-bit target: the pointer field shrinks,
    /// and so must the recovered aggregate. `gcc-m32` lifts `Node` this way.
    #[test]
    fn the_same_struct_narrows_with_the_pointer_width() {
        let cur = reg(0x0, 4);
        let addr = uniq(0x17200, 4);
        let map = collect(&func(vec![
            load(0, uniq(0x100, 4), cur.clone()),
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(addr.clone()),
                vec![cur.clone(), cst(4, 4)],
            ),
            load(2, uniq(0x200, 4), addr),
        ]));
        let acc = sorted(&map, &cur);
        assert_eq!(
            acc,
            vec![Access { offset: 0, size: 4 }, Access { offset: 4, size: 4 }]
        );
        assert!(matches!(
            aggregate_for(&acc),
            Some(NirType::Aggregate { size: 8, .. })
        ));
    }

    /// One dereference is a scalar read, not evidence of a surrounding struct.
    #[test]
    fn a_single_access_is_not_an_aggregate() {
        let p = reg(0x8, 8);
        let map = collect(&func(vec![load(0, uniq(0x100, 4), p.clone())]));
        assert_eq!(sorted(&map, &p), vec![Access { offset: 0, size: 4 }]);
        assert_eq!(aggregate_for(&sorted(&map, &p)), None);
    }

    /// A negative offset means the base points into the middle of something,
    /// so the extent measured from it is not the object's size. The stack
    /// pointer is the case that matters: it is dereferenced above and below.
    #[test]
    fn a_base_reached_from_the_middle_yields_no_size() {
        let sp = reg(0x20, 8);
        let below = uniq(0x300, 8);
        let map = collect(&func(vec![
            op(
                0,
                PcodeOpcode::IntAdd,
                Some(below.clone()),
                vec![sp.clone(), cst(-8, 8)],
            ),
            load(1, uniq(0x100, 8), below),
            load(2, uniq(0x200, 8), sp.clone()),
        ]));
        let acc = sorted(&map, &sp);
        assert!(acc.contains(&Access {
            offset: -8,
            size: 8
        }));
        assert_eq!(aggregate_for(&acc), None);
    }

    /// A `Copy` carries the derivation, so a struct walked through a moved
    /// pointer is still one base. `list_sum` copies `head` into `cur`.
    #[test]
    fn a_copied_pointer_is_the_same_base() {
        let head = reg(0x28, 8);
        let cur = reg(0x0, 8);
        let addr = uniq(0x8f00, 8);
        let map = collect(&func(vec![
            op(0, PcodeOpcode::Copy, Some(cur.clone()), vec![head.clone()]),
            load(1, uniq(0x100, 4), cur.clone()),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(addr.clone()),
                vec![cur, cst(8, 8)],
            ),
            load(3, uniq(0x200, 8), addr),
        ]));
        assert_eq!(
            sorted(&map, &head),
            vec![Access { offset: 0, size: 4 }, Access { offset: 8, size: 8 }],
            "the copy must not split one struct across two bases"
        );
    }
}
