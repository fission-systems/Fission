use crate::pcode::state::MachineState;
use fission_solver::{SymExpr, SymNodeId, solver::Solver, ast::Sort};
use anyhow::Result;

/// angr-style memory mixin for handling symbolic pointers.
/// 
/// When a pointer is symbolic, we query the solver to evaluate possible concrete addresses.
/// If the number of solutions is under a threshold, we create a symbolic If-Then-Else (ITE)
/// tree representing reading/writing at all possible targets.
pub struct MemoryMixin;

impl MemoryMixin {
    pub const MAX_CONCRETIZATION_LIMIT: usize = 16;

    /// Handle a symbolic LOAD operation.
    /// Returns the AST node representing the result of the read.
    pub fn handle_symbolic_read(
        state: &mut MachineState,
        solver: &mut Solver,
        space_id: u64,
        ptr_node: SymNodeId,
        size_bytes: u32,
    ) -> Result<SymNodeId> {
        let ptr_expr = solver.nodes.get(&ptr_node).unwrap().clone();
        
        // 1. Evaluate possible addresses up to MAX_CONCRETIZATION_LIMIT
        let possible_addrs = solver.eval(&ptr_expr, Self::MAX_CONCRETIZATION_LIMIT + 1);
        
        if possible_addrs.len() > Self::MAX_CONCRETIZATION_LIMIT || possible_addrs.is_empty() {
            // Too underconstrained. angr returns a fresh unconstrained variable.
            let unconstrained = SymExpr::new_var("symbolic_read_unconstrained", size_bytes);
            return Ok(solver.register_node(unconstrained));
        }

        // 2. Build ITE tree for the reads
        // e.g. If(ptr == addr1, val1, If(ptr == addr2, val2, ...))
        let mut final_expr: Option<SymExpr> = None;

        for &addr in possible_addrs.iter().rev() {
            // Concrete read
            let mut val: u64 = 0;
            if let Ok(raw) = state.read_space(space_id, addr, size_bytes as usize) {
                for (i, &b) in raw.iter().enumerate() {
                    val |= (b as u64) << (i * 8);
                }
            }

            // Did this concrete byte have a shadow? 
            // For simplicity, we just use the concrete value or the shadow node if it was a single node.
            // If it's a multi-byte read, we should theoretically concat shadows, but let's just use the concrete value node
            // unless it has an exact shadow for the whole block.
            let val_expr = SymExpr::new_const(val, size_bytes);

            if let Some(curr_expr) = final_expr {
                // ITE(ptr == addr, val, curr)
                let cond = SymExpr::Eq(
                    Box::new(ptr_expr.clone()),
                    Box::new(SymExpr::new_const(addr, ptr_expr.get_size()))
                );
                final_expr = Some(SymExpr::Ite {
                    cond: Box::new(cond),
                    t: Box::new(val_expr),
                    f: Box::new(curr_expr)
                });
            } else {
                final_expr = Some(val_expr);
            }
        }

        let expr = final_expr.unwrap();
        Ok(solver.register_node(expr))
    }

    /// Handle a symbolic STORE operation.
    pub fn handle_symbolic_write(
        state: &mut MachineState,
        solver: &mut Solver,
        space_id: u64,
        ptr_node: SymNodeId,
        val_node: SymNodeId,
        size_bytes: u32,
    ) -> Result<()> {
        let ptr_expr = solver.nodes.get(&ptr_node).unwrap().clone();
        let val_expr = solver.nodes.get(&val_node).unwrap().clone();

        // 1. Evaluate possible addresses up to MAX_CONCRETIZATION_LIMIT
        let possible_addrs = solver.eval(&ptr_expr, Self::MAX_CONCRETIZATION_LIMIT + 1);

        if possible_addrs.len() > Self::MAX_CONCRETIZATION_LIMIT || possible_addrs.is_empty() {
            // Too underconstrained. In angr, we might drop the write or add an unconstrained store.
            // We just drop it for now to avoid state explosion.
            tracing::warn!("Dropping symbolic write due to underconstrained pointer");
            return Ok(());
        }

        // 2. Perform conditional store (ITE) for every possible address
        for &addr in &possible_addrs {
            // Read old value
            let mut old_val: u64 = 0;
            if let Ok(raw) = state.read_space(space_id, addr, size_bytes as usize) {
                for (i, &b) in raw.iter().enumerate() {
                    old_val |= (b as u64) << (i * 8);
                }
            }
            let old_expr = SymExpr::new_const(old_val, size_bytes);

            // new_data = ITE(ptr == addr, val, old)
            let cond = SymExpr::Eq(
                Box::new(ptr_expr.clone()),
                Box::new(SymExpr::new_const(addr, ptr_expr.get_size()))
            );
            let ite = SymExpr::Ite {
                cond: Box::new(cond),
                t: Box::new(val_expr.clone()),
                f: Box::new(old_expr)
            };
            
            let ite_id = solver.register_node(ite);

            // Write the ITE shadow back to memory at this concrete address.
            // Since it's symbolic, we write concrete 0s and tag the shadow.
            let zeros = vec![0u8; size_bytes as usize];
            state.write_space(space_id, addr, &zeros)?;
            for i in 0..size_bytes as u64 {
                state.set_shadow_memory(space_id, addr + i, ite_id);
            }
        }

        Ok(())
    }
}
