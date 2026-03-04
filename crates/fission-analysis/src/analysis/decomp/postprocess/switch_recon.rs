use super::PostProcessor;
use crate::utils::patterns::*;
use regex::Regex;
use std::borrow::Cow;

impl PostProcessor {
    pub(super) fn reconstruct_switch_from_bst_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("if") || !code.contains("return") {
            return Cow::Borrowed(code);
        }

        let output = Self::reconstruct_switch_from_bst(code);
        if output == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(output)
        }
    }

    /// Reconstruct switch/case from BST (binary search tree) or sequential
    /// equality-check patterns that survive C++ post-processing.
    ///
    /// Patterns handled:
    ///
    /// 1. Flat sequential:
    ///    ```text
    ///    if (var == 0) { return 10; }
    ///    if (var == 1) { return 20; }
    ///    if (var == 2) { return 30; }
    ///    return default;
    ///    ```
    ///
    /// 2. BST with range guards:
    ///    ```text
    ///    if (var == 2) { return 30; }
    ///    if (var < 3) {
    ///        if (!var) { return 10; }
    ///        if (var == 1) { return 20; }
    ///    }
    ///    return default;
    ///    ```
    pub(super) fn reconstruct_switch_from_bst(code: &str) -> String {
        let lines: Vec<&str> = code.lines().collect();
        if lines.len() < 4 {
            return code.to_string();
        }

        struct CaseEntry {
            value: String,
            stmt: String,
        }

        let close_brace_only = Regex::new(r"^\s*\}\s*$")
            .unwrap_or_else(|e| panic!("close_brace_only regex should compile: {}", e));

        let mut result_lines: Vec<String> = Vec::new();
        let mut i = 0;
        let mut changed = false;

        while i < lines.len() {
            let mut cases: Vec<CaseEntry> = Vec::new();
            let mut var_name: Option<String> = None;
            let mut base_indent = String::new();
            let mut bst_depth: i32 = 0;
            let mut block_end = i;

            // Try to collect a run of equality-return patterns
            let mut j = i;
            while j < lines.len() {
                let line = lines[j];

                // Try equality-return: if (var == N) { return X; }  (single-line)
                if let Some(caps) = SEQ_EQ_RETURN.captures(line) {
                    let vn = caps[2].to_string();
                    if var_name.is_none() {
                        var_name = Some(vn.clone());
                        base_indent = caps[1].to_string();
                    }
                    if var_name.as_deref() == Some(vn.as_str()) {
                        cases.push(CaseEntry {
                            value: caps[3].to_string(),
                            stmt: caps[4].to_string(),
                        });
                        block_end = j;
                        j += 1;
                        continue;
                    }
                }

                // Try reverse form: if (N == var) { return X; }
                if let Some(caps) = SEQ_EQ_RETURN_REV.captures(line) {
                    let vn = caps[3].to_string();
                    if var_name.is_none() {
                        var_name = Some(vn.clone());
                        base_indent = caps[1].to_string();
                    }
                    if var_name.as_deref() == Some(vn.as_str()) {
                        cases.push(CaseEntry {
                            value: caps[2].to_string(),
                            stmt: caps[4].to_string(),
                        });
                        block_end = j;
                        j += 1;
                        continue;
                    }
                }

                // Try: if (!var) { return X; }  (single-line)
                if let Some(caps) = SEQ_NOT_RETURN.captures(line) {
                    let vn = caps[2].to_string();
                    if var_name.is_none() {
                        var_name = Some(vn.clone());
                        base_indent = caps[1].to_string();
                    }
                    if var_name.as_deref() == Some(vn.as_str()) {
                        cases.push(CaseEntry {
                            value: "0".to_string(),
                            stmt: caps[3].to_string(),
                        });
                        block_end = j;
                        j += 1;
                        continue;
                    }
                }

                // ---- Multi-line equality-return ----
                // if (var == N) {
                //     return X;
                // }
                if let Some(caps) = ML_EQ_OPEN.captures(line) {
                    let vn = caps[2].to_string();
                    if j + 2 < lines.len()
                        && let Some(rcaps) = ML_RETURN_LINE.captures(lines[j + 1])
                        && close_brace_only.is_match(lines[j + 2])
                    {
                        if var_name.is_none() {
                            var_name = Some(vn.clone());
                            base_indent = caps[1].to_string();
                        }
                        if var_name.as_deref() == Some(vn.as_str()) {
                            cases.push(CaseEntry {
                                value: caps[3].to_string(),
                                stmt: rcaps[1].to_string(),
                            });
                            block_end = j + 2;
                            j += 3;
                            continue;
                        }
                    }
                }

                // Multi-line not-return:  if (!var) { / return X; / }
                if let Some(caps) = ML_NOT_OPEN.captures(line) {
                    let vn = caps[2].to_string();
                    if j + 2 < lines.len()
                        && let Some(rcaps) = ML_RETURN_LINE.captures(lines[j + 1])
                        && close_brace_only.is_match(lines[j + 2])
                    {
                        if var_name.is_none() {
                            var_name = Some(vn.clone());
                            base_indent = caps[1].to_string();
                        }
                        if var_name.as_deref() == Some(vn.as_str()) {
                            cases.push(CaseEntry {
                                value: "0".to_string(),
                                stmt: rcaps[1].to_string(),
                            });
                            block_end = j + 2;
                            j += 3;
                            continue;
                        }
                    }
                }

                // Try range guard: if (var < N) { — BST node
                if let Some(caps) = RANGE_GUARD_OPEN.captures(line) {
                    let vn = caps[1].to_string();
                    if var_name.as_deref() == Some(vn.as_str()) {
                        // Count braces
                        let nb: i32 = line
                            .chars()
                            .map(|c| match c {
                                '{' => 1,
                                '}' => -1,
                                _ => 0,
                            })
                            .sum();
                        bst_depth += nb;
                        block_end = j;
                        j += 1;
                        continue;
                    }
                }

                // Closing brace for BST range guard
                if bst_depth > 0 && close_brace_only.is_match(line) {
                    bst_depth -= 1;
                    block_end = j;
                    j += 1;
                    continue;
                }

                // No match — end of block
                if cases.is_empty() {
                    break;
                }
                break;
            }

            // Need at least 3 cases to reconstruct a switch
            if cases.len() < 3 {
                result_lines.push(lines[i].to_string());
                i += 1;
                continue;
            }

            // Check for a default return after the block
            let mut has_default = false;
            let mut default_stmt = String::new();
            let after = block_end + 1;
            if after < lines.len()
                && let Some(caps) = DEFAULT_RETURN.captures(lines[after])
            {
                default_stmt = caps[1].to_string();
                has_default = true;
                block_end = after;
            }

            // Build switch
            result_lines.push(format!(
                "{}switch ({}) {{",
                base_indent,
                var_name.as_deref().unwrap_or("?")
            ));
            for c in &cases {
                result_lines.push(format!("{}case {}:", base_indent, c.value));
                result_lines.push(format!("{}    {}", base_indent, c.stmt));
            }
            if has_default {
                result_lines.push(format!("{}default:", base_indent));
                result_lines.push(format!("{}    {}", base_indent, default_stmt));
            }
            result_lines.push(format!("{}}}", base_indent));

            changed = true;
            i = block_end + 1;
        }

        if !changed {
            return code.to_string();
        }

        result_lines.join("\n")
    }

    // =========================================================================
    // B-6: Reconstruct switch from if/else-if assignment chains
    //
    // Pattern (Ghidra commonly emits this for switch-on-enum/int):
    //   if (!param_1) {
    //       result = "Sunday";
    //   }
    //   else if (param_1 == 1) {
    //       result = "Monday";
    //   }
    //   ...
    //   else {
    //       result = "Unknown";
    //   }
    //   return result;
    //
    // Transforms to:
    //   switch (param_1) {
    //   case 0:
    //       return "Sunday";
    //   case 1:
    //       return "Monday";
    //   ...
    //   default:
    //       return "Unknown";
    //   }
    // =========================================================================
    pub(super) fn reconstruct_switch_from_if_else_assign(code: &str) -> String {
        // Multi-line patterns:
        //   "  if (!var) {"          → opening, var==0
        //   "    target = expr;"     → body assignment
        //   "  }"                    → close
        //   "  else if (var == N) {" → arm
        //   "    target = expr;"     → body
        //   "  }"                    → close
        //   ...
        //   "  else {"              → default
        //   "    target = expr;"    → default body
        //   "  }"
        //   "  return target;"      → return

        let lines: Vec<&str> = code.lines().collect();
        if lines.len() < 6 {
            return code.to_string();
        }

        struct AssignCase {
            value: String,
            expr: String,
        }

        let mut result_lines: Vec<String> = Vec::new();
        let mut i = 0;
        let mut changed = false;

        while i < lines.len() {
            let line = lines[i];

            // Detect start: if (!var) { or if (var == N) {
            let (switch_var, first_value, base_indent) =
                if let Some(caps) = IF_NOT_OPEN.captures(line) {
                    (caps[2].to_string(), "0".to_string(), caps[1].to_string())
                } else if let Some(caps) = IF_EQ_OPEN.captures(line) {
                    (
                        caps[2].to_string(),
                        caps[3].to_string(),
                        caps[1].to_string(),
                    )
                } else {
                    result_lines.push(line.to_string());
                    i += 1;
                    continue;
                };

            // Next line must be: target = expr;
            if i + 1 >= lines.len() {
                result_lines.push(line.to_string());
                i += 1;
                continue;
            }
            let target_var;
            let first_expr;
            if let Some(caps) = ASSIGNMENT.captures(lines[i + 1]) {
                target_var = caps[1].to_string();
                first_expr = caps[2].to_string();
            } else {
                result_lines.push(line.to_string());
                i += 1;
                continue;
            }
            // Line after assignment must be closing brace (standalone or combined with else)
            if i + 2 >= lines.len() {
                result_lines.push(line.to_string());
                i += 1;
                continue;
            }

            let mut cases: Vec<AssignCase> = vec![AssignCase {
                value: first_value,
                expr: first_expr,
            }];
            let mut default_expr: Option<String> = None;
            let mut j;

            // Check if close brace is standalone or combined with else-if
            if CLOSE_BRACE.is_match(lines[i + 2]) {
                j = i + 3; // standalone `}`
            } else if ELSE_IF_EQ_OPEN.captures(lines[i + 2]).is_some()
                || ELSE_OPEN.is_match(lines[i + 2])
            {
                // `} else if (...)` or `} else {` on the same line as close brace
                j = i + 2; // re-process this line as continuation
            } else {
                result_lines.push(line.to_string());
                i += 1;
                continue;
            }

            // Collect else-if / else arms
            // Each arm can be:
            //   Format A: 3 lines — `else if (...) {` / `target = expr;` / `}`
            //   Format B: 2 lines — combined `} else if (...) {` / `target = expr;`
            //             (close brace comes from prev arm on same line)
            while j < lines.len() {
                // else if (var == N) {   OR   } else if (var == N) {
                if let Some(caps) = ELSE_IF_EQ_OPEN.captures(lines[j]) {
                    let var = &caps[1];
                    if var != switch_var {
                        break;
                    }
                    // Must have assignment line next
                    if j + 1 >= lines.len() {
                        break;
                    }
                    if let Some(acaps) = ASSIGNMENT.captures(lines[j + 1]) {
                        let tgt = &acaps[1];
                        if tgt != target_var {
                            break;
                        }
                        cases.push(AssignCase {
                            value: caps[2].to_string(),
                            expr: acaps[2].to_string(),
                        });
                        // Check what follows: standalone `}`, combined `} else if`, or combined `} else {`
                        if j + 2 >= lines.len() {
                            j += 2;
                            break;
                        }
                        if CLOSE_BRACE.is_match(lines[j + 2]) {
                            j += 3; // consumed: else-if-open, assign, close
                        } else if ELSE_IF_EQ_OPEN.captures(lines[j + 2]).is_some()
                            || ELSE_OPEN.is_match(lines[j + 2])
                        {
                            j += 2; // close combined into next arm's line
                        } else {
                            j += 2;
                            break;
                        }
                    } else {
                        break;
                    }
                }
                // else {   OR   } else {
                else if ELSE_OPEN.is_match(lines[j]) {
                    if j + 1 >= lines.len() {
                        break;
                    }
                    if let Some(acaps) = ASSIGNMENT.captures(lines[j + 1]) {
                        let tgt = &acaps[1];
                        if tgt != target_var {
                            break;
                        }
                        default_expr = Some(acaps[2].to_string());
                        if j + 2 < lines.len() && CLOSE_BRACE.is_match(lines[j + 2]) {
                            j += 3;
                        } else {
                            j += 2;
                        }
                    } else {
                        break;
                    }
                    break;
                } else {
                    break;
                }
            }

            // Need at least 3 cases for a worthwhile switch
            if cases.len() < 3 {
                result_lines.push(line.to_string());
                i += 1;
                continue;
            }

            // Check for `return target;` after the chain
            let has_return = if j < lines.len() {
                RETURN_VAR
                    .captures(lines[j])
                    .map_or(false, |c| &c[1] == target_var)
            } else {
                false
            };
            if has_return {
                j += 1; // consume the return statement
            }

            // Build switch
            result_lines.push(format!("{}switch ({}) {{", base_indent, switch_var));
            for c in &cases {
                result_lines.push(format!("{}case {}:", base_indent, c.value));
                if has_return {
                    result_lines.push(format!("{}    return {};", base_indent, c.expr));
                } else {
                    result_lines.push(format!("{}    {} = {};", base_indent, target_var, c.expr));
                    result_lines.push(format!("{}    break;", base_indent));
                }
            }
            if let Some(ref def) = default_expr {
                result_lines.push(format!("{}default:", base_indent));
                if has_return {
                    result_lines.push(format!("{}    return {};", base_indent, def));
                } else {
                    result_lines.push(format!("{}    {} = {};", base_indent, target_var, def));
                    result_lines.push(format!("{}    break;", base_indent));
                }
            }
            result_lines.push(format!("{}}}", base_indent));

            changed = true;
            i = j;
        }

        if !changed {
            return code.to_string();
        }

        result_lines.join("\n")
    }

    pub(super) fn reconstruct_switch_from_if_else_assign_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("if") || !code.contains("else") {
            return Cow::Borrowed(code);
        }

        let output = Self::reconstruct_switch_from_if_else_assign(code);
        if output == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(output)
        }
    }
}
