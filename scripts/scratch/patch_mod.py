import re
import sys

with open("crates/fission-pcode/src/nir/builder/materialize/mod.rs", "r") as f:
    content = f.read()

# Replace the loop in lower_block_stmts
old_lower_block_stmts = """    pub(in crate::nir) fn lower_block_stmts(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let terminator_index = self.block_terminator_index(block);
        let block_idx = self.lowering_block_index(block);
        body.extend(self.synthesize_explicit_merge_bindings_for_block(block)?);
        for (op_idx, op) in block.ops.iter().enumerate() {
            if Some(op_idx) == terminator_index {
                continue;
            }"""

new_lower_block_stmts = """    pub(in crate::nir) fn lower_block_stmts(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let terminator_index = self.block_terminator_index(block);
        body.extend(self.synthesize_explicit_merge_bindings_for_block(block)?);
        body.extend(self.lower_block_ops_range(block, 0, block.ops.len(), terminator_index)?);
        Ok(body)
    }

    pub(in crate::nir) fn lower_block_ops_range(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        start_idx: usize,
        end_idx: usize,
        terminator_index: Option<usize>,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let block_idx = self.lowering_block_index(block);
        
        let mut op_idx = start_idx;
        while op_idx < end_idx {
            let op = &block.ops[op_idx];
            if Some(op_idx) == terminator_index {
                op_idx += 1;
                continue;
            }"""

if old_lower_block_stmts not in content:
    print("Could not find old_lower_block_stmts")
    sys.exit(1)

content = content.replace(old_lower_block_stmts, new_lower_block_stmts)

# Find the end of the for loop
old_end = """        if self.emit_ready_trace_enabled_for_current_fn() {
            self.emit_ready_trace(format!(
                "materialized-output-binding block=0x{:x} op_seq={} output=space:{} off:0x{:x} size:{} lhs={} rhs={:?}",
                block_addr,
                op.seq_num,
                output.space_id,
                output.offset,
                output.size,
                lhs_name,
                rhs,
            ));
        }
        let lhs = HirLValue::Var(lhs_name);
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }"""

new_end = """        if self.emit_ready_trace_enabled_for_current_fn() {
            self.emit_ready_trace(format!(
                "materialized-output-binding block=0x{:x} op_seq={} output=space:{} off:0x{:x} size:{} lhs={} rhs={:?}",
                block_addr,
                op.seq_num,
                output.space_id,
                output.offset,
                output.size,
                lhs_name,
                rhs,
            ));
        }
        let lhs = HirLValue::Var(lhs_name);
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }"""

if old_end not in content:
    print("Could not find old_end")
    sys.exit(1)

# Now, we need to replace the end of lower_block_stmts
# The original code has:
#         }
#         Ok(body)
#     }
# 
#     fn live_register_lhs_name_for_passthrough_join_store_producer(

old_tail = """        let lhs = HirLValue::Var(lhs_name);
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }

    fn live_register_lhs_name_for_passthrough_join_store_producer("""

new_tail = """        let lhs = HirLValue::Var(lhs_name);
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }

    fn live_register_lhs_name_for_passthrough_join_store_producer("""

# Actually we just need to replace the loop closing `}` and `Ok(body)` inside the file.
# The `for (op_idx, op) in block.ops.iter().enumerate() {` was closed with:
old_loop_close = """            if let Some(stmt) = maybe_stmt? {
                body.push(stmt);
            }
        }
        Ok(body)
    }

    fn live_register_lhs_name_for_passthrough_join_store_producer("""

new_loop_close = """            if let Some(stmt) = maybe_stmt? {
                body.push(stmt);
            }
            op_idx += 1;
        }
        Ok(body)
    }

    fn live_register_lhs_name_for_passthrough_join_store_producer("""

if old_loop_close not in content:
    print("Could not find old_loop_close")
    sys.exit(1)

content = content.replace(old_loop_close, new_loop_close)

with open("crates/fission-pcode/src/nir/builder/materialize/mod.rs", "w") as f:
    f.write(content)

print("Patched mod.rs successfully")
