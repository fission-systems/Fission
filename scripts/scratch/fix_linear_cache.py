import re

with open("crates/fission-pcode/src/nir/structuring/linear/mod.rs", "r") as f:
    content = f.read()

# 1. Replace lower_linear_body_with_budget
target1 = """    pub(super) fn lower_linear_body_with_budget(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        mut budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {"""

replacement1 = """    pub(super) fn lower_linear_body_with_budget(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        mut budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        let mut auto_budget = None;
        let budget_ref = if let Some(b) = budget {
            b
        } else {
            let start_addr = self.block_start_address(start_idx);
            auto_budget = Some(IfLoweringBudget::new(
                self.options,
                start_idx,
                start_addr,
                "lower_linear_body_auto",
                self.structuring_start,
            ));
            auto_budget.as_mut().unwrap()
        };
        let detailed = self.lower_linear_body_cached(
            start_idx,
            exit,
            0,
            Some(budget_ref),
            false,
        )?;
        Ok(match &detailed {
            LinearBodyLoweringOutcome::Lowered(lowered) => Some(lowered.clone()),
            LinearBodyLoweringOutcome::Rejected(_) => None,
        })
    }"""

idx1 = content.find(target1)
if idx1 != -1:
    idx1_end = content.find("Ok(result)\n    }", idx1) + len("Ok(result)\n    }")
    content = content[:idx1] + replacement1 + content[idx1_end:]

# 2. Replace lower_linear_body_detailed_with_mode
target2 = """    fn lower_linear_body_detailed_with_mode(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {"""

replacement2 = """    fn lower_linear_body_detailed_with_mode(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        let mut auto_budget = None;
        let budget_ref = if let Some(b) = budget {
            b
        } else {
            let start_addr = self.block_start_address(start_idx);
            auto_budget = Some(IfLoweringBudget::new(
                self.options,
                start_idx,
                start_addr,
                "lower_linear_body_detailed_auto",
                self.structuring_start,
            ));
            auto_budget.as_mut().unwrap()
        };
        self.lower_linear_body_cached(
            start_idx,
            exit,
            0,
            Some(budget_ref),
            region_recovery,
        )
    }

    fn lower_linear_body_cached(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        let key = LinearBodyCacheKey {
            start_idx,
            exit,
            region_recovery,
        };
        if let Some(cached) = self.linear_body_cache.get(&key) {
            return Ok(match cached {
                LinearBodyCachedOutcome::Lowered(lowered) => {
                    LinearBodyLoweringOutcome::Lowered(lowered.clone())
                }
                LinearBodyCachedOutcome::Rejected(reason) => {
                    if region_recovery {
                        LinearBodyLoweringOutcome::Rejected(*reason)
                    } else {
                        LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::UnsupportedTerminator,
                        )
                    }
                }
            });
        }
        if !self.active_linear_body_keys.insert(key) {
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::RevisitCycle,
            ));
        }

        let result = self.lower_linear_body_with_depth_detailed(
            start_idx,
            exit,
            depth,
            budget.as_deref_mut(),
            region_recovery,
        )?;

        self.active_linear_body_keys.remove(&key);
        let should_cache = budget.map_or(true, |b| !b.tripped);
        if should_cache {
            let cached = match &result {
                LinearBodyLoweringOutcome::Lowered(lowered) => {
                    LinearBodyCachedOutcome::Lowered(lowered.clone())
                }
                LinearBodyLoweringOutcome::Rejected(reason) => {
                    if region_recovery {
                        LinearBodyCachedOutcome::Rejected(*reason)
                    } else {
                        LinearBodyCachedOutcome::Rejected(
                            LinearBodyRejectReason::UnsupportedTerminator,
                        )
                    }
                }
            };
            self.linear_body_cache.insert(key, cached);
        }
        Ok(result)
    }"""

idx2 = content.find(target2)
if idx2 != -1:
    idx2_end = content.find("Ok(result)\n    }", idx2) + len("Ok(result)\n    }")
    content = content[:idx2] + replacement2 + content[idx2_end:]

# 3. Replace all lower_linear_body_with_depth_detailed with lower_linear_body_cached in lower_conditional_tail
# The easiest way is to find lower_conditional_tail and replace inside it
idx3 = content.find("fn lower_conditional_tail(")
if idx3 != -1:
    # lower_conditional_tail ends around line 1300. Let's just find the end of the impl block or end of file
    tail_content = content[idx3:]
    tail_content = tail_content.replace("self.lower_linear_body_with_depth_detailed(", "self.lower_linear_body_cached(")
    content = content[:idx3] + tail_content

with open("crates/fission-pcode/src/nir/structuring/linear/mod.rs", "w") as f:
    f.write(content)

print("Done")
