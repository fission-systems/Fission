//! Call Graph Relation Validator
//!
//! Validates FID matches by checking call graph relationships,
//! similar to Ghidra's FidProgramSeeker relation matching.

use crate::signature::FunctionSignature;

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

/// The minimal graph capability needed by legacy name-based signatures.
///
/// The concrete call graph belongs to the static-analysis crate, where xrefs
/// and function metadata are owned. Keeping only this capability here avoids a
/// second graph with a different edge representation in the signatures crate.
pub trait CallGraphView {
    /// Return whether `function` calls a function with `name`.
    fn has_callee_named(&self, function: u64, name: &str) -> bool;

    /// Return whether a function with `name` calls `function`.
    fn has_caller_named(&self, function: u64, name: &str) -> bool;
}

/// Validate a signature match against the call graph
///
/// This implements Ghidra FID-style relation matching:
/// 1. If signature has expected_callees, check if function calls any of them
/// 2. If signature has expected_callers, check if any expected caller calls this function
/// 3. If force_relation is set and no callees found, reject the match
/// 4. Adjust confidence based on relation matches
pub fn validate_relation<G: CallGraphView + ?Sized>(
    sig: &FunctionSignature,
    func_addr: u64,
    call_graph: &G,
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

    let mut matched_callees: Vec<String> = Vec::new();
    let mut matched_callers: Vec<String> = Vec::new();

    // Check expected callees
    for expected in &sig.expected_callees {
        if call_graph.has_callee_named(func_addr, expected) {
            matched_callees.push(expected.clone());
        }
    }

    // Check expected callers
    for expected in &sig.expected_callers {
        if call_graph.has_caller_named(func_addr, expected) {
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
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct TestGraph {
        callees: HashMap<u64, HashSet<String>>,
        callers: HashMap<u64, HashSet<String>>,
    }

    impl CallGraphView for TestGraph {
        fn has_callee_named(&self, function: u64, name: &str) -> bool {
            self.callees
                .get(&function)
                .is_some_and(|names| names.contains(name))
        }

        fn has_caller_named(&self, function: u64, name: &str) -> bool {
            self.callers
                .get(&function)
                .is_some_and(|names| names.contains(name))
        }
    }

    #[test]
    fn test_call_graph_view_basic() {
        let graph = TestGraph {
            callees: HashMap::from([(
                0x1000,
                HashSet::from(["malloc".to_string(), "free".to_string()]),
            )]),
            ..Default::default()
        };
        assert!(graph.has_callee_named(0x1000, "malloc"));
        assert!(graph.has_callee_named(0x1000, "free"));
        assert!(!graph.has_callee_named(0x1000, "realloc"));
    }

    #[test]
    fn test_relation_validation_pass() {
        let sig = FunctionSignature::from_hex("_malloc_base", "48 89 5C 24")
            .with_callees(&["HeapAlloc", "GetProcessHeap"]);

        let graph = TestGraph {
            callees: HashMap::from([(0x1000, HashSet::from(["HeapAlloc".to_string()]))]),
            ..Default::default()
        };

        let result = validate_relation(&sig, 0x1000, &graph);
        assert!(result.passed);
        assert!(!result.matched_callees.is_empty());
    }

    #[test]
    fn test_relation_validation_force_fail() {
        let sig = FunctionSignature::from_hex("_malloc_base", "48 89 5C 24")
            .with_callees(&["HeapAlloc", "GetProcessHeap"])
            .force_relation();

        let graph = TestGraph::default();

        let result = validate_relation(&sig, 0x1000, &graph);
        assert!(!result.passed);
        assert!(result.reason.is_some());
    }
}
