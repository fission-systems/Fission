use super::{FunctionInfo, LoadedBinary};
use fission_disasm::DisasmEngine;

impl LoadedBinary {
    /// Discover internal functions by scanning executable code for CALL instructions
    /// This finds functions that are called but not exported/imported
    pub fn discover_internal_functions(&mut self) {
        use std::collections::HashSet;

        let engine = match DisasmEngine::new(self.is_64bit) {
            Ok(e) => e,
            Err(_) => return,
        };

        let executable_ranges: Vec<(u64, u64)> = self
            .sections
            .iter()
            .filter(|s| s.is_executable)
            .map(|s| (s.virtual_address, s.virtual_address + s.virtual_size))
            .collect();

        let is_in_executable_range = |target: u64| -> bool {
            executable_ranges
                .iter()
                .any(|&(start, end)| target >= start && target < end)
        };

        let total_code_size: u64 = executable_ranges.iter().map(|(s, e)| e - s).sum();
        let estimated_functions = (total_code_size / 100) as usize;
        let mut discovered: HashSet<u64> = HashSet::with_capacity(estimated_functions.max(64));

        for section in &self.sections {
            if !section.is_executable {
                continue;
            }

            let start = section.file_offset as usize;
            let size = section.file_size as usize;
            if start + size > self.data.as_slice().len() {
                continue;
            }
            let bytes = &self.data.as_slice()[start..start + size];

            let targets = engine.discover_call_targets(bytes, section.virtual_address);

            for target in targets {
                if self.function_addr_index.contains_key(&target) {
                    continue;
                }

                if !discovered.contains(&target) && is_in_executable_range(target) {
                    discovered.insert(target);
                }
            }
        }

        self.functions.reserve(discovered.len());

        for addr in discovered {
            self.functions.push(FunctionInfo {
                name: format!("sub_{:x}", addr),
                address: addr,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }

        self.functions.sort_by_key(|f| f.address);
        self.functions_sorted = true;

        self.rebuild_function_indices();
    }

    /// Discover functions by scanning for common prologue patterns and CALL targets
    ///
    /// This is useful when the control flow is obfuscated (e.g., indirect calls)
    /// and standard call-graph usage fails to find all functions.
    pub fn discover_functions_by_prologue(&mut self) -> usize {
        let mut count = 0;
        let mut candidates = std::collections::HashSet::new();

        let patterns: &[&[u8]] = if self.is_64bit {
            &[
                &[0x55, 0x48, 0x89, 0xe5],
                &[0x48, 0x83, 0xec],
                &[0x48, 0x81, 0xec],
                &[0x55, 0x48, 0x8d, 0x2c, 0x24],
                &[0x40, 0x55, 0x48, 0x8b, 0xec],
                &[0x48, 0x8b, 0xc4],
            ]
        } else {
            &[
                &[0x55, 0x89, 0xe5],
                &[0x55, 0x8b, 0xec],
                &[0x83, 0xec],
                &[0x81, 0xec],
            ]
        };

        let mut exec_ranges = Vec::new();
        for section in &self.sections {
            if section.is_executable {
                exec_ranges.push((
                    section.virtual_address,
                    section.virtual_address + section.virtual_size,
                ));
            }
        }

        for section in &self.sections {
            if !section.is_executable {
                continue;
            }

            let start = section.file_offset as usize;
            let end = (section.file_offset + section.file_size) as usize;
            if end > self.data.as_slice().len() {
                continue;
            }

            let search_limit = (512 * 1024).min(self.data.as_slice().len() - start);
            let data = &self.data.as_slice()[start..start + search_limit];
            let va_start = section.virtual_address;

            for i in 0..data.len() {
                if i + 4 <= data.len() {
                    let window = &data[i..];
                    for pat in patterns {
                        if window.starts_with(pat) {
                            let potential_addr = va_start + i as u64;
                            if !self.function_addr_index.contains_key(&potential_addr) {
                                candidates.insert(potential_addr);
                            }
                            break;
                        }
                    }
                }

                if i + 5 <= data.len() && data[i] == 0xE8 {
                    let rel_bytes = [data[i + 1], data[i + 2], data[i + 3], data[i + 4]];
                    let rel = i32::from_le_bytes(rel_bytes);

                    let call_insn_addr = va_start + i as u64;
                    let target_addr = (call_insn_addr.wrapping_add(5)).wrapping_add(rel as u64);

                    let is_valid = exec_ranges
                        .iter()
                        .any(|(s, e)| target_addr >= *s && target_addr < *e);

                    if is_valid {
                        let addr_masked = if self.is_64bit {
                            target_addr
                        } else {
                            target_addr & 0xFFFFFFFF
                        };

                        if !self.function_addr_index.contains_key(&addr_masked) {
                            candidates.insert(addr_masked);
                        }
                    }
                }
            }
        }

        for addr in candidates {
            self.functions.push(FunctionInfo {
                name: format!("sub_{:x}_scanned", addr),
                address: addr,
                size: 0,
                is_export: false,
                is_import: false,
            });
            count += 1;
        }

        if count > 0 {
            self.functions.sort_by_key(|f| f.address);
            self.functions_sorted = true;
            self.rebuild_function_indices();
        }

        count
    }

    /// Rebuild function lookup indices after modifying the functions vector
    pub fn rebuild_function_indices(&mut self) {
        self.function_addr_index.clear();
        self.function_name_index.clear();

        let entries: Vec<_> = self
            .functions
            .iter()
            .enumerate()
            .map(|(idx, func)| (idx, func.address, func.name.clone()))
            .collect();

        for (idx, addr, name) in entries {
            self.function_addr_index.insert(addr, idx);
            if !name.is_empty() {
                self.function_name_index.insert(name, idx);
            }
        }
    }
}
