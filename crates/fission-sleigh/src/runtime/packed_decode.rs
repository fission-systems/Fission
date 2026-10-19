//! Lossless cold storage for completed runtime decodes. This is an in-process
//! representation, not a persistent or externally readable archive format.

use super::DecodedPcodeFunction;
use anyhow::Result;
use bincode::Options;

/// An immutable encoding made only from a canonical runtime decode.
///
/// There is deliberately no constructor accepting bytes: callers cannot load
/// untrusted or stale archives. Codec versions need only agree within one process.
pub struct PackedDecodedPcodeFunction {
    bytes: Box<[u8]>,
}

impl std::fmt::Debug for PackedDecodedPcodeFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackedDecodedPcodeFunction")
            .field("byte_len", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

fn codec() -> impl Options {
    bincode::DefaultOptions::new()
        .with_varint_encoding()
        .reject_trailing_bytes()
}

impl PackedDecodedPcodeFunction {
    /// Encode every field without retaining or modifying the original graph.
    pub fn pack(decoded: &DecodedPcodeFunction) -> Result<Self> {
        Ok(Self {
            bytes: codec().serialize(decoded)?.into_boxed_slice(),
        })
    }

    /// Restore the exact canonical value. Does not perform decoding or inference.
    pub fn unpack(&self) -> Result<DecodedPcodeFunction> {
        Ok(codec().deserialize(&self.bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{DecodeStopReason, DecodedFlowKind, DecodedInstruction};
    use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
    use std::collections::{BTreeMap, BTreeSet};

    fn fixture(stop_reason: DecodeStopReason) -> DecodedPcodeFunction {
        let address = 0xffff_ffff_1234_5678;
        DecodedPcodeFunction {
            function: PcodeFunction {
                blocks: vec![PcodeBasicBlock {
                    index: 7,
                    start_address: address,
                    successors: vec![2, 0, 2],
                    ops: [
                        PcodeOpcode::Copy,
                        PcodeOpcode::FloatInt2Float,
                        PcodeOpcode::CallInd,
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, opcode)| PcodeOp {
                        seq_num: i as u32,
                        opcode,
                        address,
                        output: Some(Varnode {
                            space_id: u64::MAX,
                            offset: u64::MAX - 1,
                            size: 8,
                            is_constant: false,
                            constant_val: i64::MIN,
                        }),
                        inputs: vec![Varnode::constant(i64::MIN, 8), Varnode::constant(-1, 1)],
                        asm_mnemonic: [None, Some(String::new()), Some("λ".to_string())][i].clone(),
                    })
                    .collect(),
                }],
            },
            instructions: vec![DecodedInstruction {
                address,
                bytes: vec![0, 0xff, 0x7f],
                length: 3,
                mnemonic: "call".to_string(),
                operands_text: "λ".to_string(),
                flow_kind: DecodedFlowKind::Call,
                direct_target: Some(u64::MAX),
                references: vec![crate::runtime::DecodedReference {
                    target: u64::MAX,
                    kind: crate::runtime::DecodedReferenceKind::CallTarget,
                    operand_index: usize::MAX,
                }],
                pending_context_commits: vec![(address, 1, u32::MAX, 0x8000_0000)],
            }],
            decoded_instructions: 1,
            stop_reason,
            template_source_counts: BTreeMap::from([("SpecDerived".to_string(), 1)]),
            reachable_instruction_addresses: vec![address],
            instruction_lengths: BTreeMap::from([(address, 3)]),
            inferred_indirect_edges: BTreeMap::from([(address, vec![u64::MAX, 0])]),
            indirect_targets: BTreeSet::from([0, u64::MAX]),
        }
    }

    #[test]
    fn packed_decode_round_trips_every_field_and_stop_reason() {
        for reason in [
            DecodeStopReason::TerminalControlFlow,
            DecodeStopReason::InputExhausted,
            DecodeStopReason::InstructionLimit,
        ] {
            let decoded = fixture(reason);
            let archive = PackedDecodedPcodeFunction::pack(&decoded).unwrap();
            assert_eq!(archive.unpack().unwrap(), decoded);
            assert_eq!(
                PackedDecodedPcodeFunction::pack(&decoded).unwrap().bytes,
                archive.bytes
            );
        }
        let empty = DecodedPcodeFunction {
            function: PcodeFunction { blocks: Vec::new() },
            instructions: Vec::new(),
            decoded_instructions: 0,
            stop_reason: DecodeStopReason::TerminalControlFlow,
            template_source_counts: BTreeMap::new(),
            reachable_instruction_addresses: Vec::new(),
            instruction_lengths: BTreeMap::new(),
            inferred_indirect_edges: BTreeMap::new(),
            indirect_targets: BTreeSet::new(),
        };
        assert_eq!(
            PackedDecodedPcodeFunction::pack(&empty)
                .unwrap()
                .unpack()
                .unwrap(),
            empty
        );
    }

    #[test]
    fn packed_decode_rejects_malformed_private_payloads() {
        let archive =
            PackedDecodedPcodeFunction::pack(&fixture(DecodeStopReason::TerminalControlFlow))
                .unwrap();
        for length in 0..archive.bytes.len() {
            let truncated = PackedDecodedPcodeFunction {
                bytes: archive.bytes[..length].into(),
            };
            assert!(truncated.unpack().is_err());
        }
        let mut trailing = archive.bytes.into_vec();
        trailing.push(0);
        assert!(PackedDecodedPcodeFunction {
            bytes: trailing.into_boxed_slice()
        }
        .unpack()
        .is_err());
    }
}
