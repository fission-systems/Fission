use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

#[derive(Debug, Clone)]
pub struct RuntimePcodeEmitter {
    address: u64,
    seq: u32,
    next_tmp: u64,
    ops: Vec<PcodeOp>,
}

impl RuntimePcodeEmitter {
    pub fn new(address: u64, unique_seed: u64) -> Self {
        Self {
            address,
            seq: 0,
            next_tmp: unique_seed,
            ops: Vec::new(),
        }
    }

    pub fn finish(self) -> Vec<PcodeOp> {
        self.ops
    }

    pub fn tmp(&mut self, space_id: u64, size: u32) -> Varnode {
        let vn = Varnode {
            space_id,
            offset: self.next_tmp,
            size,
            is_constant: false,
            constant_val: 0,
        };
        self.next_tmp = self.next_tmp.wrapping_add(8);
        vn
    }

    pub fn push(
        &mut self,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
        mnemonic: &str,
    ) {
        self.ops.push(PcodeOp {
            seq_num: self.seq,
            opcode,
            address: self.address,
            output,
            inputs,
            asm_mnemonic: Some(mnemonic.to_string()),
        });
        self.seq = self.seq.saturating_add(1);
    }
}
