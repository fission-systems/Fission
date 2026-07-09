use crate::os::windows::image_info::{self, PeImageInfo, PeProcessArgs};
use crate::pcode::state::MachineState;
use anyhow::Result;
use fission_loader::loader::LoadedBinary;

/// Maps PE image into guest RAM with page protections, PEB/TEB, stack, and heap.
pub fn load_pe(state: &mut MachineState, binary: &LoadedBinary) -> Result<PeImageInfo> {
    load_pe_with_args(state, binary, &PeProcessArgs::default())
}

/// Same as [`load_pe`] with explicit module path / command line.
pub fn load_pe_with_args(
    state: &mut MachineState,
    binary: &LoadedBinary,
    args: &PeProcessArgs,
) -> Result<PeImageInfo> {
    image_info::load_pe_image(state, binary, args)
}

pub use image_info::apply_stack_and_entry;
