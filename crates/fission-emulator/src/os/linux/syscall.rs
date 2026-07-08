use anyhow::Result;
use crate::core::Emulator;
use crate::os::env::HleResult;
use crate::os::procedure::SimProcedure;
use crate::os::linux::abi::TargetStat;
use crate::os::linux::libc::read_string; // Re-use the helper

pub struct SysRead;
impl SimProcedure for SysRead {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let buf = emu.read_register_u64("RSI").unwrap_or(0);
        let count = emu.read_register_u64("RDX").unwrap_or(0);
        
        tracing::info!("SimProcedure: sys_read({}, 0x{:X}, {})", fd, buf, count);
        
        match emu.vfs.read(fd, count as usize) {
            Ok(data) => {
                let bytes_read = data.len();
                if bytes_read > 0 {
                    emu.state.write_space(3, buf, &data)?;
                }
                
                // If it's stdin, taint it for symbolic execution
                if fd == 0 {
                    for i in 0..bytes_read {
                        let node = emu.solver.register_var(format!("stdin_{}", buf+(i as u64)), 1);
                        emu.state.set_shadow_memory(3, buf + (i as u64), node);
                    }
                }
                
                emu.write_register_u64("RAX", bytes_read as u64)?;
            }
            Err(_) => {
                // Return -1 (error)
                emu.write_register_u64("RAX", (-1i64) as u64)?;
            }
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
        
        tracing::info!("SimProcedure: sys_write({}, 0x{:X}, {})", fd, buf, count);
        
        let data = emu.state.read_space(3, buf, count as usize).unwrap_or_default();
        match emu.vfs.write(fd, &data) {
            Ok(written) => {
                if fd == 1 || fd == 2 {
                    print!("{}", String::from_utf8_lossy(&data[..written]));
                }
                emu.write_register_u64("RAX", written as u64)?;
            }
            Err(_) => {
                emu.write_register_u64("RAX", (-1i64) as u64)?;
            }
        }

        Ok(HleResult::Continue)
    }
}

pub struct SysOpen;
impl SimProcedure for SysOpen {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let filename_ptr = emu.read_register_u64("RDI").unwrap_or(0);
        let _flags = emu.read_register_u64("RSI").unwrap_or(0);
        let _mode = emu.read_register_u64("RDX").unwrap_or(0);
        
        let filename = read_string(emu, filename_ptr).unwrap_or_else(|_| "unknown".to_string());
        tracing::info!("SimProcedure: sys_open(\"{}\")", filename);
        
        // Open empty file in VFS (we can expand this to load host files later)
        let fd = emu.vfs.open(&filename, Vec::new());
        emu.write_register_u64("RAX", fd)?;
        
        Ok(HleResult::Continue)
    }
}

pub struct SysClose;
impl SimProcedure for SysClose {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        tracing::info!("SimProcedure: sys_close({})", fd);
        
        if emu.vfs.close(fd).is_ok() {
            emu.write_register_u64("RAX", 0)?;
        } else {
            emu.write_register_u64("RAX", (-1i64) as u64)?;
        }
        
        Ok(HleResult::Continue)
    }
}

pub struct SysFstat;
impl SimProcedure for SysFstat {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let statbuf = emu.read_register_u64("RSI").unwrap_or(0);
        
        tracing::info!("SimProcedure: sys_fstat({}, 0x{:X})", fd, statbuf);
        
        if let Some(file) = emu.vfs.files.get(&fd) {
            let mut target_st = TargetStat::default();
            target_st.st_size = file.content.len() as i64;
            target_st.st_mode = 0x81B4; // Regular file, 0664
            
            let bytes = target_st.to_bytes();
            emu.state.write_space(3, statbuf, &bytes)?;
            emu.write_register_u64("RAX", 0)?;
        } else {
            emu.write_register_u64("RAX", (-1i64) as u64)?;
        }
        
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
