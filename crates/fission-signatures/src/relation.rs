//! Call Graph Relation Validator
//!
//! Validates FID matches by checking call graph relationships,
//! similar to Ghidra's FidProgramSeeker relation matching.

use crate::signature::FunctionSignature;
use std::collections::{HashMap, HashSet};

/// Result of a relation validation check
#[derive(Debug, Clone)]
pub struct RelationValidation {
    /// Whether the relation check passed
    pub passed: bool,
    /// Adjusted confidence score (0-100)
    pub confidence: u8,
    /// Names of expected callees that were found
    pub matched_callees: Vec<String>,
    /// Names of expected callers that were found
    pub matched_callers: Vec<String>,
    /// Reason if validation failed
    pub reason: Option<String>,
}

/// Call Graph for a binary, used to validate FID matches
pub struct CallGraph {
    /// Map of function address to list of addresses it calls
    pub callees: HashMap<u64, HashSet<u64>>,
    /// Map of function address to list of addresses that call it
    pub callers: HashMap<u64, HashSet<u64>>,
    /// Map of function address to resolved name (if known)
    pub function_names: HashMap<u64, String>,
    /// Reverse map: name to address
    name_to_addr: HashMap<String, u64>,
}

impl CallGraph {
    /// Create a new empty call graph
    pub fn new() -> Self {
        Self {
            callees: HashMap::new(),
            callers: HashMap::new(),
            function_names: HashMap::new(),
            name_to_addr: HashMap::new(),
        }
    }

    /// Add a call edge from caller_addr to callee_addr
    pub fn add_call(&mut self, caller_addr: u64, callee_addr: u64) {
        self.callees
            .entry(caller_addr)
            .or_insert_with(HashSet::new)
            .insert(callee_addr);
        self.callers
            .entry(callee_addr)
            .or_insert_with(HashSet::new)
            .insert(caller_addr);
    }

    /// Set the name for a function address
    pub fn set_function_name(&mut self, addr: u64, name: String) {
        if !name.is_empty() {
            self.name_to_addr.insert(name.clone(), addr);
            self.function_names.insert(addr, name);
        }
    }

    /// Get callees of a function by address
    pub fn get_callees(&self, addr: u64) -> Option<&HashSet<u64>> {
        self.callees.get(&addr)
    }

    /// Get callers of a function by address
    pub fn get_callers(&self, addr: u64) -> Option<&HashSet<u64>> {
        self.callers.get(&addr)
    }

    /// Look up function address by name
    pub fn get_addr_by_name(&self, name: &str) -> Option<u64> {
        self.name_to_addr.get(name).copied()
    }

    /// Look up function name by address
    pub fn get_name_by_addr(&self, addr: u64) -> Option<&String> {
        self.function_names.get(&addr)
    }

    /// Get the set of callee names for a function
    pub fn get_callee_names(&self, addr: u64) -> HashSet<String> {
        let mut names = HashSet::new();
        if let Some(callee_addrs) = self.callees.get(&addr) {
            for &callee_addr in callee_addrs {
                if let Some(name) = self.function_names.get(&callee_addr) {
                    names.insert(name.clone());
                }
            }
        }
        names
    }

    /// Get the set of caller names for a function
    pub fn get_caller_names(&self, addr: u64) -> HashSet<String> {
        let mut names = HashSet::new();
        if let Some(caller_addrs) = self.callers.get(&addr) {
            for &caller_addr in caller_addrs {
                if let Some(name) = self.function_names.get(&caller_addr) {
                    names.insert(name.clone());
                }
            }
        }
        names
    }
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a signature match against the call graph
///
/// This implements Ghidra FID-style relation matching:
/// 1. If signature has expected_callees, check if function calls any of them
/// 2. If signature has expected_callers, check if any expected caller calls this function
/// 3. If force_relation is set and no callees found, reject the match
/// 4. Adjust confidence based on relation matches
pub fn validate_relation(
    sig: &FunctionSignature,
    func_addr: u64,
    call_graph: &CallGraph,
) -> RelationValidation {
    // If no relation constraints, pass with full confidence
    if sig.expected_callees.is_empty() && sig.expected_callers.is_empty() {
        return RelationValidation {
            passed: true,
            confidence: sig.confidence,
            matched_callees: Vec::new(),
            matched_callers: Vec::new(),
            reason: None,
        };
    }

    let callee_names = call_graph.get_callee_names(func_addr);
    let caller_names = call_graph.get_caller_names(func_addr);

    let mut matched_callees: Vec<String> = Vec::new();
    let mut matched_callers: Vec<String> = Vec::new();

    // Check expected callees
    for expected in &sig.expected_callees {
        if callee_names.contains(expected) {
            matched_callees.push(expected.clone());
        }
    }

    // Check expected callers
    for expected in &sig.expected_callers {
        if caller_names.contains(expected) {
            matched_callers.push(expected.clone());
        }
    }

    // Determine if validation passes
    let callee_check_required = !sig.expected_callees.is_empty();
    let caller_check_required = !sig.expected_callers.is_empty();

    let callee_ok = !callee_check_required || !matched_callees.is_empty();
    let caller_ok = !caller_check_required || !matched_callers.is_empty();

    // Force relation: require at least one callee match
    if sig.force_relation && callee_check_required && matched_callees.is_empty() {
        return RelationValidation {
            passed: false,
            confidence: 0,
            matched_callees,
            matched_callers,
            reason: Some(format!(
                "force_relation: no expected callees found (expected: {:?})",
                sig.expected_callees
            )),
        };
    }

    let passed = callee_ok && caller_ok;

    // Calculate adjusted confidence
    let mut confidence = sig.confidence;

    if callee_check_required {
        let callee_ratio = matched_callees.len() as f32 / sig.expected_callees.len() as f32;
        // Reduce confidence by up to 30% based on callee match ratio
        confidence = (confidence as f32 * (0.7 + 0.3 * callee_ratio)) as u8;
    }

    if caller_check_required {
        let caller_ratio = matched_callers.len() as f32 / sig.expected_callers.len() as f32;
        // Reduce confidence by up to 20% based on caller match ratio
        confidence = (confidence as f32 * (0.8 + 0.2 * caller_ratio)) as u8;
    }

    let reason = if !passed {
        Some("relation check failed".to_string())
    } else {
        None
    };

    RelationValidation {
        passed,
        confidence,
        matched_callees,
        matched_callers,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_graph_basic() {
        let mut cg = CallGraph::new();
        cg.add_call(0x1000, 0x2000);
        cg.add_call(0x1000, 0x3000);
        cg.set_function_name(0x2000, "malloc".to_string());
        cg.set_function_name(0x3000, "free".to_string());

        let Some(callees) = cg.get_callees(0x1000) else {
            panic!("callees for caller 0x1000 should exist")
        };
        assert!(callees.contains(&0x2000));
        assert!(callees.contains(&0x3000));

        let callee_names = cg.get_callee_names(0x1000);
        assert!(callee_names.contains("malloc"));
        assert!(callee_names.contains("free"));
    }

    #[test]
    fn test_relation_validation_pass() {
        let sig = FunctionSignature::from_hex("_malloc_base", "48 89 5C 24")
            .with_callees(&["HeapAlloc", "GetProcessHeap"]);

        let mut cg = CallGraph::new();
        cg.add_call(0x1000, 0x2000);
        cg.set_function_name(0x2000, "HeapAlloc".to_string());

        let result = validate_relation(&sig, 0x1000, &cg);
        assert!(result.passed);
        assert!(!result.matched_callees.is_empty());
    }

    #[test]
    fn test_relation_validation_force_fail() {
        let sig = FunctionSignature::from_hex("_malloc_base", "48 89 5C 24")
            .with_callees(&["HeapAlloc", "GetProcessHeap"])
            .force_relation();

        let cg = CallGraph::new(); // Empty call graph

        let result = validate_relation(&sig, 0x1000, &cg);
        assert!(!result.passed);
        assert!(result.reason.is_some());
    }
}
