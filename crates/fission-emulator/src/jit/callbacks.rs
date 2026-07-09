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
}

/// Soft direct chaining: if `next_pc` is already compiled, enter it (bounded depth).
/// Returns the final next PC once chaining stops (miss or depth limit).
#[unsafe(no_mangle)]
pub extern "C" fn jit_chain(emu_ptr: *mut Emulator, next_pc: u64) -> u64 {
    let emu = unsafe { &mut *emu_ptr };
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
#[unsafe(no_mangle)]
pub extern "C" fn jit_exit_tb(emu_ptr: *mut Emulator, next_pc: u64) -> u64 {
    let emu = unsafe { &mut *emu_ptr };

    if emu.halt_requested {
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
    if emu.chain_depth >= MAX_CHAIN_DEPTH {
        return next_pc;
    }

    // Hard chain: any published host for next_pc (absolute branch/call or fallthrough).
    if let Some(host) = emu.jit_cache.hard_chain_host(next_pc) {
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
        HleResult::Continue => 0,
    }
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
        HleResult::Continue => {
            let _ = emu.simulate_return();
            0
        }
    }
}
