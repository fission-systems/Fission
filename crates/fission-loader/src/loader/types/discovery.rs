use super::LoadedBinary;

impl LoadedBinary {
    /// Rebuild function lookup indices after modifying the functions vector.
    ///
    /// Function discovery is intentionally not implemented in `fission-loader`.
    /// The loader owns authoritative binary metadata only; SLEIGH-driven
    /// analyzer passes live above it in `fission-static`.
    pub fn rebuild_function_indices(&mut self) {
        self.function_addr_index.clear();
        self.function_name_index.clear();

        let mut addr_index: std::collections::HashMap<u64, usize> =
            std::collections::HashMap::new();
        let mut name_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (idx, func) in self.functions.iter().enumerate() {
            // Several entries can share an address; keep the one the canonical
            // views prefer (an export or a named symbol over a generic
            // `sub_`), exactly as `rebuild_indices` does. Overwriting in slice
            // order let whichever came last win, so merging C++ vtable
            // functions and rebuilding here could replace a named symbol with
            // a generic one at the same address.
            match addr_index.get(&func.address) {
                Some(&current)
                    if !crate::loader::function_view::prefers_function_entry(
                        func,
                        &self.functions[current],
                    ) => {}
                _ => {
                    addr_index.insert(func.address, idx);
                }
            }
            if !func.name.is_empty() {
                name_index.insert(func.name.clone(), idx);
            }
        }
        self.function_addr_index = addr_index;
        self.function_name_index = name_index;
    }
}

#[cfg(test)]
mod tests {
    use crate::loader::FunctionInfo;
    use crate::loader::types::{DataBuffer, LoadedBinaryBuilder};

    fn function(name: &str, address: u64, is_export: bool) -> FunctionInfo {
        FunctionInfo {
            name: name.into(),
            address,
            size: 16,
            is_export,
            ..FunctionInfo::default()
        }
    }

    /// `rebuild_function_indices` kept whichever entry came last at an address,
    /// so after merging extra functions a generic `sub_` placed after a named
    /// export took its address over, while `rebuild_indices` and the canonical
    /// views kept the export.
    #[test]
    fn rebuilding_the_indices_keeps_the_preferred_entry_at_an_address() {
        let mut binary = LoadedBinaryBuilder::new("idx.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .format("ELF")
            .build()
            .expect("build");
        binary.inner_mut().functions = vec![
            function("sub_1000", 0x1000, false),
            function("exported_name", 0x1000, true),
            function("sub_1000b", 0x1000, false),
            function("other", 0x2000, false),
        ];
        binary.rebuild_function_indices();

        let at = binary.function_addr_index[&0x1000];
        assert_eq!(binary.functions[at].name, "exported_name");
        // Every name still resolves, the preferred entry's included.
        assert_eq!(binary.function_name_index["exported_name"], 1);
        assert_eq!(
            binary.functions[binary.function_addr_index[&0x2000]].name,
            "other"
        );

        // And it agrees with the other builder of the same index.
        let expected = binary.function_addr_index.clone();
        binary.rebuild_indices();
        assert_eq!(binary.function_addr_index, expected);
    }
}
