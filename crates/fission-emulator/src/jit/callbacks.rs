//! Host callouts emitted by the Cranelift JIT.
//!
//! These are the only bridge from compiled guest blocks into `Emulator` state.
//! There is no interpreter fallback path.

use crate::core::Emulator;
use crate::jit::float_ops::{float_binop, float_unop};
use crate::os::env::{HleResult, OsEnvironment};
use crate::pcode::page_map::page_align_down;

/// Max direct TB chain depth (soft chaining, QEMU-inspired).
pub const MAX_CHAIN_DEPTH: u32 = 32;

// ── Generic address-space I/O (≤8 bytes as u64) ─────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn jit_read_space(
    emu_ptr: *mut Emulator,
    space_id: u64,
    offset: u64,
    size: u64,
) -> u64 {
    let emu = unsafe { &mut *emu_ptr };
    let size = (size as usize).min(8);
    if size == 0 {
        return 0;
    }
    match emu.state.read_space(space_id, offset, size) {
        Ok(bytes) => {
            let mut val = 0u64;
            for (i, &b) in bytes.iter().enumerate() {
                val |= (b as u64) << (i * 8);
            }
            val
        }
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_write_space(
    emu_ptr: *mut Emulator,
    space_id: u64,
    offset: u64,
    size: u64,
    val: u64,
) {
    let emu = unsafe { &mut *emu_ptr };
    let size = (size as usize).min(8);
    if size == 0 {
        return;
    }
    let mut v = val;
    let mut bytes = Vec::with_capacity(size);
    for _ in 0..size {
        bytes.push((v & 0xFF) as u8);
        v >>= 8;
    }

    let smc_pages = if emu.state.spaces_layout.is_ram(space_id) {
        emu.state.page_map.exec_pages_in_range(offset, size)
    } else {
        Vec::new()
    };

    let _ = emu.state.write_space(space_id, offset, &bytes);

    for page in smc_pages {
        emu.jit_cache.invalidate_page(page_align_down(page));
    }
}

// ── Wide (>8B) bulk I/O via host buffer ──────────────────────────────────────
//
// Signature: jit_read_bytes(emu, space, offset, dst_ptr, size)
//            jit_write_bytes(emu, space, offset, src_ptr, size)
// Used for XMM/YMM and multi-chunk stores when size > 8.

#[unsafe(no_mangle)]
pub extern "C" fn jit_read_bytes(
    emu_ptr: *mut Emulator,
    space_id: u64,
    offset: u64,
    dst_ptr: *mut u8,
    size: u64,
) {
    if dst_ptr.is_null() || size == 0 {
        return;
    }
    let emu = unsafe { &mut *emu_ptr };
    let size = size as usize;
    let dst = unsafe { std::slice::from_raw_parts_mut(dst_ptr, size) };
    match emu.state.read_space(space_id, offset, size) {
        Ok(bytes) => {
            let n = bytes.len().min(size);
            dst[..n].copy_from_slice(&bytes[..n]);
            if n < size {
                dst[n..].fill(0);
            }
        }
        Err(_) => dst.fill(0),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_write_bytes(
    emu_ptr: *mut Emulator,
    space_id: u64,
    offset: u64,
    src_ptr: *const u8,
    size: u64,
) {
    if src_ptr.is_null() || size == 0 {
        return;
    }
    let emu = unsafe { &mut *emu_ptr };
    let size = size as usize;
    let src = unsafe { std::slice::from_raw_parts(src_ptr, size) };

    let smc_pages = if emu.state.spaces_layout.is_ram(space_id) {
        emu.state.page_map.exec_pages_in_range(offset, size)
    } else {
        Vec::new()
    };

    let _ = emu.state.write_space(space_id, offset, src);

    for page in smc_pages {
        emu.jit_cache.invalidate_page(page_align_down(page));
    }
}

// ── Float callouts ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn jit_float_binop(op: u32, size: u32, a: u64, b: u64) -> u64 {
    float_binop(op, size, a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_float_unop(op: u32, in_size: u32, out_size: u32, a: u64) -> u64 {
    float_unop(op, in_size, out_size, a)
}

/// Integer flag ops: kind 0=CARRY, 1=SCARRY, 2=SBORROW.
#[unsafe(no_mangle)]
pub extern "C" fn jit_int_flag(kind: u32, size: u32, a: u64, b: u64) -> u64 {
    crate::jit::float_ops::int_flag_op(kind, size, a, b)
}

// ── Instruction accounting + chaining ───────────────────────────────────────

/// Count one guest instruction inside a multi-instruction TB.
#[unsafe(no_mangle)]
pub extern "C" fn jit_count_insn(emu_ptr: *mut Emulator) {
    let emu = unsafe { &mut *emu_ptr };
    emu.inst_count = emu.inst_count.saturating_add(1);
    if let Some(m) = emu.max_inst {
        if emu.inst_count >= m && emu.metrics.exit_reason.is_none() {
            emu.metrics.exit_reason = Some("max_inst".into());
        }
    }
}

/// Count one p-code op (detects infinite relative CBRANCH loops inside a TB).
///
/// Returns 1 when the TB should exit early (guest insn budget or pcode fuse).
/// Does **not** set `halt_requested` for `max_inst` (process exit stays separate).
#[unsafe(no_mangle)]
pub extern "C" fn jit_count_pcode(emu_ptr: *mut Emulator) -> u64 {
    let emu = unsafe { &mut *emu_ptr };
    emu.pcode_ops = emu.pcode_ops.saturating_add(1);
    if emu.halt_requested {
        return 1;
    }
    if let Some(m) = emu.max_inst {
        if emu.inst_count >= m {
            if emu.metrics.exit_reason.is_none() {
                emu.metrics.exit_reason = Some("max_inst".into());
            }
            return 1;
        }
        // Tight fuse: pcode ops under a budgeted run (livelock protection).
        let cap = m.saturating_mul(2048).max(16_384);
        if emu.pcode_ops >= cap {
            if emu.metrics.exit_reason.is_none() {
                emu.metrics.exit_reason = Some("pcode_budget".into());
            }
            return 1;
        }
    }
    0
}

#[inline]
fn max_inst_reached(emu: &Emulator) -> bool {
    emu.halt_requested || emu.max_inst.is_some_and(|m| emu.inst_count >= m)
}

/// Soft direct chaining: if `next_pc` is already compiled, enter it (bounded depth).
/// Returns the final next PC once chaining stops (miss or depth limit).
#[unsafe(no_mangle)]
pub extern "C" fn jit_chain(emu_ptr: *mut Emulator, next_pc: u64) -> u64 {
    let emu = unsafe { &mut *emu_ptr };
    if max_inst_reached(emu) || emu.halt_requested || emu.sym_stop_requested {
        return next_pc;
    }
    if emu.chain_depth >= MAX_CHAIN_DEPTH {
        return next_pc;
    }
    // Magic HLE range: never chain into stubs.
    if next_pc >= 0xFFFFFFF0_00000000 {
        return next_pc;
    }
    let Some(block) = emu.jit_cache.lookup(next_pc) else {
        return next_pc;
    };
    emu.chain_depth += 1;
    let func: extern "C" fn(*mut Emulator) -> u64 =
        unsafe { std::mem::transmute(block.host_func_ptr) };
    let result = func(emu_ptr);
    emu.chain_depth -= 1;
    result
}

/// TB exit: hard-chain to `next_pc` if a host entry is published in the
/// global chain table (covers **fallthrough and absolute** branch/call targets).
/// Falls back to soft [`jit_chain`] / return of `next_pc`.
///
/// When TTD is recording, chaining is disabled so the outer run loop can take
/// snapshots at TB boundaries (Phase D: TTD over JIT segments).
#[unsafe(no_mangle)]
pub extern "C" fn jit_exit_tb(emu_ptr: *mut Emulator, next_pc: u64) -> u64 {
    let emu = unsafe { &mut *emu_ptr };

    if emu.halt_requested || max_inst_reached(emu) {
        return next_pc;
    }
    // Honor PC rewrite from syscalls like rt_sigreturn.
    let next_pc = if let Some(pc) = emu.pc_override.take() {
        pc
    } else {
        next_pc
    };
    if next_pc >= 0xFFFFFFF0_00000000 {
        return next_pc;
    }
    // Stop at symbolic branch gate (concolic).
    if emu.sym_stop_requested {
        return next_pc;
    }
    // TTD segment boundary: no hard/soft chain while recording.
    if emu.ttd_snapshot_interval > 0 && emu.ttd.is_recording() {
        return next_pc;
    }
    // Budgeted runs: return to the outer loop every TB so `max_inst` is checked.
    if emu.max_inst.is_some() {
        return next_pc;
    }
    if emu.chain_depth >= MAX_CHAIN_DEPTH {
        return next_pc;
    }

    // Hard chain: any published host for next_pc (absolute branch/call or fallthrough).
    if let Some(host) = emu.jit_cache.hard_chain_host(next_pc) {
        // Re-check budget before diving into another TB (tight loops).
        if max_inst_reached(emu) {
            return next_pc;
        }
        emu.metrics.hard_chains += 1;
        emu.chain_depth += 1;
        let func: extern "C" fn(*mut Emulator) -> u64 =
            unsafe { std::mem::transmute(host) };
        let result = func(emu_ptr);
        emu.chain_depth -= 1;
        return result;
    }

    // Soft chain (lookup Arc block — outer loop may compile on miss).
    let before = emu.jit_cache.lookup(next_pc).is_some();
    let r = jit_chain(emu_ptr, next_pc);
    if before {
        emu.metrics.soft_chains += 1;
    }
    r
}

// Legacy wrappers (register/memory fixed spaces — prefer jit_*_space).
#[unsafe(no_mangle)]
pub extern "C" fn jit_read_register(emu_ptr: *mut Emulator, offset: u64, size: u64) -> u64 {
    let emu = unsafe { &*emu_ptr };
    jit_read_space(emu_ptr, emu.state.register_space(), offset, size)
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_write_register(emu_ptr: *mut Emulator, offset: u64, size: u64, val: u64) {
    let emu = unsafe { &*emu_ptr };
    jit_write_space(emu_ptr, emu.state.register_space(), offset, size, val);
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_read_memory(emu_ptr: *mut Emulator, offset: u64, size: u64) -> u64 {
    let emu = unsafe { &*emu_ptr };
    jit_read_space(emu_ptr, emu.state.ram_space(), offset, size)
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_write_memory(emu_ptr: *mut Emulator, offset: u64, size: u64, val: u64) {
    let emu = unsafe { &*emu_ptr };
    jit_write_space(emu_ptr, emu.state.ram_space(), offset, size, val);
}

// ── Host register file (zero-callout loads) ──────────────────────────────────

/// Base pointer of the contiguous host register mirror (`MachineState::host_reg_file`).
#[unsafe(no_mangle)]
pub extern "C" fn jit_host_reg_base(emu_ptr: *mut Emulator) -> u64 {
    let emu = unsafe { &mut *emu_ptr };
    emu.state.host_reg_file_ptr() as u64
}

/// Bulk-sync dirty register slots from SSA values into guest register space.
///
/// `entries` is a packed array of `(offset:u64, size:u64, value:u64)` triples
/// (`count` triples). One callout replaces N `jit_write_space` calls at CallOther.
#[unsafe(no_mangle)]
pub extern "C" fn jit_reg_bulk_flush(
    emu_ptr: *mut Emulator,
    entries: *const u64,
    count: u64,
) {
    if entries.is_null() || count == 0 {
        return;
    }
    let emu = unsafe { &mut *emu_ptr };
    let n = count as usize;
    let slice = unsafe { std::slice::from_raw_parts(entries, n * 3) };
    let reg = emu.state.register_space();
    for i in 0..n {
        let off = slice[i * 3];
        let size = (slice[i * 3 + 1] as usize).min(8).max(1);
        let mut val = slice[i * 3 + 2];
        let mut bytes = Vec::with_capacity(size);
        for _ in 0..size {
            bytes.push((val & 0xff) as u8);
            val >>= 8;
        }
        let _ = emu.state.write_space(reg, off, &bytes);
    }
}

// ── Shadow / taint propagation (concolic) ────────────────────────────────────

/// Copy shadow from `src` varnode to `dst` (COPY). Clears dst if src is concrete.
#[unsafe(no_mangle)]
pub extern "C" fn jit_shadow_copy(
    emu_ptr: *mut Emulator,
    dst_sp: u64,
    dst_off: u64,
    dst_sz: u64,
    src_sp: u64,
    src_off: u64,
) {
    let emu = unsafe { &mut *emu_ptr };
    if dst_sp == 0 {
        return;
    }
    let node = if src_sp == 0 {
        None
    } else {
        emu.state.get_shadow_memory(src_sp, src_off)
    };
    let n = (dst_sz as usize).min(64) as u64;
    for i in 0..n {
        if let Some(id) = node {
            emu.state.set_shadow_memory(dst_sp, dst_off + i, id);
        } else {
            emu.state.clear_shadow_memory(dst_sp, dst_off + i);
        }
    }
}

/// After LOAD: if memory at `addr` is tainted, mark dest varnode.
#[unsafe(no_mangle)]
pub extern "C" fn jit_shadow_load(
    emu_ptr: *mut Emulator,
    dst_sp: u64,
    dst_off: u64,
    dst_sz: u64,
    mem_sp: u64,
    addr: u64,
) {
    let emu = unsafe { &mut *emu_ptr };
    if dst_sp == 0 {
        return;
    }
    let node = emu.state.get_shadow_memory(mem_sp, addr);
    let n = (dst_sz as usize).min(64) as u64;
    for i in 0..n {
        if let Some(id) = node {
            // Prefer per-byte shadow when present.
            let b = emu
                .state
                .get_shadow_memory(mem_sp, addr + i)
                .unwrap_or(id);
            emu.state.set_shadow_memory(dst_sp, dst_off + i, b);
        } else {
            emu.state.clear_shadow_memory(dst_sp, dst_off + i);
        }
    }
}

/// After STORE: if value varnode is tainted, taint memory; else clear.
#[unsafe(no_mangle)]
pub extern "C" fn jit_shadow_store(
    emu_ptr: *mut Emulator,
    mem_sp: u64,
    addr: u64,
    size: u64,
    val_sp: u64,
    val_off: u64,
) {
    let emu = unsafe { &mut *emu_ptr };
    let node = if val_sp == 0 {
        None
    } else {
        emu.state.get_shadow_memory(val_sp, val_off)
    };
    let n = (size as usize).min(64) as u64;
    for i in 0..n {
        if let Some(id) = node {
            emu.state.set_shadow_memory(mem_sp, addr + i, id);
        } else {
            emu.state.clear_shadow_memory(mem_sp, addr + i);
        }
    }
}

/// Binary ALU op kinds for full symbolic AST construction on the JIT path.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum SymBinOpKind {
    Add = 0,
    Sub = 1,
    Mul = 2,
    And = 3,
    Or = 4,
    Xor = 5,
    Eq = 6,
    Neq = 7,
    Ult = 8,
    Ule = 9,
    Slt = 10,
    Sle = 11,
    /// Float binops: mint a fresh symbolic Var (no IEEE theory in solver).
    FloatAdd = 20,
    FloatSub = 21,
    FloatMul = 22,
    FloatDiv = 23,
    FloatEq = 24,
    FloatNeq = 25,
    FloatLt = 26,
    FloatLe = 27,
}

/// Unary / float op kinds for symbolic AST on the JIT path.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum SymUnOpKind {
    /// Bitwise not (INT_NEGATE)
    Not = 0,
    /// Two's complement negate (INT_2COMP)
    Neg = 1,
    /// Boolean not (BOOL_NEGATE)
    BoolNot = 2,
    /// Float ops: no IEEE AST in solver yet — mint a fresh symbolic Var leaf.
    FloatNeg = 10,
    FloatAbs = 11,
    FloatSqrt = 12,
    FloatNan = 13,
    FloatCeil = 14,
    FloatFloor = 15,
    FloatRound = 16,
    FloatTrunc = 17,
    FloatInt2Float = 18,
    FloatFloat2Float = 19,
    FloatAdd = 20,
    FloatSub = 21,
    FloatMul = 22,
    FloatDiv = 23,
    FloatEq = 24,
    FloatNeq = 25,
    FloatLt = 26,
    FloatLe = 27,
}

/// Binary ALU: if either input is tainted, build a full [`SymExpr`] AST node
/// (not merely taint-id union) and attach it to the destination varnode.
///
/// Concrete values are supplied so untainted inputs become `Const` leaves.
#[unsafe(no_mangle)]
pub extern "C" fn jit_shadow_binop(
    emu_ptr: *mut Emulator,
    dst_sp: u64,
    dst_off: u64,
    dst_sz: u64,
    a_sp: u64,
    a_off: u64,
    a_val: u64,
    a_size: u64,
    b_sp: u64,
    b_off: u64,
    b_val: u64,
    b_size: u64,
    op_kind: u32,
) {
    let emu = unsafe { &mut *emu_ptr };
    if dst_sp == 0 {
        return;
    }
    let a_node = if a_sp == 0 {
        None
    } else {
        emu.state.get_shadow_memory(a_sp, a_off)
    };
    let b_node = if b_sp == 0 {
        None
    } else {
        emu.state.get_shadow_memory(b_sp, b_off)
    };
    if a_node.is_none() && b_node.is_none() {
        let n = (dst_sz as usize).min(64) as u64;
        for i in 0..n {
            emu.state.clear_shadow_memory(dst_sp, dst_off + i);
        }
        return;
    }

    use fission_solver::SymExpr;
    let a_sz = (a_size as u32).max(1).min(8);
    let b_sz = (b_size as u32).max(1).min(8);
    let a_expr = a_node
        .and_then(|id| emu.solver.nodes.get(&id).cloned())
        .unwrap_or_else(|| SymExpr::new_const(a_val, a_sz));
    let b_expr = b_node
        .and_then(|id| emu.solver.nodes.get(&id).cloned())
        .unwrap_or_else(|| SymExpr::new_const(b_val, b_sz));

    let new_expr = match op_kind {
        x if x == SymBinOpKind::Add as u32 => SymExpr::new_add(a_expr, b_expr),
        x if x == SymBinOpKind::Sub as u32 => SymExpr::new_sub(a_expr, b_expr),
        x if x == SymBinOpKind::Mul as u32 => SymExpr::Mul(Box::new(a_expr), Box::new(b_expr)),
        x if x == SymBinOpKind::And as u32 => SymExpr::new_and(a_expr, b_expr),
        x if x == SymBinOpKind::Or as u32 => SymExpr::Or(Box::new(a_expr), Box::new(b_expr)),
        x if x == SymBinOpKind::Xor as u32 => SymExpr::new_xor(a_expr, b_expr),
        x if x == SymBinOpKind::Eq as u32 => SymExpr::new_eq(a_expr, b_expr),
        x if x == SymBinOpKind::Neq as u32 => SymExpr::new_neq(a_expr, b_expr),
        x if x == SymBinOpKind::Ult as u32 => SymExpr::new_ult(a_expr, b_expr),
        x if x == SymBinOpKind::Ule as u32 => SymExpr::Ule(Box::new(a_expr), Box::new(b_expr)),
        x if x == SymBinOpKind::Slt as u32 => SymExpr::new_slt(a_expr, b_expr),
        x if x == SymBinOpKind::Sle as u32 => SymExpr::new_sle(a_expr, b_expr),
        x if x == SymBinOpKind::FloatAdd as u32 => SymExpr::new_fadd(a_expr, b_expr),
        x if x == SymBinOpKind::FloatSub as u32 => SymExpr::new_fsub(a_expr, b_expr),
        x if x == SymBinOpKind::FloatMul as u32 => SymExpr::new_fmul(a_expr, b_expr),
        x if x == SymBinOpKind::FloatDiv as u32 => SymExpr::new_fdiv(a_expr, b_expr),
        x if x == SymBinOpKind::FloatEq as u32 => SymExpr::new_feq(a_expr, b_expr),
        x if x == SymBinOpKind::FloatNeq as u32 => SymExpr::new_fneq(a_expr, b_expr),
        x if x == SymBinOpKind::FloatLt as u32 => SymExpr::new_flt(a_expr, b_expr),
        x if x == SymBinOpKind::FloatLe as u32 => SymExpr::new_fle(a_expr, b_expr),
        _ => {
            // Unknown op: fall back to first taint id (legacy union).
            let id = a_node.or(b_node).unwrap();
            let n = (dst_sz as usize).min(64) as u64;
            for i in 0..n {
                emu.state.set_shadow_memory(dst_sp, dst_off + i, id);
            }
            return;
        }
    };
    let new_id = emu.solver.register_node(new_expr);
    let n = (dst_sz as usize).min(64) as u64;
    for i in 0..n {
        emu.state.set_shadow_memory(dst_sp, dst_off + i, new_id);
    }
}

/// Unary int / float: build AST when the input is tainted.
#[unsafe(no_mangle)]
pub extern "C" fn jit_shadow_unop(
    emu_ptr: *mut Emulator,
    dst_sp: u64,
    dst_off: u64,
    dst_sz: u64,
    a_sp: u64,
    a_off: u64,
    a_val: u64,
    a_size: u64,
    op_kind: u32,
) {
    let emu = unsafe { &mut *emu_ptr };
    if dst_sp == 0 {
        return;
    }
    let a_node = if a_sp == 0 {
        None
    } else {
        emu.state.get_shadow_memory(a_sp, a_off)
    };
    if a_node.is_none() {
        let n = (dst_sz as usize).min(64) as u64;
        for i in 0..n {
            emu.state.clear_shadow_memory(dst_sp, dst_off + i);
        }
        return;
    }
    use fission_solver::SymExpr;
    let a_sz = (a_size as u32).max(1).min(8);
    let a_expr = a_node
        .and_then(|id| emu.solver.nodes.get(&id).cloned())
        .unwrap_or_else(|| SymExpr::new_const(a_val, a_sz));

    let new_expr = match op_kind {
        x if x == SymUnOpKind::Not as u32 => SymExpr::new_not(a_expr),
        x if x == SymUnOpKind::Neg as u32 => {
            SymExpr::new_sub(SymExpr::new_const(0, a_sz), a_expr)
        }
        x if x == SymUnOpKind::BoolNot as u32 => {
            SymExpr::new_eq(a_expr, SymExpr::new_const(0, 1))
        }
        x if x == SymUnOpKind::FloatNeg as u32 => SymExpr::new_fneg(a_expr),
        x if x == SymUnOpKind::FloatAbs as u32 => SymExpr::new_fabs(a_expr),
        x if x == SymUnOpKind::FloatSqrt as u32 => SymExpr::new_fsqrt(a_expr),
        x if x == SymUnOpKind::FloatNan as u32 => SymExpr::new_fisnan(a_expr),
        // Ceil/Floor/Round/Trunc/casts: keep as float-sorted symbolic leaf for now.
        x if (14..=19).contains(&x) => {
            let name = format!("fsym_{op_kind}_{}", a_node.unwrap_or(0));
            let out_sz = (dst_sz as u32).max(1).min(8);
            SymExpr::new_float_var(&name, out_sz)
        }
        _ => a_expr,
    };
    let new_id = emu.solver.register_node(new_expr);
    let n = (dst_sz as usize).min(64) as u64;
    for i in 0..n {
        emu.state.set_shadow_memory(dst_sp, dst_off + i, new_id);
    }
}

// ── Symbolic gate (concolic) ─────────────────────────────────────────────────

/// If the condition varnode is tainted (shadow live), record a [`SymBranch`]
/// and request a stop so `SimulationManager` can fork. Returns 1 when stopped.
///
/// Concrete-only conditions return 0 and leave control to normal CBranch IR.
#[unsafe(no_mangle)]
pub extern "C" fn jit_sym_cbranch_gate(
    emu_ptr: *mut Emulator,
    cond_val: u64,
    cond_space: u64,
    cond_offset: u64,
    taken_addr: u64,
    not_taken_addr: u64,
) -> u64 {
    let emu = unsafe { &mut *emu_ptr };
    if cond_space == 0 {
        return 0;
    }
    let Some(node) = emu.state.get_shadow_memory(cond_space, cond_offset) else {
        return 0;
    };
    let taken = cond_val != 0;
    emu.sym_events.push(crate::core::SymBranch {
        step_index: emu.inst_count,
        pc: emu.pc,
        condition_val_taken: taken,
        condition_node: Some(node),
        alt_rel_idx: None,
        alt_addr: Some(if taken { not_taken_addr } else { taken_addr }),
    });
    // Only stop the run when concolic exploration is enabled; otherwise keep
    // following the concrete path (tainted stdin in normal sandbox).
    if emu.concolic_stop_on_branch {
        emu.sym_stop_requested = true;
        tracing::debug!(
            "Symbolic CBranch gate STOP: node={} taken={} pc=0x{:X}",
            node,
            taken,
            emu.pc
        );
        1
    } else {
        tracing::debug!(
            "Symbolic CBranch gate record-only: node={} taken={} pc=0x{:X}",
            node,
            taken,
            emu.pc
        );
        0
    }
}

// ── CallOther / HLE ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn jit_call_other(
    emu_ptr: *mut Emulator,
    userop_id: u32,
    args_ptr: *const u64,
    argc: u64,
    output_size: u64,
) -> u64 {
    let emu = unsafe { &mut *emu_ptr };

    let userop_name = emu
        .userop_map
        .get(&userop_id)
        .cloned()
        .unwrap_or_else(|| format!("userop_{userop_id}"));

    tracing::debug!("JIT CallOther: {} (id={})", userop_name, userop_id);
    emu.metrics.note_userop(&userop_name);

    let input_vals: Vec<u64> = if argc > 0 && !args_ptr.is_null() {
        unsafe { std::slice::from_raw_parts(args_ptr, argc as usize) }.to_vec()
    } else {
        Vec::new()
    };

    // x86 SYSCALL/SYSENTER are often named "syscall" in .sla; fall back by id-less name.
    let is_syscall = userop_name == "syscall"
        || userop_name == "sysenter"
        || userop_name.eq_ignore_ascii_case("syscall")
        || userop_name.contains("syscall");

    emu.callother_result = 0;
    let result = if is_syscall {
        let os_ptr = &*emu.os as *const dyn OsEnvironment;
        let os_ref = unsafe { &*os_ptr };
        os_ref.dispatch_hle(emu, "syscall").unwrap_or_else(|e| {
            tracing::warn!("JIT syscall dispatch error: {:?}", e);
            HleResult::Continue
        })
    } else {
        let os_ptr = &*emu.os as *const dyn OsEnvironment;
        let os_ref = unsafe { &*os_ptr };
        match os_ref.dispatch_userop(emu, &userop_name, &input_vals, output_size as u32) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("JIT CallOther dispatch error: {:?}", e);
                HleResult::Continue
            }
        }
    };

    match result {
        HleResult::Halt(_) => {
            emu.halt_requested = true;
            1
        }
        HleResult::JumpTo(pc) => {
            emu.pc_override = Some(pc);
            0
        }
        HleResult::Continue => 0,
    }
}

/// Data result of the most recent CallOther / userop (e.g. `segment_fs` address).
#[unsafe(no_mangle)]
pub extern "C" fn jit_callother_result(emu_ptr: *mut Emulator) -> u64 {
    let emu = unsafe { &*emu_ptr };
    emu.callother_result
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_hle_trap(emu_ptr: *mut Emulator, magic_pc: u64) -> u64 {
    let emu = unsafe { &mut *emu_ptr };

    let func_name = {
        let opt = emu.os.resolve_stub(&emu.binary, magic_pc);
        opt.unwrap_or_else(|| format!("Unknown@0x{:X}", magic_pc))
    };

    tracing::debug!("JIT HLE trap: {} at 0x{:X}", func_name, magic_pc);

    let result = {
        let os_ptr = &*emu.os as *const dyn OsEnvironment;
        let os_ref = unsafe { &*os_ptr };
        os_ref.dispatch_hle(emu, &func_name).unwrap_or_else(|e| {
            tracing::warn!("JIT HLE dispatch error: {:?}", e);
            HleResult::Continue
        })
    };

    match result {
        HleResult::Halt(_) => {
            emu.halt_requested = true;
            1
        }
        HleResult::JumpTo(pc) => {
            emu.pc_override = Some(pc);
            0
        }
        HleResult::Continue => {
            let _ = emu.simulate_return();
            0
        }
    }
}
