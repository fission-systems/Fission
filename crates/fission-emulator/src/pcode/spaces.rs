//! SLA-native address-space layout for the emulator.
//!
//! Space ids in lifted P-Code are **SLA table indices**, not fixed constants.
//! This module resolves names (`const`, `unique`, `register`, `ram`, …) from the
//! compiled Sleigh frontend. Fallback values exist only when no frontend is
//! available (unit tests / bare probes).

use fission_sleigh::compiler::CompiledFrontend;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Resolved guest address-space indices for one language frontend.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpaceLayout {
    pub const_space: u64,
    pub unique: u64,
    pub register: u64,
    /// Default data/code space (typically named `ram`).
    pub ram: u64,
    /// Full name → index map from the SLA.
    pub by_name: BTreeMap<String, u64>,
}

impl Default for SpaceLayout {
    fn default() -> Self {
        Self::fallback()
    }
}

impl SpaceLayout {
    /// Conservative fallback when no SLA table is available.
    /// Prefer [`Self::from_compiled`] whenever a frontend exists.
    pub fn fallback() -> Self {
        let mut by_name = BTreeMap::new();
        by_name.insert("const".into(), 0);
        by_name.insert("unique".into(), 1);
        by_name.insert("register".into(), 2);
        by_name.insert("ram".into(), 3);
        Self {
            const_space: 0,
            unique: 1,
            register: 2,
            ram: 3,
            by_name,
        }
    }

    pub fn from_compiled(cf: &CompiledFrontend) -> Self {
        let mut by_name = BTreeMap::new();
        for (idx, space) in &cf.sla_spaces {
            by_name.insert(space.name.clone(), *idx);
        }

        let const_space = by_name.get("const").copied().unwrap_or(0);
        let unique = by_name
            .get("unique")
            .copied()
            .or_else(|| {
                (cf.sla_unique_space_index != u64::MAX).then_some(cf.sla_unique_space_index)
            })
            .unwrap_or(1);

        let register = by_name
            .get("register")
            .copied()
            .or_else(|| {
                (cf.sla_register_space_index != u64::MAX).then_some(cf.sla_register_space_index)
            })
            .unwrap_or(2);

        let ram = cf
            .sla_default_cur_space_index()
            .ok()
            .or_else(|| by_name.get("ram").copied())
            .unwrap_or(3);

        Self {
            const_space,
            unique,
            register,
            ram,
            by_name,
        }
    }

    pub fn name_of(&self, space_id: u64) -> Option<&str> {
        self.by_name
            .iter()
            .find(|(_, id)| **id == space_id)
            .map(|(n, _)| n.as_str())
    }

    pub fn is_ram(&self, space_id: u64) -> bool {
        space_id == self.ram
    }

    pub fn is_register(&self, space_id: u64) -> bool {
        space_id == self.register
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_ids_are_distinct() {
        let s = SpaceLayout::fallback();
        assert_ne!(s.ram, s.register);
        assert_ne!(s.ram, s.unique);
        assert!(s.is_ram(s.ram));
    }
}
