use super::PostProcessor;
use super::condition::negate_condition;
use crate::utils::patterns::*;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

impl PostProcessor {
    pub(super) fn while_true_to_for_loop_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("while") {
            return Cow::Borrowed(code);
        }

        let output = Self::while_true_to_for_loop(code);
        if output == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(output)
        }
    }

    // =========================================================================
    // B-1: while(true) → for loop  (RetDec while_true_to_for_loop_optimizer.cpp)
    //
    // Detects:
    //   init_var = start;
    //   while (true) {
    //       if (exit_cond) break;   // first statement
    //       body;
    //       init_var = init_var OP step;  // last statement (or init_var++/--)
    //   }
    // and transforms to:
    //   for (init_var = start; !exit_cond; init_var = init_var OP step) { body; }
    // =========================================================================
    pub(super) fn while_true_to_for_loop(code: &str) -> String {
        let lines: Vec<&str> = code.lines().collect();
        let mut result_lines: Vec<String> = Vec::new();
        let mut changed = false;
        let mut i = 0;

        // Regex patterns
        static INIT_ASSIGN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(\s*)(\w+)\s*=\s*(.+?)\s*;\s*$")
                .unwrap_or_else(|e| panic!("INIT_ASSIGN regex should compile: {}", e))
        });

        while i < lines.len() {
            // Check for while(true) { preceded by init assignment
            if let Some(while_caps) = WHILE_TRUE.captures(lines[i]) {
                let while_indent = while_caps[1].to_string();

                // Check previous line for init assignment
                let init_info = if i > 0 && !result_lines.is_empty() {
                    INIT_ASSIGN
                        .captures(lines[i - 1])
                        .map(|c| (c[2].to_string(), c[3].to_string()))
                } else {
                    None
                };

                if let Some((init_var, init_expr)) = init_info {
                    // Find the closing brace (brace counting)
                    let mut depth: i32 = 1;
                    let mut body_end = lines.len() - 1;
                    for (k, line) in lines.iter().enumerate().skip(i + 1) {
                        for ch in line.chars() {
                            match ch {
                                '{' => depth += 1,
                                '}' => depth -= 1,
                                _ => {}
                            }
                        }
                        if depth == 0 {
                            body_end = k;
                            break;
                        }
                    }

                    // Collect body lines
                    let body_start = i + 1;
                    if body_end > body_start + 1 {
                        let body_lines = &lines[body_start..body_end];

                        // Check first body line for if(cond) break;
                        let break_cond = IF_BREAK.captures(body_lines[0]);

                        // Check last body line for update of init_var
                        let last_body = body_lines[body_lines.len() - 1];
                        let update_info = if let Some(caps) = INC_DEC.captures(last_body) {
                            let v = caps[1].to_string();
                            let op = caps[2].to_string();
                            Some((v, format!("{}{}", &caps[1], op)))
                        } else if let Some(caps) = COMPOUND_ASSIGN.captures(last_body) {
                            let v = caps[1].to_string();
                            Some((v, format!("{} {} {}", &caps[1], &caps[2], &caps[3])))
                        } else if let Some(caps) = LOOP_ASSIGN.captures(last_body) {
                            let v = caps[1].to_string();
                            Some((v, caps[2].to_string()))
                        } else {
                            None
                        };

                        if let (Some(break_caps), Some((update_var, update_expr))) =
                            (break_cond, update_info)
                        {
                            let cond = break_caps[1].trim().to_string();

                            if update_var == init_var {
                                // All three components found → build for loop
                                let negated = negate_condition(&cond);

                                // Remove the init line we already pushed
                                result_lines.pop();

                                // Build update expression for the for header
                                let update_str = if update_expr.contains("++")
                                    || update_expr.contains("--")
                                    || update_expr.contains("+=")
                                    || update_expr.contains("-=")
                                {
                                    update_expr
                                } else {
                                    format!("{} = {}", update_var, update_expr)
                                };

                                result_lines.push(format!(
                                    "{}for ({} = {}; {}; {}) {{",
                                    while_indent, init_var, init_expr, negated, update_str
                                ));

                                // Body lines: skip first (if-break) and last (update)
                                for bl in &body_lines[1..body_lines.len() - 1] {
                                    result_lines.push(bl.to_string());
                                }

                                // Closing brace
                                result_lines.push(lines[body_end].to_string());

                                changed = true;
                                i = body_end + 1;
                                continue;
                            }
                        }
                    }
                }
            }

            result_lines.push(lines[i].to_string());
            i += 1;
        }

        if !changed {
            return code.to_string();
        }
        result_lines.join("\n")
    }

    // =========================================================================
    // B-7: General while(cond) → for loop conversion
    //
    // Pattern:
    //   var = init;
    //   while (var cmp bound) {
    //       body;
    //       var++;  // or var += step, var = var + step
    //   }
    //
    // Transforms to:
    //   for (var = init; var cmp bound; var++) {
    //       body;
    //   }
    //
    // This complements the C++ convert_while_to_for_struct() by catching
    // patterns where the condition has a cast (e.g. (int)var < N) or
    // the init isn't immediately before the while.
    // =========================================================================
    pub(super) fn while_cond_to_for(code: &str) -> String {
        static WHILE_COND: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(\s*)while\s*\((.+?)\s*(<=|<|>=|>|!=)\s*(.+?)\)\s*\{\s*$")
                .unwrap_or_else(|e| panic!("WHILE_COND regex should compile: {}", e))
        });
        static INIT_ASSIGN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(\s*)(\w+)\s*=\s*(.+?)\s*;\s*$")
                .unwrap_or_else(|e| panic!("INIT_ASSIGN regex should compile: {}", e))
        });

        let lines: Vec<&str> = code.lines().collect();
        let mut result_lines: Vec<String> = Vec::new();
        let mut changed = false;
        let mut skip_until = 0usize;

        let mut i = 0;
        while i < lines.len() {
            if i < skip_until {
                i += 1;
                continue;
            }

            if let Some(wcaps) = WHILE_COND.captures(lines[i]) {
                let indent = &wcaps[1];
                let lhs_raw = &wcaps[2];
                let op = &wcaps[3];
                let rhs = &wcaps[4];

                // Extract bare variable name from LHS (strip cast)
                if let Some(vcaps) = CAST_VAR.captures(lhs_raw) {
                    let var = &vcaps[1];

                    // Check line before while for init
                    let has_init = if i > 0 {
                        INIT_ASSIGN
                            .captures(lines[i - 1])
                            .is_some_and(|c| &c[2] == var)
                    } else {
                        false
                    };

                    // Find closing brace
                    let mut depth = 1i32;
                    let mut j = i + 1;
                    while j < lines.len() && depth > 0 {
                        for c in lines[j].chars() {
                            if c == '{' {
                                depth += 1;
                            } else if c == '}' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                        if depth > 0 {
                            j += 1;
                        }
                    }

                    if depth == 0 && j > i + 1 {
                        let close_idx = j;
                        let inc_idx = close_idx - 1;

                        // Detect increment pattern
                        let inc_str = if let Some(c) =
                            crate::utils::patterns::INC_PP.captures(lines[inc_idx])
                        {
                            if &c[1] == var {
                                Some(format!("{}++", var))
                            } else {
                                None
                            }
                        } else if let Some(c) = ADD_ASSIGN.captures(lines[inc_idx]) {
                            if &c[1] == var {
                                Some(format!("{} += {}", var, &c[2]))
                            } else {
                                None
                            }
                        } else if let Some(c) = ADD_PATTERN.captures(lines[inc_idx]) {
                            if &c[1] == var && &c[2] == var {
                                let step = &c[3];
                                if step.trim() == "1" {
                                    Some(format!("{}++", var))
                                } else {
                                    Some(format!("{} += {}", var, step))
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(inc) = inc_str {
                            // Build for loop
                            let init_part = if has_init {
                                // Safe: has_init was set when INIT_ASSIGN matched, but use if-let for safety
                                if let Some(icaps) = INIT_ASSIGN.captures(lines[i - 1]) {
                                    // Remove the init line we already pushed
                                    result_lines.pop();
                                    format!("{} = {}", var, &icaps[3])
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };

                            let for_line = format!(
                                "{}for ({}; {} {} {}; {}) {{",
                                indent, init_part, lhs_raw, op, rhs, inc
                            );
                            result_lines.push(for_line);

                            // Push body (excluding increment line)
                            for body_line in lines.iter().take(inc_idx).skip(i + 1) {
                                result_lines.push((*body_line).to_string());
                            }
                            result_lines.push(format!("{}}}", indent));

                            changed = true;
                            skip_until = close_idx + 1;
                            i = close_idx + 1;
                            continue;
                        }
                    }
                }
            }

            result_lines.push(lines[i].to_string());
            i += 1;
        }

        if !changed {
            return code.to_string();
        }

        result_lines.join("\n")
    }

    pub(super) fn while_cond_to_for_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("while") {
            return Cow::Borrowed(code);
        }

        let output = Self::while_cond_to_for(code);
        if output == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(output)
        }
    }

    /// B-10: Convert `while( true )` / `while (true)` / `while (1)` → `for (;;)`
    /// This gives the `for (` pattern to infinite-loop functions that use explicit break/return.
    pub(super) fn while_true_to_for_ever(code: &str) -> String {
        if !WHILE_TRUE_ML.is_match(code) {
            return code.to_string();
        }
        WHILE_TRUE_ML
            .replace_all(code, |caps: &regex::Captures| {
                format!("{}for (;;) {{", &caps[1])
            })
            .into_owned()
    }

    pub(super) fn while_true_to_for_ever_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !WHILE_TRUE_ML.is_match(code) {
            return Cow::Borrowed(code);
        }

        Cow::Owned(Self::while_true_to_for_ever(code))
    }

    // =========================================================================
    // B-5: Loop idiom recognition  (LLVM LoopIdiomRecognize.cpp style)
    //
    //  Patterns:
    //  1. strlen:   while (*ptr != 0) { ptr++; }  →  len = strlen(ptr)
    //  2. popcount: cnt=0; while (v) { cnt++; v = v & (v-1); }
    //                 →  cnt = __builtin_popcount(v)
    //  3. memset:   for (i=0; i<N; i++) { buf[i] = 0; }
    //                 →  memset(buf, 0, N)
    // =========================================================================
    pub(super) fn recognize_loop_idioms(code: &str) -> String {
        let mut result = code.to_string();

        // 1. strlen: while (*ptr != 0) { ptr = ptr + 1; }
        //        or: while (*(ptr + off) != 0) { off = off + 1; }
        static STRLEN_LOOP: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"while\s*\(\s*\*\s*(?P<ptr>[\w\->\.\[\]]+)\s*!=\s*(?:0|'\\0')\s*\)\s*\{\s*",
                r"(?P<upd>[\w\->\.\[\]]+)\s*=\s*(?P<upd2>[\w\->\.\[\]]+)\s*\+\s*1\s*;\s*\}",
            ))
            .unwrap_or_else(|e| panic!("STRLEN_LOOP regex should compile: {}", e))
        });

        result = STRLEN_LOOP
            .replace_all(&result, |caps: &regex::Captures| {
                let ptr = &caps["ptr"];
                let upd = &caps["upd"];
                let upd2 = &caps["upd2"];
                if upd == upd2 {
                    format!("/* strlen loop detected */ {} += strlen({})", upd, ptr)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 2. popcount: cnt = 0; while (val != 0) { cnt = cnt + 1; val = val & val - 1; }
        //   Also variant: val = val & (val - 1);
        static POPCOUNT_LOOP: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"(?P<cnt>\w+)\s*=\s*0\s*;\s*",
                r"while\s*\(\s*(?P<val>\w+)\s*!=\s*0\s*\)\s*\{\s*",
                r"(?P<cnt2>\w+)\s*=\s*(?P<cnt3>\w+)\s*\+\s*1\s*;\s*",
                r"(?P<val2>\w+)\s*=\s*(?P<val3>\w+)\s*&\s*",
                r"(?:(?P<val4>\w+)\s*-\s*1|\(\s*(?P<val5>\w+)\s*-\s*1\s*\))\s*;\s*\}",
            ))
            .unwrap_or_else(|e| panic!("POPCOUNT_LOOP regex should compile: {}", e))
        });

        result = POPCOUNT_LOOP
            .replace_all(&result, |caps: &regex::Captures| {
                let cnt = &caps["cnt"];
                let val = &caps["val"];
                let val3 = &caps["val3"];
                let val_minus = caps
                    .name("val4")
                    .or_else(|| caps.name("val5"))
                    .map(|m| m.as_str())
                    .unwrap_or("");
                if cnt == &caps["cnt2"]
                    && cnt == &caps["cnt3"]
                    && val == &caps["val2"]
                    && val == val3
                    && val == val_minus
                {
                    format!("{} = __builtin_popcount({})", cnt, val)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 3. memset: for (i = 0; i < N; i++) { buf[i] = 0; }
        //   or: for (i = 0; i < N; i = i + 1) { buf[i] = 0; }
        static MEMSET_LOOP: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"for\s*\(\s*(?P<iv>\w+)\s*=\s*0\s*;\s*",
                r"(?P<iv2>\w+)\s*<\s*(?P<sz>[^;]+?)\s*;\s*",
                r"(?P<iv3>\w+)\s*(?:\+\+|=\s*(?P<iv4>\w+)\s*\+\s*1)\s*\)\s*\{\s*",
                r"(?P<buf>\w+)\s*\[\s*(?P<iv5>\w+)\s*\]\s*=\s*(?P<val>0|'\\0')\s*;\s*\}",
            ))
            .unwrap_or_else(|e| panic!("MEMSET_LOOP regex should compile: {}", e))
        });

        result = MEMSET_LOOP
            .replace_all(&result, |caps: &regex::Captures| {
                let iv = &caps["iv"];
                let buf = &caps["buf"];
                let sz = &caps["sz"];
                let iv4 = caps.name("iv4").map(|m| m.as_str()).unwrap_or(iv);
                if iv == &caps["iv2"] && iv == &caps["iv3"] && iv == iv4 && iv == &caps["iv5"] {
                    format!("memset({}, 0, {})", buf, sz.trim())
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result
    }

    pub(super) fn recognize_loop_idioms_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("while") && !code.contains("for") {
            return Cow::Borrowed(code);
        }

        let output = Self::recognize_loop_idioms(code);
        if output == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(output)
        }
    }

    /// B-8: Convert do-while with a counter variable to a for loop.
    /// Pattern:
    ///   VAR = INIT;               (immediately before the do)
    ///   do {
    ///     BODY;
    ///     VAR++;  (or VAR += 1)
    ///   } while (VAR op LIMIT);
    /// → for (VAR = INIT; VAR op LIMIT; VAR++) { BODY; }
    pub(super) fn do_while_to_for(code: &str) -> String {
        static INIT_ASSIGN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(\s*)(\w+)\s*=\s*([^;]+);\s*$")
                .unwrap_or_else(|e| panic!("INIT_ASSIGN regex should compile: {}", e))
        });

        let lines: Vec<&str> = code.lines().collect();
        if lines.len() < 4 {
            return code.to_string();
        }
        let mut result: Vec<String> = Vec::with_capacity(lines.len());
        let mut i = 0;
        let mut changed = false;

        while i < lines.len() {
            // Detect `do {`
            if let Some(do_caps) = DO_OPEN.captures(lines[i]) {
                let do_indent = do_caps[1].to_string();

                // Find matching `} while (VAR op LIMIT);`
                let mut depth = 1i32;
                let mut close_idx = None;
                for (k, line) in lines.iter().enumerate().skip(i + 1) {
                    if DO_OPEN.is_match(line) {
                        depth += 1;
                    } else if DO_WHILE_CLOSE.is_match(line) && line.trim_start().starts_with('}') {
                        depth -= 1;
                        if depth == 0 {
                            close_idx = Some(k);
                            break;
                        }
                    } else if line.trim() == "}" {
                        // A bare `}` could close nested blocks but not the do-while
                    }
                }

                let close_idx = match close_idx {
                    Some(c) => c,
                    None => {
                        result.push(lines[i].to_string());
                        i += 1;
                        continue;
                    }
                };

                // Safe: close_idx was found by matching DO_WHILE_CLOSE, but use if-let for robustness
                let (cond_var, cond_op, cond_lim) =
                    if let Some(close_caps) = DO_WHILE_CLOSE.captures(lines[close_idx]) {
                        (
                            close_caps[2].to_string(),
                            close_caps[3].to_string(),
                            close_caps[4].trim().to_string(),
                        )
                    } else {
                        // Fallback: shouldn't happen, but skip transformation
                        result.push(lines[i].to_string());
                        i += 1;
                        continue;
                    };

                // Find increment line among the body (last occurrence of VAR++ or VAR += ...)
                let body_start = i + 1;
                let body_end = close_idx;
                let mut inc_idx = None;
                let mut inc_expr = String::new(); // the full increment expression for the for-header
                // Pattern for incrementing with indent (compile-time constants)
                let inc_pp_local = Regex::new(r"^(\s*)(\w+)\+\+;\s*$")
                    .unwrap_or_else(|e| panic!("inc_pp_local regex should compile: {}", e));
                let inc_pe_local = Regex::new(r"^(\s*)(\w+)\s*\+=\s*([^;]+);\s*$")
                    .unwrap_or_else(|e| panic!("inc_pe_local regex should compile: {}", e));
                for k in (body_start..body_end).rev() {
                    if let Some(c) = inc_pp_local.captures(lines[k])
                        && c[2] == cond_var
                    {
                        inc_idx = Some(k);
                        inc_expr = format!("{}++", cond_var);
                        break;
                    }
                    if let Some(c) = inc_pe_local.captures(lines[k])
                        && c[2] == cond_var
                    {
                        inc_idx = Some(k);
                        inc_expr = format!("{} += {}", cond_var, c[3].trim());
                        break;
                    }
                }

                let inc_idx = match inc_idx {
                    Some(idx) => idx,
                    None => {
                        // Also try: RHS of while condition is the counter
                        // e.g. `} while (limit != counter);`
                        // where counter is the variable being incremented
                        if SINGLE_IDENT.is_match(cond_lim.trim()) {
                            let rhs_var = cond_lim.trim().to_string();
                            let mut found_rhs_idx = None;
                            let mut rhs_inc_expr = String::new();
                            for k in (body_start..body_end).rev() {
                                if let Some(c) = inc_pp_local.captures(lines[k])
                                    && c[2] == rhs_var
                                {
                                    found_rhs_idx = Some(k);
                                    rhs_inc_expr = format!("{}++", rhs_var);
                                    break;
                                }
                                if let Some(c) = inc_pe_local.captures(lines[k])
                                    && c[2] == rhs_var
                                {
                                    found_rhs_idx = Some(k);
                                    rhs_inc_expr = format!("{} += {}", rhs_var, c[3].trim());
                                    break;
                                }
                            }
                            if let Some(rhs_idx) = found_rhs_idx {
                                // Rebuild with swapped cond_var and cond_lim
                                let flipped_init = if i > 0 {
                                    if let Some(ic) = INIT_ASSIGN.captures(lines[i - 1]) {
                                        if ic[2].trim() == rhs_var {
                                            result.pop();
                                            format!("{} = {}", rhs_var, ic[3].trim())
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    }
                                } else {
                                    String::new()
                                };

                                let for_line = format!(
                                    "{}for ({}; {} {} {}; {}) {{",
                                    do_indent,
                                    flipped_init,
                                    rhs_var,
                                    cond_op,
                                    cond_var,
                                    rhs_inc_expr
                                );
                                result.push(for_line);
                                for (k, line) in
                                    lines.iter().enumerate().take(body_end).skip(body_start)
                                {
                                    if k != rhs_idx {
                                        result.push((*line).to_string());
                                    }
                                }
                                result.push(format!("{}}}", do_indent));
                                changed = true;
                                i = close_idx + 1;
                                continue;
                            }
                        }
                        result.push(lines[i].to_string());
                        i += 1;
                        continue;
                    }
                };

                // Check for init assignment immediately before `do {`
                let init_part = if i > 0 {
                    if let Some(ic) = INIT_ASSIGN.captures(lines[i - 1]) {
                        if ic[2] == cond_var {
                            // Remove the init line we already pushed
                            result.pop();
                            format!("{} = {}", cond_var, ic[3].trim())
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                let for_line = format!(
                    "{}for ({}; {} {} {}; {}) {{",
                    do_indent, init_part, cond_var, cond_op, cond_lim, inc_expr
                );
                result.push(for_line);

                // Push body lines except the increment line
                for (k, line) in lines.iter().enumerate().take(body_end).skip(body_start) {
                    if k != inc_idx {
                        result.push((*line).to_string());
                    }
                }
                result.push(format!("{}}}", do_indent));

                changed = true;
                i = close_idx + 1;
                continue;
            }

            result.push(lines[i].to_string());
            i += 1;
        }

        if !changed {
            return code.to_string();
        }
        result.join("\n")
    }

    pub(super) fn do_while_to_for_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("do") || !code.contains("while") {
            return Cow::Borrowed(code);
        }

        let output = Self::do_while_to_for(code);
        if output == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(output)
        }
    }
}
