// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "instruction" => match_node_instruction_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

