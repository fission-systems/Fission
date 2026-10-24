//! Whole-function raw disassembly -- a single shared primitive for anything
//! that wants "address, bytes, decoded instruction text" per instruction of
//! a function, independent of the P-code/NIR/decompile pipeline.
//!
//! Extracted from `fission-cli`'s `oneshot::disasm`/`oneshot::raw_pcode`
//! (which were `pub(super)`-scoped to that CLI module, so nothing else could
//! call them) so the CLI's `disasm` subcommand and any other caller share
//! `/api/disasm/:session/:addr` handler can share one implementation instead
//! of the CLI reimplementing it and the server not having it at all.

use crate::pipeline::rust_sleigh::RustSleighDecompileConfig;
use crate::pipeline::rust_sleigh::bounds::{clamp_to_available_execution, next_function_distance};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::packed_context::PackedContextOverride;
use fission_sleigh::runtime::{DecodeContract, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;

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
    fission_static::analysis::decode_context_for_address(binary, frontend, from_address)
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

/// Compute the bounded `(max_bytes, instruction_limit)` window for `lift_function`.
fn compute_lift_window(
    binary: &LoadedBinary,
    func_start: u64,
    function_size: usize,
) -> (usize, usize) {
    let config = RustSleighDecompileConfig::cli_defaults();
    let max_bytes_limit = config.decode_max_bytes_cap.max(1);
    let fallback_default_bytes = config.default_decode_bytes.max(1).min(max_bytes_limit);
    let max_bytes = if function_size > 0 {
        function_size.min(max_bytes_limit)
    } else {
        next_function_distance(binary, func_start)
            .map(|dist| dist.min(max_bytes_limit))
            .unwrap_or(fallback_default_bytes)
    }
    .max(1);
    let max_bytes = clamp_to_available_execution(binary, func_start, max_bytes).max(1);
    let instruction_limit = config
        .instruction_budget_default
        .max(max_bytes.min(config.instruction_budget_cap.max(1)))
        .max(1)
        .min(config.instruction_budget_cap.max(1));
    (max_bytes, instruction_limit)
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
    let function_size = usize::try_from(func.size).unwrap_or(0);
    let (max_bytes, instruction_limit) = compute_lift_window(binary, func_start, function_size);

    let frontend = runtime_frontend_for_binary(binary)?;
    let address_state = frontend.normalize_low_bit_code_address(func_start);
    let decode_addr = address_state.address;
    let context_override =
        decode_context_for_address(binary, &frontend, address_state.context_override);
    let bytes = binary
        .view_bytes(decode_addr, max_bytes)
        .ok_or_else(|| format!("unable to read bytes at 0x{decode_addr:x}"))?;
    let mut memory_context = decode_memory_context_for(binary, decode_addr, max_bytes);
    let lift_contract = DecodeContract::decomp_function(instruction_limit);
    let mut result = frontend.lift_raw_pcode_function_with_context_and_memory_context(
        bytes,
        decode_addr,
        lift_contract,
        &memory_context,
        context_override,
    );
    let mut last_good = result.as_ref().ok().cloned();
    let mut discovered_edges: std::collections::BTreeMap<u64, Vec<u64>> = Default::default();
    let little_endian = binary.is_little_endian();
    const MAX_DECODE_ROUNDS: usize = 8;
    for _ in 0..MAX_DECODE_ROUNDS {
        let Ok(lifted) = result.as_ref() else {
            break;
        };
        let discovered = fission_sleigh::runtime::resolve_indirect_branch_targets(
            &lifted.function,
            decode_addr,
            bytes,
            &memory_context,
            little_endian,
        );
        let fresh: Vec<u64> = discovered
            .values()
            .flatten()
            .copied()
            .filter(|target| !memory_context.additional_decode_entries.contains(target))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        for (source, targets) in discovered {
            discovered_edges.entry(source).or_default().extend(targets);
        }
        if fresh.is_empty() {
            break;
        }
        memory_context.additional_decode_entries.extend(fresh);
        let next = frontend.lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_addr,
            lift_contract,
            &memory_context,
            context_override,
        );
        if let Ok(better) = &next {
            last_good = Some(better.clone());
        }
        result = next;
    }
    if !discovered_edges.is_empty() {
        if let Ok(lifted) = &mut result {
            fission_sleigh::runtime::attach_inferred_indirect_edges(
                &mut lifted.function,
                &discovered_edges,
            );
        }
        if let Some(good) = &mut last_good {
            fission_sleigh::runtime::attach_inferred_indirect_edges(
                &mut good.function,
                &discovered_edges,
            );
        }
    }
    let lifted = match result {
        Ok(lifted) => lifted,
        Err(err) => last_good.ok_or_else(|| err.to_string())?,
    };

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

    #[test]
    fn compute_lift_window_decouples_instruction_limit_and_clamps_cross_section_and_thumb() {
        use super::compute_lift_window;
        use fission_core::architecture::{
            BinaryLoadSpec, CompilerSpecId, GhidraLanguageId, LanguageCompilerSpecPair,
        };
        use fission_loader::loader::types::{
            DataBuffer, FunctionInfo, LoadedBinaryBuilder, SectionInfo,
        };

        let load_spec = BinaryLoadSpec {
            format: "ELF".to_string(),
            image_base: 0x8000000,
            pair: LanguageCompilerSpecPair {
                language_id: GhidraLanguageId("ARM:LE:32:v8".to_string()),
                compiler_spec_id: CompilerSpecId("default".to_string()),
            },
            preferred: true,
            source: "test".to_string(),
        };

        let binary =
            LoadedBinaryBuilder::new("test.elf".to_string(), DataBuffer::Heap(vec![0u8; 0x2000]))
                .load_spec(load_spec)
                .image_base(0x8000000)
                .is_64bit(false)
                .add_section(SectionInfo {
                    name: ".text".to_string(),
                    virtual_address: 0x8000000,
                    virtual_size: 0x100,
                    file_offset: 0,
                    file_size: 0x100,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                // Aligned symbol and Thumb-tagged alias for the same function:
                .add_function(FunctionInfo {
                    name: "thumb_fn_aligned".to_string(),
                    address: 0x8000010,
                    ..Default::default()
                })
                .add_function(FunctionInfo {
                    name: "thumb_fn".to_string(),
                    address: 0x8000011,
                    ..Default::default()
                })
                // Far symbol in another section (1 MB away):
                .add_function(FunctionInfo {
                    name: "far_sym".to_string(),
                    address: 0x8100000,
                    ..Default::default()
                })
                .build()
                .expect("build");

        // 1. Small explicit size (16 bytes) still gets at least instruction_budget_default (512),
        //    not a 16-instruction limit.
        let (small_bytes, small_insts) = compute_lift_window(&binary, 0x8000010, 16);
        assert_eq!(small_bytes, 16);
        assert!(small_insts >= 512);

        // 2. Zero size skips the +1 Thumb alias at 0x8000011 and clamps to the end of `.text`
        //    (0x8000100 - 0x8000010 = 0xf0), never inflating across sections to 0x8100000.
        let (unknown_bytes, unknown_insts) = compute_lift_window(&binary, 0x8000010, 0);
        assert_eq!(unknown_bytes, 0xf0);
        assert!(unknown_insts >= 512 && unknown_insts <= 4096);
    }
}
