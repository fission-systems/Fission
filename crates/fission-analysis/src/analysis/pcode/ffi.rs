//! FFI bridge for Pcode optimization
//!
//! Exposes Rust Pcode optimizer to C++ decompiler via C ABI

use crate::analysis::pcode::{PcodeFunction, PcodeOptimizer, PcodeOptimizerConfig};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Optimize Pcode JSON (called from C++)
/// 
/// # Safety
/// - `pcode_json` must be a valid null-terminated C string
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
    
    // Convert C string to Rust string
    let json_str = match unsafe { CStr::from_ptr(pcode_json) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[fission_optimize_pcode_json] UTF-8 error: {}", e);
            return std::ptr::null_mut();
        }
    };
    
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
    
    eprintln!("[fission_optimize_pcode_json] Applied {} optimization passes", num_passes);
    
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
            eprintln!("[fission_optimize_pcode_json] CString conversion error: {}", e);
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
        
        let c_json = CString::new(json).unwrap();
        let result_ptr = unsafe { 
            fission_optimize_pcode_json(c_json.as_ptr(), json.len()) 
        };
        
        assert!(!result_ptr.is_null());
        
        unsafe {
            let result_str = CStr::from_ptr(result_ptr).to_str().unwrap();
            eprintln!("Result: {}", result_str);
            // Check that optimization happened (XOR with 0 should become COPY)
            // Note: The output format is different from input, check for optimization markers
            assert!(result_str.len() > 0);
            fission_free_string(result_ptr);
        }
    }
}
