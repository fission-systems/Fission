//! PE process image metadata — symmetric to Linux [`crate::os::linux::image_info::ImageInfo`].
//!
//! Cleanroom user-mode PE load: section mapping, page protections, stack, PEB/TEB
//! placement, and entry/SP application. No vendor dependencies.

use crate::pcode::page_map::{page_align_up, prot};
use crate::pcode::state::MachineState;
use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use serde::{Deserialize, Serialize};

/// Process image facts after PE load (user-mode).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PeImageInfo {
    pub image_base: u64,
    pub entry: u64,
    pub start_code: u64,
    pub end_code: u64,
    pub start_data: u64,
    pub end_data: u64,
    /// High stack address (initial RSP) — points into a mapped stack region.
    pub start_stack: u64,
    /// Low end of stack mapping (exclusive of guard if any).
    pub stack_limit: u64,
    /// Conventional PEB linear address.
    pub peb_addr: u64,
    /// Conventional TEB linear address.
    pub teb_addr: u64,
    /// Guest path used for GetModuleFileName-style HLE.
    pub module_path: String,
    pub is_64bit: bool,
    /// Preferred heap base for VirtualAlloc / HeapAlloc HLE.
    pub heap_base: u64,
}

/// Options for PE process startup.
#[derive(Clone, Debug)]
pub struct PeProcessArgs {
    pub module_path: String,
    pub command_line: String,
}

impl Default for PeProcessArgs {
    fn default() -> Self {
        Self {
            module_path: "C:\\sandbox\\test.exe".into(),
            command_line: "test.exe".into(),
        }
    }
}

/// Map PE sections, PEB/TEB, stack and heap; return [`PeImageInfo`].
pub fn load_pe_image(
    state: &mut MachineState,
    binary: &LoadedBinary,
    args: &PeProcessArgs,
) -> Result<PeImageInfo> {
    let inner = binary.inner();
    let is_64bit = inner.is_64bit;
    let image_base = inner.image_base;
    let entry = inner.entry_point;

    tracing::info!(
        "PE image load: entry=0x{:X} base=0x{:X} 64bit={}",
        entry,
        image_base,
        is_64bit
    );

    let mut start_code = u64::MAX;
    let mut end_code = 0u64;
    let mut start_data = u64::MAX;
    let mut end_data = 0u64;
    let mut max_end = 0u64;

    for sec in &inner.sections {
        if sec.virtual_size == 0 {
            continue;
        }
        let va = sec.virtual_address;
        let vsz = sec.virtual_size;
        let end = va.saturating_add(vsz);

        let mut page_prot = prot::VALID | prot::READ;
        if sec.is_writable {
            page_prot |= prot::WRITE;
        }
        if sec.is_executable {
            page_prot |= prot::EXEC;
        }
        if !sec.is_readable && !sec.is_writable && !sec.is_executable {
            page_prot |= prot::READ;
        }

        let sec_data = binary.view_bytes(va, vsz as usize).unwrap_or(&[]);
        let mut buf = vec![0u8; vsz as usize];
        let n = sec_data.len().min(buf.len());
        buf[..n].copy_from_slice(&sec_data[..n]);
        state
            .write_space(state.ram_space(), va, &buf)
            .with_context(|| format!("map PE section {} at 0x{:X}", sec.name, va))?;
        state.page_map.map_region(va, vsz, page_prot, false);

        if sec.is_executable {
            start_code = start_code.min(va);
            end_code = end_code.max(end);
        } else {
            start_data = start_data.min(va);
            end_data = end_data.max(end);
        }
        max_end = max_end.max(end);

        tracing::debug!(
            "  PE section {} 0x{:X}..0x{:X} prot=0x{:02X}",
            sec.name,
            va,
            end,
            page_prot
        );
    }

    if start_code == u64::MAX {
        start_code = image_base;
    }
    if start_data == u64::MAX {
        start_data = end_code;
    }

    // Stack (high) + heap (low-ish, after image).
    let stack_size = 8 * 1024 * 1024u64;
    let (stack_top, teb_addr, peb_addr, heap_base) = if is_64bit {
        (
            0x0000_7FFF_FFFF_F000u64,
            0x0000_0000_7FFD_E000u64,
            0x0000_0000_7FFD_F000u64,
            page_align_up(max_end.max(image_base).saturating_add(0x10_0000)).max(0x2000_0000),
        )
    } else {
        (
            0xFFFF_E000u64,
            0x7FFD_E000u64,
            0x7FFD_F000u64,
            page_align_up(max_end.max(image_base).saturating_add(0x10_0000)).max(0x1000_0000),
        )
    };
    let stack_limit = stack_top.saturating_sub(stack_size);
    state
        .page_map
        .map_region(stack_limit, stack_size, prot::RW | prot::ANON, true);
    state
        .page_map
        .map_region(heap_base, 0x1000_0000, prot::RW | prot::ANON, true);
    // PEB/TEB pages
    state
        .page_map
        .map_region(teb_addr & !0xFFF, 0x2000, prot::RW | prot::ANON, true);

    // Write PEB/TEB fields (BeingDebugged = 0 for clean sandbox).
    if is_64bit {
        state.write_space(state.ram_space(), teb_addr + 0x60, &peb_addr.to_le_bytes())?;
        state.write_space(state.ram_space(), peb_addr + 0x2, &[0])?; // BeingDebugged
        state.write_space(
            state.ram_space(),
            peb_addr + 0x10,
            &image_base.to_le_bytes(),
        )?; // ImageBaseAddress
    } else {
        state.write_space(
            state.ram_space(),
            teb_addr + 0x30,
            &(peb_addr as u32).to_le_bytes(),
        )?;
        state.write_space(state.ram_space(), peb_addr + 0x2, &[0])?;
        state.write_space(
            state.ram_space(),
            peb_addr + 0x8,
            &(image_base as u32).to_le_bytes(),
        )?;
    }

    // Command-line / path blobs for HLE.
    let path_va = heap_base.saturating_add(0x10000);
    let mut path_bytes = args.module_path.as_bytes().to_vec();
    path_bytes.push(0);
    state.write_space(state.ram_space(), path_va, &path_bytes)?;

    // Initial RSP: top of stack minus shadow (x64) / alignment.
    let start_stack = if is_64bit {
        (stack_top - 0x28) & !0xFu64 // 32-byte shadow + align
    } else {
        (stack_top - 4) & !0xFu64
    };

    let info = PeImageInfo {
        image_base,
        entry,
        start_code,
        end_code,
        start_data,
        end_data,
        start_stack,
        stack_limit,
        peb_addr,
        teb_addr,
        module_path: args.module_path.clone(),
        is_64bit,
        heap_base,
    };

    tracing::info!(
        "PE image_info: entry=0x{:X} stack=0x{:X}..0x{:X} peb=0x{:X} teb=0x{:X} heap=0x{:X}",
        info.entry,
        info.stack_limit,
        info.start_stack,
        info.peb_addr,
        info.teb_addr,
        info.heap_base
    );

    Ok(info)
}

/// Apply PE image SP / PC onto the emulator.
pub fn apply_stack_and_entry(
    emu: &mut crate::core::Emulator,
    info: &PeImageInfo,
) -> Result<()> {
    emu.pc = info.entry;
    let sp_reg = emu.arch.sp_reg;
    emu.write_register_u64(sp_reg, info.start_stack)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::state::MachineState;

    #[test]
    fn pe_image_info_defaults() {
        let info = PeImageInfo {
            entry: 0x140001000,
            image_base: 0x140000000,
            start_stack: 0x7FFFFFFFE000,
            ..Default::default()
        };
        assert!(info.start_stack > 0);
        let _ = MachineState::new();
    }
}
