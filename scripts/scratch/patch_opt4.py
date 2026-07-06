import re

with open('crates/fission-pcode/src/nir/structuring/driver/mod.rs', 'r') as f:
    content = f.read()

# 1. Add tier1_failures declaration
content = content.replace(
    '        let mut progress = true;',
    '        let mut progress = true;\n        let mut tier1_failures = std::collections::HashMap::new();'
)

# 2. Add tier1_failures.insert
target1 = '''                    self.consider_structured_candidate(
                        rule,
                        idx,
                        &targeted,
                        &mut last_structuring_failure,
                        &mut ideal_candidates,
                        res,
                    )?;
                }'''
replacement1 = target1 + '''\n                if let Some(ref err) = last_structuring_failure {
                    tier1_failures.insert(idx, err.clone());
                }'''
content = content.replace(target1, replacement1)

# 3. Modify Tier 2 to use tier1_failures
target2 = '''                let mut last_structuring_failure = None;
                let follow = follow_blocks.get(idx).copied().flatten();
                for rule in ACTIVE_COLLAPSE_RULES {
                    if matches!(rule, CollapseRule::Sequence | CollapseRule::Unstructured) {
                        continue;
                    }
                    let res = match rule {
                        CollapseRule::Switch => self.try_lower_switch(idx),
                        CollapseRule::ForLoop => self.try_lower_for(idx),
                        CollapseRule::DoWhile => {
                            let mut dw = self.try_lower_dowhile(idx)?;
                            if dw.is_none() {
                                dw = self.try_lower_multiblock_dowhile(idx)?;
                            }
                            Ok(dw)
                        }
                        CollapseRule::WhileDo => self.try_lower_while(idx),
                        CollapseRule::InfLoopBreak => self.try_lower_infloop_with_break(idx),
                        CollapseRule::InfLoop => {
                            let mut inf = self.try_lower_infloop(idx);
                            if inf.is_err() || matches!(inf, Ok(None)) {
                                inf = self.try_lower_multiblock_infloop(idx);
                            }
                            inf
                        }
                        CollapseRule::Conditional => {
                            let mut cond = self.try_lower_short_circuit_if(idx);
                            if cond.is_err() || matches!(cond, Ok(None)) {
                                cond = self.try_reduce_if_else_with_follow(idx, follow);
                            }
                            if cond.is_err() || matches!(cond, Ok(None)) {
                                cond = self.try_lower_if_else(idx);
                            }
                            if cond.is_err() || matches!(cond, Ok(None)) {
                                cond = self.try_lower_if(idx);
                            }
                            cond
                        }
                        _ => Ok(None),
                    };
                    let _ = Self::capture_structuring_failure(res, &mut last_structuring_failure)?;
                }'''

replacement2 = '''                let last_structuring_failure = tier1_failures.remove(&idx);'''
content = content.replace(target2, replacement2)

with open('crates/fission-pcode/src/nir/structuring/driver/mod.rs', 'w') as f:
    f.write(content)
