//! Active learning of the observable machine with an apartness tree (L#,
//! Vaandrager et al., TACAS 2022), over a finite alphabet of representative
//! inputs.
//!
//! Every tree node is a word that was actually run. Two nodes are *apart*
//! when some suffix defined below both gives different observations -- a
//! concrete witness that they are different states, never an inference. The
//! basis is a set of pairwise-apart nodes, so the number of states the
//! learner claims is a lower bound backed by witnesses; it grows only when a
//! frontier node is apart from every basis node.

use std::collections::{BTreeMap, VecDeque};

use anyhow::{Result, bail};

use crate::oracle::{MembershipOracle, Observation, Symbol};

pub type NodeId = usize;

#[derive(Debug, Clone)]
struct Node {
    parent: Option<(NodeId, usize)>,
    /// Symbol index -> (observation on that step, child).
    children: BTreeMap<usize, (Observation, NodeId)>,
    /// The context this node's word left.
    context: Vec<u8>,
}

/// A learned hypothesis: basis states, their transitions and outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hypothesis {
    /// Basis node of each state; state 0 is the initial one.
    pub states: Vec<NodeId>,
    /// `transitions[state][symbol] = (observation, target state)`.
    pub transitions: Vec<Vec<(Observation, usize)>>,
}

impl Hypothesis {
    /// Observations the hypothesis predicts for `word` from `state`.
    #[must_use]
    pub fn run_from(&self, mut state: usize, word: &[usize]) -> (Vec<Observation>, usize) {
        let mut out = Vec::with_capacity(word.len());
        for symbol in word {
            let (observation, next) = self.transitions[state][*symbol];
            out.push(observation);
            state = next;
        }
        (out, state)
    }
}

pub enum Learned {
    Hypothesis(Hypothesis),
    /// The basis outgrew `max_states`: at least this many states exist.
    Partial {
        states: usize,
        frontier: usize,
    },
}

/// The observation tree plus the L# basis.
pub struct Learner {
    pub alphabet: Vec<Symbol>,
    nodes: Vec<Node>,
    basis: Vec<NodeId>,
    queries: u64,
}

impl Learner {
    #[must_use]
    pub fn new(alphabet: Vec<Symbol>, initial_context: Vec<u8>) -> Self {
        Self {
            alphabet,
            nodes: vec![Node {
                parent: None,
                children: BTreeMap::new(),
                context: initial_context,
            }],
            basis: vec![0],
            queries: 0,
        }
    }

    #[must_use]
    pub fn queries(&self) -> u64 {
        self.queries
    }

    /// Add `symbol` to the alphabet (if new) and return its index.
    pub fn intern(&mut self, symbol: &Symbol) -> usize {
        if let Some(index) = self.alphabet.iter().position(|known| known == symbol) {
            return index;
        }
        self.alphabet.push(symbol.clone());
        self.alphabet.len() - 1
    }

    #[must_use]
    pub fn context(&self, node: NodeId) -> &[u8] {
        &self.nodes[node].context
    }

    /// The word that reaches `node`.
    #[must_use]
    pub fn access(&self, mut node: NodeId) -> Vec<usize> {
        let mut word = Vec::new();
        while let Some((parent, symbol)) = self.nodes[node].parent {
            word.push(symbol);
            node = parent;
        }
        word.reverse();
        word
    }

    #[must_use]
    pub fn symbols(&self, word: &[usize]) -> Vec<Symbol> {
        word.iter().map(|s| self.alphabet[*s].clone()).collect()
    }

    fn child(&self, node: NodeId, symbol: usize) -> Option<(Observation, NodeId)> {
        self.nodes[node].children.get(&symbol).copied()
    }

    /// The node `word` reaches, if the tree holds the whole word.
    #[must_use]
    pub fn node_of(&self, word: &[usize]) -> Option<NodeId> {
        word.iter().try_fold(0, |node, symbol| {
            self.child(node, *symbol).map(|(_, next)| next)
        })
    }

    /// Run `word` unless the tree already holds it, and return the
    /// observations along it.
    pub fn query(
        &mut self,
        oracle: &mut dyn MembershipOracle,
        word: &[usize],
    ) -> Result<Vec<Observation>> {
        if self.node_of(word).is_none() {
            let symbols = self.symbols(word);
            let records = oracle.run(&symbols)?;
            self.queries += 1;
            if records.len() != word.len() {
                bail!(
                    "the oracle answered {} of {} steps",
                    records.len(),
                    word.len()
                );
            }
            let mut node = 0;
            for (symbol, record) in word.iter().zip(records) {
                node = match self.child(node, *symbol) {
                    Some((seen, next)) => {
                        if seen != record.observation {
                            bail!(
                                "the system is not deterministic: one word observed {seen:?} and {:?}",
                                record.observation
                            );
                        }
                        next
                    }
                    None => {
                        let next = self.nodes.len();
                        self.nodes.push(Node {
                            parent: Some((node, *symbol)),
                            children: BTreeMap::new(),
                            context: record.context,
                        });
                        self.nodes[node]
                            .children
                            .insert(*symbol, (record.observation, next));
                        next
                    }
                };
            }
        }
        let mut node = 0;
        let mut out = Vec::with_capacity(word.len());
        for symbol in word {
            let (observation, next) = self.child(node, *symbol).expect("just inserted");
            out.push(observation);
            node = next;
        }
        Ok(out)
    }

    /// The shortest suffix on which `a` and `b` observably differ.
    #[must_use]
    pub fn witness(&self, a: NodeId, b: NodeId) -> Option<Vec<usize>> {
        let mut queue = VecDeque::from([(a, b, Vec::new())]);
        while let Some((x, y, prefix)) = queue.pop_front() {
            for (symbol, (ox, nx)) in &self.nodes[x].children {
                let Some((oy, ny)) = self.child(y, *symbol) else {
                    continue;
                };
                let mut word = prefix.clone();
                word.push(*symbol);
                if *ox != oy {
                    return Some(word);
                }
                queue.push_back((*nx, ny, word));
            }
        }
        None
    }

    fn apart(&self, a: NodeId, b: NodeId) -> bool {
        self.witness(a, b).is_some()
    }

    #[must_use]
    pub fn basis(&self) -> &[NodeId] {
        &self.basis
    }

    /// Apply the L# rules until every frontier node is identified with
    /// exactly one basis node, then return the hypothesis.
    pub fn learn(
        &mut self,
        oracle: &mut dyn MembershipOracle,
        max_states: usize,
    ) -> Result<Learned> {
        'rules: loop {
            // Extension: every basis node has every successor.
            for b in self.basis.clone() {
                for symbol in 0..self.alphabet.len() {
                    if self.child(b, symbol).is_none() {
                        let mut word = self.access(b);
                        word.push(symbol);
                        self.query(oracle, &word)?;
                    }
                }
            }
            // Promotion: a frontier node apart from the whole basis is a new
            // state.
            for f in self.frontier() {
                if self.basis.iter().all(|b| self.apart(f, *b)) {
                    self.basis.push(f);
                    if self.basis.len() > max_states {
                        return Ok(Learned::Partial {
                            states: self.basis.len(),
                            frontier: self.frontier().len(),
                        });
                    }
                    continue 'rules;
                }
            }
            // Separation: a frontier node compatible with two basis nodes is
            // run on what tells those two apart.
            for f in self.frontier() {
                let compatible = self.compatible(f);
                if compatible.len() >= 2 {
                    let w = self
                        .witness(compatible[0], compatible[1])
                        .expect("basis nodes are pairwise apart");
                    let mut word = self.access(f);
                    word.extend(w);
                    self.query(oracle, &word)?;
                    continue 'rules;
                }
            }
            let hypothesis = self.hypothesis();
            // The tree may already contradict the hypothesis.
            if let Some(word) = self.inconsistency(&hypothesis) {
                self.process_counterexample(oracle, &hypothesis, &word)?;
                continue 'rules;
            }
            return Ok(Learned::Hypothesis(hypothesis));
        }
    }

    fn frontier(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        for b in &self.basis {
            for (_, child) in self.nodes[*b].children.values() {
                if !self.basis.contains(child) {
                    out.push(*child);
                }
            }
        }
        out
    }

    fn compatible(&self, node: NodeId) -> Vec<NodeId> {
        self.basis
            .iter()
            .copied()
            .filter(|b| !self.apart(node, *b))
            .collect()
    }

    fn hypothesis(&self) -> Hypothesis {
        let index = |node: NodeId| -> usize {
            if let Some(i) = self.basis.iter().position(|b| *b == node) {
                return i;
            }
            let compatible = self.compatible(node);
            self.basis
                .iter()
                .position(|b| *b == compatible[0])
                .expect("an identified frontier node")
        };
        let transitions = self
            .basis
            .iter()
            .map(|b| {
                (0..self.alphabet.len())
                    .map(|symbol| {
                        let (observation, child) = self.child(*b, symbol).expect("extended");
                        (observation, index(child))
                    })
                    .collect()
            })
            .collect();
        Hypothesis {
            states: self.basis.clone(),
            transitions,
        }
    }

    /// A word in the tree the hypothesis predicts wrongly, if any.
    fn inconsistency(&self, hypothesis: &Hypothesis) -> Option<Vec<usize>> {
        let mut stack = vec![(0usize, 0usize, Vec::new())];
        while let Some((node, state, word)) = stack.pop() {
            for (symbol, (observation, child)) in self.nodes[node].children.iter().rev() {
                if *symbol >= self.alphabet.len() {
                    continue;
                }
                let (predicted, next) = hypothesis.transitions[state][*symbol];
                let mut longer = word.clone();
                longer.push(*symbol);
                if predicted != *observation {
                    return Some(longer);
                }
                stack.push((*child, next, longer));
            }
        }
        None
    }

    /// Rivest-Schapire decomposition: find the step where the hypothesis's
    /// state stops predicting the system, and run the queries that make the
    /// responsible frontier node apart from the state it was taken for.
    pub fn process_counterexample(
        &mut self,
        oracle: &mut dyn MembershipOracle,
        hypothesis: &Hypothesis,
        word: &[usize],
    ) -> Result<()> {
        let real = self.query(oracle, word)?;
        if hypothesis.run_from(0, word).0 == real {
            bail!("not a counterexample");
        }
        // agree(i): from the state the hypothesis is in after word[..i], its
        // access word followed by word[i..] behaves as predicted.
        let mut agree = |learner: &mut Self, i: usize| -> Result<bool> {
            let (_, state) = hypothesis.run_from(0, &word[..i]);
            let mut probe = learner.access(hypothesis.states[state]);
            let skip = probe.len();
            probe.extend_from_slice(&word[i..]);
            let observed = learner.query(oracle, &probe)?;
            Ok(observed[skip..] == hypothesis.run_from(state, &word[i..]).0[..])
        };
        let (mut lo, mut hi) = (0usize, word.len());
        // Invariant: !agree(lo) and agree(hi).
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if agree(self, mid)? {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::StepRecord;

    /// A Mealy machine given as a table.
    struct Table {
        next: Vec<Vec<usize>>,
        out: Vec<Vec<u64>>,
        steps: u64,
    }

    impl MembershipOracle for Table {
        fn run(&mut self, word: &[Symbol]) -> Result<Vec<StepRecord>> {
            let mut state = 0;
            let mut records = Vec::new();
            for symbol in word {
                self.steps += 1;
                let s = symbol[0] as usize;
                records.push(StepRecord {
                    observation: Observation::returned(self.out[state][s]),
                    context: vec![self.next[state][s] as u8],
                });
                state = self.next[state][s];
            }
            Ok(records)
        }

        fn steps(&self) -> u64 {
            self.steps
        }
    }

    fn learn_table(table: &mut Table, symbols: usize) -> Hypothesis {
        let alphabet = (0..symbols as u64).map(|s| vec![s]).collect();
        let mut learner = Learner::new(alphabet, vec![0]);
        loop {
            let Learned::Hypothesis(h) = learner.learn(table, 16).unwrap() else {
                panic!("partial");
            };
            // Exhaustive equivalence check up to length 6.
            let mut cex = None;
            let mut words = vec![Vec::<usize>::new()];
            for _ in 0..6 {
                let mut longer = Vec::new();
                for w in &words {
                    for s in 0..symbols {
                        let mut v = w.clone();
                        v.push(s);
                        longer.push(v);
                    }
                }
                for w in &longer {
                    let real = table
                        .run(&learner.symbols(w))
                        .unwrap()
                        .into_iter()
                        .map(|r| r.observation)
                        .collect::<Vec<_>>();
                    if h.run_from(0, w).0 != real && cex.is_none() {
                        cex = Some(w.clone());
                    }
                }
                words = longer;
            }
            match cex {
                None => return h,
                Some(w) => learner.process_counterexample(table, &h, &w).unwrap(),
            }
        }
    }

    #[test]
    fn learns_the_minimal_machine_and_merges_equivalent_states() {
        // Four concrete states; states 1 and 3 behave identically.
        let mut table = Table {
            next: vec![vec![1, 0], vec![2, 0], vec![3, 3], vec![2, 0]],
            out: vec![vec![0, 0], vec![0, 0], vec![1, 1], vec![0, 0]],
            steps: 0,
        };
        let h = learn_table(&mut table, 2);
        assert_eq!(h.states.len(), 3);
    }

    #[test]
    fn a_counterexample_deep_in_the_word_still_splits_a_state() {
        // A counter mod 5 that only says so at 4: states are told apart only
        // by long suffixes.
        let next = (0..5).map(|s| vec![(s + 1) % 5, s]).collect();
        let out = (0..5).map(|s| vec![u64::from(s == 4), 0]).collect();
        let mut table = Table {
            next,
            out,
            steps: 0,
        };
        let h = learn_table(&mut table, 2);
        assert_eq!(h.states.len(), 5);
    }
}
