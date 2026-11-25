//! `decomp --work-limit NAME=VALUE`: override one deterministic work limit.
//!
//! The names are the field names of `ResourceLimits` and the keys of
//! `[decompiler.work_limits]` in `fission.toml`. A limit counts a unit of
//! work, not time, so a given value trips at the same point on any machine.

use fission_core::WorkLimitOverrides;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkLimit {
    StructuringTotalWork,
    ConditionRecoverySubcalls,
    SeseRegionProofCalls,
    VarnodeLoweringWork,
}

impl WorkLimit {
    const ALL: [Self; 4] = [
        Self::StructuringTotalWork,
        Self::ConditionRecoverySubcalls,
        Self::SeseRegionProofCalls,
        Self::VarnodeLoweringWork,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::StructuringTotalWork => "structuring_total_work",
            Self::ConditionRecoverySubcalls => "condition_recovery_subcalls",
            Self::SeseRegionProofCalls => "sese_region_proof_calls",
            Self::VarnodeLoweringWork => "varnode_lowering_work",
        }
    }

    fn names() -> String {
        Self::ALL.map(Self::name).join(", ")
    }
}

/// One parsed `--work-limit NAME=VALUE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkLimitFlag {
    limit: WorkLimit,
    value: u64,
}

impl WorkLimitFlag {
    /// Record this flag in `overrides`; a later flag for the same limit wins.
    pub fn apply_to(self, overrides: &mut WorkLimitOverrides) {
        match self.limit {
            WorkLimit::StructuringTotalWork => overrides.structuring_total_work = Some(self.value),
            WorkLimit::ConditionRecoverySubcalls => {
                // checked at parse time: the value fits a `usize`
                overrides.condition_recovery_subcalls = usize::try_from(self.value).ok();
            }
            WorkLimit::SeseRegionProofCalls => overrides.sese_region_proof_calls = Some(self.value),
            WorkLimit::VarnodeLoweringWork => overrides.varnode_lowering_work = Some(self.value),
        }
    }
}

impl FromStr for WorkLimitFlag {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, String> {
        let (name, value) = raw.split_once('=').ok_or_else(|| {
            format!(
                "expected NAME=VALUE, got '{raw}' (names: {})",
                WorkLimit::names()
            )
        })?;
        let name = name.trim();
        let limit = WorkLimit::ALL
            .into_iter()
            .find(|limit| limit.name() == name)
            .ok_or_else(|| {
                format!(
                    "unknown work limit '{name}' (names: {})",
                    WorkLimit::names()
                )
            })?;
        let value = value.trim();
        let value: u64 = value
            .parse()
            .map_err(|_| format!("'{value}' is not a non-negative integer for '{name}'"))?;
        if limit == WorkLimit::ConditionRecoverySubcalls && usize::try_from(value).is_err() {
            return Err(format!("{value} is too large for '{name}'"));
        }
        Ok(Self { limit, value })
    }
}

/// The overrides `flags` express on top of `base` (the configuration file's).
pub fn overrides_with_flags(
    base: WorkLimitOverrides,
    flags: &[WorkLimitFlag],
) -> WorkLimitOverrides {
    let mut overrides = base;
    for flag in flags {
        flag.apply_to(&mut overrides);
    }
    overrides
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(raw: &str) -> WorkLimitFlag {
        raw.parse().expect("valid flag")
    }

    #[test]
    fn every_limit_name_parses_to_its_own_override() {
        let flags: Vec<_> = WorkLimit::ALL
            .iter()
            .enumerate()
            .map(|(index, limit)| flag(&format!("{}={}", limit.name(), index + 1)))
            .collect();
        let overrides = overrides_with_flags(WorkLimitOverrides::default(), &flags);
        assert_eq!(overrides.structuring_total_work, Some(1));
        assert_eq!(overrides.condition_recovery_subcalls, Some(2));
        assert_eq!(overrides.sese_region_proof_calls, Some(3));
        assert_eq!(overrides.varnode_lowering_work, Some(4));
    }

    #[test]
    fn flags_override_the_file_and_the_last_flag_wins() {
        let file = WorkLimitOverrides {
            structuring_total_work: Some(100),
            sese_region_proof_calls: Some(200),
            ..Default::default()
        };
        let overrides = overrides_with_flags(
            file,
            &[
                flag("structuring_total_work=1"),
                flag("structuring_total_work=2"),
            ],
        );
        assert_eq!(overrides.structuring_total_work, Some(2));
        assert_eq!(
            overrides.sese_region_proof_calls,
            Some(200),
            "file value kept"
        );
    }

    #[test]
    fn whitespace_and_zero_are_accepted() {
        let parsed = flag(" varnode_lowering_work = 0 ");
        let overrides = overrides_with_flags(WorkLimitOverrides::default(), &[parsed]);
        assert_eq!(overrides.varnode_lowering_work, Some(0));
    }

    #[test]
    fn malformed_flags_say_what_is_wrong_and_list_the_names() {
        let missing = "structuring_total_work"
            .parse::<WorkLimitFlag>()
            .unwrap_err();
        assert!(missing.contains("NAME=VALUE"), "{missing}");
        let unknown = "structuring_work=5".parse::<WorkLimitFlag>().unwrap_err();
        assert!(unknown.contains("unknown work limit"), "{unknown}");
        assert!(unknown.contains("sese_region_proof_calls"), "{unknown}");
        let negative = "structuring_total_work=-1"
            .parse::<WorkLimitFlag>()
            .unwrap_err();
        assert!(negative.contains("non-negative integer"), "{negative}");
        let text = "structuring_total_work=lots"
            .parse::<WorkLimitFlag>()
            .unwrap_err();
        assert!(text.contains("non-negative integer"), "{text}");
    }
}
