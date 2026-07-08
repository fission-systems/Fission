use std::collections::HashMap;
use crate::sym::state::SimState;

/// An exploration technique for the Simulation Manager.
/// Techniques can reorder, stash, or filter states after every step.
pub trait ExplorationTechnique: Send + Sync {
    /// Called when the technique is added to the manager.
    fn setup(&mut self, _stashes: &mut HashMap<String, Vec<SimState>>) {}

    /// Called right after the standard step loop.
    /// Can move states between stashes.
    fn step(&mut self, stashes: &mut HashMap<String, Vec<SimState>>);

    /// Returns true if the exploration goal is achieved.
    fn is_complete(&self, _stashes: &HashMap<String, Vec<SimState>>) -> bool {
        false
    }
}

/// Depth-First Search exploration technique.
/// Keeps only 1 state in the `active` stash at a time, moving the rest to `deferred`.
/// When `active` is empty, it pops one state from `deferred`.
pub struct DFS;

impl ExplorationTechnique for DFS {
    fn step(&mut self, stashes: &mut HashMap<String, Vec<SimState>>) {
        // Keep only 1 state in active, move rest to deferred
        if let Some(active) = stashes.get_mut("active") {
            if active.len() > 1 {
                let rest = active.split_off(1);
                stashes.entry("deferred".to_string()).or_default().extend(rest);
            }
        }
        
        // If active is empty, pop from deferred
        if stashes.get("active").map(|a| a.is_empty()).unwrap_or(true) {
            if let Some(deferred) = stashes.get_mut("deferred") {
                if let Some(state) = deferred.pop() {
                    stashes.entry("active".to_string()).or_default().push(state);
                }
            }
        }
    }
}

/// Directed Search (Target / Avoid).
/// Moves any state hitting the `target` address to the `found` stash.
/// Moves any state hitting any `avoid` address to the `avoid` stash.
pub struct DirectedSearch {
    pub target: u64,
    pub avoid: Vec<u64>,
}

impl DirectedSearch {
    pub fn new(target: u64, avoid: Vec<u64>) -> Self {
        Self { target, avoid }
    }
}

impl ExplorationTechnique for DirectedSearch {
    fn step(&mut self, stashes: &mut HashMap<String, Vec<SimState>>) {
        let active = stashes.remove("active").unwrap_or_default();
        let mut found = Vec::new();
        let mut avoided = Vec::new();
        let mut next_active = Vec::new();
        
        for state in active {
            if state.pc == self.target {
                tracing::info!("DirectedSearch: Found target at 0x{:X}", self.target);
                found.push(state);
            } else if self.avoid.contains(&state.pc) {
                tracing::info!("DirectedSearch: Avoiding state at 0x{:X}", state.pc);
                avoided.push(state);
            } else {
                next_active.push(state);
            }
        }
        
        stashes.entry("found".to_string()).or_default().extend(found);
        stashes.entry("avoid".to_string()).or_default().extend(avoided);
        stashes.insert("active".to_string(), next_active);
    }

    fn is_complete(&self, stashes: &HashMap<String, Vec<SimState>>) -> bool {
        stashes.get("found").map(|f| !f.is_empty()).unwrap_or(false)
    }
}
