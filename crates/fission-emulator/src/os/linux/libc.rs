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
        // Host-visible stdout (smoke / sandbox).
        println!("{}", s);
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

/// musl/glibc `__libc_start_main` — transfer control to `main` without ld.so.
///
/// ABI (SysV x86-64, musl crt1): `rdi=main, rsi=argc, rdx=argv`.
/// Replaces the CALL return slot with a synthetic exit stub so `main`'s `ret`
/// becomes a clean process halt (exit code in RAX).
pub struct LibcStartMain;
impl SimProcedure for LibcStartMain {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let main_fn = emu.read_arg(0).unwrap_or(0);
        let argc = emu.read_arg(1).unwrap_or(1);
        let argv = emu.read_arg(2).unwrap_or(0);
        tracing::info!(
            "SimProcedure: __libc_start_main(main=0x{:X}, argc={}, argv=0x{:X})",
            main_fn,
            argc,
            argv
        );
        if main_fn == 0 {
            tracing::warn!("__libc_start_main: null main — halting");
            return Ok(HleResult::Halt(1));
        }
        // CALL __libc_start_main already pushed a return address; rewrite it to
        // our post-main exit stub so main's ret halts cleanly.
        const POST_MAIN_EXIT_STUB: u64 = 0xFFFFFFF1000000F8;
        if let Ok(rsp) = emu.read_register_u64("RSP") {
            let _ = emu
                .state
                .write_space(emu.state.ram_space(), rsp, &POST_MAIN_EXIT_STUB.to_le_bytes());
        }
        // musl main(int argc, char **argv): rdi=argc, rsi=argv
        let _ = emu.write_register_u64("RDI", argc);
        let _ = emu.write_register_u64("RSI", argv);
        Ok(HleResult::JumpTo(main_fn))
    }
}

/// No-op CRT helpers commonly present in dynamic musl/glibc binaries.
pub struct NopOk;
impl SimProcedure for NopOk {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_return_val(0)?;
        Ok(HleResult::Continue)
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
