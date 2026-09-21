use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn canonical_x86_gpr64_name_for_store_value(
        &self,
        op: &PcodeOp,
        value: &Varnode,
    ) -> Option<(&'static str, usize)> {
        self.canonical_x86_gpr64_name_for_value(value)
            .or_else(|| self.canonical_x86_gpr64_name_for_value_source(value, 4))
            .or_else(|| {
                let raw_name = Self::x86_store_source_register_name_from_asm(op)?;
                Self::canonical_x86_gpr64_name_for_raw_name(&raw_name)
            })
    }

    pub(super) fn canonical_x86_gpr64_name_for_value(
        &self,
        value: &Varnode,
    ) -> Option<(&'static str, usize)> {
        let raw_name = self.sla_hw_name(value.offset, value.size).or_else(|| {
            crate::arch::x86::unique_x86_register_name(value.offset, value.size).map(str::to_string)
        })?;
        Self::canonical_x86_gpr64_name_for_raw_name(raw_name.as_str())
    }

    fn canonical_x86_gpr64_name_for_value_source(
        &self,
        value: &Varnode,
        budget: usize,
    ) -> Option<(&'static str, usize)> {
        if budget == 0 {
            return None;
        }
        let Some((_, op)) = self.lookup_def_site(value) else {
            return None;
        };
        match op.opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::SubPiece => {
                let input = op.inputs.first()?;
                self.canonical_x86_gpr64_name_for_value(input)
                    .or_else(|| self.canonical_x86_gpr64_name_for_value_source(input, budget - 1))
            }
            _ => None,
        }
    }

    fn canonical_x86_gpr64_name_for_raw_name(raw_name: &str) -> Option<(&'static str, usize)> {
        let family_idx = crate::arch::x86::x86_gpr_family_index(raw_name)?;
        const GPR64: [&str; 16] = [
            "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11",
            "r12", "r13", "r14", "r15",
        ];
        GPR64
            .get(family_idx)
            .copied()
            .map(|name| (name, family_idx))
    }

    fn x86_store_source_register_name_from_asm(op: &PcodeOp) -> Option<String> {
        let asm = op.asm_mnemonic.as_deref()?.trim();
        let source = asm.rsplit_once(',')?.1.trim();
        let source = source
            .split_whitespace()
            .next()
            .unwrap_or(source)
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
            .to_ascii_lowercase();
        crate::arch::x86::x86_gpr_family_index(&source).map(|_| source)
    }
}
