use anyhow::Result;
use crate::core::Emulator;
use crate::os::env::HleResult;
use crate::os::procedure::SimProcedure;

pub struct Malloc;
impl SimProcedure for Malloc {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let size = emu.read_arg(0).unwrap_or(0);
        tracing::info!("SimProcedure: malloc(0x{:X})", size);
        emu.write_return_val(0x50000000)?; // Dummy heap address for now
        Ok(HleResult::Continue)
    }
}

pub struct Free;
impl SimProcedure for Free {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let ptr = emu.read_arg(0).unwrap_or(0);
        tracing::info!("SimProcedure: free(0x{:X})", ptr);
        emu.write_return_val(0)?;
        Ok(HleResult::Continue)
    }
}

pub struct Puts;
impl SimProcedure for Puts {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let addr = emu.read_arg(0).unwrap_or(0);
        let s = read_string(emu, addr)?;
        tracing::info!("SimProcedure: puts(\"{}\")", s);
        emu.write_return_val(s.len() as u64 + 1)?;
        Ok(HleResult::Continue)
    }
}

pub struct Printf;
impl SimProcedure for Printf {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let addr = emu.read_arg(0).unwrap_or(0);
        let fmt = read_string(emu, addr)?;
        tracing::info!("SimProcedure: printf(\"{}\")", fmt.escape_debug());
        emu.write_return_val(fmt.len() as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct Read;
impl SimProcedure for Read {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_arg(0).unwrap_or(0);
        let buf = emu.read_arg(1).unwrap_or(0);
        let count = emu.read_arg(2).unwrap_or(0) as usize;

        if fd == 0 {
            let mut data = vec![0u8; count];
            let mut bytes_read = 0;
            if let Some(ref mut mock_buf) = emu.stdin_buffer {
                let to_read = std::cmp::min(count, mock_buf.len());
                data[..to_read].copy_from_slice(&mock_buf[..to_read]);
                mock_buf.drain(..to_read);
                bytes_read = to_read;
            } else {
                use std::io::Read as IoRead;
                if let Ok(n) = std::io::stdin().read(&mut data) {
                    bytes_read = n;
                }
            }
            if bytes_read > 0 {
                emu.state.write_space(emu.state.ram_space(), buf, &data[..bytes_read])?;
            }
            emu.write_return_val(bytes_read as u64)?;
        } else {
            tracing::info!("SimProcedure: read({}, 0x{:X}, {})", fd, buf, count);
            emu.write_return_val(0)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct Write;
impl SimProcedure for Write {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_arg(0).unwrap_or(0);
        let buf = emu.read_arg(1).unwrap_or(0);
        let count = emu.read_arg(2).unwrap_or(0);

        if fd == 1 || fd == 2 {
            let data = emu.state.read_space(emu.state.ram_space(), buf, count as usize).unwrap_or_default();
            print!("{}", String::from_utf8_lossy(&data));
        } else {
            tracing::info!("SimProcedure: write({}, 0x{:X}, {})", fd, buf, count);
        }
        emu.write_return_val(count)?;
        Ok(HleResult::Continue)
    }
}

pub struct Exit;
impl SimProcedure for Exit {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let code = emu.read_arg(0).unwrap_or(0) as u32;
        tracing::info!("SimProcedure: exit({}). Emulation finished.", code);
        Ok(HleResult::Halt(code))
    }
}

/// Helper to read a concrete C string from the emulator's RAM.
pub fn read_string(emu: &mut Emulator, addr: u64) -> Result<String> {
    let mut bytes = Vec::new();
    let mut cur = addr;
    loop {
        let b = emu.state.read_space(emu.state.ram_space(), cur, 1).unwrap_or(vec![0])[0];
        if b == 0 { break; }
        bytes.push(b);
        cur += 1;
        if bytes.len() > 4096 { break; }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
