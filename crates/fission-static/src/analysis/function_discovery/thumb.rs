//! Whether an ARM image executes Thumb code, and how to know without symbols.
//!
//! ARM's ABI marks Thumb by setting bit 0 of a *function symbol's* value, and
//! the loader clears that bit when it reads one. On a stripped Cortex-M image
//! there are no function symbols left to read it from, so a linear sweep
//! starting at an even section address decodes ARM-mode instructions over
//! Thumb bytes. It does not fail -- ARM has an encoding for almost every
//! 32-bit word -- it silently produces plausible nonsense, and the sweep
//! finds no call or jump targets at all in it. `libopencm3/mandel`'s
//! `push {r7}` (`80 b4`) reads as `addlt r11,r3,r0, lsl #0x9` that way.

use fission_loader::loader::LoadedBinary;

/// Number of exception-vector entries examined after the initial stack
/// pointer. Cortex-M's table is longer, but the first handfulNMI, HardFault
/// and friends -- are populated by every toolchain's startup file.
const EXAMINED_VECTORS: usize = 7;

/// ARMv7-M places SRAM at `0x2000_0000`; the reset vector's initial stack
/// pointer points into it.
const SRAM_REGION: u64 = 0x2000_0000;
const SRAM_REGION_MASK: u64 = 0xF000_0000;

/// Whether this binary's code is Thumb.
///
/// Two independent signals, either of which is decisive:
///
/// 1. The entry point carries the ABI's bit-0 Thumb marker. Toolchains
///    normally emit it, but not always -- crazyflie's images enter at an
///    even `0x8004_1c0` while being Cortex-M throughout.
/// 2. The image opens with a Cortex-M vector table: initial stack pointer
///    into SRAM, followed by exception handlers that all carry the Thumb
///    bit. This is what separates a Cortex-M image from a Cortex-A one --
///    u-boot's ARM-mode image opens with a branch instruction
///    (`0xea0000b8`) and no Thumb-marked words follow it.
pub fn image_executes_thumb(binary: &LoadedBinary) -> bool {
    if !is_arm32(binary) {
        return false;
    }
    if binary.entry_point & 1 == 1 {
        return true;
    }
    has_cortex_m_vector_table(binary)
}

fn is_arm32(binary: &LoadedBinary) -> bool {
    binary
        .architecture
        .as_ref()
        .is_some_and(|arch| arch.processor == "ARM" && !binary.is_64bit)
}

fn has_cortex_m_vector_table(binary: &LoadedBinary) -> bool {
    let Some(words) = vector_table_words(binary) else {
        return false;
    };
    let (initial_sp, handlers) = words.split_first().expect("non-empty by construction");
    if initial_sp & SRAM_REGION_MASK != SRAM_REGION {
        return false;
    }
    // A populated handler is a Thumb function address; an unused slot is
    // zero. Requiring every *populated* one to be Thumb-marked keeps a table
    // with reserved gaps (they are common) from reading as ambiguous, while
    // still refusing a table with no populated entries at all.
    let populated: Vec<u64> = handlers.iter().copied().filter(|word| *word != 0).collect();
    !populated.is_empty() && populated.iter().all(|word| word & 1 == 1)
}

/// The first `EXAMINED_VECTORS + 1` words at the lowest loaded address.
///
/// The table is looked for in the lowest *loaded* section, not the lowest
/// executable one. A Cortex-M vector table is a table of addresses, so
/// toolchains mark it allocated and not executable -- crazyflie's
/// `.isr_vector` is `A` while `.text` is `AX` -- and starting from the
/// lowest executable section reads the first instructions of `.text`
/// instead, which look nothing like a vector table.
fn vector_table_words(binary: &LoadedBinary) -> Option<Vec<u64>> {
    let section = binary
        .sections
        .iter()
        .filter(|section| section.virtual_address != 0 && section.file_size >= 4)
        .min_by_key(|section| section.virtual_address)?;
    let start = section.file_offset as usize;
    let needed = (EXAMINED_VECTORS + 1) * 4;
    let data = binary.data.as_slice();
    let end = start.checked_add(needed)?;
    if end > data.len() {
        return None;
    }
    Some(
        data[start..end]
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]) as u64)
            .collect(),
    )
}
