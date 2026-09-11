//! ARMv7-M system state: the registers `MRS` and `MSR` reach.
//!
//! Cortex-M keeps a handful of things outside the general registers -- two
//! banked stack pointers, the interrupt masks, the priority ceiling, the
//! privilege and stack-selection bits of `CONTROL`, and the current exception
//! number. SLEIGH does not model any of them; it lifts each access to a
//! `CALLOTHER` (`getMainStackPointer`, `setBasePriority`,
//! `isCurrentModePrivileged`, …) and leaves the meaning to whoever executes
//! the p-code.
//!
//! Nobody did, so every one of them returned zero. That is not a small
//! approximation: `isCurrentModePrivileged` guards each of the `MRS` reads,
//! and a zero there means "unprivileged", so the guarded read is skipped and
//! the destination register keeps the zero the spec wrote into it first. Every
//! special-register read in every Cortex-M image came back as zero, and the
//! most common of them in the benchmark corpus was reached 960 times.
//!
//! # What is being modelled
//!
//! A processor at reset that no interrupt source is wired to: thread mode,
//! privileged, main stack selected, interrupts unmasked, no priority ceiling,
//! exception number zero. That is the honest state for an emulator with no
//! interrupt controller -- firmware that reads these back sees a machine
//! nothing has interrupted, which is exactly what has happened.
//!
//! The stack pointers need one piece of care. `sp` in the SLEIGH register file
//! *is* the active stack pointer, banked by `CONTROL.SPSEL`; only the inactive
//! one needs storing. So `getMainStackPointer` reads `sp` when MSP is
//! selected, and the saved bank when it is not.

/// The ARMv7-M system registers this emulator models.
#[derive(Debug, Clone, Copy)]
pub struct CortexMState {
    /// The stack pointer that is *not* in `sp` right now.
    pub banked_sp: u64,
    /// `CONTROL.SPSEL`: is the process stack the active one?
    pub process_stack_active: bool,
    /// `CONTROL.nPRIV`, inverted: is thread mode privileged?
    pub thread_mode_privileged: bool,
    /// `IPSR`: zero in thread mode, else the exception being handled.
    pub exception_number: u32,
    /// `PRIMASK`, inverted: are IRQs unmasked?
    pub irq_enabled: bool,
    /// `FAULTMASK`, inverted.
    pub fault_enabled: bool,
    /// `BASEPRI`: the priority ceiling, zero meaning none.
    pub base_priority: u64,
    /// `MSPLIM` / `PSPLIM`, which v8-M added and which nothing here enforces --
    /// stored so a read gives back what was written rather than a zero.
    pub main_stack_limit: u64,
    pub process_stack_limit: u64,
}

impl Default for CortexMState {
    fn default() -> Self {
        Self::at_reset()
    }
}

impl CortexMState {
    /// The state an ARMv7-M core comes out of reset in.
    pub const fn at_reset() -> Self {
        Self {
            banked_sp: 0,
            process_stack_active: false,
            thread_mode_privileged: true,
            exception_number: 0,
            irq_enabled: true,
            fault_enabled: true,
            base_priority: 0,
            main_stack_limit: 0,
            process_stack_limit: 0,
        }
    }

    /// Handler mode is always privileged; thread mode asks `CONTROL`.
    pub fn current_mode_privileged(&self) -> bool {
        self.exception_number != 0 || self.thread_mode_privileged
    }

    pub fn in_thread_mode(&self) -> bool {
        self.exception_number == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_core_out_of_reset_is_privileged_on_the_main_stack() {
        let state = CortexMState::at_reset();
        assert!(state.current_mode_privileged());
        assert!(state.in_thread_mode());
        assert!(!state.process_stack_active);
        assert!(state.irq_enabled);
        assert_eq!(state.base_priority, 0);
    }

    #[test]
    fn handler_mode_is_privileged_even_when_thread_mode_is_not() {
        let mut state = CortexMState::at_reset();
        state.thread_mode_privileged = false;
        assert!(
            !state.current_mode_privileged(),
            "thread mode, unprivileged"
        );

        state.exception_number = 3;
        assert!(
            state.current_mode_privileged(),
            "an exception handler is privileged whatever CONTROL says"
        );
        assert!(!state.in_thread_mode());
    }
}
