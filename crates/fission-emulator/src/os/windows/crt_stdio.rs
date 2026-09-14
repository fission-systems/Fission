//! The C runtime's stdio, as a mingw program actually imports it.
//!
//! A program compiled against the UCRT does not import `printf`. It imports
//! `__stdio_common_vfprintf`, and `printf` is an inline in the header that
//! fetches `stdout` from `__acrt_iob_func(1)` and forwards its `va_list`. So
//! the pair at the top of a real program's wanted-API list is not a formatter
//! and a stream but *one* call, and answering it is what turns a run from
//! "reached exit" into "printed what it was going to print".
//!
//! The descriptor half exists for the same reason. `_fileno`, `_isatty`,
//! `_setmode` and `GetFileType` are what the CRT asks on the way to deciding
//! whether stdout is a console -- and what it decides changes how it buffers,
//! so a wrong answer here shows up as output that never arrives.

use anyhow::Result;

use crate::core::Emulator;
use crate::os::format::{VaList, format_c, read_c_string};
use crate::os::windows::crt_data::CrtGlobals;

/// The handles `GetStdHandle` hands out, which `_get_osfhandle` has to agree
/// with: a program that gets a different handle for fd 1 than for `stdout`
/// writes its output twice or not at all.
pub const STD_HANDLES: [u64; 3] = [0x50, 0x51, 0x52];

/// `FILE_TYPE_CHAR`, which is what a console is.
const FILE_TYPE_CHAR: u64 = 0x0002;
/// `FILE_TYPE_DISK`.
const FILE_TYPE_DISK: u64 = 0x0001;

/// `_O_TEXT`, the mode a stream starts in.
const O_TEXT: u64 = 0x4000;

/// `__acrt_iob_func(index)` -- the address of `stdin`, `stdout` or `stderr`.
pub fn acrt_iob_func(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let index = emu.read_arg(0).unwrap_or(0);
    emu.write_return_val(globals.stream(index))
}

/// Which descriptor a `FILE*` or a fd argument means, for the handlers that
/// accept either.
fn stream_or_fd(globals: &CrtGlobals, value: u64) -> u64 {
    globals.stream_index(value).unwrap_or(value)
}

/// `__stdio_common_vfprintf(options, stream, format, locale, arglist)`.
///
/// Returns the number of characters written, which is what the `printf`
/// inline returns to the program.
pub fn stdio_common_vfprintf(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let stream = emu.read_arg(1).unwrap_or(0);
    let format = emu.read_arg(2).unwrap_or(0);
    let arglist = emu.read_arg(4).unwrap_or(0);

    let fmt = read_c_string(emu, format, 4096);
    let is_64bit = emu.arch.pointer_size == 8;
    let mut args = VaList::new(arglist, is_64bit);
    let text = format_c(emu, &fmt, &mut args)?;

    match globals.stream_index(stream) {
        Some(2) => emu.guest_stderr(text.as_bytes()),
        _ => emu.guest_stdout(text.as_bytes()),
    }
    emu.write_return_val(text.len() as u64)
}

/// `__stdio_common_vsprintf(options, buffer, count, format, locale, arglist)`.
///
/// The return is the length the formatted string *would* have had, not the
/// length written -- a program sizing a buffer with a zero-length call
/// depends on the difference.
pub fn stdio_common_vsprintf(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let _ = globals;
    let buffer = emu.read_arg(1).unwrap_or(0);
    let count = emu.read_arg(2).unwrap_or(0);
    let format = emu.read_arg(3).unwrap_or(0);
    let arglist = emu.read_arg(5).unwrap_or(0);

    let fmt = read_c_string(emu, format, 4096);
    let is_64bit = emu.arch.pointer_size == 8;
    let mut args = VaList::new(arglist, is_64bit);
    let text = format_c(emu, &fmt, &mut args)?;

    if buffer != 0 && count > 0 {
        let keep = (count as usize - 1).min(text.len());
        let mut bytes = text.as_bytes()[..keep].to_vec();
        bytes.push(0);
        let space = emu.state.ram_space();
        emu.state.write_space(space, buffer, &bytes)?;
    }
    emu.write_return_val(text.len() as u64)
}

/// `__stdio_common_vsscanf(options, input, count, format, locale, arglist)`.
///
/// Returns the number of fields assigned, which is what every caller
/// branches on.
pub fn stdio_common_vsscanf(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let _ = globals;
    let input = emu.read_arg(1).unwrap_or(0);
    let format = emu.read_arg(3).unwrap_or(0);
    let arglist = emu.read_arg(5).unwrap_or(0);

    let text = read_c_string(emu, input, 4096);
    let fmt = read_c_string(emu, format, 4096);
    let is_64bit = emu.arch.pointer_size == 8;
    let assigned = scan(emu, &text, &fmt, arglist, is_64bit)?;
    emu.write_return_val(assigned)
}

/// A `sscanf` for the conversions programs actually use on a string they
/// just built themselves: whitespace, literals, `%d`, `%u`, `%x`, `%s`,
/// `%c`, `%f`. Anything else stops the scan, which is what C does too -- a
/// conversion that fails ends the call and the count says how far it got.
fn scan(emu: &mut Emulator, text: &str, fmt: &str, arglist: u64, is_64bit: bool) -> Result<u64> {
    let input = text.as_bytes();
    let spec = fmt.as_bytes();
    let step = if is_64bit { 8 } else { 4 };
    let mut at = 0usize;
    let mut i = 0usize;
    let mut slot = arglist;
    let mut assigned = 0u64;

    let next_pointer = |emu: &mut Emulator, slot: &mut u64| -> u64 {
        let space = emu.state.ram_space();
        let value = emu
            .state
            .read_space(space, *slot, step as usize)
            .ok()
            .map(|bytes| {
                let mut word = [0u8; 8];
                word[..bytes.len().min(8)].copy_from_slice(&bytes[..bytes.len().min(8)]);
                u64::from_le_bytes(word)
            })
            .unwrap_or(0);
        *slot += step;
        value
    };

    while i < spec.len() {
        if spec[i].is_ascii_whitespace() {
            while at < input.len() && input[at].is_ascii_whitespace() {
                at += 1;
            }
            i += 1;
            continue;
        }
        if spec[i] != b'%' {
            if at >= input.len() || input[at] != spec[i] {
                break;
            }
            at += 1;
            i += 1;
            continue;
        }
        i += 1;
        // A width, and the assignment-suppressing star.
        let suppress = spec.get(i) == Some(&b'*');
        if suppress {
            i += 1;
        }
        let mut width = 0usize;
        while i < spec.len() && spec[i].is_ascii_digit() {
            width = width * 10 + (spec[i] - b'0') as usize;
            i += 1;
        }
        let width = (width == 0).then_some(usize::MAX).unwrap_or(width);
        while i < spec.len() && matches!(spec[i], b'l' | b'h' | b'z' | b'j' | b't' | b'L') {
            i += 1;
        }
        let Some(&conversion) = spec.get(i) else {
            break;
        };
        i += 1;

        if conversion != b'c' {
            while at < input.len() && input[at].is_ascii_whitespace() {
                at += 1;
            }
        }
        if at >= input.len() {
            break;
        }

        let start = at;
        let (value, text_value) = match conversion {
            b'd' | b'i' | b'u' => {
                if matches!(input[at], b'-' | b'+') {
                    at += 1;
                }
                while at < input.len() && input[at].is_ascii_digit() && at - start < width {
                    at += 1;
                }
                if at == start {
                    break;
                }
                let parsed: i64 = text[start..at].parse().unwrap_or(0);
                (Some(parsed as u64), None)
            }
            b'x' | b'X' => {
                while at < input.len() && input[at].is_ascii_hexdigit() && at - start < width {
                    at += 1;
                }
                if at == start {
                    break;
                }
                (u64::from_str_radix(&text[start..at], 16).ok(), None)
            }
            b'f' | b'e' | b'g' => {
                while at < input.len()
                    && (input[at].is_ascii_digit()
                        || matches!(input[at], b'.' | b'-' | b'+' | b'e' | b'E'))
                    && at - start < width
                {
                    at += 1;
                }
                if at == start {
                    break;
                }
                let parsed: f64 = text[start..at].parse().unwrap_or(0.0);
                (Some(parsed.to_bits()), None)
            }
            b'c' => {
                let take = if width == usize::MAX { 1 } else { width };
                at = (at + take).min(input.len());
                (None, Some(text[start..at].to_string()))
            }
            b's' => {
                while at < input.len() && !input[at].is_ascii_whitespace() && at - start < width {
                    at += 1;
                }
                if at == start {
                    break;
                }
                (None, Some(text[start..at].to_string()))
            }
            b'%' => {
                if input[at] != b'%' {
                    break;
                }
                at += 1;
                continue;
            }
            _ => break,
        };

        if suppress {
            continue;
        }
        let destination = next_pointer(emu, &mut slot);
        if destination == 0 {
            break;
        }
        let space = emu.state.ram_space();
        match (value, text_value) {
            (Some(word), _) if matches!(conversion, b'f' | b'e' | b'g') => {
                emu.state
                    .write_space(space, destination, &word.to_le_bytes())?;
            }
            (Some(word), _) => {
                // The declared width is unknown here, so four bytes: `%d`
                // into an `int` is what every caller wrote.
                emu.state
                    .write_space(space, destination, &(word as u32).to_le_bytes())?;
            }
            (_, Some(mut string)) => {
                if conversion == b's' {
                    string.push('\0');
                }
                emu.state
                    .write_space(space, destination, string.as_bytes())?;
            }
            _ => {}
        }
        assigned += 1;
    }
    Ok(assigned)
}

/// `_fileno(FILE*)`.
pub fn fileno(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let stream = emu.read_arg(0).unwrap_or(0);
    match globals.stream_index(stream) {
        Some(index) => emu.write_return_val(index),
        // Not one of ours: a real descriptor would be an error, and -1 with
        // `EBADF` is what the CRT expects to see.
        None => {
            set_errno(emu, globals, 9)?;
            emu.write_return_val(u64::MAX)
        }
    }
}

/// `_isatty(fd)` -- the three standard streams are a console here, and
/// nothing else is.
pub fn isatty(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let fd = stream_or_fd(globals, emu.read_arg(0).unwrap_or(0));
    emu.write_return_val(u64::from(fd < 3))
}

/// `_setmode(fd, mode)` -- accepted, and the previous mode reported.
///
/// The mode is not tracked: nothing here translates line endings, so text
/// and binary are the same stream. Saying so is the point -- a program that
/// sets binary mode and then writes `\n` gets `\n`.
pub fn setmode(emu: &mut Emulator) -> Result<()> {
    emu.write_return_val(O_TEXT)
}

/// `GetFileType(handle)`.
pub fn get_file_type(emu: &mut Emulator) -> Result<()> {
    let handle = emu.read_arg(0).unwrap_or(0);
    let kind = if STD_HANDLES.contains(&handle) {
        FILE_TYPE_CHAR
    } else {
        FILE_TYPE_DISK
    };
    emu.write_return_val(kind)
}

/// `_get_osfhandle(fd)` -- the same handle `GetStdHandle` gives out.
pub fn get_osfhandle(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let fd = stream_or_fd(globals, emu.read_arg(0).unwrap_or(0));
    match STD_HANDLES.get(fd as usize) {
        Some(handle) => emu.write_return_val(*handle),
        None => emu.write_return_val(u64::MAX),
    }
}

/// `_read(fd, buffer, count)` from whatever stands in for stdin.
pub fn read(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let fd = stream_or_fd(globals, emu.read_arg(0).unwrap_or(0));
    let buffer = emu.read_arg(1).unwrap_or(0);
    let count = emu.read_arg(2).unwrap_or(0) as usize;

    let bytes = emu.guest_stdin(count.min(0x10_0000));
    if !bytes.is_empty() && buffer != 0 {
        let space = emu.state.ram_space();
        emu.state.write_space(space, buffer, &bytes)?;
    }
    let _ = fd;
    emu.write_return_val(bytes.len() as u64)
}

/// `_close(fd)`.
pub fn close(emu: &mut Emulator) -> Result<()> {
    emu.write_return_val(0)
}

/// `_lseeki64(fd, offset, origin)` -- the standard streams are not seekable,
/// which is exactly what a program checks this to find out.
pub fn lseeki64(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    set_errno(emu, globals, 29)?; // ESPIPE
    emu.write_return_val(u64::MAX)
}

/// `_access(path, mode)` and `_stat64(path, buffer)`: nothing is on this
/// filesystem unless the VFS was seeded with it.
pub fn access(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let pointer = emu.read_arg(0).unwrap_or(0);
    let path = read_c_string(emu, pointer, 512);
    let known = emu.vfs.path_seeds.contains_key(&path) || emu.vfs.host_aliases.contains_key(&path);
    if known {
        emu.write_return_val(0)
    } else {
        set_errno(emu, globals, 2)?; // ENOENT
        emu.write_return_val(u64::MAX)
    }
}

pub fn stat64(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    set_errno(emu, globals, 2)?; // ENOENT
    emu.write_return_val(u64::MAX)
}

/// `_errno()` -- the *address* of the variable, because the header defines
/// `errno` as `(*_errno())`.
pub fn errno_pointer(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    emu.write_return_val(globals.errno)
}

fn set_errno(emu: &mut Emulator, globals: &CrtGlobals, value: u32) -> Result<()> {
    let space = emu.state.ram_space();
    emu.state
        .write_space(space, globals.errno, &value.to_le_bytes())?;
    Ok(())
}

/// `getenv(name)` -- nothing is set.
///
/// An empty environment is a real environment, and it is the one this
/// process has: `environ` is an empty vector, so answering anything else
/// would contradict what the program can already read for itself.
pub fn getenv(emu: &mut Emulator) -> Result<()> {
    let pointer = emu.read_arg(0).unwrap_or(0);
    let name = read_c_string(emu, pointer, 256);
    tracing::debug!("getenv({name}) -> unset");
    emu.write_return_val(0)
}

/// The CRT calls that need the process's data page. Returns whether the name
/// was one of them, so the caller can fall through to its miss reporting.
pub fn dispatch(emu: &mut Emulator, globals: &CrtGlobals, name: &str) -> Result<bool> {
    match name {
        "__acrt_iob_func" | "__iob_func" => acrt_iob_func(emu, globals)?,
        "__stdio_common_vfprintf"
        | "__stdio_common_vfprintf_s"
        | "__stdio_common_vfprintf_p"
        | "__stdio_common_vfwprintf" => stdio_common_vfprintf(emu, globals)?,
        "__stdio_common_vsprintf"
        | "__stdio_common_vsprintf_s"
        | "__stdio_common_vsnprintf_s"
        | "__stdio_common_vswprintf" => stdio_common_vsprintf(emu, globals)?,
        "__stdio_common_vsscanf" | "__stdio_common_vfscanf" => stdio_common_vsscanf(emu, globals)?,
        "_fileno" | "fileno" => fileno(emu, globals)?,
        "_isatty" | "isatty" => isatty(emu, globals)?,
        "_setmode" | "setmode" => setmode(emu)?,
        "GetFileType" => get_file_type(emu)?,
        "_get_osfhandle" => get_osfhandle(emu, globals)?,
        "_read" | "read" => read(emu, globals)?,
        "_close" | "close" => close(emu)?,
        "_lseeki64" | "_lseek" | "lseek" => lseeki64(emu, globals)?,
        "_access" | "access" | "_waccess" => access(emu, globals)?,
        "_stat64" | "_stat" | "_stat64i32" | "_fstat64" | "stat" => stat64(emu, globals)?,
        "_errno" | "__errno_location" => errno_pointer(emu, globals)?,
        "getenv" | "_wgetenv" | "getenv_s" => getenv(emu)?,
        "_write" | "write" => write(emu, globals)?,

        "fopen" | "fopen_s" | "_wfopen" | "freopen" => fopen(emu, globals)?,
        "fclose" => fclose(emu, globals)?,
        // A descriptor *is* a stream slot here, so `_fdopen` is the
        // conversion and nothing else.
        "_fdopen" | "fdopen" | "_wfdopen" => {
            let fd = stream_or_fd(globals, emu.read_arg(0).unwrap_or(0));
            emu.write_return_val(globals.stream(fd))?;
        }
        "fread" | "fread_s" => fread(emu, globals)?,
        "fwrite" => fwrite(emu, globals)?,
        "puts" | "_putws" => puts(emu, globals, true)?,
        "fputs" | "fputws" => puts(emu, globals, false)?,
        "putchar" | "_putchar" => fputc(emu, globals, true)?,
        "fputc" | "putc" | "_fputc_nolock" => fputc(emu, globals, false)?,
        "fgetc" | "getc" | "_fgetc_nolock" => fgetc(emu, globals)?,
        "fgets" => fgets(emu, globals)?,
        "feof" => feof(emu, globals)?,
        "ftell" | "_ftelli64" => ftell(emu, globals)?,
        "fseek" | "_fseeki64" => fseek(emu, globals)?,
        // Nothing here buffers, so a flush has already happened and a
        // buffering request changes nothing. `ferror` reports no error
        // because every failure above is reported at the call that failed.
        "fflush" | "_flushall" => {
            emu.write_return_val(0)?;
        }
        "setvbuf" | "setbuf" => {
            emu.write_return_val(0)?;
        }
        "ferror" | "clearerr" => {
            emu.write_return_val(0)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

// ── The `FILE` layer ─────────────────────────────────────────────────────────
//
// `fopen` through `fclose` are the second half of what a real program wants:
// the first sweep after the formatter landed had bzip2 and cJSON printing and
// minigzip stuck, because it opens a file before it does anything else.
//
// A descriptor is a `FILE` slot and a `FILE` slot is a descriptor, so these
// and the `_read`/`_write` pair below are two spellings of the same state.

/// `fopen(path, mode)`.
///
/// A path the VFS has never heard of does **not** open. Returning a stream
/// for a file that is not there reads as an empty file, and a program then
/// does its work on nothing and reports success -- far worse than the failure
/// it already knows how to handle.
pub fn fopen(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let path_pointer = emu.read_arg(0).unwrap_or(0);
    let mode_pointer = emu.read_arg(1).unwrap_or(0);
    let path = read_c_string(emu, path_pointer, 512);
    let mode = read_c_string(emu, mode_pointer, 16);

    let writing = mode.contains(['w', 'a', '+']);
    let known = emu.vfs.path_seeds.contains_key(&path)
        || emu.vfs.host_aliases.contains_key(&path)
        || std::path::Path::new(&path).is_file();
    if !known && !writing {
        tracing::debug!("fopen({path}, {mode}) -> no such file");
        set_errno(emu, globals, 2)?; // ENOENT
        return emu.write_return_val(0);
    }

    let fd = emu.vfs.open(&path, Vec::new());
    let stream = globals.stream(fd);
    if stream == 0 {
        // Out of `FILE` slots. Give the descriptor back rather than leaving
        // it open against a stream nobody can name.
        let _ = emu.vfs.close(fd);
        set_errno(emu, globals, 24)?; // EMFILE
        return emu.write_return_val(0);
    }
    tracing::debug!("fopen({path}, {mode}) -> fd {fd}");
    emu.write_return_val(stream)
}

/// `fclose(FILE*)`.
pub fn fclose(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let stream = emu.read_arg(0).unwrap_or(0);
    if let Some(fd) = globals.stream_index(stream) {
        let _ = emu.vfs.close(fd);
    }
    emu.write_return_val(0)
}

/// `fread(buffer, size, count, FILE*)` -- the return is *items*, not bytes,
/// which is what the caller's loop condition compares against.
pub fn fread(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let buffer = emu.read_arg(0).unwrap_or(0);
    let size = emu.read_arg(1).unwrap_or(0);
    let count = emu.read_arg(2).unwrap_or(0);
    let stream = emu.read_arg(3).unwrap_or(0);
    let Some(fd) = globals.stream_index(stream) else {
        return emu.write_return_val(0);
    };

    let wanted = size.saturating_mul(count).min(0x100_0000) as usize;
    let bytes = if fd == 0 {
        emu.guest_stdin(wanted)
    } else {
        emu.vfs.read(fd, wanted).unwrap_or_default()
    };
    if !bytes.is_empty() && buffer != 0 {
        let space = emu.state.ram_space();
        emu.state.write_space(space, buffer, &bytes)?;
    }
    let items = if size == 0 {
        0
    } else {
        bytes.len() as u64 / size
    };
    emu.write_return_val(items)
}

/// `fwrite(buffer, size, count, FILE*)`, again in items.
pub fn fwrite(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let buffer = emu.read_arg(0).unwrap_or(0);
    let size = emu.read_arg(1).unwrap_or(0);
    let count = emu.read_arg(2).unwrap_or(0);
    let stream = emu.read_arg(3).unwrap_or(0);
    let Some(fd) = globals.stream_index(stream) else {
        return emu.write_return_val(0);
    };

    let length = size.saturating_mul(count).min(0x100_0000) as usize;
    let bytes = if length == 0 || buffer == 0 {
        Vec::new()
    } else {
        let space = emu.state.ram_space();
        emu.state.read_space(space, buffer, length)?
    };
    write_descriptor(emu, fd, &bytes);
    let items = if size == 0 {
        0
    } else {
        bytes.len() as u64 / size
    };
    emu.write_return_val(items)
}

/// Where bytes written to a descriptor go: the two console streams to the
/// host, everything else into the VFS file.
fn write_descriptor(emu: &mut Emulator, fd: u64, bytes: &[u8]) {
    match fd {
        1 => emu.guest_stdout(bytes),
        2 => emu.guest_stderr(bytes),
        _ => {
            let _ = emu.vfs.write(fd, bytes);
        }
    }
}

/// `_write(fd, buffer, count)`.
pub fn write(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let fd = stream_or_fd(globals, emu.read_arg(0).unwrap_or(0));
    let buffer = emu.read_arg(1).unwrap_or(0);
    let count = emu.read_arg(2).unwrap_or(0) as usize;
    let bytes = if count == 0 || buffer == 0 {
        Vec::new()
    } else {
        let space = emu.state.ram_space();
        emu.state.read_space(space, buffer, count.min(0x100_0000))?
    };
    write_descriptor(emu, fd, &bytes);
    emu.write_return_val(bytes.len() as u64)
}

/// `fputs(text, FILE*)` and `puts(text)`. `puts` appends a newline and
/// `fputs` does not, which is the one difference between them.
pub fn puts(emu: &mut Emulator, globals: &CrtGlobals, with_newline: bool) -> Result<()> {
    let text_pointer = emu.read_arg(0).unwrap_or(0);
    let mut text = read_c_string(emu, text_pointer, 0x10000).into_bytes();
    if with_newline {
        text.push(b'\n');
    }
    let fd = if with_newline {
        1
    } else {
        globals
            .stream_index(emu.read_arg(1).unwrap_or(0))
            .unwrap_or(1)
    };
    write_descriptor(emu, fd, &text);
    emu.write_return_val(0)
}

/// `fputc(c, FILE*)` / `putc` / `putchar`.
pub fn fputc(emu: &mut Emulator, globals: &CrtGlobals, to_stdout: bool) -> Result<()> {
    let character = emu.read_arg(0).unwrap_or(0) as u8;
    let fd = if to_stdout {
        1
    } else {
        globals
            .stream_index(emu.read_arg(1).unwrap_or(0))
            .unwrap_or(1)
    };
    write_descriptor(emu, fd, &[character]);
    emu.write_return_val(character as u64)
}

/// `fgetc(FILE*)` / `getc`, and `EOF` when the file is spent.
pub fn fgetc(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let stream = emu.read_arg(0).unwrap_or(0);
    let Some(fd) = globals.stream_index(stream) else {
        return emu.write_return_val(u64::MAX);
    };
    let bytes = if fd == 0 {
        emu.guest_stdin(1)
    } else {
        emu.vfs.read(fd, 1).unwrap_or_default()
    };
    match bytes.first() {
        Some(byte) => emu.write_return_val(*byte as u64),
        None => emu.write_return_val(u64::MAX), // EOF
    }
}

/// `fgets(buffer, size, FILE*)` -- up to a newline, which it keeps.
pub fn fgets(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let buffer = emu.read_arg(0).unwrap_or(0);
    let size = emu.read_arg(1).unwrap_or(0) as usize;
    let stream = emu.read_arg(2).unwrap_or(0);
    let Some(fd) = globals.stream_index(stream) else {
        return emu.write_return_val(0);
    };
    if buffer == 0 || size < 2 {
        return emu.write_return_val(0);
    }

    let mut line = Vec::new();
    while line.len() + 1 < size {
        let byte = if fd == 0 {
            emu.guest_stdin(1)
        } else {
            emu.vfs.read(fd, 1).unwrap_or_default()
        };
        let Some(&byte) = byte.first() else { break };
        line.push(byte);
        if byte == b'\n' {
            break;
        }
    }
    if line.is_empty() {
        return emu.write_return_val(0); // EOF with nothing read
    }
    line.push(0);
    let space = emu.state.ram_space();
    emu.state.write_space(space, buffer, &line)?;
    emu.write_return_val(buffer)
}

/// `feof(FILE*)` -- the cursor has reached the end.
pub fn feof(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let stream = emu.read_arg(0).unwrap_or(0);
    let at_end = globals
        .stream_index(stream)
        .and_then(|fd| {
            let size = emu.vfs.file_size(fd)?;
            let cursor = emu.vfs.files.get(&fd)?.cursor;
            Some(cursor >= size)
        })
        .unwrap_or(false);
    emu.write_return_val(u64::from(at_end))
}

/// `ftell(FILE*)`.
pub fn ftell(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    let stream = emu.read_arg(0).unwrap_or(0);
    let cursor = globals
        .stream_index(stream)
        .and_then(|fd| emu.vfs.files.get(&fd).map(|file| file.cursor as u64))
        .unwrap_or(0);
    emu.write_return_val(cursor)
}

/// `fseek(FILE*, offset, origin)`.
pub fn fseek(emu: &mut Emulator, globals: &CrtGlobals) -> Result<()> {
    const SEEK_CUR: u64 = 1;
    const SEEK_END: u64 = 2;
    let stream = emu.read_arg(0).unwrap_or(0);
    let offset = emu.read_arg(1).unwrap_or(0) as i64;
    let origin = emu.read_arg(2).unwrap_or(0);
    let Some(fd) = globals.stream_index(stream) else {
        return emu.write_return_val(u64::MAX);
    };
    let size = emu.vfs.file_size(fd).unwrap_or(0) as i64;
    let cursor = emu.vfs.files.get(&fd).map(|f| f.cursor as i64).unwrap_or(0);
    let base = match origin {
        SEEK_CUR => cursor,
        SEEK_END => size,
        _ => 0,
    };
    let target = (base + offset).clamp(0, size) as usize;
    let _ = emu.vfs.seek(fd, target);
    emu.write_return_val(0)
}
