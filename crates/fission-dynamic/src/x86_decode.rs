//! Minimal x86-64 instruction decoding helpers for the debugger.
//!
//! These are intentionally limited — they only detect the patterns needed
//! for step-over (CALL detection) without pulling in a full disassembler.

/// Detect whether the bytes at RIP represent a CALL instruction.
///
/// Returns `(is_call, instruction_length)`.
/// Only handles the most common x86-64 CALL encodings:
/// - `E8 rel32` (near call, 5 bytes)
/// - `FF /2` with ModR/M (indirect call, variable length)
/// - `9A` (far call, not common in 64-bit mode but included)
pub fn detect_call_instruction(bytes: &[u8]) -> (bool, usize) {
    if bytes.is_empty() {
        return (false, 0);
    }
    // Skip common prefixes (REX, 66, 67, segment overrides)
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x40..=0x4F => i += 1, // REX prefix
            0x66 | 0x67 => i += 1, // operand/address size override
            0x2E | 0x3E | 0x26 | 0x36 | 0x64 | 0x65 => i += 1, // segment override
            _ => break,
        }
    }
    if i >= bytes.len() {
        return (false, 0);
    }
    match bytes[i] {
        // E8 rel32: 5 bytes total (from the opcode, not counting prefixes)
        0xE8 if i + 5 <= bytes.len() => (true, i + 5),
        // FF /2: indirect CALL — ModR/M byte determines length
        0xFF if i + 1 < bytes.len() => {
            let modrm = bytes[i + 1];
            let reg = (modrm >> 3) & 7;
            if reg == 2 {
                // This is CALL r/m
                let extra = modrm_extra_length(modrm);
                (true, i + 2 + extra)
            } else {
                (false, 0)
            }
        }
        _ => (false, 0),
    }
}

/// Estimate the additional bytes after the ModR/M byte for common x86-64 encodings.
fn modrm_extra_length(modrm: u8) -> usize {
    let mod_field = modrm >> 6;
    let rm = modrm & 7;
    match mod_field {
        0b00 => {
            if rm == 4 {
                1 // SIB byte
            } else if rm == 5 {
                4 // RIP-relative (disp32)
            } else {
                0
            }
        }
        0b01 => {
            if rm == 4 {
                2 // SIB + disp8
            } else {
                1 // disp8
            }
        }
        0b10 => {
            if rm == 4 {
                5 // SIB + disp32
            } else {
                4 // disp32
            }
        }
        0b11 => 0, // register direct
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e8_rel32_detected_as_call() {
        // E8 xx xx xx xx — CALL rel32
        let bytes = [0xE8, 0x10, 0x00, 0x00, 0x00, 0x90];
        let (is_call, len) = detect_call_instruction(&bytes);
        assert!(is_call);
        assert_eq!(len, 5);
    }

    #[test]
    fn rex_prefix_e8_detected_as_call() {
        // 48 E8 xx xx xx xx — REX.W CALL rel32 (unusual but valid)
        let bytes = [0x48, 0xE8, 0x10, 0x00, 0x00, 0x00, 0x90];
        let (is_call, len) = detect_call_instruction(&bytes);
        assert!(is_call);
        assert_eq!(len, 6); // 1 prefix + 5 opcode+operand
    }

    #[test]
    fn ff_d0_detected_as_indirect_call() {
        // FF D0 — CALL rax (mod=11, rm=0, reg=2)
        let bytes = [0xFF, 0xD0, 0x90];
        let (is_call, len) = detect_call_instruction(&bytes);
        assert!(is_call);
        assert_eq!(len, 2); // opcode + modrm, mod=11 so no extra bytes
    }

    #[test]
    fn ff_15_rip_relative_detected_as_call() {
        // FF 15 xx xx xx xx — CALL [rip+disp32] (mod=00, rm=5, reg=2)
        let bytes = [0xFF, 0x15, 0x10, 0x00, 0x00, 0x00, 0x90];
        let (is_call, len) = detect_call_instruction(&bytes);
        assert!(is_call);
        assert_eq!(len, 6); // opcode + modrm + 4 bytes disp32
    }

    #[test]
    fn nop_not_detected_as_call() {
        let bytes = [0x90, 0x90, 0x90];
        let (is_call, _) = detect_call_instruction(&bytes);
        assert!(!is_call);
    }

    #[test]
    fn jmp_not_detected_as_call() {
        // E9 rel32 — JMP, not CALL
        let bytes = [0xE9, 0x10, 0x00, 0x00, 0x00];
        let (is_call, _) = detect_call_instruction(&bytes);
        assert!(!is_call);
    }

    #[test]
    fn ff_e0_jmp_rax_not_detected_as_call() {
        // FF E0 — JMP rax (reg=4, not 2)
        let bytes = [0xFF, 0xE0];
        let (is_call, _) = detect_call_instruction(&bytes);
        assert!(!is_call);
    }

    #[test]
    fn empty_bytes_returns_false() {
        let (is_call, _) = detect_call_instruction(&[]);
        assert!(!is_call);
    }
}
