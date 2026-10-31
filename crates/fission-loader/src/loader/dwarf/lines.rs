//! DWARF `.debug_line` Line-Number Program Extraction
//!
//! Runs each compilation unit's line-number program (the byte-coded state
//! machine described in DWARF section 6.2) to produce a flat, address-sorted
//! address -> (file, line) matrix, giving `LoadedBinary::line_for_address`
//! something to look up against. The raw `.debug_line` section was already
//! loaded into `gimli::Dwarf` (see `analyzer::build_dwarf`) but its
//! `line_program`/row iterator was never run anywhere in the loader before
//! this -- every unit's line table existed only as unread bytes.

use crate::loader::types::DwarfLineRow;
use std::collections::HashMap;

impl<'a> super::analyzer::DwarfAnalyzer<'a> {
    /// Extract the full address-to-source-line matrix from every
    /// compilation unit's line-number program, sorted ascending by
    /// `address` (the order `LoadedBinary::line_for_address`'s binary
    /// search requires).
    pub fn analyze_lines(&self) -> Vec<DwarfLineRow> {
        if !self.has_debug_info() {
            return Vec::new();
        }

        match self.analyze_lines_inner() {
            Ok(mut rows) => {
                // At one address a marker (end of a sequence) sorts before a real
                // row (start of the next), so the lookup's last-at-address pick is
                // the real one.
                rows.sort_by_key(|row| (row.address, row.line != 0));
                tracing::info!(
                    "[DwarfAnalyzer] Extracted {} line-table rows from .debug_line",
                    rows.len()
                );
                rows
            }
            Err(e) => {
                tracing::warn!("[DwarfAnalyzer] Error parsing .debug_line: {}", e);
                Vec::new()
            }
        }
    }

    fn analyze_lines_inner(&self) -> Result<Vec<DwarfLineRow>, gimli::Error> {
        let dwarf = self.build_dwarf()?;
        let mut out = Vec::new();

        let mut units = dwarf.units();
        while let Some(unit_header) = units.next()? {
            let unit = dwarf.unit(unit_header)?;
            let Some(program) = unit.line_program.clone() else {
                continue;
            };

            // File names repeat across nearly every row of a real program;
            // resolving `path_name()` (a `DebugStrRef`/`DebugLineStrRef`
            // indirection) once per distinct file index instead of once per
            // row avoids redundant string-table lookups.
            let mut file_names: HashMap<u64, String> = HashMap::new();

            let mut rows = program.rows();
            while let Some((header, row)) = rows.next_row()? {
                if row.end_sequence() {
                    // The address just past the last instruction of a
                    // sequence. Nothing from here until the next sequence
                    // starts has a source line, so keep it as a marker
                    // (`line == 0`) that ends the previous row's reach.
                    out.push(DwarfLineRow::unattributed(row.address()));
                    continue;
                }
                let Some(line) = row.line() else {
                    // Producers use line 0 for instructions that can't be
                    // attributed to any source line (e.g. compiler-generated
                    // prologue padding). Same marker: the previous line must
                    // not extend over them.
                    out.push(DwarfLineRow::unattributed(row.address()));
                    continue;
                };

                let file_index = row.file_index();
                let file = match file_names.get(&file_index) {
                    Some(name) => name.clone(),
                    None => {
                        let Some(entry) = row.file(header) else {
                            continue;
                        };
                        let name = dwarf
                            .attr_string(&unit, entry.path_name())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        file_names.insert(file_index, name.clone());
                        name
                    }
                };
                if file.is_empty() {
                    continue;
                }

                out.push(DwarfLineRow {
                    address: row.address(),
                    file,
                    line: u32::try_from(line.get()).unwrap_or(u32::MAX),
                });
            }
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use crate::loader::LoadedBinary;
    use crate::loader::dwarf::DwarfAnalyzer;

    #[test]
    fn analyze_lines_returns_empty_without_debug_info() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_pdb_struct_test.exe");
        let Ok(binary) = LoadedBinary::from_file(&path) else {
            return;
        };
        let analyzer = DwarfAnalyzer::new(&binary);
        assert!(analyzer.analyze_lines().is_empty());
    }

    #[test]
    fn analyze_lines_extracts_real_address_to_line_matrix() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_dyn_enum_test.elf");
        let binary = LoadedBinary::from_file(&path).expect("load enum test ELF");
        let analyzer = DwarfAnalyzer::new(&binary);
        let rows = analyzer.analyze_lines();
        assert!(
            rows.windows(2).all(|w| w[0].address <= w[1].address),
            "rows must be sorted ascending by address: {rows:?}"
        );
        assert!(
            rows.iter()
                .filter(|r| r.line == 0)
                .all(|r| r.file.is_empty()),
            "a line-0 row is an unattributed marker with no file: {rows:?}"
        );

        // Cross-checked byte-for-byte against `llvm-dwarfdump --debug-line`
        // on this exact fixture: `pick()`'s body runs 0x401106..0x401120,
        // lines 8-11 (the `end_sequence` row at 0x401131 is the one address
        // this table intentionally omits -- it's a boundary marker, not an
        // instruction attributable to a line).
        let expected: &[(u64, &str, u32)] = &[
            (0x401106, "enum_test.c", 8),
            (0x40110d, "enum_test.c", 9),
            (0x401113, "enum_test.c", 9),
            (0x40111a, "enum_test.c", 10),
            (0x40111f, "enum_test.c", 11),
        ];
        for &(address, file, line) in expected {
            let row = rows
                .iter()
                .find(|r| r.address == address)
                .unwrap_or_else(|| panic!("missing row for 0x{address:x} in {rows:?}"));
            assert_eq!(row.file, file, "wrong file at 0x{address:x}");
            assert_eq!(row.line, line, "wrong line at 0x{address:x}");
        }
        let end = rows
            .iter()
            .find(|r| r.address == 0x401131)
            .unwrap_or_else(|| panic!("end_sequence at 0x401131 must be kept: {rows:?}"));
        assert_eq!(end.line, 0, "the end_sequence row is a marker, not a line");
    }

    #[test]
    fn line_for_address_finds_nearest_preceding_row() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_dyn_enum_test.elf");
        let binary = LoadedBinary::from_file(&path).expect("load enum test ELF");

        // Exact row hit.
        let row = binary
            .line_for_address(0x40110d)
            .expect("exact-address lookup");
        assert_eq!(row.line, 9);

        // Mid-instruction address: falls back to the nearest preceding row,
        // matching the "line covers everything up to the next row" DWARF
        // convention.
        let row = binary.line_for_address(0x40110f).expect("mid-range lookup");
        assert_eq!(row.line, 9);

        // Before the first row entirely.
        assert!(binary.line_for_address(0x1).is_none());

        // 0x401131 is the `end_sequence` address: the last real instruction
        // is before it, so it and everything after (a gap, other sections,
        // the top of the address space) has no line. These used to answer
        // with the last row of the sequence.
        let marker = binary
            .dwarf_lines
            .iter()
            .position(|r| r.address == 0x401131)
            .expect("end_sequence marker");
        let last_real = &binary.dwarf_lines[marker - 1];
        assert_ne!(last_real.line, 0);
        assert_eq!(
            binary.line_for_address(0x401130).map(|r| r.line),
            Some(last_real.line)
        );
        assert!(binary.line_for_address(0x401131).is_none());
        assert!(binary.line_for_address(0x402000).is_none());
        assert!(binary.line_for_address(u64::MAX).is_none());
    }

    #[test]
    fn a_marker_at_the_same_address_as_the_next_sequence_does_not_hide_it() {
        use crate::loader::types::DwarfLineRow;
        let mut binary = LoadedBinary::from_file(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("testdata/x64_dyn_enum_test.elf"),
        )
        .expect("load enum test ELF");
        let real = |address, line| DwarfLineRow {
            address,
            file: "a.c".into(),
            line,
        };
        // Sequence A ends where sequence B begins (0x20).
        binary.dwarf_lines = vec![
            real(0x10, 1),
            DwarfLineRow::unattributed(0x20),
            real(0x20, 7),
            DwarfLineRow::unattributed(0x30),
        ];
        assert_eq!(binary.line_for_address(0x1f).unwrap().line, 1);
        assert_eq!(binary.line_for_address(0x20).unwrap().line, 7);
        assert_eq!(binary.line_for_address(0x2f).unwrap().line, 7);
        assert!(binary.line_for_address(0x30).is_none());
    }
}
