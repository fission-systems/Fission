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
