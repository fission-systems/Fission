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
use fission_sleigh::runtime::{PackedContextOverride, RuntimeSleighFrontend};

/// The decode context to lift an address with, once a Thumb-only image is
/// accounted for.
///
/// `normalize_low_bit_code_address` can only speak for the *address*: it
/// yields a Thumb context when the caller passed the ABI's bit-0 marker, and
/// nothing when the address is even. Every path that stopped there lifted a
/// Cortex-M image's even-addressed functions in the language's default ARM
/// mode -- and those are all of them, because the marker went with the
/// symbols. `disasm` and function discovery each grew their own copy of this
/// fallback; `raw-pcode`, `similar`, and the decomp fact cache did not, so
/// they failed to decode the very addresses `list` had just handed out.
///
/// One primitive, so a path cannot be written that forgets it.
pub fn decode_context_for_address(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    from_address: Option<PackedContextOverride>,
) -> Option<PackedContextOverride> {
    // What the address itself says always wins: a mode switch encoded in the
    // stream is more specific than a whole-image guess.
    if from_address.is_some() {
        return from_address;
    }
    if !image_executes_thumb(binary) {
        return None;
    }
    frontend.low_bit_code_mode_override()
}

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

#[cfg(test)]
mod tests {
    use super::{decode_context_for_address, image_executes_thumb};
    use fission_core::architecture::ArchitectureDescriptor;
    use fission_loader::loader::{DataBuffer, LoadedBinary, LoadedBinaryBuilder};
    use fission_sleigh::runtime::RuntimeSleighFrontend;

    fn arm32_image(entry_point: u64) -> LoadedBinary {
        LoadedBinaryBuilder::new("unit.bin".to_string(), DataBuffer::Heap(vec![0; 0x100]))
            .format("ELF")
            .entry_point(entry_point)
            .arch_spec("ARM:LE:32:v8")
            .architecture(ArchitectureDescriptor {
                processor: "ARM".to_string(),
                endian: "little".to_string(),
                bitness: 32,
                variant: "v8".to_string(),
                abi: None,
                raw_machine: "EM_ARM".to_string(),
            })
            .build()
            .expect("ARM32 shell")
    }

    #[test]
    fn an_even_address_in_a_thumb_image_still_lifts_as_thumb() {
        let frontend = RuntimeSleighFrontend::new_for_language("ARM8_le").expect("ARM8 runtime");

        // A stripped Cortex-M image: the entry keeps the ABI's bit-0 marker,
        // every other address lost it with the symbols. Passing `None` is
        // what every even address produces, and the answer must still be
        // Thumb -- `raw-pcode`, `similar`, and the decomp fact cache each
        // stopped here and lifted ARM over Thumb bytes, failing to decode the
        // very addresses `list` had just printed.
        let thumb = arm32_image(0x4429);
        assert!(image_executes_thumb(&thumb));
        assert!(decode_context_for_address(&thumb, &frontend, None).is_some());

        // And an ARM-mode image is not forced into Thumb by the same call.
        let arm = arm32_image(0x4428);
        assert!(!image_executes_thumb(&arm));
        assert!(decode_context_for_address(&arm, &frontend, None).is_none());
    }

    #[test]
    fn the_address_s_own_mode_outranks_the_image_s() {
        let frontend = RuntimeSleighFrontend::new_for_language("ARM8_le").expect("ARM8 runtime");
        let from_address = frontend
            .low_bit_code_mode_override()
            .expect("ARM has a Thumb context field");
        // A mode the address carries is more specific than a whole-image
        // guess, so it is returned unchanged -- including on an image the
        // whole-image signal calls ARM.
        let arm = arm32_image(0x4428);
        assert_eq!(
            decode_context_for_address(&arm, &frontend, Some(from_address)),
            Some(from_address)
        );
    }
}
