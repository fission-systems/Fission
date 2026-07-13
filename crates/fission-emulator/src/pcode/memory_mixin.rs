use crate::pcode::state::MachineState;
use fission_solver::{solver::Solver, SymExpr, SymNodeId};
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
        
        let array_id = if let Some(id) = state.get_theory_array_id(space_id) {
            id
        } else {
            let arr_expr = SymExpr::new_array_var(&format!("space_{}", space_id), ptr_expr.get_size(), 8);
            let id = solver.register_node(arr_expr);
            state.set_theory_array_id(space_id, id);
            id
        };
        
        let array_expr = solver.nodes.get(&array_id).unwrap().clone();
        
        // Single byte read vs Multi-byte read.
        // Memory arrays map addresses to bytes. If size_bytes > 1, we must concat multiple ArraySelects.
        let mut final_expr = SymExpr::ArraySelect {
            array: Box::new(array_expr.clone()),
            index: Box::new(ptr_expr.clone()),
        };
        
        for i in 1..size_bytes {
            let offset = SymExpr::new_const(i as u64, ptr_expr.get_size());
            let next_ptr = SymExpr::new_add(ptr_expr.clone(), offset);
            let next_byte = SymExpr::ArraySelect {
                array: Box::new(array_expr.clone()),
                index: Box::new(next_ptr),
            };
            // LE concat: next_byte ++ final_expr
            final_expr = SymExpr::Concat(Box::new(next_byte), Box::new(final_expr));
        }

        Ok(solver.register_node(final_expr))
    }

    /// Handle a symbolic STORE operation using Pure SMT Array Theory.
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

        let array_id = if let Some(id) = state.get_theory_array_id(space_id) {
            id
        } else {
            let arr_expr = SymExpr::new_array_var(&format!("space_{}", space_id), ptr_expr.get_size(), 8);
            let id = solver.register_node(arr_expr);
            state.set_theory_array_id(space_id, id);
            id
        };

        let mut current_array = solver.nodes.get(&array_id).unwrap().clone();
        
        // If writing multiple bytes, we need multiple ArrayStores
        for i in 0..size_bytes {
            let offset = SymExpr::new_const(i as u64, ptr_expr.get_size());
            let next_ptr = SymExpr::new_add(ptr_expr.clone(), offset);
            
            // Extract byte i from val_expr
            let byte_val = if size_bytes == 1 {
                val_expr.clone()
            } else {
                SymExpr::Extract { expr: Box::new(val_expr.clone()), lsb: i * 8, size: 8 }
            };
            
            current_array = SymExpr::ArrayStore {
                array: Box::new(current_array),
                index: Box::new(next_ptr),
                value: Box::new(byte_val),
            };
        }

        let new_id = solver.register_node(current_array);
        state.set_theory_array_id(space_id, new_id);

        Ok(())
    }
}
