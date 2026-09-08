use super::*;
use std::collections::{BTreeMap, HashMap, VecDeque};

fn template_source_evidence_key(source: crate::compiler::CompiledTemplateSource) -> &'static str {
    match source {
        crate::compiler::CompiledTemplateSource::SpecDerived => "sla_construct_tpl",
    }
}

fn internal_byte_offset(entry_address: u64, bytes_len: usize, address: u64) -> Option<usize> {
    let rel = address.checked_sub(entry_address)?;
    let offset = usize_from_u64(rel)?;
    (offset < bytes_len).then_some(offset)
}

fn usize_from_u64(value: u64) -> Option<usize> {
    match usize::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => None,
    }
}

fn u64_from_usize(value: usize) -> Option<u64> {
    match u64::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => None,
    }
}

fn checked_shift_amount(value: u64) -> Option<u32> {
    match u32::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => None,
    }
}

fn checked_slice_end(offset: usize, width: usize, len: usize) -> Option<usize> {
    let end = offset.checked_add(width)?;
    (end <= len).then_some(end)
}

fn direct_pcode_branch_target(op: &PcodeOp) -> Option<u64> {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => {
            let target = op.inputs.first()?;
            if target.is_constant {
                if target.offset != 0 {
                    Some(target.offset)
                } else if target.constant_val >= 0 {
                    Some(target.constant_val as u64)
                } else {
                    None
                }
            } else if target.offset != 0 {
                Some(target.offset)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn relative_pcode_target_seq(op: &PcodeOp, vn: &Varnode) -> Option<u32> {
    if vn.space_id != 0 || !vn.is_constant {
        return None;
    }
    let raw = if vn.offset != 0 {
        vn.offset as u32
    } else {
        vn.constant_val as u32
    };
    let delta = i32::from_le_bytes(raw.to_le_bytes());
    if delta == 0 {
        return None;
    }
    if delta > 0 {
        op.seq_num.checked_add(delta as u32)
    } else {
        op.seq_num.checked_sub(delta.unsigned_abs())
    }
}

fn instruction_cbranch_exits_to_fallthrough(ops: &[PcodeOp], fallthrough: u64) -> bool {
    let Some(last) = ops.last() else {
        return false;
    };
    if !matches!(
        last.opcode,
        PcodeOpcode::Return | PcodeOpcode::BranchInd | PcodeOpcode::Branch
    ) {
        return false;
    }
    ops[..ops.len() - 1]
        .iter()
        .filter(|op| op.opcode == PcodeOpcode::CBranch)
        .any(|op| {
            if direct_pcode_branch_target(op) == Some(fallthrough) {
                return true;
            }
            let Some(target) = op.inputs.first() else {
                return false;
            };
            let Some(target_seq) = relative_pcode_target_seq(op, target) else {
                return false;
            };
            !ops.iter()
                .any(|candidate| candidate.address == op.address && candidate.seq_num == target_seq)
        })
}

fn enqueue_internal_target(
    queue: &mut VecDeque<u64>,
    entry_address: u64,
    bytes_len: usize,
    target: u64,
) {
    if internal_byte_offset(entry_address, bytes_len, target).is_some() && !queue.contains(&target)
    {
        queue.push_back(target);
    }
}

fn const_value(vn: &Varnode) -> Option<u64> {
    if !vn.is_constant {
        return None;
    }
    if vn.offset != 0 {
        return Some(vn.offset);
    }
    (vn.constant_val >= 0).then_some(vn.constant_val as u64)
}

fn clears_only_low_pointer_bit(vn: &Varnode) -> bool {
    let width_bits = vn.size.saturating_mul(8).min(64);
    let width_mask = if width_bits == 64 {
        u64::MAX
    } else {
        (1u64 << width_bits) - 1
    };
    const_value(vn).is_some_and(|value| (value & width_mask) | 1 == width_mask)
        || (vn.is_constant && vn.constant_val == -2)
}

fn collect_defs<'a>(
    decoded: &'a BTreeMap<u64, Vec<PcodeOp>>,
    current: &'a [PcodeOp],
) -> HashMap<Varnode, &'a PcodeOp> {
    let mut defs = HashMap::new();
    for op in decoded
        .values()
        .flat_map(|ops| ops.iter())
        .chain(current.iter())
    {
        if let Some(output) = &op.output {
            defs.insert(output.clone(), op);
        }
    }
    defs
}

fn eval_const_expr(vn: &Varnode, defs: &HashMap<Varnode, &PcodeOp>, depth: usize) -> Option<u64> {
    if depth > 12 {
        return None;
    }
    if let Some(value) = const_value(vn) {
        return Some(value);
    }
    let op = defs.get(vn)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            eval_const_expr(op.inputs.first()?, defs, depth + 1)
        }
        PcodeOpcode::IntAdd if op.inputs.len() == 2 => eval_const_expr(
            &op.inputs[0],
            defs,
            depth + 1,
        )?
        .checked_add(eval_const_expr(&op.inputs[1], defs, depth + 1)?),
        PcodeOpcode::IntSub if op.inputs.len() == 2 => eval_const_expr(
            &op.inputs[0],
            defs,
            depth + 1,
        )?
        .checked_sub(eval_const_expr(&op.inputs[1], defs, depth + 1)?),
        PcodeOpcode::IntLeft if op.inputs.len() == 2 => {
            let value = eval_const_expr(&op.inputs[0], defs, depth + 1)?;
            let shift = checked_shift_amount(eval_const_expr(&op.inputs[1], defs, depth + 1)?)?;
            value.checked_shl(shift)
        }
        PcodeOpcode::IntMult if op.inputs.len() == 2 => eval_const_expr(
            &op.inputs[0],
            defs,
            depth + 1,
        )?
        .checked_mul(eval_const_expr(&op.inputs[1], defs, depth + 1)?),
        PcodeOpcode::IntAnd if op.inputs.len() == 2 => {
            eval_const_expr(&op.inputs[0], defs, depth + 1)
                .zip(eval_const_expr(&op.inputs[1], defs, depth + 1))
                .map(|(lhs, rhs)| lhs & rhs)
        }
        _ => None,
    }
}

fn additive_const_component(
    vn: &Varnode,
    defs: &HashMap<Varnode, &PcodeOp>,
    depth: usize,
) -> Option<u64> {
    if depth > 12 {
        return None;
    }
    if let Some(value) = eval_const_expr(vn, defs, depth + 1) {
        return Some(value);
    }
    let op = defs.get(vn)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            additive_const_component(op.inputs.first()?, defs, depth + 1)
        }
        PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
            let lhs = additive_const_component(&op.inputs[0], defs, depth + 1);
            let rhs = additive_const_component(&op.inputs[1], defs, depth + 1);
            match (lhs, rhs) {
                (Some(lhs), Some(rhs)) => lhs.checked_add(rhs),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            }
        }
        PcodeOpcode::IntSub if op.inputs.len() == 2 => {
            let lhs = additive_const_component(&op.inputs[0], defs, depth + 1);
            let rhs = eval_const_expr(&op.inputs[1], defs, depth + 1);
            match (lhs, rhs) {
                (Some(lhs), Some(rhs)) => lhs.checked_sub(rhs),
                (Some(value), None) => Some(value),
                _ => None,
            }
        }
        _ => None,
    }
}

fn branchind_load_table_base(
    vn: &Varnode,
    defs: &HashMap<Varnode, &PcodeOp>,
    depth: usize,
) -> Option<(u64, usize)> {
    if depth > 12 {
        return None;
    }
    let op = defs.get(vn)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            branchind_load_table_base(op.inputs.first()?, defs, depth + 1)
        }
        PcodeOpcode::IntAnd if op.inputs.len() == 2 => {
            if clears_only_low_pointer_bit(&op.inputs[0]) {
                return branchind_load_table_base(&op.inputs[1], defs, depth + 1);
            }
            if clears_only_low_pointer_bit(&op.inputs[1]) {
                return branchind_load_table_base(&op.inputs[0], defs, depth + 1);
            }
            None
        }
        PcodeOpcode::Load if op.inputs.len() == 2 => {
            let table_base = additive_const_component(&op.inputs[1], defs, depth + 1)?;
            let width = op.output.as_ref().map_or(vn.size, |out| out.size);
            Some((table_base, width.clamp(4, 8) as usize))
        }
        _ => None,
    }
}

/// A jump table whose entries are *scaled offsets from a base*, rather than
/// addresses or plain displacements.
///
/// ARM Thumb-2's `tbb`/`tbh` build one: the table sits immediately after the
/// instruction, an entry is one or two bytes, and the target is
/// `base + 2 * entry` -- halving the entry lets a byte reach 512 bytes and a
/// halfword reach 128KB. `mspProcessInCommand` in betaflight dispatches 240
/// cases this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScaledOffsetTable {
    /// Where the entries live.
    table_base: u64,
    /// What an entry is an offset from.
    target_base: u64,
    /// Bytes per entry: 1 for `tbb`, 2 for `tbh`.
    entry_width: usize,
    /// What an entry is multiplied by: 2, so the target stays halfword-aligned.
    scale: u64,
}

/// Recognise `target = BASE + SCALE * load_W[TABLE + index * W]` inside one
/// instruction's ops.
///
/// Position-aware on purpose. `collect_defs` keeps the *last* definition of
/// each varnode, and `tbh` writes the same temporary three times in the one
/// instruction -- the table address, the loaded entry, the scaled offset --
/// so a last-write-wins lookup resolves the load's address operand to a
/// definition that comes after it. Every step here asks for the nearest
/// definition *before* the op doing the asking.
fn def_before<'a>(ops: &'a [PcodeOp], index: usize, vn: &Varnode) -> Option<(usize, &'a PcodeOp)> {
    ops[..index.min(ops.len())]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, op)| op.output.as_ref() == Some(vn))
}

fn branchind_scaled_offset_table(
    ops: &[PcodeOp],
    branch_target: &Varnode,
) -> Option<ScaledOffsetTable> {
    let (index, _) = def_before(ops, ops.len(), branch_target)?;
    scaled_offset_target_expr(ops, index, 0)
}

fn scaled_offset_target_expr(
    ops: &[PcodeOp],
    index: usize,
    depth: usize,
) -> Option<ScaledOffsetTable> {
    if depth > 12 {
        return None;
    }
    let op = ops.get(index)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            let (next, _) = def_before(ops, index, op.inputs.first()?)?;
            scaled_offset_target_expr(ops, next, depth + 1)
        }
        PcodeOpcode::IntAnd if op.inputs.len() == 2 => {
            // The Thumb bit is cleared on the way into the PC.
            for (mask_side, value_side) in [(0usize, 1usize), (1, 0)] {
                if clears_only_low_pointer_bit(&op.inputs[mask_side]) {
                    if let Some((next, _)) = def_before(ops, index, &op.inputs[value_side]) {
                        return scaled_offset_target_expr(ops, next, depth + 1);
                    }
                }
            }
            None
        }
        PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
            for (base_side, scaled_side) in [(0usize, 1usize), (1, 0)] {
                let Some(target_base) = const_value(&op.inputs[base_side]) else {
                    continue;
                };
                let Some((next, _)) = def_before(ops, index, &op.inputs[scaled_side]) else {
                    continue;
                };
                if let Some((table_base, entry_width, scale)) =
                    scaled_table_load(ops, next, depth + 1)
                {
                    return Some(ScaledOffsetTable {
                        table_base,
                        target_base,
                        entry_width,
                        scale,
                    });
                }
            }
            None
        }
        _ => None,
    }
}

/// The `SCALE * load_W[TABLE + index * W]` half, as `(table, width, scale)`.
fn scaled_table_load(ops: &[PcodeOp], index: usize, depth: usize) -> Option<(u64, usize, u64)> {
    if depth > 12 {
        return None;
    }
    let op = ops.get(index)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            let (next, _) = def_before(ops, index, op.inputs.first()?)?;
            scaled_table_load(ops, next, depth + 1)
        }
        PcodeOpcode::IntMult if op.inputs.len() == 2 => {
            for (scale_side, value_side) in [(0usize, 1usize), (1, 0)] {
                let Some(scale) = const_value(&op.inputs[scale_side]) else {
                    continue;
                };
                if scale == 0 || scale > 8 {
                    continue;
                }
                let Some((next, _)) = def_before(ops, index, &op.inputs[value_side]) else {
                    continue;
                };
                if let Some((table_base, width)) = table_entry_load(ops, next, depth + 1) {
                    return Some((table_base, width, scale));
                }
            }
            None
        }
        _ => None,
    }
}

/// A `load_W[TABLE + ...]`, as `(table, W)` -- the true load width, not
/// clamped: `tbb` reads a byte and `tbh` a halfword.
fn table_entry_load(ops: &[PcodeOp], index: usize, depth: usize) -> Option<(u64, usize)> {
    if depth > 12 {
        return None;
    }
    let op = ops.get(index)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            let (next, _) = def_before(ops, index, op.inputs.first()?)?;
            table_entry_load(ops, next, depth + 1)
        }
        PcodeOpcode::Load if op.inputs.len() == 2 => {
            let width = op.output.as_ref()?.size as usize;
            if !(width == 1 || width == 2 || width == 4) {
                return None;
            }
            let (addr_index, _) = def_before(ops, index, &op.inputs[1])?;
            let table_base = additive_const_before(ops, addr_index, depth + 1)?;
            Some((table_base, width))
        }
        _ => None,
    }
}

/// The constant part of an address computed by the ops before `index`.
fn additive_const_before(ops: &[PcodeOp], index: usize, depth: usize) -> Option<u64> {
    if depth > 12 {
        return None;
    }
    let op = ops.get(index)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            let (next, _) = def_before(ops, index, op.inputs.first()?)?;
            additive_const_before(ops, next, depth + 1)
        }
        PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
            for side in [0usize, 1] {
                if let Some(value) = const_value(&op.inputs[side]) {
                    return Some(value);
                }
            }
            for side in [0usize, 1] {
                if let Some((next, _)) = def_before(ops, index, &op.inputs[side]) {
                    if let Some(value) = additive_const_before(ops, next, depth + 1) {
                        return Some(value);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Walk a scaled-offset table, stopping where it stops making sense.
///
/// There is no count in the instruction -- the bound lives in a compare the
/// compiler emitted earlier -- so the table is read until an entry points
/// outside the decode window or back into the table itself, the same rules
/// the address-table walker uses.
fn scaled_offset_table_targets(
    table: ScaledOffsetTable,
    entry_address: u64,
    bytes: &[u8],
    little_endian: bool,
    max_cases: u64,
) -> Vec<u64> {
    let mut targets = Vec::new();
    let width = table.entry_width as u64;
    for ordinal in 0..max_cases {
        let Some(entry_addr) = ordinal
            .checked_mul(width)
            .and_then(|delta| table.table_base.checked_add(delta))
        else {
            break;
        };
        let Some(offset) = internal_byte_offset(entry_address, bytes.len(), entry_addr) else {
            break;
        };
        let Some(end) = checked_slice_end(offset, table.entry_width, bytes.len()) else {
            break;
        };
        let raw = &bytes[offset..end];
        let entry = match (table.entry_width, little_endian) {
            (1, _) => u64::from(raw[0]),
            (2, true) => u64::from(u16::from_le_bytes([raw[0], raw[1]])),
            (2, false) => u64::from(u16::from_be_bytes([raw[0], raw[1]])),
            (4, true) => u64::from(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])),
            (4, false) => u64::from(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]])),
            _ => break,
        };
        let Some(target) = entry
            .checked_mul(table.scale)
            .and_then(|delta| table.target_base.checked_add(delta))
        else {
            break;
        };
        if internal_byte_offset(entry_address, bytes.len(), target).is_none() {
            break;
        }
        // An entry that lands inside the table is the table running out: the
        // bytes after it are code, and reading them as entries walks off.
        let Some(scan_end) = entry_addr.checked_add(width) else {
            break;
        };
        if (table.table_base..scan_end).contains(&target) {
            break;
        }
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

fn read_unsigned_entry(bytes: &[u8], little_endian: bool) -> Option<u64> {
    match bytes.len() {
        4 => {
            let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
            Some(if little_endian {
                u32::from_le_bytes(raw) as u64
            } else {
                u32::from_be_bytes(raw) as u64
            })
        }
        8 => {
            let raw = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            Some(if little_endian {
                u64::from_le_bytes(raw)
            } else {
                u64::from_be_bytes(raw)
            })
        }
        _ => None,
    }
}

fn read_signed_entry(bytes: &[u8], little_endian: bool) -> Option<i128> {
    match bytes.len() {
        4 => {
            let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
            Some(i128::from(if little_endian {
                i32::from_le_bytes(raw)
            } else {
                i32::from_be_bytes(raw)
            }))
        }
        8 => {
            let raw = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            Some(i128::from(if little_endian {
                i64::from_le_bytes(raw)
            } else {
                i64::from_be_bytes(raw)
            }))
        }
        _ => None,
    }
}

fn add_signed_base(base: u64, displacement: i128) -> Option<u64> {
    let target = i128::from(base) + displacement;
    (0..=i128::from(u64::MAX))
        .contains(&target)
        .then_some(target as u64)
}

/// Bytes at `addr`, from the decoded function's own window or, failing that,
/// from a read-only image window.
///
/// The table a `BranchInd` reads is not necessarily inside the function that
/// reads it -- see `DecodeMemoryContext::readonly_windows`. Targets are still
/// required to land inside the function; only the table itself may be
/// elsewhere.
fn table_bytes_at<'a>(
    entry_address: u64,
    bytes: &'a [u8],
    memory_context: &'a DecodeMemoryContext,
    addr: u64,
    len: usize,
) -> Option<&'a [u8]> {
    if let Some(offset) = internal_byte_offset(entry_address, bytes.len(), addr) {
        let end = checked_slice_end(offset, len, bytes.len())?;
        return Some(&bytes[offset..end]);
    }
    for (base, window) in &memory_context.readonly_windows {
        if addr < *base {
            continue;
        }
        let Ok(offset) = usize::try_from(addr - *base) else {
            continue;
        };
        let Some(end) = offset.checked_add(len) else {
            continue;
        };
        if end <= window.len() {
            return Some(&window[offset..end]);
        }
    }
    None
}

fn infer_branchind_jump_table_targets(
    branch_target: &Varnode,
    decoded: &BTreeMap<u64, Vec<PcodeOp>>,
    current_ops: &[PcodeOp],
    entry_address: u64,
    bytes: &[u8],
    memory_context: &DecodeMemoryContext,
    little_endian: bool,
) -> Vec<u64> {
    const MAX_JUMP_TABLE_CASES: u64 = 256;

    let defs = collect_defs(decoded, current_ops);
    jump_table_targets_from(
        branch_target,
        &defs,
        entry_address,
        bytes,
        memory_context,
        little_endian,
        current_ops,
    )
}

/// Targets a `BranchInd` reads out of a jump table, given the definitions that
/// reach it.
///
/// Split out from [`infer_branchind_jump_table_targets`] so the same reader can
/// run again *after* a function is decoded, against block-local definitions --
/// see [`resolve_indirect_branch_targets`]. During the lift the only
/// definitions available are "the last write to this varnode anywhere decoded
/// so far", which at a dispatch is frequently some other block's write.
fn jump_table_targets_from(
    branch_target: &Varnode,
    defs: &HashMap<Varnode, &PcodeOp>,
    entry_address: u64,
    bytes: &[u8],
    memory_context: &DecodeMemoryContext,
    little_endian: bool,
    current_ops: &[PcodeOp],
) -> Vec<u64> {
    const MAX_JUMP_TABLE_CASES: u64 = 256;
    // A scaled-offset table first: its shape starts with an `IntAdd`, which
    // the address-table walker below does not descend, so ARM's `tbb`/`tbh`
    // produced no targets at all and the decode stopped at the dispatch --
    // 15 blocks of a 10KB function, and none of its 240 cases.
    if let Some(table) = branchind_scaled_offset_table(current_ops, branch_target) {
        let targets = scaled_offset_table_targets(
            table,
            entry_address,
            bytes,
            little_endian,
            MAX_JUMP_TABLE_CASES,
        );
        if targets.len() >= 2 {
            return targets;
        }
    }

    // Deliberately the single-candidate walk, and deliberately reading only
    // the function's own bytes.
    //
    // This runs *during* the lift, where the only definitions available are
    // "the last write to this varnode anywhere decoded so far" -- at a
    // dispatch that is frequently an unrelated block's write. Letting it
    // descend an `IntAdd` and read `.rodata` on top of that guessed
    // definition finds tables that are not there: `coreutils` `head` `main`
    // decoded 195 lines before, and afterwards failed outright with
    // "no match at 0x4c38", one of 14 functions lost that way on a corpus
    // sweep. The displacement-table shape those changes exist for is
    // recovered by `resolve_indirect_branch_targets` instead, which runs
    // after the decode and has real per-block definitions to work from.
    let Some((table_base, entry_width)) = branchind_load_table_base(branch_target, defs, 0) else {
        return Vec::new();
    };
    if entry_width != 4 && entry_width != 8 {
        return Vec::new();
    }
    if internal_byte_offset(entry_address, bytes.len(), table_base).is_none() {
        return Vec::new();
    }

    let mut mode_targets = Vec::<Vec<u64>>::new();
    let mut mode_bases = vec![None, Some(table_base)];
    for base in &memory_context.relative_address_bases {
        if !mode_bases.contains(&Some(*base)) {
            mode_bases.push(Some(*base));
        }
    }

    for base in mode_bases {
        let mut targets = Vec::new();
        let Some(entry_width_u64) = u64_from_usize(entry_width) else {
            break;
        };
        for ordinal in 0..MAX_JUMP_TABLE_CASES {
            let Some(entry_delta) = ordinal.checked_mul(entry_width_u64) else {
                break;
            };
            let Some(entry_addr) = table_base.checked_add(entry_delta) else {
                break;
            };
            let Some(offset) = internal_byte_offset(entry_address, bytes.len(), entry_addr) else {
                break;
            };
            let Some(end) = checked_slice_end(offset, entry_width, bytes.len()) else {
                break;
            };
            let raw = &bytes[offset..end];
            let target = if let Some(base) = base {
                read_signed_entry(raw, little_endian).and_then(|disp| add_signed_base(base, disp))
            } else {
                read_unsigned_entry(raw, little_endian)
            };
            let Some(target) = target else {
                break;
            };
            if internal_byte_offset(entry_address, bytes.len(), target).is_none() {
                break;
            }
            let Some(table_scan_end) = entry_addr.checked_add(entry_width_u64) else {
                break;
            };
            if (table_base..table_scan_end).contains(&target) {
                break;
            }
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        if targets.len() >= 2 {
            mode_targets.push(targets);
        }
    }

    mode_targets
        .into_iter()
        .max_by_key(|targets| targets.len())
        .unwrap_or_default()
}

/// Bytes at `addr`, from a read-only image window only.
///
/// A jump table is data, and the post-decode resolver requires it to live in a
/// section that is not executable. Accepting a table inside the function's own
/// bytes sounds harmless and is not: "two 4-byte values that happen to land
/// inside this function" is a test a 764-byte function's own instruction
/// stream passes by accident, and `bash`'s `unwind_frame_discard_internal`
/// did -- the resolver fed the decoder addresses in the middle of
/// instructions, which decoded to more garbage with more indirect branches,
/// and the fixed point never converged.
///
/// Architectures that really do put a table between instructions (ARM's
/// `tbb`/`tbh`) are matched during the lift instead, by shape rather than by
/// guess -- see `branchind_scaled_offset_table`.
fn readonly_bytes_at<'a>(
    memory_context: &'a DecodeMemoryContext,
    addr: u64,
    len: usize,
) -> Option<&'a [u8]> {
    for (base, window) in &memory_context.readonly_windows {
        if addr < *base {
            continue;
        }
        let Ok(offset) = usize::try_from(addr - *base) else {
            continue;
        };
        let Some(end) = offset.checked_add(len) else {
            continue;
        };
        if end <= window.len() {
            return Some(&window[offset..end]);
        }
    }
    None
}

/// What a varnode holds at a point in a block, as far as a jump table needs.
///
/// Two constants, not an expression tree. A tree is the obvious way to write
/// this and it is exponential: every read of a value clones it, so a block
/// with a long chain of adds doubles the term count per link. It survived
/// `BZ2_decompress`'s eight-deep dispatcher and made a 20-function batch of
/// `bash` take longer than two minutes where the whole binary used to take
/// eighty seconds.
///
/// A dispatch only ever needs two facts, and both are constants: which address
/// the table was read from, and what the loaded entry is a displacement from.
/// Carrying just those makes each p-code op O(1) and the whole scan linear.
#[derive(Debug, Clone, Copy, Default)]
struct Dispatch {
    /// Constants added into this value. `None` once anything unknown is mixed
    /// in -- an index, a register the block did not write.
    added_const: Option<u64>,
    /// If this value was loaded from memory, the constant in the address it
    /// was loaded from: the table's own address.
    table: Option<u64>,
    /// What an unknown value was multiplied by on the way into this one -- the
    /// stride of an index.
    index_scale: Option<u64>,
    /// The stride the table load was indexed by, carried past the load so the
    /// reader can insist it matches the entry width.
    table_scale: Option<u64>,
}

/// Symbolically evaluate a block forward and report what its `BranchInd` jumps
/// through, as `(table address, displacement base)`.
///
/// Forward evaluation rather than a definition map, because a definition map
/// cannot express this: SLEIGH's unique space reuses the same offset many
/// times inside one block, so a map keyed by `(space, offset)` makes an
/// instruction's temporary alias every other instruction's temporary -- on
/// `BZ2_decompress`'s dispatcher the branch target's "definition" was an
/// `IntAdd` whose own input resolved back to itself, and the walk spun until
/// its depth limit. Evaluating in order resolves each temporary at the moment
/// it is written, so the aliasing never arises.
fn dispatch_at_branch(ops: &[PcodeOp]) -> Option<Dispatch> {
    let mut env: HashMap<Varnode, Dispatch> = HashMap::new();
    let read = |env: &HashMap<Varnode, Dispatch>, vn: &Varnode| -> Dispatch {
        if let Some(value) = const_value(vn) {
            return Dispatch {
                added_const: Some(value),
                ..Dispatch::default()
            };
        }
        env.get(vn).copied().unwrap_or_default()
    };

    for op in ops {
        if op.opcode == PcodeOpcode::BranchInd {
            return op.inputs.first().map(|target| read(&env, target));
        }
        let Some(output) = &op.output else {
            continue;
        };
        let value = match op.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                op.inputs
                    .first()
                    .map(|input| read(&env, input))
                    .unwrap_or_default()
            }
            PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
                let lhs = read(&env, &op.inputs[0]);
                let rhs = read(&env, &op.inputs[1]);
                Dispatch {
                    added_const: match (lhs.added_const, rhs.added_const) {
                        (Some(a), Some(b)) => Some(a.wrapping_add(b)),
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        (None, None) => None,
                    },
                    table: lhs.table.or(rhs.table),
                    index_scale: lhs.index_scale.or(rhs.index_scale),
                    table_scale: lhs.table_scale.or(rhs.table_scale),
                }
            }
            // Scaling appears even when it scales by one: an `[base + index*1]`
            // address lifts to an `IntMult` by the constant 1, and treating
            // that as opaque hid the table's address behind an unknown -- which
            // is what it did on `BZ2_decompress`.
            PcodeOpcode::IntMult if op.inputs.len() == 2 => {
                let lhs = read(&env, &op.inputs[0]);
                let rhs = read(&env, &op.inputs[1]);
                match (lhs.added_const, rhs.added_const) {
                    (Some(a), Some(b)) => Dispatch {
                        added_const: Some(a.wrapping_mul(b)),
                        ..Dispatch::default()
                    },
                    // An unknown scaled by a constant is an index, and its
                    // stride is what tells a table read apart from a plain
                    // pointer load: `mov rax,[rip+X]; jmp rax` has a constant
                    // address and no index at all, and used to pass as a
                    // two-case table whenever `.rodata` at X happened to hold
                    // two values pointing into the function.
                    (Some(scale), None) | (None, Some(scale)) => Dispatch {
                        index_scale: Some(scale),
                        ..Dispatch::default()
                    },
                    (None, None) => Dispatch::default(),
                }
            }
            // `Load`'s first input is the space id, the second the address.
            PcodeOpcode::Load if op.inputs.len() == 2 => {
                let addr = read(&env, &op.inputs[1]);
                Dispatch {
                    table: addr.added_const,
                    table_scale: addr.index_scale,
                    ..Dispatch::default()
                }
            }
            _ => Dispatch::default(),
        };
        env.insert(output.clone(), value);
    }
    None
}

/// Indirect-branch targets recoverable from an already-decoded function.
///
/// This is the analysis half of the decode fixpoint. Recursive descent cannot
/// enumerate the successors of a computed jump, so a `switch` dispatch leaves
/// every case unreachable and undecoded -- on `bzip2`'s `BZ2_decompress` that
/// was 232 of ~6,000 instructions, and the emitted C had no control flow at
/// all. The decoder is not wrong about what it reaches; it simply cannot know
/// where to go. Only an analysis over decoded code can say, so the caller
/// feeds what this returns back into `additional_decode_entries` and lifts
/// again, to a fixed point.
///
/// Definitions are taken **per block**, which is what makes this answer
/// trustworthy where the in-lift reader is not: during the lift the only
/// available definition of a varnode is the last write to it anywhere decoded
/// so far, and at a dispatch that is routinely a write from an unrelated
/// block -- on `BZ2_decompress` it resolved the jump register to a stack slot
/// 376 bytes into the frame. A compiler-generated dispatch computes its target
/// inside one basic block, and nothing can branch into the middle of a block,
/// so the block's own writes are exactly the definitions that reach its
/// terminator.
pub fn resolve_indirect_branch_targets(
    function: &PcodeFunction,
    entry_address: u64,
    bytes: &[u8],
    memory_context: &DecodeMemoryContext,
    little_endian: bool,
) -> Vec<u64> {
    const MAX_JUMP_TABLE_CASES: u64 = 256;
    let mut found = Vec::new();

    for block in &function.blocks {
        let Some(value) = dispatch_at_branch(&block.ops) else {
            continue;
        };
        // The table's own address, from the load the dispatch reads it with.
        let Some(table_base) = value.table else {
            continue;
        };
        // What entries are displacements *from*: the constant the dispatch adds
        // back, when it adds one. GCC uses the table's own address. Absent
        // means the entries are absolute addresses.
        let displacement_base = value.added_const;

        // The stride the dispatch indexed by *is* the entry width. Trying
        // both widths and keeping whichever produced two plausible targets is
        // how a run of unrelated `.rodata` gets accepted as a table.
        let Some(entry_width) = value
            .table_scale
            .filter(|scale| *scale == 4 || *scale == 8)
            .and_then(|scale| usize::try_from(scale).ok())
        else {
            continue;
        };
        for entry_width in [entry_width] {
            let Some(entry_width_u64) = u64_from_usize(entry_width) else {
                continue;
            };
            let mut targets = Vec::new();
            for ordinal in 0..MAX_JUMP_TABLE_CASES {
                let Some(entry_addr) = ordinal
                    .checked_mul(entry_width_u64)
                    .and_then(|delta| table_base.checked_add(delta))
                else {
                    break;
                };
                let Some(raw) = readonly_bytes_at(memory_context, entry_addr, entry_width) else {
                    break;
                };
                let target = match displacement_base {
                    Some(base) => read_signed_entry(raw, little_endian)
                        .and_then(|disp| add_signed_base(base, disp)),
                    None => read_unsigned_entry(raw, little_endian),
                };
                let Some(target) = target else {
                    break;
                };
                // A target outside the function is how a table ends: the bytes
                // after it are some other table, or not a table at all.
                if internal_byte_offset(entry_address, bytes.len(), target).is_none() {
                    break;
                }
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
            // One entry proves nothing -- any four readable bytes that happen
            // to point into the function would pass.
            if targets.len() >= 2 {
                for target in targets {
                    if !found.contains(&target) {
                        found.push(target);
                    }
                }
                break;
            }
        }
    }
    found
}

fn attach_inferred_indirect_edges(
    function: &mut PcodeFunction,
    inferred_edges: &BTreeMap<u64, Vec<u64>>,
) {
    if inferred_edges.is_empty() {
        return;
    }

    let block_start_to_index = function
        .blocks
        .iter()
        .map(|block| (block.start_address, block.index))
        .collect::<BTreeMap<_, _>>();
    let source_to_block_index = function
        .blocks
        .iter()
        .filter(|block| {
            block
                .ops
                .last()
                .is_some_and(|op| op.opcode == PcodeOpcode::BranchInd)
        })
        .flat_map(|block| block.ops.iter().map(move |op| (op.address, block.index)))
        .collect::<BTreeMap<_, _>>();

    for (source, targets) in inferred_edges {
        let Some(source_idx) = source_to_block_index.get(source).copied() else {
            continue;
        };
        let Some(block) = function
            .blocks
            .iter_mut()
            .find(|block| block.index == source_idx)
        else {
            continue;
        };
        for target in targets {
            let Some(target_idx) = block_start_to_index.get(target).copied() else {
                continue;
            };
            if !block.successors.contains(&target_idx) {
                block.successors.push(target_idx);
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(super) fn var(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: 1,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    pub(super) fn op(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    #[test]
    fn conditional_terminal_instruction_preserves_instruction_fallthrough() {
        let ops = vec![
            op(
                0,
                0x1000,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(2, 8), var(0x20, 1)],
            ),
            op(1, 0x1000, PcodeOpcode::Return, None, vec![var(0x10, 4)]),
        ];

        assert!(instruction_cbranch_exits_to_fallthrough(&ops, 0x1004));
    }

    #[test]
    fn conditional_terminal_instruction_keeps_internal_branch_local() {
        let ops = vec![
            op(
                0,
                0x1000,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(1, 8), var(0x20, 1)],
            ),
            op(1, 0x1000, PcodeOpcode::Return, None, vec![var(0x10, 4)]),
        ];

        assert!(!instruction_cbranch_exits_to_fallthrough(&ops, 0x1004));
    }

    #[test]
    fn conditional_terminal_instruction_handles_single_terminal_without_length_fallback() {
        let ops = vec![op(0, 0x1000, PcodeOpcode::Return, None, vec![var(0x10, 4)])];

        assert!(!instruction_cbranch_exits_to_fallthrough(&ops, 0x1004));
    }

    #[test]
    fn aarch64_madd_lift_preserves_addend_dataflow() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("AARCH64").expect("AARCH64 frontend");
        let bytes = [0x00, 0x20, 0x0a, 0x1b];
        let (ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x100034)
            .expect("decode madd");

        assert_eq!(len, 4);
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntMult),
            "MADD must multiply the first two operands"
        );
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd),
            "MADD must add the accumulator operand"
        );
    }

    #[test]
    fn aarch64_udiv_madd_function_lift_preserves_accumulator_path() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("AARCH64").expect("AARCH64 frontend");
        let bytes = [
            0xa8, 0x99, 0x99, 0x52, 0x49, 0x01, 0x80, 0x52, 0x2a, 0xa7, 0x80, 0x52, 0x88, 0x99,
            0xb9, 0x72, 0x08, 0x7c, 0xa8, 0x9b, 0x08, 0xfd, 0x63, 0xd3, 0x08, 0x81, 0x09, 0x1b,
            0xe9, 0xdd, 0x97, 0x52, 0xa9, 0xd5, 0xbb, 0x72, 0x09, 0x00, 0x09, 0x4a, 0x08, 0x05,
            0x00, 0x11, 0x28, 0x09, 0xc8, 0x1a, 0x09, 0x6c, 0x89, 0x13, 0x08, 0x20, 0x0a, 0x1b,
            0xea, 0x1d, 0x80, 0x52, 0xaa, 0x15, 0xa0, 0x72, 0x0a, 0x00, 0x0a, 0x4a, 0x29, 0x01,
            0x0a, 0x0b, 0x08, 0x01, 0x00, 0x4a, 0x00, 0x7d, 0x09, 0x1b, 0x08, 0x00, 0x00, 0x90,
            0x00, 0x01, 0x00, 0xb9, 0xc0, 0x03, 0x5f, 0xd6,
        ];
        let function = frontend
            .lift_raw_pcode_function(&bytes, 0x100000)
            .expect("lift function");

        assert!(function.blocks.iter().any(|block| {
            block
                .ops
                .iter()
                .any(|op| op.address == 0x100034 && op.opcode == PcodeOpcode::IntAdd)
        }));
    }

    #[test]
    fn template_source_evidence_key_names_sla_construct_tpl() {
        assert_eq!(
            template_source_evidence_key(crate::compiler::CompiledTemplateSource::SpecDerived),
            "sla_construct_tpl"
        );
    }

    #[test]
    fn internal_byte_offsets_use_checked_width_conversions() {
        assert_eq!(internal_byte_offset(0x1000, 4, 0x1002), Some(2));
        assert_eq!(internal_byte_offset(0x1000, 4, 0x1004), None);
        assert_eq!(internal_byte_offset(u64::MAX - 1, 4, u64::MAX), Some(1));
        assert_eq!(checked_slice_end(usize::MAX, 1, usize::MAX), None);
    }

    #[test]
    fn const_eval_rejects_oversized_left_shift_counts() {
        let output = var(0x10, 8);
        let left_shift = op(
            0,
            0x1000,
            PcodeOpcode::IntLeft,
            Some(output.clone()),
            vec![
                Varnode::constant(1, 8),
                Varnode::constant(i64::from(u32::MAX) + 1, 8),
            ],
        );
        let defs = HashMap::from([(output.clone(), &left_shift)]);

        assert_eq!(eval_const_expr(&output, &defs, 0), None);
    }
}

impl RuntimeSleighFrontend {
    pub fn lift_raw_pcode_function(
        &self,
        bytes: &[u8],
        entry_address: u64,
    ) -> Result<PcodeFunction> {
        Ok(self
            .lift_raw_pcode_function_with_contract(
                bytes,
                entry_address,
                DEFAULT_FUNCTION_INSTRUCTION_LIMIT,
            )?
            .function)
    }

    pub fn lift_raw_pcode_function_with_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        instruction_limit: usize,
    ) -> Result<DecodedPcodeFunction> {
        self.lift_raw_pcode_function_with_decode_contract(
            bytes,
            entry_address,
            DecodeContract::strict_function(instruction_limit),
        )
    }

    pub fn lift_raw_pcode_function_with_decode_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: DecodeContract,
    ) -> Result<DecodedPcodeFunction> {
        self.lift_raw_pcode_function_with_decode_contract_and_memory_context(
            bytes,
            entry_address,
            contract,
            &DecodeMemoryContext::default(),
        )
    }

    pub fn lift_raw_pcode_function_with_decode_contract_and_memory_context(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: DecodeContract,
        memory_context: &DecodeMemoryContext,
    ) -> Result<DecodedPcodeFunction> {
        self.lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            entry_address,
            contract,
            memory_context,
            None,
        )
    }

    pub fn lift_raw_pcode_function_with_context_and_memory_context(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: DecodeContract,
        memory_context: &DecodeMemoryContext,
        initial_context_override: Option<PackedContextOverride>,
    ) -> Result<DecodedPcodeFunction> {
        if bytes.is_empty() {
            bail!("No function bytes available at 0x{:x}", entry_address);
        }
        if contract.instruction_limit == 0 {
            bail!("instruction_limit must be > 0");
        }

        let mut decoded = BTreeMap::<u64, Vec<PcodeOp>>::new();
        let mut decoded_instruction_records = BTreeMap::<u64, DecodedInstruction>::new();
        let mut decoded_contexts = BTreeMap::<u64, PackedContextOverride>::new();
        let mut instruction_lengths = BTreeMap::<u64, u64>::new();
        let mut template_sources = BTreeMap::<u64, String>::new();
        let mut decode_counts = HashMap::<u64, usize>::new();
        let mut inferred_indirect_edges = BTreeMap::<u64, Vec<u64>>::new();
        let mut template_source_counts = BTreeMap::<String, usize>::new();
        let base_context_override = initial_context_override;
        let mut context_overrides = BTreeMap::<u64, PackedContextOverride>::new();
        if let Some(override_bits) = initial_context_override {
            context_overrides.insert(entry_address, override_bits);
        }
        let mut queue = VecDeque::from([entry_address]);
        queue.extend(memory_context.additional_decode_entries.iter().copied());
        let mut stop_reason = DecodeStopReason::InputExhausted;

        while let Some(current) = queue.pop_front() {
            let context_override = match (
                base_context_override,
                context_overrides.get(&current).copied(),
            ) {
                (Some(base), Some(pending)) => Some(base.merge_override(pending)),
                (Some(base), None) => Some(base),
                (None, Some(pending)) => Some(pending),
                (None, None) => None,
            };

            let current_override_val = context_override.unwrap_or_default();
            if decoded.contains_key(&current) {
                if decoded_contexts.get(&current).copied().unwrap_or_default()
                    == current_override_val
                {
                    continue;
                }
            }

            let count = decode_counts.entry(current).or_insert(0);
            if *count >= 16 {
                continue;
            }
            *count += 1;

            if !decoded.contains_key(&current) && decoded.len() >= contract.instruction_limit {
                stop_reason = DecodeStopReason::InstructionLimit;
                break;
            }

            let Some(offset) = internal_byte_offset(entry_address, bytes.len(), current) else {
                continue;
            };
            let remaining = &bytes[offset..];

            let (instruction, mut ins_ops, decoded_len, details) = self
                .decode_instruction_and_lift_with_context_override(
                    remaining,
                    current,
                    context_override,
                )
                .map_err(|err| anyhow!("decode failed at 0x{:x}: {:#}", current, err))?;

            if decoded_len == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }
            let step = usize::try_from(decoded_len)?;
            if step > remaining.len() {
                bail!(
                    "decoded length {} exceeds available bytes {} at 0x{:x}",
                    step,
                    remaining.len(),
                    current
                );
            }

            // Remove old template source from counts if re-decoding
            if let Some(old_source) = template_sources.remove(&current) {
                if let Some(c) = template_source_counts.get_mut(&old_source) {
                    *c = c.saturating_sub(1);
                }
            }
            if let Some(source) = details.template_source {
                let src_key = template_source_evidence_key(source).to_string();
                *template_source_counts.entry(src_key.clone()).or_insert(0) += 1;
                template_sources.insert(current, src_key);
            }

            decoded_contexts.insert(current, current_override_val);
            decoded_instruction_records.insert(current, instruction);
            instruction_lengths.insert(current, decoded_len);

            let terminal = ins_ops
                .last()
                .is_some_and(|op| contract.is_terminal_control_flow(op.opcode));
            let last_opcode = ins_ops.last().map(|op| op.opcode);
            let direct_target = ins_ops.last().and_then(direct_pcode_branch_target);
            let fallthrough = checked_instruction_fallthrough(current, decoded_len)?;

            let cbranch_exits_to_fallthrough =
                instruction_cbranch_exits_to_fallthrough(&ins_ops, fallthrough);
            let little_endian = !matches!(
                registry::runtime_variant_for_entry(&self.entry)
                    .ok()
                    .map(|variant| variant.endian),
                Some(RuntimeEndian::Big)
            );

            for (target_addr, word_index, mask, value) in &details.pending_context_commits {
                let entry = context_overrides.entry(*target_addr).or_default();
                let old_entry = *entry;
                entry.merge_commit_word(*word_index, *mask, *value)?;
                if *entry != old_entry {
                    if !queue.contains(target_addr) {
                        queue.push_back(*target_addr);
                    }
                }
            }

            match last_opcode {
                Some(PcodeOpcode::Branch) => {
                    if let Some(target) = direct_target {
                        enqueue_internal_target(&mut queue, entry_address, bytes.len(), target);
                    }
                    if cbranch_exits_to_fallthrough {
                        enqueue_internal_target(
                            &mut queue,
                            entry_address,
                            bytes.len(),
                            fallthrough,
                        );
                    }
                }
                Some(PcodeOpcode::CBranch) => {
                    if let Some(target) = direct_target {
                        enqueue_internal_target(&mut queue, entry_address, bytes.len(), target);
                    }
                    enqueue_internal_target(&mut queue, entry_address, bytes.len(), fallthrough);
                }
                Some(PcodeOpcode::Return) => {
                    if cbranch_exits_to_fallthrough {
                        enqueue_internal_target(
                            &mut queue,
                            entry_address,
                            bytes.len(),
                            fallthrough,
                        );
                    } else {
                        stop_reason = DecodeStopReason::TerminalControlFlow;
                    }
                }
                Some(PcodeOpcode::BranchInd) => {
                    if contract.stop_at_indirect_branch {
                        if cbranch_exits_to_fallthrough {
                            enqueue_internal_target(
                                &mut queue,
                                entry_address,
                                bytes.len(),
                                fallthrough,
                            );
                        } else {
                            stop_reason = DecodeStopReason::TerminalControlFlow;
                        }
                    } else if let Some(branch_target) =
                        ins_ops.last().and_then(|op| op.inputs.first())
                    {
                        let inferred_targets = infer_branchind_jump_table_targets(
                            branch_target,
                            &decoded,
                            &ins_ops,
                            entry_address,
                            bytes,
                            memory_context,
                            little_endian,
                        );
                        if !inferred_targets.is_empty() {
                            inferred_indirect_edges.insert(current, inferred_targets.clone());
                        }
                        for target in inferred_targets {
                            enqueue_internal_target(&mut queue, entry_address, bytes.len(), target);
                        }
                    }
                }
                _ if !terminal => {
                    enqueue_internal_target(&mut queue, entry_address, bytes.len(), fallthrough);
                }
                _ => {}
            }

            decoded.insert(current, ins_ops);
        }

        let mut reachable = BTreeSet::new();
        let mut reach_queue = VecDeque::from([entry_address]);
        reach_queue.extend(memory_context.additional_decode_entries.iter().copied());
        while let Some(addr) = reach_queue.pop_front() {
            if reachable.contains(&addr) {
                continue;
            }
            if let Some(ins_ops) = decoded.get(&addr) {
                reachable.insert(addr);
                let terminal = ins_ops
                    .last()
                    .is_some_and(|op| contract.is_terminal_control_flow(op.opcode));
                let last_opcode = ins_ops.last().map(|op| op.opcode);
                let direct_target = ins_ops.last().and_then(direct_pcode_branch_target);
                let decoded_len = *instruction_lengths
                    .get(&addr)
                    .ok_or_else(|| anyhow!("missing instruction length for 0x{:x}", addr))?;
                let fallthrough = checked_instruction_fallthrough(addr, decoded_len)?;
                let cbranch_exits_to_fallthrough =
                    instruction_cbranch_exits_to_fallthrough(ins_ops, fallthrough);

                match last_opcode {
                    Some(PcodeOpcode::Branch) => {
                        if let Some(target) = direct_target {
                            if internal_byte_offset(entry_address, bytes.len(), target).is_some() {
                                reach_queue.push_back(target);
                            }
                        }
                        if cbranch_exits_to_fallthrough
                            && internal_byte_offset(entry_address, bytes.len(), fallthrough)
                                .is_some()
                        {
                            reach_queue.push_back(fallthrough);
                        }
                    }
                    Some(PcodeOpcode::CBranch) => {
                        if let Some(target) = direct_target {
                            if internal_byte_offset(entry_address, bytes.len(), target).is_some() {
                                reach_queue.push_back(target);
                            }
                        }
                        if internal_byte_offset(entry_address, bytes.len(), fallthrough).is_some() {
                            reach_queue.push_back(fallthrough);
                        }
                    }
                    Some(PcodeOpcode::Return) => {
                        if cbranch_exits_to_fallthrough {
                            if internal_byte_offset(entry_address, bytes.len(), fallthrough)
                                .is_some()
                            {
                                reach_queue.push_back(fallthrough);
                            }
                        }
                    }
                    Some(PcodeOpcode::BranchInd) => {
                        if contract.stop_at_indirect_branch {
                            if cbranch_exits_to_fallthrough {
                                if internal_byte_offset(entry_address, bytes.len(), fallthrough)
                                    .is_some()
                                {
                                    reach_queue.push_back(fallthrough);
                                }
                            }
                        } else if let Some(targets) = inferred_indirect_edges.get(&addr) {
                            for &target in targets {
                                if internal_byte_offset(entry_address, bytes.len(), target)
                                    .is_some()
                                {
                                    reach_queue.push_back(target);
                                }
                            }
                        }
                    }
                    _ if !terminal => {
                        if internal_byte_offset(entry_address, bytes.len(), fallthrough).is_some() {
                            reach_queue.push_back(fallthrough);
                        }
                    }
                    _ => {}
                }
            }
        }

        decoded.retain(|addr, _| reachable.contains(addr));
        decoded_instruction_records.retain(|addr, _| reachable.contains(addr));
        inferred_indirect_edges.retain(|addr, _| reachable.contains(addr));
        template_sources.retain(|addr, _| reachable.contains(addr));
        instruction_lengths.retain(|addr, _| reachable.contains(addr));
        template_source_counts.clear();
        for src in template_sources.values() {
            *template_source_counts.entry(src.clone()).or_insert(0) += 1;
        }

        let mut reachable_instruction_addresses: Vec<u64> = reachable.iter().copied().collect();
        reachable_instruction_addresses.sort_unstable();
        let reachable_instruction_lengths = instruction_lengths.clone();
        let retained_inferred_indirect_edges = inferred_indirect_edges.clone();

        let instruction_count = decoded.len();
        let mut ops = Vec::new();
        let mut global_seq = 0u32;
        for mut ins_ops in decoded.into_values() {
            for op in &mut ins_ops {
                op.seq_num = global_seq;
                global_seq = global_seq
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("p-code seq_num overflowed"))?;
            }
            ops.extend(ins_ops);
        }

        if ops.is_empty() {
            bail!("failed to decode any instruction at 0x{:x}", entry_address);
        }

        let mut indirect_targets: BTreeSet<u64> = inferred_indirect_edges
            .values()
            .flatten()
            .copied()
            .collect();
        indirect_targets.extend(memory_context.jump_table_targets.iter().copied());
        let indirect_targets_for_snapshot = indirect_targets.clone();

        let cfg_hints = InstructionCfgHints::from_memory_context(memory_context);

        let mut function = PcodeFunction {
            blocks: build_cfg_blocks_with_hints(
                entry_address,
                &reachable_instruction_addresses,
                &reachable_instruction_lengths,
                ops,
                &indirect_targets,
                &retained_inferred_indirect_edges,
                &cfg_hints,
            ),
        };
        attach_inferred_indirect_edges(&mut function, &inferred_indirect_edges);
        function
            .validate()
            .map_err(|err| RuntimeSleighError::InvalidPcodeShape {
                language: self.entry.entry_id.clone(),
                reason: err.to_string(),
            })?;

        Ok(DecodedPcodeFunction {
            function,
            instructions: decoded_instruction_records.into_values().collect(),
            decoded_instructions: instruction_count,
            stop_reason,
            template_source_counts,
            reachable_instruction_addresses,
            instruction_lengths: reachable_instruction_lengths,
            inferred_indirect_edges: retained_inferred_indirect_edges,
            indirect_targets: indirect_targets_for_snapshot,
        })
    }
}

#[cfg(test)]
mod scaled_offset_table_tests {
    use super::tests::{op, var};
    use super::{branchind_scaled_offset_table, scaled_offset_table_targets, ScaledOffsetTable};
    use super::{PcodeOp, PcodeOpcode, Varnode};

    /// ARM Thumb-2 `tbh [pc, r5]` at 0x8028164, as SLEIGH lifts it.
    ///
    /// The table sits at 0x8028190 -- the instruction's address plus four --
    /// and the target is `base + 2 * halfword[base + 2 * index]`. Note that
    /// `t` is written three times: the table address, the loaded entry, then
    /// the scaled offset. A last-write-wins definition map resolves the
    /// load's own address operand to the write that comes *after* it, which
    /// is why this shape needs position-aware lookup.
    fn tbh_ops() -> Vec<PcodeOp> {
        let at = 0x8028164;
        let t = var(0x1b1b00, 4);
        let half = var(0x1b1f00, 2);
        let scaled = var(0x1b2300, 4);
        let pc = var(0x5c, 4);
        let base = Varnode::constant(0x8028190, 4);
        vec![
            op(
                0,
                at,
                PcodeOpcode::Copy,
                Some(t.clone()),
                vec![var(0x34, 4)],
            ),
            op(
                1,
                at,
                PcodeOpcode::IntMult,
                Some(var(0x1b1c00, 4)),
                vec![t.clone(), Varnode::constant(2, 4)],
            ),
            op(
                2,
                at,
                PcodeOpcode::IntAdd,
                Some(t.clone()),
                vec![base.clone(), var(0x1b1c00, 4)],
            ),
            op(
                3,
                at,
                PcodeOpcode::Load,
                Some(half.clone()),
                vec![Varnode::constant(3, 8), t.clone()],
            ),
            op(4, at, PcodeOpcode::IntZExt, Some(t.clone()), vec![half]),
            op(
                5,
                at,
                PcodeOpcode::IntMult,
                Some(scaled.clone()),
                vec![t, Varnode::constant(2, 4)],
            ),
            op(
                6,
                at,
                PcodeOpcode::IntAdd,
                Some(pc.clone()),
                vec![base, scaled],
            ),
            op(7, at, PcodeOpcode::BranchInd, None, vec![pc]),
        ]
    }

    #[test]
    fn a_tbh_dispatch_is_recognised_as_a_scaled_offset_table() {
        let ops = tbh_ops();
        let target = ops.last().expect("branchind").inputs[0].clone();
        assert_eq!(
            branchind_scaled_offset_table(&ops, &target),
            Some(ScaledOffsetTable {
                table_base: 0x8028190,
                target_base: 0x8028190,
                entry_width: 2,
                scale: 2,
            })
        );
    }

    #[test]
    fn entries_are_halved_offsets_from_the_table_base() {
        // Two entries, then a third that points back into the table itself --
        // which is the table ending and code beginning, not a case.
        let mut bytes = vec![0u8; 0x2000];
        let table_at = 0x8028190 - 0x8028164;
        let put = |bytes: &mut Vec<u8>, index: usize, value: u16| {
            bytes[table_at + index * 2..table_at + index * 2 + 2]
                .copy_from_slice(&value.to_le_bytes());
        };
        put(&mut bytes, 0, 0x10); // -> base + 0x20
        put(&mut bytes, 1, 0x18); // -> base + 0x30
        put(&mut bytes, 2, 0x02); // -> base + 4, inside the table
        let table = ScaledOffsetTable {
            table_base: 0x8028190,
            target_base: 0x8028190,
            entry_width: 2,
            scale: 2,
        };
        assert_eq!(
            scaled_offset_table_targets(table, 0x8028164, &bytes, true, 256),
            vec![0x80281b0, 0x80281c0]
        );
    }

    #[test]
    fn a_target_outside_the_decode_window_ends_the_walk() {
        // The window is what the caller can actually decode; an entry past it
        // is not a case this pass can hand back.
        let mut bytes = vec![0u8; 0x40];
        let table_at = 0x8028190 - 0x8028164;
        bytes[table_at..table_at + 2].copy_from_slice(&0x4000u16.to_le_bytes());
        let table = ScaledOffsetTable {
            table_base: 0x8028190,
            target_base: 0x8028190,
            entry_width: 2,
            scale: 2,
        };
        assert!(scaled_offset_table_targets(table, 0x8028164, &bytes, true, 256).is_empty());
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    fn vn(space_id: u64, offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn konst(value: u64) -> Varnode {
        Varnode {
            space_id: 0,
            offset: value,
            size: 8,
            is_constant: true,
            constant_val: value as i64,
        }
    }

    fn op(opcode: PcodeOpcode, output: Option<Varnode>, inputs: Vec<Varnode>) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode,
            address: 0,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    /// The x86-64 GCC `-O0` dispatch, as p-code:
    ///
    /// ```text
    /// lea    rdx,[rax*4]          ; index * 4
    /// lea    rax,[rip+TABLE]
    /// mov    eax,DWORD PTR [rdx+rax*1]
    /// cdqe
    /// lea    rdx,[rip+TABLE]
    /// add    rax,rdx
    /// jmp    rax
    /// ```
    fn gcc_displacement_dispatch(table: u64) -> Vec<PcodeOp> {
        let index = vn(2, 0x100, 8);
        let scaled = vn(4, 0x10, 8);
        let base = vn(4, 0x20, 8);
        let addr = vn(4, 0x30, 8);
        let entry = vn(4, 0x40, 8);
        let target = vn(4, 0x50, 8);
        vec![
            op(
                PcodeOpcode::IntMult,
                Some(scaled.clone()),
                vec![index, konst(4)],
            ),
            op(PcodeOpcode::Copy, Some(base.clone()), vec![konst(table)]),
            op(
                PcodeOpcode::IntAdd,
                Some(addr.clone()),
                vec![scaled, base.clone()],
            ),
            op(PcodeOpcode::Load, Some(entry.clone()), vec![konst(3), addr]),
            op(
                PcodeOpcode::IntSExt,
                Some(entry.clone()),
                vec![entry.clone()],
            ),
            op(PcodeOpcode::IntAdd, Some(target.clone()), vec![entry, base]),
            op(PcodeOpcode::BranchInd, None, vec![target]),
        ]
    }

    #[test]
    fn a_displacement_dispatch_names_its_table_and_its_base() {
        let value = dispatch_at_branch(&gcc_displacement_dispatch(0x1e494))
            .expect("the block ends in BranchInd");
        assert_eq!(value.table, Some(0x1e494), "table address");
        assert_eq!(value.added_const, Some(0x1e494), "displacement base");
        assert_eq!(
            value.table_scale,
            Some(4),
            "entry width, from the index stride"
        );
    }

    /// `mov rax,[rip+X]; jmp rax` -- a pointer, not a table. It has a constant
    /// address and no index, and accepting it as a two-case table is how
    /// `.rodata` that happens to hold two in-function values sent the decoder
    /// into the middle of an instruction.
    #[test]
    fn a_plain_pointer_load_is_not_a_table() {
        let slot = vn(4, 0x10, 8);
        let ops = vec![
            op(
                PcodeOpcode::Load,
                Some(slot.clone()),
                vec![konst(3), konst(0x20e90)],
            ),
            op(PcodeOpcode::BranchInd, None, vec![slot]),
        ];
        let value = dispatch_at_branch(&ops).expect("the block ends in BranchInd");
        assert_eq!(value.table, Some(0x20e90), "the address is still visible");
        assert_eq!(
            value.table_scale, None,
            "but with no index the reader must refuse it"
        );
    }

    /// A block with no computed jump has nothing to say.
    #[test]
    fn a_block_without_an_indirect_branch_reports_nothing() {
        let out = vn(4, 0x10, 8);
        let ops = vec![op(PcodeOpcode::Copy, Some(out), vec![konst(7)])];
        assert!(dispatch_at_branch(&ops).is_none());
    }

    /// Scaling by one is still scaling: `[base + index*1]` lifts to an
    /// `IntMult` by the constant 1, and treating that as opaque hid the table.
    #[test]
    fn a_stride_of_one_still_leaves_the_base_visible() {
        let index = vn(2, 0x100, 8);
        let scaled = vn(4, 0x10, 8);
        let base = vn(4, 0x20, 8);
        let addr = vn(4, 0x30, 8);
        let entry = vn(4, 0x40, 8);
        let ops = vec![
            op(PcodeOpcode::Copy, Some(base.clone()), vec![konst(0x1000)]),
            op(
                PcodeOpcode::IntMult,
                Some(scaled.clone()),
                vec![base, konst(1)],
            ),
            op(
                PcodeOpcode::IntMult,
                Some(addr.clone()),
                vec![index, konst(8)],
            ),
            op(
                PcodeOpcode::IntAdd,
                Some(addr.clone()),
                vec![addr.clone(), scaled],
            ),
            op(PcodeOpcode::Load, Some(entry.clone()), vec![konst(3), addr]),
            op(PcodeOpcode::BranchInd, None, vec![entry]),
        ];
        let value = dispatch_at_branch(&ops).expect("the block ends in BranchInd");
        assert_eq!(value.table, Some(0x1000));
        assert_eq!(value.table_scale, Some(8));
    }
}
