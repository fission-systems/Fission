//! The C formatting engine, once, for every environment that needs one.
//!
//! There were two reasons to lift this out of the Linux libc layer. The first
//! is that Windows needs it: a mingw program does not import `printf`, it
//! imports `__stdio_common_vfprintf`, and a sweep of six real programs found
//! that call at the top of what they were waiting on. The second is that the
//! arguments arrive differently. `printf(fmt, ...)` passes them in the
//! calling convention; `__stdio_common_vfprintf(options, stream, format,
//! locale, arglist)` passes a *pointer* to them. One engine, two sources.
//!
//! What is supported is what real programs use rather than what the standard
//! lists: flags `-+ #0`, a width and precision (literal or `*`), the length
//! modifiers including the Microsoft `I64`/`I32`, and the conversions
//! `diouxXeEfFgGaAcspn%`. A conversion nobody recognises is emitted literally
//! and consumes no argument -- the alternative is a silently dropped argument
//! and every later conversion reading the wrong slot.

use anyhow::Result;

use crate::core::Emulator;

/// Where the next argument comes from.
///
/// The emulator is passed in rather than held, because a source that reads
/// guest memory and the formatter that reads guest strings would otherwise
/// both want it mutably.
pub trait ArgSource {
    /// The next argument-sized slot. Eight bytes on a 64-bit guest; a
    /// narrower type was promoted into one by the caller.
    fn next_word(&mut self, emu: &mut Emulator) -> u64;
}

/// Arguments passed in the calling convention: `printf(fmt, ...)`.
pub struct CallArgs {
    next: usize,
}

impl CallArgs {
    /// `first` is the index of the first conversion value -- 1 for
    /// `printf(fmt, ...)`, 3 for `snprintf(buf, size, fmt, ...)`.
    pub fn new(first: usize) -> Self {
        Self { next: first }
    }
}

impl ArgSource for CallArgs {
    fn next_word(&mut self, emu: &mut Emulator) -> u64 {
        let value = emu.read_arg(self.next).unwrap_or(0);
        self.next += 1;
        value
    }
}

/// Arguments behind a `va_list`: a run of argument-sized slots in guest
/// memory, which is what the x64 ABI leaves for the variadic tail.
pub struct VaList {
    cursor: u64,
    step: u64,
}

impl VaList {
    pub fn new(address: u64, is_64bit: bool) -> Self {
        Self {
            cursor: address,
            step: if is_64bit { 8 } else { 4 },
        }
    }
}

impl ArgSource for VaList {
    fn next_word(&mut self, emu: &mut Emulator) -> u64 {
        let space = emu.state.ram_space();
        let size = self.step as usize;
        let value = match emu.state.read_space(space, self.cursor, size) {
            Ok(bytes) if bytes.len() >= size => {
                let mut word = [0u8; 8];
                word[..size].copy_from_slice(&bytes[..size]);
                u64::from_le_bytes(word)
            }
            _ => 0,
        };
        self.cursor = self.cursor.wrapping_add(self.step);
        value
    }
}

/// A C string out of guest memory, stopping at the NUL or at `limit` bytes.
pub fn read_c_string(emu: &mut Emulator, address: u64, limit: usize) -> String {
    if address == 0 {
        // What glibc prints, and what a program that hits it is usually
        // trying to find out.
        return "(null)".to_string();
    }
    let space = emu.state.ram_space();
    let mut bytes = Vec::new();
    let mut cursor = address;
    while bytes.len() < limit {
        match emu.state.read_space(space, cursor, 1) {
            Ok(b) if !b.is_empty() && b[0] != 0 => bytes.push(b[0]),
            _ => break,
        }
        cursor = cursor.wrapping_add(1);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A UTF-16 string out of guest memory, stopping at the NUL or at `limit`
/// characters.
pub fn read_wide_c_string(emu: &mut Emulator, address: u64, limit: usize) -> String {
    if address == 0 {
        return "(null)".to_string();
    }
    let space = emu.state.ram_space();
    let mut units = Vec::new();
    let mut cursor = address;
    while units.len() < limit {
        match emu.state.read_space(space, cursor, 2) {
            Ok(pair) if pair.len() == 2 && (pair[0] != 0 || pair[1] != 0) => {
                units.push(u16::from_le_bytes([pair[0], pair[1]]));
            }
            _ => break,
        }
        cursor = cursor.wrapping_add(2);
    }
    String::from_utf16_lossy(&units)
}

/// How wide the integer conversions are.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Length {
    Char,
    Short,
    Int,
    Long,
    LongLong,
    Size,
}

impl Length {
    /// A slot narrowed to the declared type, sign-extended where signed.
    /// `long_is_64bit` is the data model, not the pointer size: LP64 (Linux,
    /// macOS) has a 64-bit `long`, LLP64 (Windows) keeps it at 32 bits even
    /// on a 64-bit guest.
    fn signed(self, word: u64, long_is_64bit: bool) -> i64 {
        match self {
            Length::Char => word as u8 as i8 as i64,
            Length::Short => word as u16 as i16 as i64,
            Length::Int => word as u32 as i32 as i64,
            Length::Long if !long_is_64bit => word as u32 as i32 as i64,
            _ => word as i64,
        }
    }

    fn unsigned(self, word: u64, long_is_64bit: bool) -> u64 {
        match self {
            Length::Char => word as u8 as u64,
            Length::Short => word as u16 as u64,
            Length::Int => word as u32 as u64,
            Length::Long if !long_is_64bit => word as u32 as u64,
            _ => word,
        }
    }
}

struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: Option<usize>,
    precision: Option<usize>,
    length: Length,
}

/// Format `fmt`, pulling values from `args`.
///
/// `chars_written` is reported through the return value's length; a caller
/// that has to answer "how many characters would this have been" gets it from
/// the whole string, before any truncation it does itself.
pub fn format_c(emu: &mut Emulator, fmt: &str, args: &mut dyn ArgSource) -> Result<String> {
    let is_64bit = emu.arch.pointer_size == 8;
    // Windows is LLP64: `long` stays 32 bits on a 64-bit guest. Reading it as
    // 64 printed Lua's `math.floor(-2.5)` -- a C89-configured Lua keeps its
    // integers in a `long` -- as 4294967293. The formatter's own tests run on
    // an ELF, where `long` really is 64 bits, which is how it went unseen.
    let long_is_64bit = is_64bit && emu.binary.format != "PE";
    let bytes = fmt.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        if i >= bytes.len() {
            out.push('%');
            break;
        }

        let mut spec = Spec {
            left: false,
            plus: false,
            space: false,
            alt: false,
            zero: false,
            width: None,
            precision: None,
            length: Length::Int,
        };

        // Flags.
        while i < bytes.len() {
            match bytes[i] {
                b'-' => spec.left = true,
                b'+' => spec.plus = true,
                b' ' => spec.space = true,
                b'#' => spec.alt = true,
                b'0' => spec.zero = true,
                _ => break,
            }
            i += 1;
        }

        // Width, literal or taken from an argument.
        if i < bytes.len() && bytes[i] == b'*' {
            i += 1;
            let given = args.next_word(emu) as i32;
            if given < 0 {
                spec.left = true;
                spec.width = Some(given.unsigned_abs() as usize);
            } else {
                spec.width = Some(given as usize);
            }
        } else {
            let mut width = None;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                width = Some(width.unwrap_or(0usize) * 10 + (bytes[i] - b'0') as usize);
                i += 1;
            }
            spec.width = width;
        }

        // Precision.
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'*' {
                i += 1;
                let given = args.next_word(emu) as i32;
                spec.precision = (given >= 0).then_some(given as usize);
            } else {
                let mut precision = 0usize;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    precision = precision * 10 + (bytes[i] - b'0') as usize;
                    i += 1;
                }
                spec.precision = Some(precision);
            }
        }

        // Length, including the Microsoft spellings a mingw program's format
        // strings are full of.
        if bytes[i..].starts_with(b"I64") {
            spec.length = Length::LongLong;
            i += 3;
        } else if bytes[i..].starts_with(b"I32") {
            spec.length = Length::Int;
            i += 3;
        } else {
            loop {
                let Some(&c) = bytes.get(i) else { break };
                spec.length = match (c, spec.length) {
                    (b'h', Length::Short) => Length::Char,
                    (b'h', _) => Length::Short,
                    (b'l', Length::Long) => Length::LongLong,
                    (b'l' | b'w', _) => Length::Long,
                    (b'L' | b'q' | b'j', _) => Length::LongLong,
                    (b'z' | b't' | b'I', _) => Length::Size,
                    _ => break,
                };
                i += 1;
            }
        }

        let Some(&conversion) = bytes.get(i) else {
            out.push_str(&fmt[start..]);
            break;
        };
        i += 1;

        let body = match conversion {
            b'%' => {
                out.push('%');
                continue;
            }
            b'd' | b'i' => {
                let value = spec.length.signed(args.next_word(emu), long_is_64bit);
                signed_digits(value, &spec)
            }
            b'u' => {
                let value = spec.length.unsigned(args.next_word(emu), long_is_64bit);
                pad_digits(&value.to_string(), &spec)
            }
            b'o' => {
                let value = spec.length.unsigned(args.next_word(emu), long_is_64bit);
                let digits = pad_digits(&format!("{value:o}"), &spec);
                if spec.alt && !digits.starts_with('0') {
                    format!("0{digits}")
                } else {
                    digits
                }
            }
            b'x' | b'X' => {
                let value = spec.length.unsigned(args.next_word(emu), long_is_64bit);
                let digits = if conversion == b'x' {
                    format!("{value:x}")
                } else {
                    format!("{value:X}")
                };
                let digits = pad_digits(&digits, &spec);
                if spec.alt && value != 0 {
                    format!("0{}{digits}", conversion as char)
                } else {
                    digits
                }
            }
            // `%lc` and Microsoft's `%C` are a wide character. Reading it as a
            // byte is right for ASCII and wrong for everything else.
            b'c' | b'C' if conversion == b'C' || spec.length == Length::Long => {
                let unit = args.next_word(emu) as u16;
                String::from_utf16_lossy(&[unit])
            }
            b'c' => ((args.next_word(emu) as u8) as char).to_string(),
            // `%ls` and Microsoft's `%S`/`%ws` are a wide string. A program
            // built with `-municode` prints its own name this way, and read as
            // bytes a UTF-16 "program.exe" stops after the `p`.
            b's' | b'S' => {
                let pointer = args.next_word(emu);
                let limit = spec.precision.unwrap_or(4096);
                let wide = conversion == b'S' || spec.length == Length::Long;
                let mut text = if wide {
                    read_wide_c_string(emu, pointer, limit.max(1))
                } else {
                    read_c_string(emu, pointer, limit.max(1))
                };
                if let Some(precision) = spec.precision {
                    text = text.chars().take(precision).collect();
                }
                text
            }
            b'p' => {
                let value = args.next_word(emu);
                if value == 0 {
                    "(nil)".to_string()
                } else {
                    format!("0x{value:x}")
                }
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' | b'a' | b'A' => {
                let value = f64::from_bits(args.next_word(emu));
                floating(value, conversion, &spec)
            }
            b'n' => {
                // The count so far, written back through the pointer. Rare,
                // and a program that uses it notices immediately if it is
                // answered with nothing.
                let pointer = args.next_word(emu);
                if pointer != 0 {
                    let space = emu.state.ram_space();
                    let written = out.len() as u32;
                    let _ = emu
                        .state
                        .write_space(space, pointer, &written.to_le_bytes());
                }
                continue;
            }
            _ => {
                // Unrecognised: emit it literally and consume nothing. A
                // dropped argument would put every later conversion one slot
                // out, which is far harder to see than a stray `%q`.
                out.push_str(&fmt[start..i]);
                continue;
            }
        };

        out.push_str(&pad(
            &body,
            &spec,
            matches!(conversion, b's' | b'c' | b'S' | b'C'),
        ));
    }

    Ok(out)
}

/// The sign, and the zero-padding that goes *inside* it.
fn signed_digits(value: i64, spec: &Spec) -> String {
    let sign = if value < 0 {
        "-"
    } else if spec.plus {
        "+"
    } else if spec.space {
        " "
    } else {
        ""
    };
    format!(
        "{sign}{}",
        pad_digits(&value.unsigned_abs().to_string(), spec)
    )
}

/// A precision on an integer conversion is a minimum digit count, not a
/// truncation.
fn pad_digits(digits: &str, spec: &Spec) -> String {
    match spec.precision {
        Some(precision) if digits.len() < precision => {
            format!("{}{digits}", "0".repeat(precision - digits.len()))
        }
        Some(0) if digits == "0" => String::new(),
        _ => digits.to_string(),
    }
}

fn floating(value: f64, conversion: u8, spec: &Spec) -> String {
    let precision = spec.precision.unwrap_or(6);
    let sign = if value.is_sign_negative() {
        "-"
    } else if spec.plus {
        "+"
    } else if spec.space {
        " "
    } else {
        ""
    };
    let magnitude = value.abs();
    let body = match conversion {
        b'f' | b'F' => format!("{magnitude:.precision$}"),
        b'e' => exponential(magnitude, precision, false),
        b'E' => exponential(magnitude, precision, true),
        b'a' | b'A' => format!("{magnitude:e}"),
        // `%g` drops trailing zeros and picks the shorter of the two forms,
        // which is the whole reason a program chose it.
        _ => {
            let exponent = if magnitude == 0.0 {
                0
            } else {
                magnitude.log10().floor() as i32
            };
            let significant = precision.max(1);
            let text = if exponent < -4 || exponent >= significant as i32 {
                exponential(magnitude, significant.saturating_sub(1), conversion == b'G')
            } else {
                let decimals = significant
                    .saturating_sub(1)
                    .saturating_sub(exponent.max(0) as usize);
                format!("{magnitude:.decimals$}")
            };
            trim_trailing_zeros(&text)
        }
    };
    format!("{sign}{body}")
}

fn exponential(magnitude: f64, precision: usize, upper: bool) -> String {
    let exponent = if magnitude == 0.0 {
        0
    } else {
        magnitude.log10().floor() as i32
    };
    let mantissa = if magnitude == 0.0 {
        0.0
    } else {
        magnitude / 10f64.powi(exponent)
    };
    let e = if upper { 'E' } else { 'e' };
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa:.precision$}{e}{sign}{:02}", exponent.abs())
}

fn trim_trailing_zeros(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], &text[at..]),
        None => (text, ""),
    };
    let trimmed = mantissa.trim_end_matches('0').trim_end_matches('.');
    format!("{trimmed}{exponent}")
}

/// Width padding. Zero-padding applies to numbers only, and never when the
/// field is left-aligned.
fn pad(body: &str, spec: &Spec, is_text: bool) -> String {
    let Some(width) = spec.width else {
        return body.to_string();
    };
    if body.len() >= width {
        return body.to_string();
    }
    let fill = width - body.len();
    if spec.left {
        format!("{body}{}", " ".repeat(fill))
    } else if spec.zero && !is_text && spec.precision.is_none() {
        // The zeros go after the sign or the `0x`, not before it.
        let split = body
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(0);
        let (prefix, digits) = body.split_at(split);
        format!("{prefix}{}{digits}", "0".repeat(fill))
    } else {
        format!("{}{body}", " ".repeat(fill))
    }
}
