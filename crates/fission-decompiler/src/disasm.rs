//! Whole-function raw disassembly -- a single shared primitive for anything
//! that wants "address, bytes, decoded instruction text" per instruction of
//! a function, independent of the P-code/NIR/decompile pipeline.
//!
//! Extracted from `fission-cli`'s `oneshot::disasm`/`oneshot::raw_pcode`
//! (which were `pub(super)`-scoped to that CLI module, so nothing else could
//! call them) so both the CLI's `disasm` subcommand and `fission-serve`'s
//! `/api/disasm/:session/:addr` handler can share one implementation instead
//! of the CLI reimplementing it and the server not having it at all.

use fission_core::PAGE_SIZE;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::packed_context::PackedContextOverride;
use fission_sleigh::runtime::{DecodeContract, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;
use fission_static::analysis::image_executes_thumb;

/// One decoded instruction, ready for direct GUI/API display.
#[derive(Debug, Clone)]
pub struct InstructionRow {
    pub address: u64,
    /// Space-separated lowercase hex byte pairs, e.g. "48 89 e5".
    pub bytes_hex: String,
    /// Decoded mnemonic + operands, e.g. "mov rbp, rsp".
    pub text: String,
    /// Direct call/branch target, when the instruction has one -- lets a
    /// caller (e.g. the GUI's click-to-navigate) jump there without having
    /// to parse `text`.
    pub target_addr: Option<u64>,
    /// What the addresses in this instruction name, when the loader can say:
    /// an import's symbol, a function's name, the string at that address.
    ///
    /// Computed here rather than by each caller so the CLI listing, the HTTP
    /// API and the GUI all show the same thing.
    pub refers_to: Option<String>,
}

/// The decode context for a bare address window, once a Thumb-only image is
/// accounted for. Same reasoning as `thumb_image_context`, exposed for
/// callers that decode a fixed instruction count rather than a whole
/// function.
pub fn decode_context_for_address(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    from_address: Option<PackedContextOverride>,
) -> Option<PackedContextOverride> {
    thumb_image_context(binary, frontend, from_address)
}

/// The decode context to lift with, once a Thumb-only image is accounted for.
///
/// `normalize_low_bit_code_address` can only speak for the *address*: it
/// yields a Thumb context when the caller passed the ABI's bit-0 marker, and
/// nothing when the address is even. That leaves every even-addressed
/// function of a Cortex-M image -- which cannot execute ARM at all -- being
/// lifted in the language's default ARM mode, decoding Thumb bytes into
/// dense, plausible, entirely wrong ARM instructions (`80 b5`, a
/// `push {r7,lr}`, reads as `addlt`). Fall back to the whole-image signal
/// the discovery pass already relies on, which is decisive exactly when the
/// per-address one has nothing to say.
fn thumb_image_context(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    from_address: Option<PackedContextOverride>,
) -> Option<PackedContextOverride> {
    if from_address.is_some() {
        return from_address;
    }
    if !image_executes_thumb(binary) {
        return None;
    }
    frontend.low_bit_code_mode_override()
}

fn runtime_frontend_for_binary(binary: &LoadedBinary) -> Result<RuntimeSleighFrontend, String> {
    let load_spec = binary
        .load_spec()
        .ok_or_else(|| format!("missing Ghidra load spec for '{}'", binary.path))?;
    RuntimeSleighFrontend::new_for_load_spec(load_spec).map_err(|e| e.to_string())
}

/// Disassemble the whole function at `addr` (from its resolved start, not
/// necessarily `addr` itself). Falls back to guessing the function's size
/// from the next-higher known function address when the loader didn't
/// record one (e.g. a function discovered without a symbol-table size).
/// The raw p-code of one function, lifted exactly as `disassemble_function`
/// lifts it.
///
/// Same decode contract, same Thumb-image handling, same memory context -- the
/// two differ only in what they hand back. Anything that wants to reason about
/// the lifted semantics rather than print instructions (DIR's verification
/// path, coverage measurement) needs the function, not the instruction rows.
pub fn raw_pcode_function(
    binary: &LoadedBinary,
    addr: u64,
) -> Result<fission_pcode::PcodeFunction, String> {
    lift_function(binary, addr).map(|lifted| lifted.function)
}

/// The lift both public entry points share: locate the function, size the
/// decode window, resolve the Thumb-image context, and lift.
fn lift_function(
    binary: &LoadedBinary,
    addr: u64,
) -> Result<fission_sleigh::runtime::DecodedPcodeFunction, String> {
    let func = binary
        .function_at(addr)
        .ok_or_else(|| format!("No function found at address 0x{addr:x}"))?;
    let func_start = func.address;
    let mut func_size = func.size;
    if func_size == 0 {
        func_size = binary
            .functions
            .iter()
            .filter(|f| f.address > func_start)
            .map(|f| f.address)
            .min()
            .map(|next_addr| next_addr - func_start)
            .unwrap_or(PAGE_SIZE as u64);
    }
    let max_bytes = usize::try_from(func_size).unwrap_or(PAGE_SIZE).max(1);

    let frontend = runtime_frontend_for_binary(binary)?;
    let address_state = frontend.normalize_low_bit_code_address(func_start);
    let decode_addr = address_state.address;
    let context_override = thumb_image_context(binary, &frontend, address_state.context_override);
    let max_bytes = binary
        .available_execution_bytes(decode_addr)
        .map(|available| max_bytes.min(available).max(1))
        .unwrap_or(max_bytes.max(1));
    let bytes = binary
        .view_bytes(decode_addr, max_bytes)
        .ok_or_else(|| format!("unable to read bytes at 0x{decode_addr:x}"))?;
    let memory_context = decode_memory_context_for(binary, decode_addr, max_bytes);
    let lifted = frontend
        .lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_addr,
            DecodeContract::strict_function(max_bytes),
            &memory_context,
            context_override,
        )
        .map_err(|e| e.to_string())?;

    Ok(lifted)
}

pub fn disassemble_function(
    binary: &LoadedBinary,
    addr: u64,
) -> Result<Vec<InstructionRow>, String> {
    let lifted = lift_function(binary, addr)?;
    let mut rows = lifted
        .instructions
        .iter()
        .map(|instruction| InstructionRow {
            address: instruction.address,
            bytes_hex: instruction
                .bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" "),
            text: instruction.instruction_text(),
            target_addr: instruction.direct_target,
            refers_to: None,
        })
        .collect::<Vec<_>>();
    annotate_rows(binary, &mut rows);
    Ok(rows)
}

/// Annotate a whole function's rows, carrying the state a pair needs.
///
/// AArch64 builds an address from two instructions -- `adrp` loads the page,
/// a following `add`/`ldr`/`str` adds the offset -- so neither instruction
/// alone names anything. Annotating row by row saw only the page, which is not
/// an address anyone asked about: of sixty `adrp` in a test binary, four
/// carried a note and none of the instructions that completed them did.
fn annotate_rows(binary: &LoadedBinary, rows: &mut [InstructionRow]) {
    let mut pages: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for row in rows.iter_mut() {
        row.refers_to = annotate(binary, row).or_else(|| {
            adrp_pair_target(&row.text, &pages).and_then(|target| {
                describe_address(binary, target).or_else(|| Some(format!("0x{target:x}")))
            })
        });
        update_adrp_pages(&row.text, &mut pages);
    }
}

/// What the addresses in one instruction refer to, for the comment column.
///
/// A listing that prints `call 0x140002870` and `lea RDX,[0x140004178]` makes
/// the reader look up both by hand, and the second one -- a string -- is
/// usually the line that says what the function is for. Every name here is
/// already in the loader's tables; the listing just never asked.
fn annotate(binary: &LoadedBinary, row: &InstructionRow) -> Option<String> {
    // A branch target is the instruction's subject, so it wins over any
    // address that happens to appear in the operands.
    if let Some(target) = row.target_addr
        && let Some(label) = describe_address(binary, target)
    {
        return Some(label);
    }
    operand_addresses(&row.text)
        .into_iter()
        .filter(|address| Some(*address) != row.target_addr)
        .find_map(|address| describe_address(binary, address))
}

/// Hex literals appearing in an instruction's operand text.
fn operand_addresses(text: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(found) = text[i..].find("0x") {
        let start = i + found + 2;
        let mut end = start;
        while end < bytes.len() && (bytes[end] as char).is_ascii_hexdigit() {
            end += 1;
        }
        if end > start
            && let Ok(value) = u64::from_str_radix(&text[start..end], 16)
        {
            out.push(value);
        }
        i = end.max(i + found + 2);
    }
    out
}

/// The most specific thing the loader can say about one address.
fn describe_address(binary: &LoadedBinary, address: u64) -> Option<String> {
    if let Some(symbol) = binary.iat_symbols.get(&address) {
        return Some(symbol.clone());
    }
    if let Some(function) = binary.function_at_exact(address)
        && !function.name.is_empty()
    {
        return Some(function.name.clone());
    }
    if let Some(text) = binary.string_map.get(&address) {
        let escaped = text.escape_default().to_string();
        let shown = if escaped.chars().count() > 48 {
            format!("{}...", escaped.chars().take(45).collect::<String>())
        } else {
            escaped
        };
        return Some(format!("\"{shown}\""));
    }
    binary.global_symbols.get(&address).cloned()
}

/// The address an `adrp` page and this instruction's offset add up to.
///
/// Only the forms that really complete an address: `add rd, rn, #imm` and the
/// load/store family's `[rn, #imm]`. `and`/`asr`/`ubfiz` take the same operand
/// shape without meaning an address, and reading them as one invents targets.
fn adrp_pair_target(text: &str, pages: &std::collections::HashMap<String, u64>) -> Option<u64> {
    let mut words = text.split_whitespace();
    let mnemonic = words.next()?;
    let is_memory = matches!(
        mnemonic,
        "ldr" | "ldrb" | "ldrh" | "ldrsb" | "ldrsh" | "ldrsw" | "str" | "strb" | "strh"
    );
    if !is_memory && mnemonic != "add" {
        return None;
    }
    let rest = text[mnemonic.len()..].trim();
    // `add x21, x19, #0x9f0` and `ldr x0, [x0, #0xc18]` both put the base
    // register immediately before the immediate.
    let (base, offset) = rest.rsplit_once(",")?;
    let base = base.rsplit(['[', ',']).next()?.trim().trim_matches(']');
    let page = pages.get(base)?;
    let offset = offset.trim().trim_matches(']').trim_matches('!').trim();
    let offset = offset.strip_prefix('#')?;
    let offset = if let Some(hex) = offset.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()?
    } else {
        offset.parse::<u64>().ok()?
    };
    page.checked_add(offset)
}

/// Track which registers hold an `adrp` page, and forget ones overwritten.
fn update_adrp_pages(text: &str, pages: &mut std::collections::HashMap<String, u64>) {
    let mut words = text.split_whitespace();
    let Some(mnemonic) = words.next() else {
        return;
    };
    if mnemonic == "adrp" {
        let rest = text[mnemonic.len()..].trim();
        if let Some((register, page)) = rest.split_once(',')
            && let Some(hex) = page.trim().strip_prefix("0x")
            && let Ok(page) = u64::from_str_radix(hex, 16)
        {
            pages.insert(register.trim().to_string(), page);
        }
        return;
    }
    // A store writes memory, not its first operand; everything else here
    // writes it, and whatever it writes is no longer a page.
    if mnemonic.starts_with("str") {
        return;
    }
    if let Some(destination) = text[mnemonic.len()..]
        .trim()
        .split(',')
        .next()
        .map(|word| word.trim().trim_matches('[').trim_matches(']'))
    {
        pages.remove(destination);
    }
}

#[cfg(test)]
mod adrp_pair_tests {
    use super::{adrp_pair_target, update_adrp_pages};
    use std::collections::HashMap;

    fn pages_after(instructions: &[&str]) -> HashMap<String, u64> {
        let mut pages = HashMap::new();
        for text in instructions {
            update_adrp_pages(text, &mut pages);
        }
        pages
    }

    /// AArch64 builds an address from two instructions, so neither alone names
    /// anything: `adrp` carries the page, the next one the offset.
    #[test]
    fn a_page_and_an_offset_make_an_address() {
        let pages = pages_after(&["adrp x19, 0x492000"]);
        assert_eq!(
            adrp_pair_target("add x21, x19, #0x9f0", &pages),
            Some(0x4929f0)
        );
        let pages = pages_after(&["adrp x0, 0x48f000"]);
        assert_eq!(
            adrp_pair_target("ldr x0, [x0, #0xc18]", &pages),
            Some(0x48fc18)
        );
    }

    /// `and`, `asr` and `ubfiz` take the same operand shape without meaning an
    /// address. Reading them as one invents targets -- they were 17 of the 300
    /// instructions a first pass matched.
    #[test]
    fn operand_shape_alone_is_not_an_address() {
        let pages = pages_after(&["adrp x19, 0x492000"]);
        assert_eq!(adrp_pair_target("and x21, x19, #0xff", &pages), None);
        assert_eq!(adrp_pair_target("asr x21, x19, #0x2", &pages), None);
        assert_eq!(
            adrp_pair_target("ubfiz x21, x19, #0x3, #0x20", &pages),
            None
        );
    }

    #[test]
    fn a_register_stops_being_a_page_once_it_is_overwritten() {
        let pages = pages_after(&["adrp x0, 0x48f000", "mov x0, sp"]);
        assert_eq!(adrp_pair_target("ldr x1, [x0, #0x8]", &pages), None);
        // A store writes memory, not its first operand, so the page survives.
        let pages = pages_after(&["adrp x0, 0x48f000", "str x0, [sp, #0x20]"]);
        assert_eq!(
            adrp_pair_target("ldr x1, [x0, #0x8]", &pages),
            Some(0x48f008)
        );
    }

    #[test]
    fn an_architecture_without_adrp_never_pairs() {
        let pages = pages_after(&["mov RBP,RSP", "sub RSP,0x78"]);
        assert!(pages.is_empty());
        assert_eq!(adrp_pair_target("add RAX, RDX, #0x10", &pages), None);
    }
}
