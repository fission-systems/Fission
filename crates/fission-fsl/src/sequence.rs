//! A bounded sequence of instances of existing FIR bodies, with byte origins.
//! No function discovery, guest branch target recovery or second semantic IR.
use crate::{
    emit_instruction, state::execute_decoded_at, DecodedInstruction, ExecutionStatus, FslError,
    FslcPackage, MachineState, OutputLayer,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt::Write};
const MAGIC: &[u8; 8] = b"FSLSEQ\0\x01";
/// v2 is written only when some selected body reads/writes the guest PC. It adds
/// no header fields; the version tells loaders to expect forward-only branches.
const MAGIC_V2: &[u8; 8] = b"FSLSEQ\0\x02";
const HEADER_SIZE: usize = 96;
const MAX_SEQUENCE_SIZE: usize = 64 * 1024 * 1024;
const MAX_STEPS: usize = 4096;
const MAX_BANK_SLOTS: usize = 4096;

fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Logical u64 register slots and u1 flag slots. Exact bank lengths are required.
/// This is an execution ABI, not a guest register layout or calling convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceStateContract {
    pub register_count: usize,
    pub flag_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionOrigin {
    pub address: u64,
    pub input_offset: u32,
    pub byte_length: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirInstructionInstance {
    pub origin: InstructionOrigin,
    pub decoded: DecodedInstruction,
}

/// Immutable execution plan. Instances reference one canonical package's bodies;
/// they do not copy, merge or reinterpret values/ops/blocks. Origin hashes bind
/// the supplied byte window and the canonical compiled package, not a whole
/// executable file or the original .fsl source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirSequence {
    package: FslcPackage,
    input: Vec<u8>,
    base_address: u64,
    contract: SequenceStateContract,
    input_sha256: [u8; 32],
    package_sha256: [u8; 32],
    instances: Vec<FirInstructionInstance>,
    /// True when any selected body uses guest PC ops (forward-only branching).
    guest_control: bool,
}

impl FirSequence {
    pub fn compose(
        package: &FslcPackage,
        profile: &str,
        base_address: u64,
        input: &[u8],
        contract: SequenceStateContract,
    ) -> Result<Self, FslError> {
        package.validate()?;
        if profile != package.language {
            return Err(error("sequence profile does not match package"));
        }
        if contract.register_count > MAX_BANK_SLOTS || contract.flag_count > MAX_BANK_SLOTS {
            return Err(error("sequence bank contract exceeds 4096 slots"));
        }
        let width = usize::from(package.instructions[0].encoding.bits / 8);
        if input.is_empty() || !input.len().is_multiple_of(width) || input.len() / width > MAX_STEPS
        {
            return Err(error(
                "sequence requires 1..4096 complete fixed-width instructions",
            ));
        }
        base_address
            .checked_add(input.len() as u64)
            .ok_or_else(|| error("sequence address range overflows"))?;
        let mut instances = Vec::with_capacity(input.len() / width);
        for (index, raw) in input.chunks_exact(width).enumerate() {
            let offset = index * width;
            let decoded = package
                .decode_bytes(profile, raw)?
                .ok_or_else(|| error("unsupported encoding in sequence"))?;
            let instruction = &package.instructions[decoded.instruction_index];
            crate::state::admit_guest(instruction)?;
            if !crate::state::bank_indices_fit(
                instruction,
                &decoded,
                contract.register_count,
                contract.flag_count,
            ) {
                return Err(error("sequence selector exceeds declared state contract"));
            }
            instances.push(FirInstructionInstance {
                origin: InstructionOrigin {
                    address: base_address + offset as u64,
                    input_offset: offset as u32,
                    byte_length: width as u16,
                },
                decoded,
            });
        }
        let guest_control = instances.iter().any(|i| {
            crate::state::uses_guest_pc(&package.instructions[i.decoded.instruction_index])
        });
        let package_bytes = package.encode_binary()?;
        if HEADER_SIZE + package_bytes.len() + input.len() > MAX_SEQUENCE_SIZE {
            return Err(error("sequence container exceeds 64 MiB"));
        }
        Ok(Self {
            package: package.clone(),
            input: input.to_vec(),
            base_address,
            contract,
            input_sha256: digest(input),
            package_sha256: digest(&package_bytes),
            instances,
            guest_control,
        })
    }
    /// Whether execution may branch between instances (forward-only, runtime-checked).
    pub fn has_guest_control(&self) -> bool {
        self.guest_control
    }
    pub fn package(&self) -> &FslcPackage {
        &self.package
    }
    pub fn instances(&self) -> &[FirInstructionInstance] {
        &self.instances
    }
    pub fn contract(&self) -> SequenceStateContract {
        self.contract
    }
    pub fn input_sha256(&self) -> &[u8; 32] {
        &self.input_sha256
    }
    pub fn package_sha256(&self) -> &[u8; 32] {
        &self.package_sha256
    }

    /// Execute atomically with respect to this wrapper: failed status/error never
    /// commits a prefix. This is not guest trap/fault or concurrent memory semantics.
    pub fn execute(&self, state: &mut MachineState) -> Result<ExecutionStatus, FslError> {
        if state.registers.len() != self.contract.register_count
            || state.flags.len() != self.contract.flag_count
            || state.flags.iter().any(|&v| v > 1)
        {
            return Ok(ExecutionStatus::InvalidState);
        }
        let mut next = state.clone();
        let width = u64::from(self.instances[0].origin.byte_length);
        let end = self.base_address + self.input.len() as u64;
        let mut index = 0;
        while index < self.instances.len() {
            let instance = &self.instances[index];
            let address = instance.origin.address;
            let (status, next_pc) =
                execute_decoded_at(&self.package, &instance.decoded, &mut next, Some(address))?;
            if status != ExecutionStatus::Success {
                return Ok(status);
            }
            index = match next_pc {
                None => index + 1,
                // Strictly forward, word-aligned, inside the window or exactly at
                // its end. Forward-only targets bound execution to the instance count.
                Some(target)
                    if target > address
                        && target <= end
                        && (target - self.base_address).is_multiple_of(width) =>
                {
                    ((target - self.base_address) / width) as usize
                }
                Some(_) => return Ok(ExecutionStatus::BadBranchTarget),
            };
        }
        *state = next;
        Ok(ExecutionStatus::Success)
    }

    /// v1 sequence envelope embeds the unchanged .fslc and exact input bytes.
    /// All origins and instruction selections are derived again when loading.
    pub fn encode_binary(&self) -> Result<Vec<u8>, FslError> {
        let package = self.package.encode_binary()?;
        let mut out = Vec::with_capacity(HEADER_SIZE + package.len() + self.input.len());
        out.extend_from_slice(if self.guest_control { MAGIC_V2 } else { MAGIC });
        out.extend_from_slice(&self.base_address.to_le_bytes());
        for value in [
            self.contract.register_count,
            self.contract.flag_count,
            package.len(),
            self.input.len(),
        ] {
            out.extend_from_slice(&(value as u32).to_le_bytes());
        }
        out.extend_from_slice(&self.package_sha256);
        out.extend_from_slice(&self.input_sha256);
        out.extend_from_slice(&package);
        out.extend_from_slice(&self.input);
        Ok(out)
    }
    pub fn decode_binary(bytes: &[u8]) -> Result<Self, FslError> {
        if bytes.len() < HEADER_SIZE
            || bytes.len() > MAX_SEQUENCE_SIZE
            || (&bytes[..8] != MAGIC && &bytes[..8] != MAGIC_V2)
        {
            return Err(error("invalid sequence header or size"));
        }
        let v2 = &bytes[..8] == MAGIC_V2;
        let u32_at =
            |start| u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()) as usize;
        let base_address = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let contract = SequenceStateContract {
            register_count: u32_at(16),
            flag_count: u32_at(20),
        };
        let package_length = u32_at(24);
        let input_length = u32_at(28);
        let end = HEADER_SIZE
            .checked_add(package_length)
            .and_then(|n| n.checked_add(input_length))
            .ok_or_else(|| error("sequence length overflows"))?;
        if end != bytes.len() {
            return Err(error("sequence length mismatch or trailing bytes"));
        }
        let package_bytes = &bytes[HEADER_SIZE..HEADER_SIZE + package_length];
        let input = &bytes[HEADER_SIZE + package_length..];
        if digest(package_bytes) != bytes[32..64] || digest(input) != bytes[64..96] {
            return Err(error("sequence origin hash mismatch"));
        }
        let package = FslcPackage::decode_binary(package_bytes)?;
        let sequence = Self::compose(&package, &package.language, base_address, input, contract)?;
        // The envelope version must match whether guest control is present.
        if sequence.guest_control != v2 {
            return Err(error(
                "sequence envelope version does not match guest control",
            ));
        }
        // Bind to the canonical versioned package serialization too.
        if sequence.package_sha256 != bytes[32..64] {
            return Err(error("noncanonical sequence package"));
        }
        Ok(sequence)
    }

    pub fn emit(&self, layer: OutputLayer, symbol: &str) -> Result<String, FslError> {
        let metadata = self.metadata()?;
        if layer == OutputLayer::Fir {
            return Ok(metadata);
        }
        if !matches!(layer, OutputLayer::C | OutputLayer::Rust) {
            return Err(error("sequence output supports FIR/C/Rust only"));
        }
        if !symbol.starts_with("fsl_")
            || !symbol
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(error(
                "output symbol must be an ASCII identifier starting with fsl_",
            ));
        }
        let c = layer == OutputLayer::C;
        let comment = |line: &str| format!("// {line}\n");
        // Keep raw evidence strings in diagnostic metadata; comments use only
        // derived hashes/addresses/indices, so evidence cannot inject source code.
        let mut out = comment(&format!(
            "sequence input-sha256={} package-sha256={} steps={}",
            hex(&self.input_sha256),
            hex(&self.package_sha256),
            self.instances.len()
        ));
        let bodies: BTreeSet<_> = self
            .instances
            .iter()
            .map(|i| i.decoded.instruction_index)
            .collect();
        for index in bodies {
            let body = &self.package.instructions[index];
            let name = format!("{symbol}_body_{index}");
            out.push_str(&if crate::state::uses_guest_pc(body) {
                crate::state::emit_state_instruction_guest(body, layer, &name)?
            } else {
                emit_instruction(body, layer, &name)?
            });
        }
        let nr = self.contract.register_count;
        let nf = self.contract.flag_count;
        if c {
            write!(out, "\nuint32_t {symbol}(uint64_t *registers, size_t register_count, uint64_t *flags, size_t flag_count) {{\n  if (!registers || !flags || register_count != {nr} || flag_count != {nf}) return 3;\n  for (size_t i=0;i<flag_count;++i) if (flags[i]>1) return 3;\n  uint64_t next_registers[{}] = {{0}};\n  uint64_t next_flags[{}] = {{0}};\n  for (size_t i=0;i<register_count;++i) next_registers[i]=registers[i];\n  for (size_t i=0;i<flag_count;++i) next_flags[i]=flags[i];\n", nr.max(1), nf.max(1)).unwrap();
            if self.guest_control {
                writeln!(out, "  uint64_t pc = UINT64_C(0x{:x});", self.base_address).unwrap();
            }
        } else {
            write!(out, "\npub fn {symbol}(registers: &mut [u64], flags: &mut [u64]) -> u32 {{\n  if registers.len() != {nr} || flags.len() != {nf} || flags.iter().any(|&v| v>1) {{ return 3; }}\n  let mut next_registers = [0u64; {nr}];\n  let mut next_flags = [0u64; {nf}];\n  next_registers.copy_from_slice(registers);\n  next_flags.copy_from_slice(flags);\n").unwrap();
            if self.guest_control {
                writeln!(out, "  let mut pc = 0x{:x}u64;", self.base_address).unwrap();
            }
        }
        let width = u64::from(self.instances[0].origin.byte_length);
        let end = self.base_address + self.input.len() as u64;
        for (step, instance) in self.instances.iter().enumerate() {
            out.push_str(&comment(&format!(
                "step={step} address=0x{:x} offset={} length={} body={} raw={}",
                instance.origin.address,
                instance.origin.input_offset,
                instance.origin.byte_length,
                instance.decoded.instruction_index,
                hex(&instance.decoded.raw)
            )));
            let values = instance
                .decoded
                .fields
                .iter()
                .map(|(_, v)| format!("0x{v:x}{}", if c { "ULL" } else { "u64" }))
                .collect::<Vec<_>>()
                .join(", ");
            let count = instance.decoded.fields.len();
            let index = instance.decoded.instruction_index;
            if self.guest_control {
                let address = instance.origin.address;
                let uses_pc = crate::state::uses_guest_pc(&self.package.instructions[index]);
                // Forward-only dispatch: a step runs only when the guest PC reaches it.
                // Targets are checked at run time and never move backward.
                if c {
                    let call = if uses_pc {
                        format!("{symbol}_body_{index}(next_registers, {nr}, next_flags, {nf}, fields, {count}, UINT64_C(0x{address:x}), &next_pc, &next_pc_set)")
                    } else {
                        format!("{symbol}_body_{index}(next_registers, {nr}, next_flags, {nf}, fields, {count})")
                    };
                    let check = if uses_pc {
                        format!("    if (next_pc_set) {{ if (next_pc <= UINT64_C(0x{address:x}) || next_pc > UINT64_C(0x{end:x}) || (next_pc - UINT64_C(0x{:x})) % {width} != 0) return 4; pc = next_pc; }}\n", self.base_address)
                    } else {
                        String::new()
                    };
                    writeln!(out, "  if (pc == UINT64_C(0x{address:x})) {{ const uint64_t fields[{}] = {{{}}};\n    uint64_t next_pc = 0; uint32_t next_pc_set = 0; (void)next_pc; (void)next_pc_set;\n    uint32_t status={call};\n    if (status!=0) return status;\n    pc = UINT64_C(0x{:x});\n{check}  }}", count.max(1), if count == 0 { "0" } else { &values }, address + width).unwrap();
                } else {
                    let call = if uses_pc {
                        format!("{symbol}_body_{index}(&mut next_registers, &mut next_flags, &fields, 0x{address:x}u64, &mut next_pc, &mut next_pc_set)")
                    } else {
                        format!(
                            "{symbol}_body_{index}(&mut next_registers, &mut next_flags, &fields)"
                        )
                    };
                    let check = if uses_pc {
                        format!("    if next_pc_set {{ if next_pc <= 0x{address:x}u64 || next_pc > 0x{end:x}u64 || (next_pc - 0x{:x}u64) % {width} != 0 {{ return 4; }} pc = next_pc; }}\n", self.base_address)
                    } else {
                        String::new()
                    };
                    writeln!(out, "  if pc == 0x{address:x}u64 {{ let fields = [{values}];\n    let mut next_pc = 0u64; let mut next_pc_set = false; let _ = (&mut next_pc, &mut next_pc_set);\n    let status = {call};\n    if status != 0 {{ return status; }}\n    pc = 0x{:x}u64;\n{check}  }}", address + width).unwrap();
                }
            } else if c {
                writeln!(out, "  {{ const uint64_t fields[{}] = {{{}}};\n    uint32_t status={symbol}_body_{index}(next_registers, {nr}, next_flags, {nf}, fields, {count});\n    if (status!=0) return status; }}", count.max(1), if count == 0 { "0" } else { &values }).unwrap();
            } else {
                writeln!(out, "  {{ let fields = [{values}];\n    let status = {symbol}_body_{index}(&mut next_registers, &mut next_flags, &fields);\n    if status != 0 {{ return status; }} }}").unwrap();
            }
        }
        if self.guest_control {
            out.push_str(if c {
                "  (void)pc;\n"
            } else {
                "  let _ = pc;\n"
            });
        }
        if c {
            out.push_str("  for (size_t i=0;i<register_count;++i) registers[i]=next_registers[i];\n  for (size_t i=0;i<flag_count;++i) flags[i]=next_flags[i];\n  return 0;\n}\n");
        } else {
            out.push_str("  registers.copy_from_slice(&next_registers);\n  flags.copy_from_slice(&next_flags);\n  0\n}\n");
        }
        Ok(out)
    }
    fn metadata(&self) -> Result<String, FslError> {
        let mut out = format!("fir-sequence version={} profile={:?} base=0x{:x} registers={} flags={} steps={}\ninput-sha256={}\npackage-sha256={}\n", if self.guest_control { 2 } else { 1 }, self.package.language, self.base_address, self.contract.register_count, self.contract.flag_count, self.instances.len(), hex(&self.input_sha256), hex(&self.package_sha256));
        for (step, instance) in self.instances.iter().enumerate() {
            let index = instance.decoded.instruction_index;
            let body = &self.package.instructions[index];
            writeln!(out, "step={step} address=0x{:x} offset={} length={} raw={} body={} name={:?} fields={:?}\nevidence={:?}", instance.origin.address, instance.origin.input_offset, instance.origin.byte_length, hex(&instance.decoded.raw), index, body.name, instance.decoded.fields, body.evidence).unwrap();
        }
        let bodies: BTreeSet<_> = self
            .instances
            .iter()
            .map(|i| i.decoded.instruction_index)
            .collect();
        for index in bodies {
            writeln!(out, "body={index}").unwrap();
            out.push_str(&emit_instruction(
                &self.package.instructions[index],
                OutputLayer::Fir,
                "ignored",
            )?);
        }
        Ok(out)
    }
}
