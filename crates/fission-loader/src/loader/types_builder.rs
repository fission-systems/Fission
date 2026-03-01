use super::{FunctionInfo, LoadedBinary, LoadedBinaryBuilder, LoadedBinaryInner, SectionInfo};
use crate::prelude::*;
use std::sync::Arc;

impl LoadedBinaryBuilder {
    pub fn new(path: String, data: super::DataBuffer) -> Self {
        let hash = blake3::hash(data.as_slice()).to_hex().to_string();
        Self {
            path,
            hash,
            data,
            arch_spec: "x86:LE:64:default".to_string(),
            entry_point: 0,
            image_base: 0,
            functions: Vec::new(),
            sections: Vec::new(),
            is_64bit: false,
            format: "unknown".to_string(),
            iat_symbols: std::collections::HashMap::new(),
            global_symbols: std::collections::HashMap::new(),
        }
    }

    pub fn arch_spec(mut self, arch_spec: impl Into<String>) -> Self {
        self.arch_spec = arch_spec.into();
        self
    }

    pub fn entry_point(mut self, entry_point: u64) -> Self {
        self.entry_point = entry_point;
        self
    }

    pub fn image_base(mut self, image_base: u64) -> Self {
        self.image_base = image_base;
        self
    }

    pub fn is_64bit(mut self, is_64bit: bool) -> Self {
        self.is_64bit = is_64bit;
        self
    }

    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = format.into();
        self
    }

    pub fn add_function(mut self, function: FunctionInfo) -> Self {
        self.functions.push(function);
        self
    }

    pub fn add_functions(mut self, functions: impl IntoIterator<Item = FunctionInfo>) -> Self {
        self.functions.extend(functions);
        self
    }

    pub fn add_section(mut self, section: SectionInfo) -> Self {
        self.sections.push(section);
        self
    }

    pub fn add_sections(mut self, sections: impl IntoIterator<Item = SectionInfo>) -> Self {
        self.sections.extend(sections);
        self
    }

    pub fn add_iat_symbol(mut self, va: u64, name: String) -> Self {
        self.iat_symbols.insert(va, name);
        self
    }

    pub fn add_iat_symbols(mut self, symbols: std::collections::HashMap<u64, String>) -> Self {
        self.iat_symbols.extend(symbols);
        self
    }

    pub fn add_global_symbol(mut self, va: u64, name: String) -> Self {
        self.global_symbols.insert(va, name);
        self
    }

    pub fn add_global_symbols(mut self, symbols: std::collections::HashMap<u64, String>) -> Self {
        self.global_symbols.extend(symbols);
        self
    }

    pub fn build(self) -> Result<LoadedBinary> {
        let mut functions = self.functions;
        functions.sort_by_key(|f| f.address);

        let mut function_addr_index = std::collections::HashMap::new();
        let mut function_name_index = std::collections::HashMap::new();
        for (idx, func) in functions.iter_mut().enumerate() {
            if !func.name.is_empty() {
                let demangled = crate::loader::demangle::demangle(&func.name);
                if demangled != func.name {
                    func.name = demangled;
                }
                function_name_index.insert(func.name.clone(), idx);
            }
            function_addr_index.insert(func.address, idx);
        }

        let mut iat_symbols = std::collections::HashMap::new();
        for (addr, name) in self.iat_symbols {
            let demangled = crate::loader::demangle::demangle(&name);
            iat_symbols.insert(addr, demangled);
        }

        let mut global_symbols = std::collections::HashMap::new();
        for (addr, name) in self.global_symbols {
            let demangled = crate::loader::demangle::demangle(&name);
            global_symbols.insert(addr, demangled);
        }

        let inner = LoadedBinaryInner {
            path: self.path,
            hash: self.hash,
            data: Arc::new(self.data),
            arch_spec: self.arch_spec,
            entry_point: self.entry_point,
            image_base: self.image_base,
            functions,
            sections: self.sections,
            is_64bit: self.is_64bit,
            format: self.format,
            iat_symbols,
            global_symbols,
            function_addr_index,
            function_name_index,
            functions_sorted: true,
            inferred_types: Vec::new(),
        };

        Ok(LoadedBinary::from_inner(inner))
    }
}
