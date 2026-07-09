use anyhow::Result;
use crate::core::Emulator;
use crate::os::env::HleResult;
use crate::os::procedure::SimProcedure;

pub struct Malloc;
impl SimProcedure for Malloc {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let size = emu.read_arg(0).unwrap_or(0);
        let ptr = emu.heap_alloc(size)?;
        tracing::info!("SimProcedure: malloc(0x{:X}) -> 0x{:X}", size, ptr);
        emu.write_return_val(ptr)?;
        Ok(HleResult::Continue)
    }
}

pub struct Calloc;
impl SimProcedure for Calloc {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let nmemb = emu.read_arg(0).unwrap_or(0);
        let size = emu.read_arg(1).unwrap_or(0);
        let total = nmemb.saturating_mul(size);
        let ptr = emu.heap_alloc(total)?;
        // heap_alloc already zeros the region.
        tracing::info!(
            "SimProcedure: calloc({}, {}) -> 0x{:X}",
            nmemb,
            size,
            ptr
        );
        emu.write_return_val(ptr)?;
        Ok(HleResult::Continue)
    }
}

pub struct Free;
impl SimProcedure for Free {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let ptr = emu.read_arg(0).unwrap_or(0);
        // Bump allocator: free is a no-op (no reuse).
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
        let out = format_printf(emu, &fmt, 1)?;
        print!("{}", out);
        tracing::info!("SimProcedure: printf => {:?}", out);
        emu.write_return_val(out.len() as u64)?;
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
        // our post-main exit stub so main's ret is a clean process halt.
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

/// Minimal printf formatter: `%%`, `%s`, `%c`, `%d`/`%i`, `%u`, `%x`/`%X`, `%p`,
/// plus a single `l` length modifier (`%ld`, `%lu`, `%lx`).
///
/// `first_arg` is the argument index of the first conversion value (1 for
/// `printf(fmt, ...)` where arg0 is the format string).
pub fn format_printf(emu: &mut Emulator, fmt: &str, first_arg: usize) -> Result<String> {
    let mut out = String::new();
    let mut arg_i = first_arg;
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            out.push('%');
            break;
        }
        // Optional single 'l' length modifier (treated as 64-bit where relevant).
        let mut long = false;
        if bytes[i] == b'l' {
            long = true;
            i += 1;
            if i >= bytes.len() {
                out.push_str("%l");
                break;
            }
        }
        match bytes[i] {
            b'%' => out.push('%'),
            b's' => {
                let p = emu.read_arg(arg_i).unwrap_or(0);
                arg_i += 1;
                out.push_str(&read_string(emu, p)?);
            }
            b'c' => {
                let v = emu.read_arg(arg_i).unwrap_or(0) as u8;
                arg_i += 1;
                out.push(v as char);
            }
            b'd' | b'i' => {
                let v = emu.read_arg(arg_i).unwrap_or(0);
                arg_i += 1;
                if long {
                    out.push_str(&format!("{}", v as i64));
                } else {
                    out.push_str(&format!("{}", v as i32));
                }
            }
            b'u' => {
                let v = emu.read_arg(arg_i).unwrap_or(0);
                arg_i += 1;
                if long {
                    out.push_str(&format!("{}", v));
                } else {
                    out.push_str(&format!("{}", v as u32));
                }
            }
            b'x' => {
                let v = emu.read_arg(arg_i).unwrap_or(0);
                arg_i += 1;
                if long {
                    out.push_str(&format!("{:x}", v));
                } else {
                    out.push_str(&format!("{:x}", v as u32));
                }
            }
            b'X' => {
                let v = emu.read_arg(arg_i).unwrap_or(0);
                arg_i += 1;
                if long {
                    out.push_str(&format!("{:X}", v));
                } else {
                    out.push_str(&format!("{:X}", v as u32));
                }
            }
            b'p' => {
                let v = emu.read_arg(arg_i).unwrap_or(0);
                arg_i += 1;
                out.push_str(&format!("0x{:x}", v));
            }
            other => {
                // Unknown conversion: emit literally and do not consume an arg.
                out.push('%');
                if long {
                    out.push('l');
                }
                out.push(other as char);
            }
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::ArchInfo;
    use crate::core::Emulator;
    use crate::os::LinuxEnv;
    use crate::MachineState;
    use fission_loader::loader::LoadedBinary;
    use fission_sleigh::runtime::RuntimeSleighFrontend;
    use std::path::PathBuf;

    fn tiny_emu() -> Emulator {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_x64_hello_sys.elf");
        let binary = LoadedBinary::from_file(&path).expect("fixture");
        let mut state = MachineState::new();
        let _ = crate::os::linux::loader::load_elf(&mut state, &binary);
        let load_spec = binary.load_spec().expect("load_spec").clone();
        let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let lang_id = load_spec.pair.language_id.as_str();
        let arch = ArchInfo::from_language_id(lang_id, Some(&binary)).unwrap();
        Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).unwrap()
    }

    #[test]
    fn printf_formats_basic() {
        let mut emu = tiny_emu();
        // Plant a C string in RAM for %s.
        let s_addr = 0x401000u64;
        emu.state
            .page_map
            .map_region(s_addr, 0x1000, crate::pcode::page_map::prot::RW, true);
        emu.state
            .write_space(emu.state.ram_space(), s_addr, b"world\0")
            .unwrap();
        // SysV: arg0=fmt (unused here), arg1=string, arg2=int
        // format_printf starts at first_arg=1 for values after fmt.
        // We set RSI=s_addr (arg1), RDX=42 (arg2) via write_register / read_arg path.
        emu.write_register_u64("RSI", s_addr).unwrap();
        emu.write_register_u64("RDX", 42).unwrap();
        let out = format_printf(&mut emu, "hello %s %d %%", 1).unwrap();
        assert_eq!(out, "hello world 42 %");
    }

    #[test]
    fn malloc_bump_distinct() {
        let mut emu = tiny_emu();
        let a = emu.heap_alloc(24).unwrap();
        let b = emu.heap_alloc(24).unwrap();
        assert_ne!(a, b);
        assert!(b >= a + 24);
        // Region is mapped and zeroed.
        let bytes = emu.state.read_space(emu.state.ram_space(), a, 8).unwrap();
        assert_eq!(bytes, vec![0u8; 8]);
    }
}
