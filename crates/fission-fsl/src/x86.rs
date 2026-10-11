//! Fission-owned variable-length x86 decoding and canonical FIR projections.
//! No external instruction decoder, SLEIGH runtime, or P-code translation.
use crate::x86_decode::{Form, Rule};
use crate::{
    sequence::{hex, InstructionOrigin},
    state, DecodedInstruction, ExecutionStatus, FslError, FslcPackage, MachineState, OutputLayer,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt::Write};
const DECODER: &str = "owned-x86-v1";
const MAGIC: &[u8; 8] = b"FSLXPKG1";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_INSTRUCTIONS: usize = 4096;
fn error(s: impl Into<String>) -> FslError {
    FslError::at(1, 1, s)
}
fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Compiled encoding rules plus the sole canonical FIR body pool. Encoding
/// fields are normalized operand tokens; original bytes remain in origins.
#[derive(Clone, Debug)]
pub struct X86Package {
    rules: Vec<Rule>,
    bodies: FslcPackage,
}
impl X86Package {
    pub fn compile(manifest: &str, source: &str) -> Result<Self, FslError> {
        if manifest.len() > MAX_BYTES || source.len() > MAX_BYTES {
            return Err(error("x86 sources too large"));
        }
        let value = Self {
            rules: crate::x86_decode::parse(manifest)?,
            bodies: crate::compile_source(source)?,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), FslError> {
        crate::x86_decode::validate(&self.rules)?;
        self.bodies.validate()?;
        for r in &self.rules {
            for width in [16, 32, 64] {
                let name = r.body.replace("{width}", &width.to_string());
                let targets = if r.form == Form::Lea {
                    [16, 32, 64]
                        .into_iter()
                        .map(|a| format!("{name}_a{a}"))
                        .collect()
                } else {
                    vec![name]
                };
                for target in targets {
                    let b = self
                        .bodies
                        .instructions
                        .iter()
                        .find(|b| b.name == target)
                        .ok_or_else(|| error(format!("missing body {target}")))?;
                    state::admit_context(b, true, true)?;
                    if b.encoding.bits != 128
                        || b.encoding
                            .fields
                            .iter()
                            .map(|f| (f.name.as_str(), f.offset, f.bits))
                            .collect::<Vec<_>>()
                            != [
                                ("destination", 0, 5),
                                ("source", 5, 5),
                                ("base", 10, 5),
                                ("index", 15, 5),
                                ("immediate", 20, 64),
                                ("scale", 84, 2),
                            ]
                    {
                        return Err(error("normalized token schema mismatch"));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
    pub fn bodies(&self) -> &FslcPackage {
        &self.bodies
    }
    pub fn encode_binary(&self) -> Result<Vec<u8>, FslError> {
        self.validate()?;
        let rules = crate::x86_decode::encode(&self.rules)?;
        let bodies = self.bodies.encode_binary()?;
        let mut out = MAGIC.to_vec();
        out.extend((rules.len() as u32).to_le_bytes());
        out.extend((bodies.len() as u32).to_le_bytes());
        out.extend(Sha256::digest(&rules));
        out.extend(Sha256::digest(&bodies));
        out.extend(rules);
        out.extend(bodies);
        if out.len() > MAX_BYTES {
            return Err(error("x86 package exceeds 16 MiB"));
        }
        Ok(out)
    }
    pub fn decode_binary(raw: &[u8]) -> Result<Self, FslError> {
        if raw.len() < 80 || raw.len() > MAX_BYTES || raw.get(..8) != Some(MAGIC) {
            return Err(error("bad owned x86 package header"));
        }
        let n = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
        let m = u32::from_le_bytes(raw[12..16].try_into().unwrap()) as usize;
        if n > MAX_BYTES || m > MAX_BYTES || 80 + n + m != raw.len() {
            return Err(error("bad x86 package lengths/EOF"));
        }
        let rule_bytes = &raw[80..80 + n];
        let body_bytes = &raw[80 + n..];
        if Sha256::digest(rule_bytes).as_slice() != &raw[16..48]
            || Sha256::digest(body_bytes).as_slice() != &raw[48..80]
        {
            return Err(error("x86 package hash mismatch"));
        }
        let result = Self {
            rules: crate::x86_decode::decode_rules(rule_bytes)?,
            bodies: FslcPackage::decode_binary(body_bytes)?,
        };
        result.validate()?;
        Ok(result)
    }
}

#[derive(Clone, Debug)]
struct Instance {
    origin: InstructionOrigin,
    raw: Vec<u8>,
    code: String,
    observation: DecodedInstruction,
    returns: bool,
}
#[derive(Clone, Debug)]
pub struct BinaryOrigin {
    pub file_sha256: String,
    pub file_offset: u64,
    pub section: String,
}
#[derive(Clone, Debug)]
pub struct X86Program {
    package: X86Package,
    mode: u32,
    input: Vec<u8>,
    base: u64,
    instances: Vec<Instance>,
    binary: Option<BinaryOrigin>,
}

impl X86Program {
    pub fn lift(
        package: &X86Package,
        mode: u32,
        base: u64,
        input: &[u8],
    ) -> Result<Self, FslError> {
        if ![16, 32, 64].contains(&mode) || input.is_empty() || input.len() > 65536 {
            return Err(error("mode 16/32/64 and 1..65536 bytes required"));
        }
        let end = base
            .checked_add(input.len() as u64)
            .ok_or_else(|| error("address overflow"))?;
        if mode < 64 && end > (1u64 << mode) {
            return Err(error("window exceeds mode address space"));
        }
        let mut instances = Vec::new();
        let mut offset = 0;
        while offset < input.len() {
            if instances.len() >= MAX_INSTRUCTIONS {
                return Err(error("window exceeds 4096 instructions"));
            }
            let i = crate::x86_decode::decode(&package.rules, input, offset, base, mode).map_err(
                |e| {
                    error(format!(
                        "x86 address=0x{:x} offset={offset}: {}",
                        base + offset as u64,
                        e.message
                    ))
                },
            )?;
            let rule = &package.rules[i.rule];
            let mut body = rule.body.replace("{width}", &i.width.to_string());
            if rule.form == Form::Lea {
                body = format!("{body}_a{}", i.address_bits);
            }
            let index = package
                .bodies
                .instructions
                .iter()
                .position(|b| b.name == body)
                .ok_or_else(|| error("missing FIR body"))?;
            let encoding = &package.bodies.instructions[index].encoding;
            let mut token = encoding.value;
            for (field, value) in encoding.fields.iter().zip(i.fields) {
                token |= u128::from(value) << field.offset;
            }
            let observation = package
                .bodies
                .decode_bytes(&package.bodies.language, &token.to_le_bytes())?
                .ok_or_else(|| error("normalized operand token rejected"))?;
            if observation.instruction_index != index
                || !state::bank_indices_fit(
                    &package.bodies.instructions[index],
                    &observation,
                    17,
                    12,
                )
            {
                return Err(error("invalid operand binding"));
            }
            instances.push(Instance {
                origin: InstructionOrigin {
                    address: base + offset as u64,
                    input_offset: offset as u32,
                    byte_length: i.length as u16,
                },
                raw: input[offset..offset + i.length].to_vec(),
                code: rule.name.clone(),
                observation,
                returns: rule.form == Form::Ret,
            });
            offset += i.length;
        }
        if !instances.iter().any(|i| i.returns) {
            return Err(error("function window must include an admitted near RET"));
        }
        // Direct targets must designate an instruction start in this window.
        let starts: BTreeSet<_> = instances.iter().map(|i| i.origin.address).collect();
        for i in &instances {
            let r = package
                .rules
                .iter()
                .find(|r| r.name == i.code)
                .ok_or_else(|| error("missing rule identity"))?;
            if matches!(r.form, Form::Branch | Form::Branch8)
                && !starts.contains(&i.observation.fields[4].1)
            {
                return Err(error(format!(
                    "branch at 0x{:x} has external or unaligned target",
                    i.origin.address
                )));
            }
        }
        Ok(Self {
            package: package.clone(),
            mode,
            input: input.to_vec(),
            base,
            instances,
            binary: None,
        })
    }
    /// Selected file-backed executable span through the canonical Fission loader.
    /// No function discovery or parallel program metadata store is created.
    pub fn from_binary(
        package: &X86Package,
        path: &std::path::Path,
        address: u64,
        size: usize,
    ) -> Result<Self, FslError> {
        if size == 0 || size > 65536 {
            return Err(error("binary span size must be 1..65536"));
        }
        let binary = fission_loader::LoadedBinary::from_file(path)
            .map_err(|e| error(format!("loader: {e}")))?;
        let architecture = binary
            .architecture
            .as_ref()
            .ok_or_else(|| error("loader has no authoritative architecture"))?;
        if !architecture.processor.eq_ignore_ascii_case("x86") || architecture.endian != "little" {
            return Err(error("loader architecture is not little-endian x86"));
        }
        let section = binary
            .executable_section_containing(address)
            .ok_or_else(|| error("span is not executable"))?;
        let delta = address
            .checked_sub(section.virtual_address)
            .ok_or_else(|| error("bad section mapping"))?;
        if delta
            .checked_add(size as u64)
            .is_none_or(|n| n > section.file_size || n > section.virtual_size)
        {
            return Err(error("span exceeds file-backed executable section"));
        }
        let file_offset = section
            .file_offset
            .checked_add(delta)
            .ok_or_else(|| error("file offset overflow"))?;
        let raw = binary
            .view_executable_bytes(address, size)
            .ok_or_else(|| error("loader rejected byte mapping"))?;
        let end = address
            .checked_add(size as u64)
            .ok_or_else(|| error("span overflow"))?;
        if binary.relocations.iter().any(|r| {
            r.address < end
                && r.address
                    .checked_add(u64::from(r.size.max(1)))
                    .is_none_or(|e| e > address)
        }) {
            return Err(error("relocated instruction window is not admitted"));
        }
        let start =
            usize::try_from(file_offset).map_err(|_| error("file offset outside host range"))?;
        let file_end = start
            .checked_add(size)
            .ok_or_else(|| error("file span overflow"))?;
        if binary.data.as_slice().get(start..file_end) != Some(raw) {
            return Err(error("mapped bytes differ from original file span"));
        }
        let mut result = Self::lift(package, u32::from(architecture.bitness), address, raw)?;
        result.binary = Some(BinaryOrigin {
            file_sha256: hash(binary.data.as_slice()),
            file_offset,
            section: section.name.clone(),
        });
        Ok(result)
    }
    pub fn execute(
        &self,
        state: &mut MachineState,
        memory: &[u8],
        memory_base: u64,
        budget: usize,
    ) -> Result<(ExecutionStatus, Option<u64>), FslError> {
        if state.registers.len() != 17
            || state.flags.len() != 12
            || state.registers[16] != 0
            || state.flags.iter().any(|&f| f > 1)
            || state.flags[6..].iter().any(|&f| f != 1)
            || memory_base.checked_add(memory.len() as u64).is_none()
            || !(1..=1_000_000).contains(&budget)
        {
            return Ok((ExecutionStatus::InvalidState, None));
        }
        let mut trial = state.clone();
        let mut pc = self.base;
        for _ in 0..budget {
            let Some(i) = self.instances.iter().find(|i| i.origin.address == pc) else {
                return Ok((ExecutionStatus::InvalidState, None));
            };
            let (status, next) = state::execute_decoded_context(
                &self.package.bodies,
                &i.observation,
                &mut trial,
                Some(pc),
                Some((memory, memory_base)),
            )?;
            if status != ExecutionStatus::Success {
                return Ok((status, None));
            }
            if i.returns {
                let Some(exit) = next else {
                    return Ok((ExecutionStatus::InvalidState, None));
                };
                *state = trial;
                return Ok((ExecutionStatus::Success, Some(exit)));
            }
            pc = next.unwrap_or(pc + u64::from(i.origin.byte_length));
        }
        Ok((ExecutionStatus::InvalidState, None))
    }
    pub fn emit(&self, layer: OutputLayer) -> Result<String, FslError> {
        if layer == OutputLayer::Fir {
            let mut out=format!("x86 program mode={} base=0x{:x} bytes={} decoder={} input-sha256={} package-sha256={}\n",self.mode,self.base,self.input.len(),DECODER,hash(&self.input),hash(&self.package.encode_binary()?));
            if let Some(b) = &self.binary {
                writeln!(
                    out,
                    "binary file-sha256={} file-offset={} section={}",
                    b.file_sha256, b.file_offset, b.section
                )
                .unwrap();
            }
            for i in &self.instances {
                writeln!(out,"instance address=0x{:x} offset={} length={} raw={} code={:?} body={} fields={:?}",i.origin.address,i.origin.input_offset,i.origin.byte_length,hex(&i.raw),i.code,self.package.bodies.instructions[i.observation.instruction_index].name,i.observation.fields).unwrap();
            }
            for id in self
                .instances
                .iter()
                .map(|i| i.observation.instruction_index)
                .collect::<BTreeSet<_>>()
            {
                out += &crate::emit_instruction(
                    &self.package.bodies.instructions[id],
                    OutputLayer::Fir,
                    "unused",
                )?;
            }
            return Ok(out);
        }
        if !matches!(layer, OutputLayer::C | OutputLayer::Rust) {
            return Err(error("x86 output supports FIR/C/Rust only"));
        }
        let c = layer == OutputLayer::C;
        let mut out=format!("/* x86 mode={} input-sha256={} package-sha256={}; owned decoder {}.\n * Explicit state function: GPR[0..16]=RAX,RCX,RDX,RBX,RSP,RBP,RSI,RDI,R8..R15; GPR[16]=0.\n * flags=CF,PF,ZF,SF,OF,AF then six known bits. Unknown AF is not observable.\n * Disjoint valid storage required in C. Bounded byte memory, flat SS.base=0, no MMU/fault model.\n * Status 0 commits state and return PC; status 3 preserves caller storage. */\n",self.mode,hash(&self.input),hash(&self.package.encode_binary()?),DECODER);
        if let Some(b) = &self.binary {
            writeln!(
                out,
                "/* binary file-sha256={} file-offset={} */",
                b.file_sha256, b.file_offset
            )
            .unwrap();
        }
        for id in self
            .instances
            .iter()
            .map(|i| i.observation.instruction_index)
            .collect::<BTreeSet<_>>()
        {
            out += &state::emit_state_context(
                &self.package.bodies.instructions[id],
                layer,
                &format!("x86_body_{id}"),
            )?;
        }
        if c {
            out+="#include <string.h>\nuint32_t fsl_x86(uint64_t *registers, size_t register_count, uint64_t *flags, size_t flag_count, const uint8_t *memory, size_t memory_length, uint64_t memory_base, size_t budget, uint64_t *exit_pc) {\nif (!registers || !flags || !memory || !exit_pc || register_count != 17 || flag_count != 12 || registers[16] != 0 || budget == 0 || budget > 1000000 || memory_base > UINT64_MAX - memory_length) return 3;\nfor (size_t j=0;j<12;j++) if (flags[j] > 1 || (j>=6 && flags[j]!=1)) return 3;\nuint64_t r[17], f[12]; memcpy(r, registers, sizeof r); memcpy(f, flags, sizeof f);\n";
            writeln!(out,"uint64_t pc=UINT64_C(0x{:x});\nfor (size_t step=0;step<budget;step++) {{\nuint64_t next=0; uint32_t changed=0, status=0;\n(void)next; (void)changed;\nswitch(pc) {{",self.base).unwrap();
        } else {
            out+="pub fn fsl_x86(registers: &mut [u64], flags: &mut [u64], memory: &[u8], memory_base: u64, budget: usize, exit_pc: &mut u64) -> u32 {\nif registers.len()!=17 || flags.len()!=12 || registers[16]!=0 || budget==0 || budget>1000000 || flags.iter().any(|&v|v>1) || flags[6..].iter().any(|&v|v!=1) || memory_base.checked_add(memory.len() as u64).is_none() { return 3; }\nlet mut r=[0u64;17]; let mut f=[0u64;12]; r.copy_from_slice(registers); f.copy_from_slice(flags);\n";
            writeln!(out,"let mut pc=0x{:x}u64;\nfor _step in 0..budget {{\nlet mut next=0u64; let mut changed=false;\nlet _ = (&memory, memory_base, &next, &changed);\nmatch pc {{",self.base).unwrap();
        }
        for i in &self.instances {
            let id = i.observation.instruction_index;
            let body = &self.package.bodies.instructions[id];
            let fields = i
                .observation
                .fields
                .iter()
                .map(|(_, v)| {
                    if c {
                        format!("UINT64_C(0x{v:x})")
                    } else {
                        format!("0x{v:x}u64")
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            writeln!(
                out,
                "{} {{ /* raw={} */",
                if c {
                    format!("case UINT64_C(0x{:x}):", i.origin.address)
                } else {
                    format!("0x{:x} =>", i.origin.address)
                },
                hex(&i.raw)
            )
            .unwrap();
            if c {
                writeln!(out,"const uint64_t fields[6]={{{fields}}};\nstatus=x86_body_{id}(r,17,f,12,fields,6{}{});\nif (status) return status;",if state::uses_guest_pc(body){",pc,&next,&changed"}else{""},if state::uses_memory(body){",memory,memory_length,memory_base"}else{""}).unwrap();
            } else {
                writeln!(out,"let fields=[{fields}];\nlet status=x86_body_{id}(&mut r,&mut f,&fields{}{});\nif status!=0 {{ return status; }}",if state::uses_guest_pc(body){",pc,&mut next,&mut changed"}else{""},if state::uses_memory(body){",memory,memory_base"}else{""}).unwrap();
            }
            if i.returns {
                out += if c {
                    "if (!changed) return 3; memcpy(registers,r,sizeof r); memcpy(flags,f,sizeof f); *exit_pc=next; return 0;\n"
                } else {
                    "if !changed { return 3; } registers.copy_from_slice(&r); flags.copy_from_slice(&f); *exit_pc=next; return 0;\n"
                };
            } else {
                writeln!(
                    out,
                    "{}",
                    if c {
                        format!(
                            "pc=changed?next:UINT64_C(0x{:x}); break;",
                            i.origin.address + u64::from(i.origin.byte_length)
                        )
                    } else {
                        format!(
                            "pc=if changed {{next}} else {{0x{:x}u64}};",
                            i.origin.address + u64::from(i.origin.byte_length)
                        )
                    }
                )
                .unwrap();
            }
            out += "}\n";
        }
        out += if c {
            "default: return 3;\n}\n}\nreturn 3;\n}\n"
        } else {
            "_ => return 3,\n}\n}\n3\n}\n"
        };
        Ok(out)
    }
}
