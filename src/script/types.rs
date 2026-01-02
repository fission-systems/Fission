//! Python-exportable type wrappers for script bridge.
//!
//! These types are exposed to Python scripts via PyO3.

use pyo3::prelude::*;

/// Binary information exposed to Python
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyBinaryInfo {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub format: String,
    #[pyo3(get)]
    pub arch: String,
    #[pyo3(get)]
    pub entry_point: u64,
    #[pyo3(get)]
    pub is_64bit: bool,
    #[pyo3(get)]
    pub is_dotnet: bool,
    #[pyo3(get)]
    pub section_count: usize,
    #[pyo3(get)]
    pub function_count: usize,
}

#[pymethods]
impl PyBinaryInfo {
    fn __repr__(&self) -> String {
        format!(
            "Binary('{}', format='{}', arch='{}', entry={:#x})",
            self.name, self.format, self.arch, self.entry_point
        )
    }
}

/// Function information exposed to Python
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyFunctionInfo {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub address: u64,
    #[pyo3(get)]
    pub size: u64,
    #[pyo3(get)]
    pub is_export: bool,
    #[pyo3(get)]
    pub is_import: bool,
}

#[pymethods]
impl PyFunctionInfo {
    fn __repr__(&self) -> String {
        format!(
            "Function('{}', addr={:#x}, size={})",
            self.name, self.address, self.size
        )
    }
}

/// Disassembled instruction exposed to Python
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyInstruction {
    #[pyo3(get)]
    pub address: u64,
    #[pyo3(get)]
    pub bytes: Vec<u8>,
    #[pyo3(get)]
    pub mnemonic: String,
    #[pyo3(get)]
    pub operands: String,
}

#[pymethods]
impl PyInstruction {
    fn __repr__(&self) -> String {
        format!("{:#x}: {} {}", self.address, self.mnemonic, self.operands)
    }
}

/// Section information exposed to Python
#[pyclass]
#[derive(Debug, Clone)]
pub struct PySection {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub virtual_address: u64,
    #[pyo3(get)]
    pub virtual_size: u64,
    #[pyo3(get)]
    pub file_offset: u64,
    #[pyo3(get)]
    pub file_size: u64,
    #[pyo3(get)]
    pub is_executable: bool,
    #[pyo3(get)]
    pub is_writable: bool,
}

#[pymethods]
impl PySection {
    fn __repr__(&self) -> String {
        format!(
            "Section('{}', va={:#x}, size={:#x})",
            self.name, self.virtual_address, self.virtual_size
        )
    }
}
