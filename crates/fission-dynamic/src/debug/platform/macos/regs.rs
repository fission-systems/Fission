//! Mapping between a Mach thread-state buffer and Fission's `RegisterState`.
//!
//! The buffer layouts are those of `<mach/arm/thread_status.h>` and
//! `<mach/i386/thread_status.h>`. They are pure functions of a word buffer, so
//! they are tested here without a process.

use fission_ttd::RegisterState;

/// 32-bit words in a thread-state buffer, and the Mach flavor that fills it.
#[cfg(target_arch = "aarch64")]
pub const STATE_WORDS: usize = 68;
#[cfg(target_arch = "aarch64")]
pub const THREAD_STATE_FLAVOR: i32 = 6; // ARM_THREAD_STATE64

#[cfg(target_arch = "x86_64")]
pub const STATE_WORDS: usize = 42;
#[cfg(target_arch = "x86_64")]
pub const THREAD_STATE_FLAVOR: i32 = 4; // x86_THREAD_STATE64

/// The instruction a software breakpoint is made of.
#[cfg(target_arch = "aarch64")]
pub const BREAKPOINT_INSTRUCTION: &[u8] = &0xd420_0000u32.to_le_bytes(); // BRK #0
#[cfg(target_arch = "x86_64")]
pub const BREAKPOINT_INSTRUCTION: &[u8] = &[0xcc]; // INT3

/// How far past the breakpoint address the PC is when the trap is reported.
/// `BRK` reports at the instruction; `INT3` has already executed one byte.
#[cfg(target_arch = "aarch64")]
pub const PC_ADVANCE_AFTER_BREAKPOINT: u64 = 0;
#[cfg(target_arch = "x86_64")]
pub const PC_ADVANCE_AFTER_BREAKPOINT: u64 = 1;

/// The Mach flavor that reports why a thread last trapped, and its size in
/// 32-bit words.
#[cfg(target_arch = "aarch64")]
pub const EXCEPTION_STATE_FLAVOR: i32 = 7; // ARM_EXCEPTION_STATE64
#[cfg(target_arch = "x86_64")]
pub const EXCEPTION_STATE_FLAVOR: i32 = 6; // x86_EXCEPTION_STATE64
pub const EXCEPTION_STATE_WORDS: usize = 4;

/// Whether an exception-state buffer says the thread's last trap was a
/// breakpoint instruction.
///
/// * aarch64 (`far: u64, esr: u32, exception: u32`): the exception class in
///   `esr[31:26]` is `0x3C`, "BRK instruction execution in AArch64 state".
/// * x86-64 (`trapno: u16, cpu: u16, err: u32, faultvaddr: u64`): trap number
///   3, `#BP`.
pub fn trapped_on_breakpoint(exception_words: &[u32]) -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        exception_words[2] >> 26 == 0x3C
    }
    #[cfg(target_arch = "x86_64")]
    {
        exception_words[0] & 0xffff == 3
    }
}

/// The Mach flavor that carries a thread's single-step switch, and its size.
///
/// On aarch64 that is the debug state, whose last field is `mdscr_el1`; bit 0
/// (`SS`) arms a software step. On x86-64 it is the trap flag in `rflags`,
/// part of the ordinary thread state.
#[cfg(target_arch = "aarch64")]
pub const SINGLE_STEP_STATE_FLAVOR: i32 = 15; // ARM_DEBUG_STATE64
#[cfg(target_arch = "aarch64")]
pub const SINGLE_STEP_STATE_WORDS: usize = 130; // bvr, bcr, wvr, wcr: 16 x u64 each, then mdscr_el1
#[cfg(target_arch = "x86_64")]
pub const SINGLE_STEP_STATE_FLAVOR: i32 = THREAD_STATE_FLAVOR;
#[cfg(target_arch = "x86_64")]
pub const SINGLE_STEP_STATE_WORDS: usize = STATE_WORDS;

/// Arm or disarm the single step in a buffer of [`SINGLE_STEP_STATE_FLAVOR`].
pub fn set_single_step(words: &mut [u32], on: bool) {
    #[cfg(target_arch = "aarch64")]
    let (index, bit) = (128, 1u32); // mdscr_el1.SS
    #[cfg(target_arch = "x86_64")]
    let (index, bit) = (136 / 4, 0x100u32); // rflags.TF
    if on {
        words[index] |= bit;
    } else {
        words[index] &= !bit;
    }
}

fn word_at(words: &[u32], byte_offset: usize) -> u64 {
    let index = byte_offset / 4;
    u64::from(words[index]) | (u64::from(words[index + 1]) << 32)
}

fn put_word(words: &mut [u32], byte_offset: usize, value: u64) {
    let index = byte_offset / 4;
    words[index] = value as u32;
    words[index + 1] = (value >> 32) as u32;
}

/// `x0`..`x28`, then `fp`, `lr`, `sp`, `pc` as 64-bit fields, then `cpsr`.
#[cfg(target_arch = "aarch64")]
pub fn to_registers(words: &[u32]) -> RegisterState {
    use fission_ttd::aarch64_register_name;
    let pc = word_at(words, 256);
    let mut state = RegisterState::at(pc);
    for number in 0..=28u32 {
        state.set(
            aarch64_register_name(number).expect("x0..x28 are named"),
            word_at(words, number as usize * 8),
        );
    }
    state.set(aarch64_register_name(29).expect("x29"), word_at(words, 232));
    state.set(aarch64_register_name(30).expect("x30"), word_at(words, 240));
    state.set(aarch64_register_name(31).expect("sp"), word_at(words, 248));
    state.set(aarch64_register_name(32).expect("pc"), pc);
    state.set(
        aarch64_register_name(33).expect("cpsr"),
        u64::from(words[66]),
    );
    state
}

/// Write the registers `state` carries into `words`, leaving the rest as read.
#[cfg(target_arch = "aarch64")]
pub fn apply_registers(words: &mut [u32], state: &RegisterState) {
    use fission_ttd::aarch64_register_name;
    for number in 0..=28u32 {
        if let Some(value) = aarch64_register_name(number).and_then(|name| state.get(name)) {
            put_word(words, number as usize * 8, value);
        }
    }
    for (number, offset) in [(29u32, 232usize), (30, 240), (31, 248)] {
        if let Some(value) = aarch64_register_name(number).and_then(|name| state.get(name)) {
            put_word(words, offset, value);
        }
    }
    // `pc` is both a named register and the state's program counter; the
    // field a caller changed last is the one it set, so honour either.
    let named_pc = aarch64_register_name(32).and_then(|name| state.get(name));
    put_word(words, 256, named_pc.unwrap_or(state.pc));
    if let Some(value) = aarch64_register_name(33).and_then(|name| state.get(name)) {
        words[66] = value as u32;
    }
}

/// `rax rbx rcx rdx rdi rsi rbp rsp r8..r15 rip rflags cs fs gs`.
#[cfg(target_arch = "x86_64")]
const X86_FIELDS: [(&str, usize); 18] = [
    ("RAX", 0),
    ("RBX", 8),
    ("RCX", 16),
    ("RDX", 24),
    ("RDI", 32),
    ("RSI", 40),
    ("RBP", 48),
    ("RSP", 56),
    ("R8", 64),
    ("R9", 72),
    ("R10", 80),
    ("R11", 88),
    ("R12", 96),
    ("R13", 104),
    ("R14", 112),
    ("R15", 120),
    ("RIP", 128),
    ("RFLAGS", 136),
];

#[cfg(target_arch = "x86_64")]
pub fn to_registers(words: &[u32]) -> RegisterState {
    let mut state = RegisterState::at(word_at(words, 128));
    for (name, offset) in X86_FIELDS {
        state.set(name, word_at(words, offset));
    }
    state
}

#[cfg(target_arch = "x86_64")]
pub fn apply_registers(words: &mut [u32], state: &RegisterState) {
    for (name, offset) in X86_FIELDS {
        let value = if name == "RIP" {
            state.get("RIP").unwrap_or(state.pc)
        } else {
            match state.get(name) {
                Some(value) => value,
                None => continue,
            }
        };
        put_word(words, offset, value);
    }
}

/// The program counter in a thread-state buffer.
pub fn program_counter(words: &[u32]) -> u64 {
    #[cfg(target_arch = "aarch64")]
    {
        word_at(words, 256)
    }
    #[cfg(target_arch = "x86_64")]
    {
        word_at(words, 128)
    }
}

/// Set the program counter in a thread-state buffer.
pub fn set_program_counter(words: &mut [u32], value: u64) {
    #[cfg(target_arch = "aarch64")]
    put_word(words, 256, value);
    #[cfg(target_arch = "x86_64")]
    put_word(words, 128, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_survive_a_trip_through_the_word_buffer() {
        let mut words = vec![0u32; STATE_WORDS];
        let mut state = to_registers(&words);
        #[cfg(target_arch = "aarch64")]
        {
            state.set("X0", 0x1122_3344_5566_7788);
            state.set("X28", 28);
            state.set("X29", 29);
            state.set("X30", 30);
            state.set("SP", 0x7ffe_0000);
            state.set("PC", 0x1_0000_3f80);
            state.set("CPSR", 0x6000_0000);
        }
        #[cfg(target_arch = "x86_64")]
        {
            state.set("RAX", 0x1122_3344_5566_7788);
            state.set("R15", 15);
            state.set("RSP", 0x7ffe_0000);
            state.set("RIP", 0x1_0000_3f80);
            state.set("RFLAGS", 0x246);
        }
        apply_registers(&mut words, &state);
        let back = to_registers(&words);
        for (name, value) in state.iter() {
            assert_eq!(back.get(name), Some(value), "{name}");
        }
        assert_eq!(program_counter(&words), 0x1_0000_3f80);
        assert_eq!(back.pc, 0x1_0000_3f80);
    }

    #[test]
    fn the_program_counter_is_settable_on_its_own() {
        let mut words = vec![0u32; STATE_WORDS];
        set_program_counter(&mut words, 0xdead_beef_cafe);
        assert_eq!(program_counter(&words), 0xdead_beef_cafe);
        assert_eq!(to_registers(&words).pc, 0xdead_beef_cafe);
    }

    #[test]
    fn the_exception_state_names_a_breakpoint_trap() {
        let mut words = [0u32; EXCEPTION_STATE_WORDS];
        assert!(!trapped_on_breakpoint(&words), "an empty state is no trap");
        #[cfg(target_arch = "aarch64")]
        {
            words[2] = 0xf200_0000; // ESR of `BRK #0`
            assert!(trapped_on_breakpoint(&words));
            words[2] = 0xca00_0022; // software step, EC 0x32
            assert!(!trapped_on_breakpoint(&words));
            words[2] = 0x5600_0080; // SVC, EC 0x15
            assert!(!trapped_on_breakpoint(&words));
        }
        #[cfg(target_arch = "x86_64")]
        {
            words[0] = 3 | (7 << 16); // #BP on cpu 7
            assert!(trapped_on_breakpoint(&words));
            words[0] = 1; // #DB, a single step
            assert!(!trapped_on_breakpoint(&words));
        }
    }

    #[test]
    fn the_single_step_switch_is_one_bit_and_nothing_else_moves() {
        // Every other bit set, the switch's own bit clear on both machines.
        let mut words = vec![0xAAAA_AAAAu32; SINGLE_STEP_STATE_WORDS];
        let before = words.clone();
        set_single_step(&mut words, true);
        let changed: Vec<usize> = (0..words.len())
            .filter(|i| words[*i] != before[*i])
            .collect();
        assert_eq!(changed.len(), 1, "exactly one word changes: {changed:?}");
        assert_eq!((words[changed[0]] ^ before[changed[0]]).count_ones(), 1);
        #[cfg(target_arch = "aarch64")]
        assert_eq!(changed, [128], "mdscr_el1 is the last 64-bit field");
        #[cfg(target_arch = "x86_64")]
        assert_eq!(changed, [34], "rflags is at byte 136");
        set_single_step(&mut words, false);
        assert_eq!(words[changed[0]] & !before[changed[0]], 0, "the bit is off");
        // Disarming twice leaves it off.
        set_single_step(&mut words, false);
        let mut once = before.clone();
        set_single_step(&mut once, false);
        assert_eq!(words, once);
    }

    #[test]
    fn a_breakpoint_is_one_instruction_and_the_pc_rule_matches_it() {
        #[cfg(target_arch = "aarch64")]
        {
            assert_eq!(BREAKPOINT_INSTRUCTION.len(), 4);
            assert_eq!(PC_ADVANCE_AFTER_BREAKPOINT, 0);
        }
        #[cfg(target_arch = "x86_64")]
        {
            assert_eq!(BREAKPOINT_INSTRUCTION, [0xcc]);
            assert_eq!(PC_ADVANCE_AFTER_BREAKPOINT, 1);
        }
    }
}
