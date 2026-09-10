//! Where a byte came from.
//!
//! Taint is the same machinery the concolic path already uses -- a label per
//! byte, propagated through copy/load/store/arithmetic -- with a cheaper
//! domain hung off it. The symbolic domain answers "what expression produced
//! this byte" and pays for a solver AST per operation. Taint answers only
//! "which sources is this byte derived from", which is a set union.
//!
//! Ghidra draws the same line: `AuxPcodeEmulator` parameterises its executor
//! over an *arithmetic*, and its taint extension is a `TaintPcodeArithmetic`
//! whose values are `TaintSet`s. This is that idea against the shadow layer
//! that was already here.
//!
//! # Why sets are interned
//!
//! The shadow layer stores one `u32` per byte, so a byte cannot carry a set
//! directly. Interning gives set identity: the id *is* the set, union is a
//! lookup, and equal sets are the same id -- which matters, because a loop
//! that keeps combining the same two sources must stop allocating after the
//! first iteration or the table grows with the run rather than with the
//! program.

use std::collections::HashMap;

/// Something a run treated as untrusted input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaintSource {
    /// What it was, for a report: `read(fd 3)`, `argv[1]`, `getrandom`.
    pub label: String,
    /// Where the run was when the source appeared.
    pub pc: u64,
    /// Guest address range the source covered.
    pub addr: u64,
    pub len: u64,
}

/// Tainted data reaching somewhere worth reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaintHit {
    pub pc: u64,
    /// What was reached: `"syscall arg"`, `"indirect branch"`.
    pub sink: String,
    /// Detail for the sink -- the syscall name, say.
    pub detail: String,
    /// Labels of the sources this value derives from.
    pub sources: Vec<String>,
}

/// The taint domain: sources, the interned sets over them, and what reached a
/// sink.
#[derive(Debug, Default)]
pub struct TaintState {
    sources: Vec<TaintSource>,
    /// Set id → sorted source indices. Index 0 is deliberately the empty set,
    /// so a set id of 0 can never be confused with "one source, the first one".
    sets: Vec<Vec<u32>>,
    interner: HashMap<Vec<u32>, u32>,
    pub hits: Vec<TaintHit>,
    /// Hits past the cap. A tainted value in a loop reaches the same sink
    /// every iteration, and a report that grows with the run is unreadable.
    pub hits_dropped: u64,
    hit_cap: usize,
}

impl TaintState {
    pub const DEFAULT_HIT_CAP: usize = 4096;

    pub fn new() -> Self {
        let mut state = Self {
            hit_cap: Self::DEFAULT_HIT_CAP,
            ..Self::default()
        };
        // Set 0 is the empty set.
        state.sets.push(Vec::new());
        state.interner.insert(Vec::new(), 0);
        state
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.hits.is_empty()
    }

    pub fn sources(&self) -> &[TaintSource] {
        &self.sources
    }

    /// Declare a source and return the set id naming just it.
    pub fn add_source(&mut self, source: TaintSource) -> u32 {
        let index = self.sources.len() as u32;
        self.sources.push(source);
        self.intern(vec![index])
    }

    /// The set id for the union of two sets. Either may be `None`, meaning
    /// clean; the union of two clean values is clean.
    pub fn union(&mut self, a: Option<u32>, b: Option<u32>) -> Option<u32> {
        match (a, b) {
            (None, None) => None,
            (Some(x), None) | (None, Some(x)) => Some(x),
            (Some(x), Some(y)) if x == y => Some(x),
            (Some(x), Some(y)) => {
                let mut merged = self.set(x).to_vec();
                merged.extend_from_slice(self.set(y));
                merged.sort_unstable();
                merged.dedup();
                Some(self.intern(merged))
            }
        }
    }

    /// The source labels behind a set id, for a report.
    pub fn labels(&self, set: u32) -> Vec<String> {
        self.set(set)
            .iter()
            .filter_map(|i| self.sources.get(*i as usize))
            .map(|s| s.label.clone())
            .collect()
    }

    /// Record tainted data reaching a sink.
    pub fn hit(&mut self, pc: u64, sink: &str, detail: String, set: u32) {
        if self.hits.len() >= self.hit_cap {
            self.hits_dropped = self.hits_dropped.saturating_add(1);
            return;
        }
        let sources = self.labels(set);
        let hit = TaintHit {
            pc,
            sink: sink.to_string(),
            detail,
            sources,
        };
        // A loop reaching the same sink with the same sources every iteration
        // is one finding, not a thousand.
        if self.hits.last() == Some(&hit) {
            return;
        }
        self.hits.push(hit);
    }

    fn set(&self, id: u32) -> &[u32] {
        self.sets.get(id as usize).map(|v| &v[..]).unwrap_or(&[])
    }

    fn intern(&mut self, sorted: Vec<u32>) -> u32 {
        if let Some(id) = self.interner.get(&sorted) {
            return *id;
        }
        let id = self.sets.len() as u32;
        self.interner.insert(sorted.clone(), id);
        self.sets.push(sorted);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(label: &str) -> TaintSource {
        TaintSource {
            label: label.to_string(),
            pc: 0x1000,
            addr: 0x2000,
            len: 8,
        }
    }

    #[test]
    fn the_same_union_is_the_same_id() {
        let mut t = TaintState::new();
        let a = t.add_source(source("read(fd 3)"));
        let b = t.add_source(source("argv[1]"));

        let first = t.union(Some(a), Some(b));
        let again = t.union(Some(b), Some(a));
        assert_eq!(
            first, again,
            "union must not depend on order, or a loop allocates for ever"
        );

        let sets_before = t.sets.len();
        for _ in 0..100 {
            t.union(Some(a), Some(b));
        }
        assert_eq!(
            t.sets.len(),
            sets_before,
            "repeating a union must allocate nothing"
        );
    }

    #[test]
    fn clean_stays_clean_and_a_union_names_both_sources() {
        let mut t = TaintState::new();
        assert_eq!(t.union(None, None), None);

        let a = t.add_source(source("read(fd 3)"));
        assert_eq!(t.union(Some(a), None), Some(a));

        let b = t.add_source(source("argv[1]"));
        let both = t.union(Some(a), Some(b)).expect("tainted");
        let mut labels = t.labels(both);
        labels.sort();
        assert_eq!(labels, vec!["argv[1]", "read(fd 3)"]);
    }

    #[test]
    fn a_loop_hitting_one_sink_is_one_finding() {
        let mut t = TaintState::new();
        let a = t.add_source(source("read(fd 3)"));
        for _ in 0..50 {
            t.hit(0x400100, "indirect branch", "jmp rax".into(), a);
        }
        assert_eq!(t.hits.len(), 1, "consecutive identical hits collapse");

        t.hit(0x400200, "syscall arg", "write".into(), a);
        t.hit(0x400100, "indirect branch", "jmp rax".into(), a);
        assert_eq!(
            t.hits.len(),
            3,
            "a different sink in between is a new finding"
        );
    }

    #[test]
    fn the_hit_log_is_bounded_and_says_how_much_it_dropped() {
        let mut t = TaintState::new();
        t.hit_cap = 2;
        let a = t.add_source(source("s"));
        for i in 0..10 {
            t.hit(0x1000 + i, "syscall arg", format!("call{i}"), a);
        }
        assert_eq!(t.hits.len(), 2);
        assert_eq!(t.hits_dropped, 8);
    }
}
