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

use serde::{Deserialize, Serialize};

/// Whether a sink depends on the source through a value or through a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaintDependencyKind {
    Data,
    Control,
}

impl Default for TaintDependencyKind {
    fn default() -> Self {
        Self::Data
    }
}

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
    pub kind: TaintDependencyKind,
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
    /// Set id → sorted provenance members. Even members are direct-data source
    /// indices; the following odd member denotes that source as control flow.
    /// Index 0 is deliberately the empty set.
    sets: Vec<Vec<u32>>,
    interner: HashMap<Vec<u32>, u32>,
    control_scopes: Vec<ControlScope>,
    /// Proven control scopes refused because the active-scope bound was full.
    control_scopes_dropped: u64,
    pub hits: Vec<TaintHit>,
    /// Hits past the cap. A tainted value in a loop reaches the same sink
    /// every iteration, and a report that grows with the run is unreadable.
    pub hits_dropped: u64,
    hit_cap: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlScope {
    branch_pc: u64,
    reconvergence_pc: u64,
    source_set: u32,
}

impl TaintState {
    pub const DEFAULT_HIT_CAP: usize = 4096;
    /// Maximum number of simultaneously tracked implicit-flow scopes.
    pub const DEFAULT_CONTROL_SCOPE_CAP: usize = 128;

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

    /// Number of proven control-scope activations omitted because the active
    /// scope bound was full. A non-zero value means control-taint tracking for
    /// this execution is incomplete.
    pub fn control_scopes_dropped(&self) -> u64 {
        self.control_scopes_dropped
    }

    pub fn control_tracking_complete(&self) -> bool {
        self.control_scopes_dropped == 0
    }

    /// Declare a source and return the set id naming just it.
    pub fn add_source(&mut self, source: TaintSource) -> u32 {
        let index = u32::try_from(self.sources.len())
            .expect("taint source count exceeds the u32 source-index limit");
        self.sources.push(source);
        self.intern(vec![
            index.checked_mul(2).expect("taint source id overflow"),
        ])
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
        let mut labels = Vec::new();
        for member in self.set(set) {
            if let Some(source) = self.sources.get((member / 2) as usize)
                && !labels.contains(&source.label)
            {
                labels.push(source.label.clone());
            }
        }
        labels
    }

    /// Record tainted data reaching a sink.
    pub fn hit(&mut self, pc: u64, sink: &str, detail: String, set: u32) {
        self.hit_kind(pc, sink, detail, set, TaintDependencyKind::Data);
    }

    /// Record control dependence reaching a sink.
    pub fn hit_control(&mut self, pc: u64, sink: &str, detail: String, set: u32) {
        self.hit_kind(pc, sink, detail, set, TaintDependencyKind::Control);
    }

    fn hit_kind(
        &mut self,
        pc: u64,
        sink: &str,
        detail: String,
        set: u32,
        kind: TaintDependencyKind,
    ) {
        let control = matches!(kind, TaintDependencyKind::Control);
        let sources = self
            .set(set)
            .iter()
            .filter(|member| (**member % 2 == 1) == control)
            .filter_map(|member| self.sources.get((member / 2) as usize))
            .map(|source| source.label.clone())
            .fold(Vec::new(), |mut labels, label| {
                if !labels.contains(&label) {
                    labels.push(label);
                }
                labels
            });
        if sources.is_empty() {
            return;
        }
        if self.hits.len() >= self.hit_cap {
            self.hits_dropped = self.hits_dropped.saturating_add(1);
            return;
        }
        let hit = TaintHit {
            pc,
            kind,
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

    /// Convert every source contributing to a branch predicate into control
    /// provenance. A later data operation may carry both kinds independently.
    pub fn controlize(&mut self, set: u32) -> Option<u32> {
        let mut members = self
            .set(set)
            .iter()
            .map(|member| (member / 2) * 2 + 1)
            .collect::<Vec<_>>();
        members.sort_unstable();
        members.dedup();
        (!members.is_empty()).then(|| self.intern(members))
    }

    /// Keep only control-provenance members from a value's shadow set.
    pub fn control_projection(&mut self, set: u32) -> Option<u32> {
        let members = self
            .set(set)
            .iter()
            .copied()
            .filter(|member| member % 2 == 1)
            .collect::<Vec<_>>();
        (!members.is_empty()).then(|| self.intern(members))
    }

    /// Keep only direct-data provenance from a shadow set.
    pub fn data_projection(&mut self, set: u32) -> Option<u32> {
        let members = self
            .set(set)
            .iter()
            .copied()
            .filter(|member| member % 2 == 0)
            .collect::<Vec<_>>();
        (!members.is_empty()).then(|| self.intern(members))
    }

    /// Start a bounded implicit-flow scope. Its taint is attached only to
    /// values written before the proven reconvergence, not to all later state.
    pub fn begin_control_scope(&mut self, branch_pc: u64, join_pc: u64, predicate_set: u32) {
        if branch_pc == join_pc {
            return;
        }
        let Some(source_set) = self.controlize(predicate_set) else {
            return;
        };
        if let Some(index) = self
            .control_scopes
            .iter()
            .position(|scope| scope.branch_pc == branch_pc && scope.reconvergence_pc == join_pc)
        {
            if let Some(merged) = self.union(
                Some(self.control_scopes[index].source_set),
                Some(source_set),
            ) {
                self.control_scopes[index].source_set = merged;
            }
        } else if self.control_scopes.len() < Self::DEFAULT_CONTROL_SCOPE_CAP {
            self.control_scopes.push(ControlScope {
                branch_pc,
                reconvergence_pc: join_pc,
                source_set,
            });
        } else {
            self.control_scopes_dropped = self.control_scopes_dropped.saturating_add(1);
        }
    }

    /// End scopes at their exact postdominator. The reconvergence instruction
    /// itself is outside the controlled region.
    pub fn expire_control_scopes_at(&mut self, pc: u64) {
        self.control_scopes
            .retain(|scope| scope.reconvergence_pc != pc);
    }

    /// Union active control provenance for a write occurring before a join.
    pub fn active_control_set(&mut self) -> Option<u32> {
        let sets = self
            .control_scopes
            .iter()
            .map(|scope| scope.source_set)
            .collect::<Vec<_>>();
        sets.into_iter()
            .fold(None, |merged, set| self.union(merged, Some(set)))
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

    #[test]
    fn data_and_control_hits_report_the_same_source_as_distinct_kinds() {
        let mut t = TaintState::new();
        let source = t.add_source(source("stdin"));
        let control = t.controlize(source).expect("control member");
        t.hit(0x1000, "syscall buffer", "write arg1".into(), source);
        assert_eq!(t.hits[0].kind, TaintDependencyKind::Data);
        assert_eq!(t.hits[0].sources, vec!["stdin"]);
        t.hit(0x1010, "syscall arg", "write arg0".into(), control);
        t.hit_control(0x1010, "syscall arg", "write arg0".into(), control);
        assert_eq!(t.hits.len(), 2);
        assert_eq!(t.hits[1].kind, TaintDependencyKind::Control);
        assert_eq!(t.hits[1].sources, vec!["stdin"]);

        let mixed = t
            .union(Some(source), Some(control))
            .expect("mixed provenance");
        assert_eq!(t.controlize(mixed), Some(control));
        t.hit(0x1020, "syscall arg", "write arg1".into(), mixed);
        t.hit_control(0x1020, "syscall arg", "write arg1".into(), mixed);
        assert_eq!(t.hits.len(), 4);
        assert_eq!(t.hits[2].kind, TaintDependencyKind::Data);
        assert_eq!(t.hits[3].kind, TaintDependencyKind::Control);
        assert_eq!(t.data_projection(control), None);
        assert_eq!(t.control_projection(source), None);
    }

    #[test]
    fn scope_overflow_is_counted_but_updates_to_tracked_scopes_are_retained() {
        let mut t = TaintState::new();
        for index in 0..TaintState::DEFAULT_CONTROL_SCOPE_CAP {
            let label = format!("branch-{index}");
            let predicate = t.add_source(source(&label));
            t.begin_control_scope(0x1000 + index as u64, 0x2000, predicate);
        }

        let updated = t.add_source(source("existing-scope-update"));
        t.begin_control_scope(0x1000, 0x2000, updated);
        assert_eq!(t.control_scopes_dropped(), 0);

        let omitted = t.add_source(source("overflow-scope"));
        t.begin_control_scope(
            0x1000 + TaintState::DEFAULT_CONTROL_SCOPE_CAP as u64,
            0x2000,
            omitted,
        );
        assert_eq!(t.control_scopes_dropped(), 1);
        assert!(!t.control_tracking_complete());

        let active = t.active_control_set().expect("active control provenance");
        let labels = t.labels(active);
        assert!(labels.contains(&"existing-scope-update".to_string()));
        assert!(!labels.contains(&"overflow-scope".to_string()));
    }

    #[test]
    fn control_tracking_is_complete_until_a_scope_is_dropped() {
        let mut t = TaintState::new();
        let source = t.add_source(source("input"));
        assert!(t.control_tracking_complete());
        t.begin_control_scope(0x10, 0x20, source);
        assert!(t.control_tracking_complete());
    }

    #[test]
    fn nested_control_scopes_expire_only_at_their_own_join() {
        let mut t = TaintState::new();
        let outer = t.add_source(source("outer"));
        let inner = t.add_source(source("inner"));
        t.begin_control_scope(0x10, 0x80, outer);
        t.begin_control_scope(0x30, 0x50, inner);

        t.expire_control_scopes_at(0x50);
        let active = t.active_control_set().expect("outer scope remains");
        assert_eq!(t.labels(active), vec!["outer"]);

        t.expire_control_scopes_at(0x80);
        assert_eq!(t.active_control_set(), None);
    }
}
