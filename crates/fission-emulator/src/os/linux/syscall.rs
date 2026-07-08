use anyhow::Result;
use crate::core::Emulator;
use crate::os::env::HleResult;
use crate::os::procedure::SimProcedure;

pub struct SysRead;
impl SimProcedure for SysRead {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let buf = emu.read_register_u64("RSI").unwrap_or(0);
        let count = emu.read_register_u64("RDX").unwrap_or(0);
        
        tracing::info!("SimProcedure: sys_read({}, 0x{:X}, {})", fd, buf, count);
        
        if fd == 0 {
            if let Some(mut stdin) = emu.stdin_buffer.take() {
                let mut bytes_read = 0;
                let mut data = Vec::new();
                while bytes_read < count && !stdin.is_empty() {
                    data.push(stdin.remove(0) as u8);
                    bytes_read += 1;
                }
                emu.stdin_buffer = Some(stdin);
                emu.state.write_space(3, buf, &data)?;
                
                // Taint stdin bytes!
                for i in 0..bytes_read {
                    let node = emu.solver.register_var(format!("stdin_{}", buf+i), 1);
                    emu.state.set_shadow_memory(3, buf + i, node);
                }
                
                emu.write_register_u64("RAX", bytes_read)?;
            } else {
                // For now, return EOF
                emu.write_register_u64("RAX", 0)?;
            }
        } else {
            emu.write_register_u64("RAX", 0)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysWrite;
impl SimProcedure for SysWrite {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let buf = emu.read_register_u64("RSI").unwrap_or(0);
        let count = emu.read_register_u64("RDX").unwrap_or(0);
        if fd == 1 || fd == 2 {
            let data = emu.state.read_space(3, buf, count as usize).unwrap_or_default();
            print!("{}", String::from_utf8_lossy(&data));
        } else {
            tracing::info!("SimProcedure: sys_write({}, 0x{:X}, {})", fd, buf, count);
        }
        emu.write_register_u64("RAX", count)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysMmap;
impl SimProcedure for SysMmap {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let length = emu.read_register_u64("RSI").unwrap_or(0);
        emu.write_register_u64("RAX", 0x60000000)?;
        tracing::info!("SimProcedure: sys_mmap(len={}) -> 0x60000000", length);
        Ok(HleResult::Continue)
    }
}

pub struct SysBrk;
impl SimProcedure for SysBrk {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let brk = emu.read_register_u64("RDI").unwrap_or(0);
        let new_brk = if brk == 0 { 0x50000000 } else { brk };
        emu.write_register_u64("RAX", new_brk)?;
        tracing::info!("SimProcedure: sys_brk(0x{:X}) -> 0x{:X}", brk, new_brk);
        Ok(HleResult::Continue)
    }
}

pub struct SysExit;
impl SimProcedure for SysExit {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let code = emu.read_register_u64("RDI").unwrap_or(0) as u32;
        tracing::info!("SimProcedure: sys_exit({}). Emulation finished.", code);
        Ok(HleResult::Halt(code))
    }
}
