use std::collections::{HashMap, HashSet};

/// Representation of an explicit SSA variable in Fission NIR
/// Follows standard Cytron et al. and LLVM-like SSA definitions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub struct SsaVarId(pub u32);

/// Union-find (Disjoint-Set) data structure for Hindley-Milner type inference closures.
#[derive(Default, Debug)]
pub struct TypeEquivalenceClass {
    parent: HashMap<SsaVarId, SsaVarId>,
    rank: HashMap<SsaVarId, u32>,
}

impl TypeEquivalenceClass {
    pub fn new() -> Self {
        Self {
            parent: HashMap::new(),
            rank: HashMap::new(),
        }
    }

    /// Finds the representative Type ID for a given SSA variable
    pub fn find(&mut self, i: SsaVarId) -> SsaVarId {
        if self.parent.get(&i).copied().unwrap_or(i) == i {
            return i;
        }
        let root = self.find(self.parent[&i]);
        self.parent.insert(i, root);
        root
    }

    /// Unifies the types of two SSA variables based on constraints
    pub fn union(&mut self, i: SsaVarId, j: SsaVarId) {
        let root_i = self.find(i);
        let root_j = self.find(j);

        if root_i != root_j {
            let rank_i = *self.rank.get(&root_i).unwrap_or(&0);
            let rank_j = *self.rank.get(&root_j).unwrap_or(&0);

            if rank_i > rank_j {
                self.parent.insert(root_j, root_i);
            } else if rank_i < rank_j {
                self.parent.insert(root_i, root_j);
            } else {
                self.parent.insert(root_j, root_i);
                self.rank.insert(root_i, rank_i + 1);
            }
        }
    }
}
