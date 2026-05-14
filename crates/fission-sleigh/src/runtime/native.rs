use anyhow::{anyhow, Result};
use libloading::{Library, Symbol};
use std::ffi::CString;
use std::path::Path;

#[derive(Debug)]
pub struct NativeBackend {
    _library: Library,
    decode_match_fn: FissionDecodeMatchFn,
}

type FissionDecodeMatchFn = unsafe extern "C" fn(
    table_name: *const i8,
    bytes: *const u8,
    len: usize,
    ctx_ptr: *const u64,
) -> i32;

impl NativeBackend {
    pub fn load(path: &Path) -> Result<Self> {
        eprintln!(
            "[fission-sleigh] Loading native backend from: {}",
            path.display()
        );
        let library = unsafe { Library::new(path) }
            .map_err(|e| anyhow!("failed to load native backend {}: {}", path.display(), e))?;
        let decode_match_fn = {
            let func: Symbol<FissionDecodeMatchFn> =
                unsafe { library.get(b"fission_decode_match") }?;
            *func
        };
        Ok(Self {
            _library: library,
            decode_match_fn,
        })
    }

    pub fn decode_match(
        &self,
        table_name: &str,
        bytes: &[u8],
        context_register: u64,
    ) -> Result<Option<usize>> {
        let table_name_c = CString::new(table_name)?;
        let result = unsafe {
            (self.decode_match_fn)(
                table_name_c.as_ptr(),
                bytes.as_ptr(),
                bytes.len(),
                &context_register,
            )
        };
        if result >= 0 {
            Ok(Some(result as usize))
        } else {
            Ok(None)
        }
    }
}
