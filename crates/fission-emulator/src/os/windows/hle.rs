use crate::pcode::state::MachineState;
use anyhow::{Result, bail};

/// Dispatches an HLE (High-Level Emulation) call for a given API name.
pub fn dispatch_api(state: &mut MachineState, api_name: &str) -> Result<()> {
    tracing::info!("HLE Intercept: {}", api_name);
    match api_name {
        "LoadLibraryA" => handle_load_library_a(state),
        "GetProcAddress" => handle_get_proc_address(state),
        "VirtualAlloc" => handle_virtual_alloc(state),
        "ExitProcess" => {
            tracing::info!("ExitProcess called. Emulation finished.");
            // In a real implementation we would signal the emulator loop to stop
            Ok(())
        }
        _ => {
            tracing::warn!("Unimplemented Win32 API: {}. Returning 0.", api_name);
            // set EAX/RAX to 0
            Ok(())
        }
    }
}

fn handle_load_library_a(_state: &mut MachineState) -> Result<()> {
    tracing::info!("Emulating LoadLibraryA");
    // Return a dummy handle
    Ok(())
}

fn handle_get_proc_address(_state: &mut MachineState) -> Result<()> {
    tracing::info!("Emulating GetProcAddress");
    // Return a dummy pointer
    Ok(())
}

fn handle_virtual_alloc(_state: &mut MachineState) -> Result<()> {
    tracing::info!("Emulating VirtualAlloc");
    Ok(())
}
