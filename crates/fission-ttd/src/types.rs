//! Shared register state used by debugger and TTD layers.

/// One machine's registers, named the way its own architecture names them.
///
/// This was eighteen `u64` fields called `rax`..`r15`, `rip`, `rflags`. The
/// emulator runs aarch64, ARM, MIPS and PowerPC images, and on every one of
/// them each of those names resolved to nothing: a recorded snapshot held
/// sixteen zeroes, and `ttd_seek` failed outright on the first
/// `write_register_u64("RAX", ..)`. Time travel was not lossy off x86-64, it
/// was unavailable, and nothing said so -- the recorder happily stored
/// fifteen snapshots of nothing.
///
/// So the registers are carried by name. A producer records what its machine
/// actually has; a consumer asks for what it actually wants and finds out
/// when it is not there. Names are compared case-insensitively, because the
/// emulator's register map is upper-case (`RAX`, `X0`) and the debuggers and
/// printers below are not.
///
/// The program counter is a field rather than an entry because every consumer
/// wants it and no two architectures agree on its name (`RIP`, `PC`).
///
/// Values are `u64`: a register wider than eight bytes (x86-64's `ZMM0`,
/// aarch64's `Q0`) is not carried here, so a snapshot does not restore vector
/// state. That is a real gap, and a narrower one than the whole-architecture
/// gap this replaces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegisterState {
    entries: Vec<(Box<str>, u64)>,
    /// Program counter, whatever this architecture calls it.
    pub pc: u64,
}

impl RegisterState {
    /// An empty state for a machine executing at `pc`.
    pub fn at(pc: u64) -> Self {
        Self {
            entries: Vec::new(),
            pc,
        }
    }

    /// Builder form of [`Self::set`], for the fixed register lists the
    /// native debuggers read out of an OS context structure.
    #[must_use]
    pub fn with(mut self, name: &str, value: u64) -> Self {
        self.set(name, value);
        self
    }

    /// Record `name`'s value, replacing any previous value for that name.
    pub fn set(&mut self, name: &str, value: u64) {
        match self
            .entries
            .iter_mut()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
        {
            Some(entry) => entry.1 = value,
            None => self.entries.push((name.into(), value)),
        }
    }

    /// This machine's value for `name`, or `None` if it has no such register.
    ///
    /// `None` is the answer that used to be a silent zero, and the whole
    /// point of the type: asking an aarch64 snapshot for `RAX` is a question
    /// with no answer, not a register that happens to hold 0.
    pub fn get(&self, name: &str) -> Option<u64> {
        self.entries
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, value)| *value)
    }

    /// Every recorded register, in the order it was recorded.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u64)> {
        self.entries.iter().map(|(name, value)| (&**name, *value))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The general-purpose registers a Windows `CONTEXT` or a Linux
/// `user_regs_struct` hands back on x86-64, in the order GDB numbers them --
/// which is also the order `rr`'s register dumps use.
pub const X86_64_GP_REGISTERS: [&str; 16] = [
    "RAX", "RBX", "RCX", "RDX", "RSI", "RDI", "RBP", "RSP", "R8", "R9", "R10", "R11", "R12", "R13",
    "R14", "R15",
];

/// GDB's x86-64 register numbering, as it appears in `rr`'s MI register
/// dumps: 0..=15 are [`X86_64_GP_REGISTERS`], 16 is the program counter and
/// 17 the flags. Returns `None` for a number this does not name, rather than
/// dropping the value into whichever field happened to be last.
pub fn x86_64_register_name(number: u32) -> Option<&'static str> {
    match number {
        0..=15 => Some(X86_64_GP_REGISTERS[number as usize]),
        16 => Some("RIP"),
        17 => Some("RFLAGS"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The distinction the old struct could not make.
    #[test]
    fn a_register_the_machine_does_not_have_is_absent_not_zero() {
        let aarch64 = RegisterState::at(0x401018).with("X0", 0x1f).with("SP", 0);
        assert_eq!(aarch64.get("RAX"), None);
        assert_eq!(aarch64.get("X0"), Some(0x1f));
        // Present and zero is its own answer.
        assert_eq!(aarch64.get("SP"), Some(0));
    }

    #[test]
    fn names_are_matched_without_regard_to_case() {
        let state = RegisterState::at(0).with("RAX", 7);
        assert_eq!(state.get("rax"), Some(7));
        let mut state = state;
        state.set("rAx", 9);
        assert_eq!(state.len(), 1, "a re-set must not add a second entry");
        assert_eq!(state.get("RAX"), Some(9));
    }

    #[test]
    fn gdb_numbering_does_not_invent_a_register() {
        assert_eq!(x86_64_register_name(0), Some("RAX"));
        assert_eq!(x86_64_register_name(16), Some("RIP"));
        assert_eq!(x86_64_register_name(18), None);
    }
}
