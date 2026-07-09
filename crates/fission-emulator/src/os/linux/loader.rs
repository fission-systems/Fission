use crate::os::linux::image_info::{self, ImageInfo, ProcessArgs};
use crate::pcode::state::MachineState;
use anyhow::Result;
use fission_loader::loader::LoadedBinary;

/// Maps ELF image into guest RAM, records page protections, sets brk, and
/// builds the initial process stack (argc/argv/envp/auxv).
///
/// Prefer this over raw section dumps when running user-mode Linux guests.
pub fn load_elf(state: &mut MachineState, binary: &LoadedBinary) -> Result<ImageInfo> {
    load_elf_with_args(state, binary, &ProcessArgs::default())
}

/// Same as [`load_elf`] with explicit argv/envp.
pub fn load_elf_with_args(
    state: &mut MachineState,
    binary: &LoadedBinary,
    args: &ProcessArgs,
) -> Result<ImageInfo> {
    image_info::load_elf_image(state, binary, args)
}

pub use image_info::apply_stack_pointer;
