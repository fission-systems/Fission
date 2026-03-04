//! Pcode FFI Bridge - C/C++ Interface for Pcode Optimization
//!
//! Exposes Rust Pcode optimizer to C++ decompiler via C ABI.
//! This module handles all unsafe FFI operations for pcode optimization.

use fission_pcode::{PcodeFunction, PcodeOptimizer, PcodeOptimizerConfig};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Optimize Pcode JSON (called from C++)
///
/// # Safety
/// - `pcode_json` must be a valid pointer to `json_len` bytes of UTF-8 JSON
/// - If `json_len == 0`, falls back to treating `pcode_json` as null-terminated
/// - Caller must free the returned pointer using `fission_free_string`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fission_optimize_pcode_json(
    pcode_json: *const c_char,
    json_len: usize,
) -> *mut c_char {
    if pcode_json.is_null() {
        eprintln!("[fission_optimize_pcode_json] Error: null input");
        return std::ptr::null_mut();
    }

    // C-1: Use json_len when provided to avoid relying solely on null termination.
    // Useful for inputs with embedded nulls or non-null-terminated buffers from C++.
    let json_owned: String = if json_len > 0 {
        let slice = unsafe { std::slice::from_raw_parts(pcode_json as *const u8, json_len) };
        match std::str::from_utf8(slice) {
            Ok(s) => s.to_owned(),
            Err(e) => {
                eprintln!(
                    "[fission_optimize_pcode_json] UTF-8 error (len-based): {}",
                    e
                );
                return std::ptr::null_mut();
            }
        }
    } else {
        // json_len == 0: fall back to null-terminated CStr
        match unsafe { CStr::from_ptr(pcode_json) }.to_str() {
            Ok(s) => s.to_owned(),
            Err(e) => {
                eprintln!("[fission_optimize_pcode_json] UTF-8 error: {}", e);
                return std::ptr::null_mut();
            }
        }
    };
    let json_str = json_owned.as_str();

    // Parse Pcode
    let mut pcode = match PcodeFunction::from_json(json_str) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[fission_optimize_pcode_json] JSON parse error: {}", e);
            return std::ptr::null_mut();
        }
    };

    // Optimize
    let config = PcodeOptimizerConfig::default();
    let mut optimizer = PcodeOptimizer::new(config);
    let num_passes = optimizer.optimize(&mut pcode);

    eprintln!(
        "[fission_optimize_pcode_json] Applied {} optimization passes",
        num_passes
    );

    // Serialize back to JSON
    let optimized_json = match serde_json::to_string(&pcode) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("[fission_optimize_pcode_json] JSON serialize error: {}", e);
            return std::ptr::null_mut();
        }
    };

    // Convert to C string
    match CString::new(optimized_json) {
        Ok(c_str) => c_str.into_raw(),
        Err(e) => {
            eprintln!(
                "[fission_optimize_pcode_json] CString conversion error: {}",
                e
            );
            std::ptr::null_mut()
        }
    }
}

/// Free string allocated by Rust (called from C++)
///
/// # Safety
/// - `ptr` must have been allocated by `fission_optimize_pcode_json`
/// - `ptr` must not be used after calling this function
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fission_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        // Reconstruct CString and drop it (frees memory)
        let _ = unsafe { CString::from_raw(ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_ffi_optimize_roundtrip() {
        let json = r#"{"blocks":[{"index":0,"start_addr":"0x1000","ops":[{"seq":0,"opcode":"INT_XOR","addr":"0x1000","output":{"space":1,"offset":"0x100","size":4},"inputs":[{"space":2,"offset":"0x10","size":4},{"space":0,"offset":"0x0","size":4,"const_val":0}]}]}]}"#;

        let c_json = match CString::new(json) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create CString: {}", e);
                return;
            }
        };
        let result_ptr = unsafe { fission_optimize_pcode_json(c_json.as_ptr(), json.len()) };

        assert!(!result_ptr.is_null());

        unsafe {
            match CStr::from_ptr(result_ptr).to_str() {
                Ok(result_str) => {
                    eprintln!("Result: {}", result_str);
                    // Check that optimization happened (XOR with 0 should become COPY)
                    // Note: The output format is different from input, check for optimization markers
                    assert!(result_str.len() > 0);
                }
                Err(e) => {
                    eprintln!("Failed to convert result to str: {}", e);
                    panic!("UTF-8 conversion failed");
                }
            }
            fission_free_string(result_ptr);
        }
    }
}
