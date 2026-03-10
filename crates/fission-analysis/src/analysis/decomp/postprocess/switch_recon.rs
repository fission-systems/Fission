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

    pub(super) fn cluster_switch_case_runs_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("switch") || !code.contains("case ") {
            return Cow::Borrowed(code);
        }

        let output = Self::cluster_switch_case_runs(code);
        if output == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(output)
        }
    }

    pub(super) fn cluster_switch_case_runs(code: &str) -> String {
        #[derive(Clone)]
        struct CaseBlock {
            case_lines: Vec<String>,
            body: Vec<String>,
            end_idx: usize,
        }

        fn brace_delta(line: &str) -> i32 {
            line.chars()
                .map(|c| match c {
                    '{' => 1,
                    '}' => -1,
                    _ => 0,
                })
                .sum()
        }

        fn parse_numeric_token(text: &str) -> Option<i64> {
            let token = Regex::new(r"(0x[0-9a-fA-F]+|\d+)")
                .unwrap_or_else(|e| panic!("numeric token regex should compile: {e}"))
                .find_iter(text)
                .last()?
                .as_str()
                .to_string();
            if let Some(hex) = token.strip_prefix("0x") {
                i64::from_str_radix(hex, 16).ok()
            } else {
                token.parse::<i64>().ok()
            }
        }

        fn format_offset(delta: i64) -> String {
            let abs = delta.unsigned_abs();
            if abs >= 10 {
                format!("0x{abs:x}")
            } else {
                abs.to_string()
            }
        }

        fn build_formula(switch_expr: &str, delta: i64) -> String {
            let base = format!("(ulonglong)({})", switch_expr.trim());
            if delta == 0 {
                base
            } else if delta > 0 {
                format!("{base} + {}", format_offset(delta))
            } else {
                format!("{base} - {}", format_offset(delta))
            }
        }

        fn indent_of(line: &str) -> &str {
            let trimmed = line.trim_start();
            &line[..line.len() - trimmed.len()]
        }

        fn parse_assignment(line: &str) -> Option<(String, String, String)> {
            let assign = Regex::new(r"^(\s*)([^=]+?)\s*=\s*(.+);\s*$")
                .unwrap_or_else(|e| panic!("assignment regex should compile: {e}"));
            let caps = assign.captures(line)?;
            Some((
                caps.get(1)?.as_str().to_string(),
                caps.get(2)?.as_str().trim().to_string(),
                caps.get(3)?.as_str().trim().to_string(),
            ))
        }

        fn collect_case_block(
            lines: &[&str],
            start: usize,
            switch_end: usize,
            case_re: &Regex,
            default_re: &Regex,
        ) -> Option<CaseBlock> {
            let label_re = Regex::new(r"^\s*[A-Za-z_]\w*:\s*$")
                .unwrap_or_else(|e| panic!("label regex should compile: {e}"));
            if !case_re.is_match(lines[start]) {
                return None;
            }

            let mut case_lines = vec![lines[start].to_string()];
            let mut idx = start + 1;
            while idx < switch_end && case_re.is_match(lines[idx]) {
                case_lines.push(lines[idx].to_string());
                idx += 1;
            }

            let mut body = Vec::new();
            let mut depth = 1i32;
            while idx < switch_end {
                if depth == 1
                    && (case_re.is_match(lines[idx])
                        || default_re.is_match(lines[idx])
                        || label_re.is_match(lines[idx])
                        || lines[idx].trim() == "}")
                {
                    break;
                }
                body.push(lines[idx].to_string());
                depth += brace_delta(lines[idx]);
                idx += 1;
            }

            Some(CaseBlock {
                case_lines,
                body,
                end_idx: idx,
            })
        }

        fn try_cluster_run(
            blocks: &[CaseBlock],
            switch_expr: &str,
        ) -> Option<(Vec<String>, usize)> {
            if blocks.len() < 3 {
                return None;
            }

            fn nonempty_lines(body: &[String]) -> Vec<&str> {
                body.iter()
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .collect()
            }

            fn nonempty_raw(body: &[String]) -> Vec<&String> {
                body.iter().filter(|line| !line.trim().is_empty()).collect()
            }

            fn trailing_goto_target<'a>(lines: &'a [&'a str]) -> Option<&'a str> {
                let last = *lines.last()?;
                Regex::new(r"^goto\s+([A-Za-z_]\w*)\s*;\s*$")
                    .unwrap_or_else(|e| panic!("goto tail regex should compile: {e}"))
                    .captures(last)
                    .and_then(|caps| caps.get(1))
                    .map(|m| m.as_str())
            }

            if blocks.iter().all(|block| block.body == blocks[0].body) {
                let mut out = Vec::new();
                for block in blocks {
                    out.extend(block.case_lines.clone());
                }
                out.extend(blocks[0].body.clone());
                return Some((out, blocks.len()));
            }

            let first_body_nonempty_raw = nonempty_raw(&blocks[0].body);
            let first_body_nonempty: Vec<&str> = first_body_nonempty_raw
                .iter()
                .map(|line| line.trim())
                .collect();
            let first_assign = parse_assignment(first_body_nonempty.first()?)?;
            let last_body_nonempty_raw = nonempty_raw(&blocks.last()?.body);
            let last_body_nonempty: Vec<&str> = last_body_nonempty_raw
                .iter()
                .map(|line| line.trim())
                .collect();
            let last_suffix: Vec<&str> = last_body_nonempty.iter().skip(1).copied().collect();
            let mut common_tail_target: Option<String> = None;

            let mut delta: Option<i64> = None;
            for block in blocks {
                let body_nonempty = nonempty_lines(&block.body);
                let head = parse_assignment(body_nonempty.first()?)?;
                if head.0 != first_assign.0 || head.1 != first_assign.1 {
                    return None;
                }

                let suffix_b: Vec<&str> = body_nonempty.iter().skip(1).copied().collect();
                if suffix_b != last_suffix {
                    if suffix_b.len() == last_suffix.len() + 1
                        && suffix_b[..last_suffix.len()] == last_suffix[..]
                    {
                        let Some(target) = trailing_goto_target(&suffix_b) else {
                            return None;
                        };
                        if let Some(prev) = &common_tail_target {
                            if prev != target {
                                return None;
                            }
                        } else {
                            common_tail_target = Some(target.to_string());
                        }
                    } else {
                        return None;
                    }
                }

                let case_value = parse_numeric_token(block.case_lines.last()?.trim())?;
                let rhs_value = parse_numeric_token(&head.2)?;
                let cur_delta = rhs_value - case_value;
                if let Some(prev) = delta {
                    if prev != cur_delta {
                        return None;
                    }
                } else {
                    delta = Some(cur_delta);
                }
            }

            let (_, lhs, _) = first_assign;
            let assign_indent = indent_of(first_body_nonempty_raw.first()?.as_str()).to_string();
            let suffix: Vec<String> = last_body_nonempty_raw
                .iter()
                .skip(1)
                .map(|line| (*line).clone())
                .collect();

            let mut out = Vec::new();
            for block in blocks {
                out.extend(block.case_lines.clone());
            }
            out.push(format!(
                "{assign_indent}{lhs} = {};",
                build_formula(switch_expr, delta.unwrap_or(0))
            ));
            out.extend(suffix);
            Some((out, blocks.len()))
        }

        let switch_open = Regex::new(r"^\s*switch\s*\((.+)\)\s*\{\s*$")
            .unwrap_or_else(|e| panic!("switch open regex should compile: {e}"));
        let case_re = Regex::new(r"^\s*case\s+.+:\s*$")
            .unwrap_or_else(|e| panic!("case regex should compile: {e}"));
        let default_re = Regex::new(r"^\s*default:\s*$")
            .unwrap_or_else(|e| panic!("default regex should compile: {e}"));

        let lines: Vec<&str> = code.lines().collect();
        if lines.is_empty() {
            return code.to_string();
        }

        let mut result = Vec::new();
        let mut i = 0;
        let mut changed = false;

        while i < lines.len() {
            let Some(caps) = switch_open.captures(lines[i]) else {
                result.push(lines[i].to_string());
                i += 1;
                continue;
            };
            let switch_expr = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let switch_start = i;
            let mut switch_end = i + 1;
            let mut depth = 1i32;
            while switch_end < lines.len() {
                depth += brace_delta(lines[switch_end]);
                if depth == 0 {
                    break;
                }
                switch_end += 1;
            }
            if switch_end >= lines.len() {
                result.push(lines[i].to_string());
                i += 1;
                continue;
            }

            result.push(lines[switch_start].to_string());
            let mut j = switch_start + 1;
            while j < switch_end {
                if !case_re.is_match(lines[j]) {
                    result.push(lines[j].to_string());
                    j += 1;
                    continue;
                }

                let mut run = Vec::new();
                let mut k = j;
                while k < switch_end {
                    let Some(block) =
                        collect_case_block(&lines, k, switch_end, &case_re, &default_re)
                    else {
                        break;
                    };
                    k = block.end_idx;
                    run.push(block);
                    if k >= switch_end || !case_re.is_match(lines[k]) {
                        break;
                    }
                }

                if let Some((clustered, consumed)) = try_cluster_run(&run, &switch_expr) {
                    result.extend(clustered);
                    changed = true;
                    j = run[consumed - 1].end_idx;
                    continue;
                }

                let block = &run[0];
                result.extend(block.case_lines.clone());
                result.extend(block.body.clone());
                j = block.end_idx;
            }
            result.push(lines[switch_end].to_string());
            i = switch_end + 1;
        }

        if changed {
            result.join("\n")
        } else {
            code.to_string()
        }
    }
}
