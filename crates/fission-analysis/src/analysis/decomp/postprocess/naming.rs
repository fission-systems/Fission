use crate::analysis::decomp::postprocess::PostProcessor;
use crate::utils::patterns::*;
use fission_loader::loader::types::DwarfLocation;
use std::borrow::Cow;

impl PostProcessor {
    /// Replace pointer offset accesses with field names
    /// e.g., *(ptr + 0x18) -> this->counter (if offset 24 maps to 'counter')
    pub(super) fn replace_field_offsets_cow<'a>(&self, code: &'a str) -> Cow<'a, str> {
        let mut result = code.to_string();

        let mut offset_map: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        for ty in &self.inferred_types {
            for field in &ty.fields {
                offset_map.insert(field.offset, field.name.clone());
            }
        }

        if offset_map.is_empty() {
            return Cow::Borrowed(code);
        }

        result = PTR_OFFSET
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let offset_str = &caps[2];

                let offset: u32 = if offset_str.starts_with("0x") || offset_str.starts_with("0X") {
                    u32::from_str_radix(&offset_str[2..], 16).unwrap_or(0)
                } else {
                    offset_str.parse().unwrap_or(0)
                };

                if let Some(field_name) = offset_map.get(&offset) {
                    format!("{}->{}/* @{} */", base, field_name, offset_str)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = CAST_PTR_OFFSET
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let offset_str = &caps[2];

                let offset: u32 = if offset_str.starts_with("0x") || offset_str.starts_with("0X") {
                    u32::from_str_radix(&offset_str[2..], 16).unwrap_or(0)
                } else {
                    offset_str.parse().unwrap_or(0)
                };

                if let Some(field_name) = offset_map.get(&offset) {
                    format!("{}->{}/* @{} */", base, field_name, offset_str)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = ARRAY_INDEX
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let offset_str = &caps[2];

                let offset: u32 = u32::from_str_radix(&offset_str[2..], 16).unwrap_or(0);

                if let Some(field_name) = offset_map.get(&offset) {
                    format!("{}->{}/* @{} */", base, field_name, offset_str)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = FIELD_OFFSET
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let offset: u32 = caps[2].parse().unwrap_or(0);
                let _size: u32 = caps[3].parse().unwrap_or(0);

                if let Some(field_name) = offset_map.get(&offset) {
                    format!("{}.{}/* @{} */", base, field_name, offset)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = self.recognize_swift_accessors_cow(&result).into_owned();

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn replace_field_offsets(&self, code: &str) -> String {
        self.replace_field_offsets_cow(code).into_owned()
    }

    /// Recognize Swift accessor patterns and convert to field access
    /// Swift uses VTable calls for property access:
    /// getter: (**(ptr + 0x88))(buffer) -> ptr->get_fieldName()
    /// setter: (**(ptr + 0x90))(value, buffer) -> ptr->set_fieldName(value)
    pub(super) fn recognize_swift_accessors_cow<'a>(&self, code: &'a str) -> Cow<'a, str> {
        if !DOUBLE_PTR_DEREF.is_match(code) && !XMM_FIELD.is_match(code) {
            return Cow::Borrowed(code);
        }

        let mut result = code.to_string();

        result = DOUBLE_PTR_DEREF
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let vtable_offset_str = &caps[2];

                let vtable_offset: u32 = if vtable_offset_str.starts_with("0x") {
                    u32::from_str_radix(&vtable_offset_str[2..], 16).unwrap_or(0)
                } else {
                    vtable_offset_str.parse().unwrap_or(0)
                };

                let accessor_type = match vtable_offset & 0x0f {
                    0x8 => "get",
                    0x0 => "set",
                    _ => "access",
                };

                let estimated_field_index = vtable_offset.saturating_sub(0x50) / 0x10;

                let field_hint: String = self
                    .inferred_types
                    .iter()
                    .flat_map(|t| t.fields.iter())
                    .nth(estimated_field_index as usize)
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| format!("property_{}", estimated_field_index));

                format!(
                    "/* Swift {} {} via VTable@{} */(**(void**)(*{} + {}))",
                    accessor_type, field_hint, vtable_offset_str, base, vtable_offset_str
                )
            })
            .to_string();

        result = XMM_FIELD
            .replace_all(&result, |caps: &regex::Captures| {
                let var = &caps[1];
                format!("{}->value/* Swift property value */", var)
            })
            .to_string();

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn demangle_swift_symbols_cow<'a>(&self, code: &'a str) -> Cow<'a, str> {
        if !MANGLED_NAME.is_match(code) {
            return Cow::Borrowed(code);
        }

        Cow::Owned(
            MANGLED_NAME
                .replace_all(code, |caps: &regex::Captures| {
                    let symbol = &caps[0];
                    fission_loader::loader::demangle::demangle(symbol)
                })
                .to_string(),
        )
    }

    pub(super) fn demangle_swift_symbols(&self, code: &str) -> String {
        self.demangle_swift_symbols_cow(code).into_owned()
    }

    // =========================================================================
    // B-3: Induction variable naming
    //  (RetDec readable_var_renamer.cpp — visit(ForLoopStmt))
    //
    // Rename compiler-generated loop counter variables (local_XX, xVarN)
    // in for-loop headers to i, j, k, l, m, n based on nesting order.
    // Avoids collision with already-used identifiers.
    // =========================================================================
    pub(super) fn rename_induction_vars_cow<'a>(code: &'a str) -> Cow<'a, str> {
        let candidate_names = ["i", "j", "k", "l", "m", "n"];

        let mut used_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for cap in IDENTIFIER.find_iter(code) {
            used_ids.insert(cap.as_str().to_string());
        }

        let mut induction_vars: Vec<String> = Vec::new();
        for caps in FOR_INIT.captures_iter(code) {
            let var = caps[1].to_string();
            if GENERIC_VAR.is_match(&var) && !induction_vars.contains(&var) {
                induction_vars.push(var);
            }
        }

        if induction_vars.is_empty() {
            return Cow::Borrowed(code);
        }

        let mut result = code.to_string();
        let mut assigned_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut name_idx = 0;

        for var in &induction_vars {
            while name_idx < candidate_names.len() {
                let name = candidate_names[name_idx];
                if !used_ids.contains(name) || name == var.as_str() {
                    if !assigned_names.contains(name) {
                        break;
                    }
                }
                name_idx += 1;
            }
            if name_idx >= candidate_names.len() {
                break;
            }

            let new_name = candidate_names[name_idx];
            assigned_names.insert(new_name.to_string());
            name_idx += 1;

            let pattern = format!(r"\b{}\b", regex::escape(var));
            if let Ok(re) = regex::Regex::new(&pattern) {
                result = re.replace_all(&result, new_name).to_string();
            }
        }

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn rename_induction_vars(code: &str) -> String {
        Self::rename_induction_vars_cow(code).into_owned()
    }

    // =========================================================================
    // B-4: Semantic variable naming
    //  (RetDec readable_var_renamer.cpp — multiple visitors)
    //
    //  1. main() → param_1 = argc, param_2 = argv
    //  2. Single return-value temp → "result"
    //  3. API result naming: var = malloc(...) → ptr, strlen() → len, etc.
    // =========================================================================
    pub(super) fn rename_semantic_vars_cow<'a>(code: &'a str) -> Cow<'a, str> {
        let mut result = code.to_string();

        if MAIN_FUNC.is_match(&result) {
            if let Ok(re) = regex::Regex::new(r"\bparam_1\b") {
                if re.is_match(&result) {
                    result = re.replace_all(&result, "argc").to_string();
                }
            }
            if let Ok(re) = regex::Regex::new(r"\bparam_2\b") {
                if re.is_match(&result) {
                    result = re.replace_all(&result, "argv").to_string();
                }
            }
        }

        {
            let mut return_vars: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for caps in RETURN_STMT.captures_iter(&result) {
                let v = caps[1].to_string();
                if GENERIC_VAR.is_match(&v) {
                    return_vars.insert(v);
                }
            }
            if return_vars.len() == 1 {
                // Safe: We just confirmed len == 1, so next() will return Some
                if let Some(var) = return_vars.into_iter().next() {
                    if !result.contains("result") || result.contains(&var) {
                        let pat = format!(r"\b{}\b", regex::escape(&var));
                        if let Ok(re) = regex::Regex::new(&pat) {
                            result = re.replace_all(&result, "result").to_string();
                        }
                    }
                }
            }
        }

        let api_map: std::collections::HashMap<&str, &str> = [
            ("malloc", "ptr"),
            ("calloc", "ptr"),
            ("realloc", "ptr"),
            ("mmap", "mapped"),
            ("strlen", "len"),
            ("wcslen", "len"),
            ("sizeof", "size"),
            ("fopen", "fp"),
            ("fdopen", "fp"),
            ("tmpfile", "fp"),
            ("fgets", "line"),
            ("fread", "bytes_read"),
            ("socket", "sock_fd"),
            ("accept", "client_fd"),
            ("open", "fd"),
            ("creat", "fd"),
            ("getenv", "env_val"),
            ("strcmp", "cmp"),
            ("strncmp", "cmp"),
            ("memcmp", "cmp"),
            ("strstr", "found"),
            ("strchr", "found"),
            ("strrchr", "found"),
            ("atoi", "num"),
            ("atol", "num"),
            ("strtol", "num"),
            ("strtoul", "num"),
            ("pthread_create", "thread_err"),
        ]
        .into_iter()
        .collect();

        let mut renames: Vec<(String, String)> = Vec::new();
        for caps in FUNC_CALL_ASSIGN.captures_iter(&result.clone()) {
            let var = caps[1].to_string();
            let func = &caps[2];
            if let Some(&new_base) = api_map.get(func) {
                if var != "result" && !renames.iter().any(|(o, _)| o == &var) {
                    renames.push((var, new_base.to_string()));
                }
            }
        }

        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (old, new_base) in &renames {
            let mut new_name = new_base.clone();
            let mut suffix = 2u32;
            while used.contains(&new_name) {
                new_name = format!("{}{}", new_base, suffix);
                suffix += 1;
            }
            used.insert(new_name.clone());
            let pat = format!(r"\b{}\b", regex::escape(old));
            if let Ok(re) = regex::Regex::new(&pat) {
                result = re.replace_all(&result, new_name.as_str()).to_string();
            }
        }

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn rename_semantic_vars(code: &str) -> String {
        Self::rename_semantic_vars_cow(code).into_owned()
    }

    /// Apply DWARF debug info to substitute parameter and local variable names.
    ///
    /// Ghidra generates names like `param_1`, `param_2`, `local_38`, `local_10`, etc.
    /// If DWARF provides real names, we substitute them.
    ///
    /// Matching strategies:
    /// - `param_N` → Nth DWARF parameter name (1-indexed)
    /// - `local_XX` → DWARF local var where XX is the absolute hex stack offset
    ///   (Ghidra: `local_38` means StackOffset(-0x38))
    /// - `in_REG` → DWARF param/var located in that register
    pub(super) fn apply_dwarf_names_cow<'a>(&self, code: &'a str) -> Cow<'a, str> {
        let Some(ref dwarf) = self.dwarf_info else {
            return Cow::Borrowed(code);
        };

        let has_candidate = code.contains("param_")
            || code.contains("local_")
            || code.contains("in_")
            || (dwarf.return_type.is_some() && UNDEF_TYPE_DECL.is_match(code));
        if !has_candidate {
            return Cow::Borrowed(code);
        }

        let mut result = code.to_string();

        for (i, param) in dwarf.params.iter().enumerate() {
            let ghidra_name = format!("param_{}", i + 1);
            let pattern = format!(r"\b{}\b", regex::escape(&ghidra_name));
            if let Ok(re) = regex::Regex::new(&pattern) {
                result = re.replace_all(&result, param.name.as_str()).to_string();
            }
        }

        for var in &dwarf.local_vars {
            if let DwarfLocation::StackOffset(offset) = &var.location {
                let abs_offset = offset.unsigned_abs();
                if abs_offset > 0 {
                    let ghidra_name = format!("local_{:x}", abs_offset);
                    let pattern = format!(r"\b{}\b", regex::escape(&ghidra_name));
                    if let Ok(re) = regex::Regex::new(&pattern) {
                        result = re.replace_all(&result, var.name.as_str()).to_string();
                    }
                }
            }
        }

        let x86_64_dwarf_to_name = |reg_num: u16| -> Option<&'static str> {
            match reg_num {
                0 => Some("RAX"),
                1 => Some("RDX"),
                2 => Some("RCX"),
                3 => Some("RBX"),
                4 => Some("RSI"),
                5 => Some("RDI"),
                6 => Some("RBP"),
                7 => Some("RSP"),
                8 => Some("R8"),
                9 => Some("R9"),
                10 => Some("R10"),
                11 => Some("R11"),
                12 => Some("R12"),
                13 => Some("R13"),
                14 => Some("R14"),
                15 => Some("R15"),
                _ => None,
            }
        };

        for param in &dwarf.params {
            if let DwarfLocation::Register(ref reg_str) = param.location {
                if let Some(num_str) = reg_str.strip_prefix("reg") {
                    if let Ok(reg_num) = num_str.parse::<u16>() {
                        if let Some(reg_name) = x86_64_dwarf_to_name(reg_num) {
                            let ghidra_name = format!("in_{}", reg_name);
                            let pattern = format!(r"\b{}\b", regex::escape(&ghidra_name));
                            if let Ok(re) = regex::Regex::new(&pattern) {
                                result = re.replace_all(&result, param.name.as_str()).to_string();
                            }
                        }
                    }
                }
            }
        }

        if let Some(ref ret_type) = dwarf.return_type {
            result = UNDEF_TYPE_DECL
                .replace(&result, |caps: &regex::Captures| {
                    format!("{} {}", ret_type, &caps[2])
                })
                .to_string();
        }

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn apply_dwarf_names(&self, code: &str) -> String {
        self.apply_dwarf_names_cow(code).into_owned()
    }
}
