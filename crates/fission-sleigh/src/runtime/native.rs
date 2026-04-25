use std::path::Path;
use std::ffi::CString;
use libloading::{Library, Symbol};
use anyhow::{Result, anyhow};

#[derive(Debug)]
pub struct NativeBackend {
    _library: Library,
}

type FissionDecodeMatchFn = unsafe extern "C" fn(table_name: *const i8, bytes: *const u8, len: usize, ctx_ptr: *const u64) -> i32;

impl NativeBackend {
    pub fn load(path: &Path) -> Result<Self> {
        eprintln!("[fission-sleigh] Loading native backend from: {}", path.display());
        let library = unsafe { Library::new(path) }
            .map_err(|e| anyhow!("failed to load native backend {}: {}", path.display(), e))?;
        Ok(Self { _library: library })
    }

    pub fn decode_match(&self, table_name: &str, bytes: &[u8], context_register: u64) -> Result<Option<usize>> {
        let func: Symbol<FissionDecodeMatchFn> = unsafe { self._library.get(b"fission_decode_match") }?;
        let table_name_c = CString::new(table_name)?;
        let result = unsafe { func(table_name_c.as_ptr(), bytes.as_ptr(), bytes.len(), &context_register) };
        if result >= 0 {
            Ok(Some(result as usize))
        } else {
            Ok(None)
        }
    }
}
