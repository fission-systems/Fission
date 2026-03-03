//! Windows API constants
//!
//! Well-known constants for Windows debugging, process management, and exception handling.
//! Only compiled on Windows targets.
//!
//! References:
//! - Windows SDK headers (winnt.h, winbase.h)
//! - MSDN Exception Codes documentation

#![cfg(windows)]

// =============================================================================
// Exception Codes
// =============================================================================

/// Exception: Breakpoint (INT 3)
pub const EXCEPTION_BREAKPOINT: u32 = 0x80000003;

/// Exception: Single step (trap flag)
pub const EXCEPTION_SINGLE_STEP: u32 = 0x80000004;

/// Exception: Access violation (segfault)
pub const EXCEPTION_ACCESS_VIOLATION: u32 = 0xC0000005;

/// Exception: Array bounds exceeded
pub const EXCEPTION_ARRAY_BOUNDS_EXCEEDED: u32 = 0xC000008C;

/// Exception: Datatype misalignment
pub const EXCEPTION_DATATYPE_MISALIGNMENT: u32 = 0x80000002;

/// Exception: Floating point denormal operand
pub const EXCEPTION_FLT_DENORMAL_OPERAND: u32 = 0xC000008D;

/// Exception: Floating point divide by zero
pub const EXCEPTION_FLT_DIVIDE_BY_ZERO: u32 = 0xC000008E;

/// Exception: Integer divide by zero
pub const EXCEPTION_INT_DIVIDE_BY_ZERO: u32 = 0xC0000094;

/// Exception: Integer overflow
pub const EXCEPTION_INT_OVERFLOW: u32 = 0xC0000095;

/// Exception: Illegal instruction
pub const EXCEPTION_ILLEGAL_INSTRUCTION: u32 = 0xC000001D;

/// Exception: Privileged instruction
pub const EXCEPTION_PRIV_INSTRUCTION: u32 = 0xC0000096;

/// Exception: Invalid handle
pub const EXCEPTION_INVALID_HANDLE: u32 = 0xC0000008;

/// Exception: Stack overflow
pub const EXCEPTION_STACK_OVERFLOW: u32 = 0xC00000FD;

/// Exception: Invalid disposition
pub const EXCEPTION_INVALID_DISPOSITION: u32 = 0xC0000026;

/// Exception: Guard page violation
pub const EXCEPTION_GUARD_PAGE: u32 = 0x80000001;

/// Exception: In-page error
pub const EXCEPTION_IN_PAGE_ERROR: u32 = 0xC0000006;

/// Exception: Non-continuable exception
pub const EXCEPTION_NONCONTINUABLE_EXCEPTION: u32 = 0xC0000025;

// =============================================================================
// Process Access Rights
// =============================================================================

/// Process access: All possible access rights
pub const PROCESS_ALL_ACCESS: u32 = 0x1F0FFF;

/// Process access: Terminate process
pub const PROCESS_TERMINATE: u32 = 0x0001;

/// Process access: Create thread
pub const PROCESS_CREATE_THREAD: u32 = 0x0002;

/// Process access: Set session ID
pub const PROCESS_SET_SESSIONID: u32 = 0x0004;

/// Process access: VM operation
pub const PROCESS_VM_OPERATION: u32 = 0x0008;

/// Process access: VM read
pub const PROCESS_VM_READ: u32 = 0x0010;

/// Process access: VM write
pub const PROCESS_VM_WRITE: u32 = 0x0020;

/// Process access: Duplicate handle
pub const PROCESS_DUP_HANDLE: u32 = 0x0040;

/// Process access: Create process
pub const PROCESS_CREATE_PROCESS: u32 = 0x0080;

/// Process access: Set quota
pub const PROCESS_SET_QUOTA: u32 = 0x0100;

/// Process access: Set information
pub const PROCESS_SET_INFORMATION: u32 = 0x0200;

/// Process access: Query information
pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;

/// Process access: Suspend/resume
pub const PROCESS_SUSPEND_RESUME: u32 = 0x0800;

/// Process access: Query limited information
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

// =============================================================================
// Thread Access Rights
// =============================================================================

/// Thread access: All possible access rights
pub const THREAD_ALL_ACCESS: u32 = 0x1FFFFF;

/// Thread access: Terminate thread
pub const THREAD_TERMINATE: u32 = 0x0001;

/// Thread access: Suspend/resume thread
pub const THREAD_SUSPEND_RESUME: u32 = 0x0002;

/// Thread access: Get thread context
pub const THREAD_GET_CONTEXT: u32 = 0x0008;

/// Thread access: Set thread context
pub const THREAD_SET_CONTEXT: u32 = 0x0010;

/// Thread access: Query information
pub const THREAD_QUERY_INFORMATION: u32 = 0x0040;

/// Thread access: Set information
pub const THREAD_SET_INFORMATION: u32 = 0x0020;

/// Thread access: Set thread token
pub const THREAD_SET_THREAD_TOKEN: u32 = 0x0080;

/// Thread access: Impersonate
pub const THREAD_IMPERSONATE: u32 = 0x0100;

/// Thread access: Direct impersonation
pub const THREAD_DIRECT_IMPERSONATION: u32 = 0x0200;

// =============================================================================
// Memory Protection Constants
// =============================================================================

/// Memory protection: Page execute
pub const PAGE_EXECUTE: u32 = 0x10;

/// Memory protection: Page execute read
pub const PAGE_EXECUTE_READ: u32 = 0x20;

/// Memory protection: Page execute read/write
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;

/// Memory protection: Page execute write/copy
pub const PAGE_EXECUTE_WRITECOPY: u32 = 0x80;

/// Memory protection: Page no access
pub const PAGE_NOACCESS: u32 = 0x01;

/// Memory protection: Page read only
pub const PAGE_READONLY: u32 = 0x02;

/// Memory protection: Page read/write
pub const PAGE_READWRITE: u32 = 0x04;

/// Memory protection: Page write/copy
pub const PAGE_WRITECOPY: u32 = 0x08;

/// Memory protection: Guard page
pub const PAGE_GUARD: u32 = 0x100;

/// Memory protection: Non-cacheable
pub const PAGE_NOCACHE: u32 = 0x200;

/// Memory protection: Write combine
pub const PAGE_WRITECOMBINE: u32 = 0x400;

// =============================================================================
// Memory Allocation Types
// =============================================================================

/// Memory allocation: Commit pages
pub const MEM_COMMIT: u32 = 0x1000;

/// Memory allocation: Reserve address space
pub const MEM_RESERVE: u32 = 0x2000;

/// Memory allocation: Decommit pages
pub const MEM_DECOMMIT: u32 = 0x4000;

/// Memory allocation: Release pages
pub const MEM_RELEASE: u32 = 0x8000;

/// Memory allocation: Free pages
pub const MEM_FREE: u32 = 0x10000;

/// Memory allocation: Private memory
pub const MEM_PRIVATE: u32 = 0x20000;

/// Memory allocation: Mapped memory
pub const MEM_MAPPED: u32 = 0x40000;

/// Memory allocation: Reset pages
pub const MEM_RESET: u32 = 0x80000;

/// Memory allocation: Top down allocation
pub const MEM_TOP_DOWN: u32 = 0x100000;

// =============================================================================
// Debug Event Codes
// =============================================================================

/// Debug event: Exception
pub const EXCEPTION_DEBUG_EVENT: u32 = 1;

/// Debug event: Create thread
pub const CREATE_THREAD_DEBUG_EVENT: u32 = 2;

/// Debug event: Create process
pub const CREATE_PROCESS_DEBUG_EVENT: u32 = 3;

/// Debug event: Exit thread
pub const EXIT_THREAD_DEBUG_EVENT: u32 = 4;

/// Debug event: Exit process
pub const EXIT_PROCESS_DEBUG_EVENT: u32 = 5;

/// Debug event: Load DLL
pub const LOAD_DLL_DEBUG_EVENT: u32 = 6;

/// Debug event: Unload DLL
pub const UNLOAD_DLL_DEBUG_EVENT: u32 = 7;

/// Debug event: Output debug string
pub const OUTPUT_DEBUG_STRING_EVENT: u32 = 8;

/// Debug event: RIP (system debugging error)
pub const RIP_EVENT: u32 = 9;

// =============================================================================
// Common Constants
// =============================================================================

/// Invalid handle value
pub const INVALID_HANDLE_VALUE: isize = -1;

/// Boolean TRUE
pub const TRUE: i32 = 1;

/// Boolean FALSE
pub const FALSE: i32 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exception_codes() {
        assert_eq!(EXCEPTION_BREAKPOINT, 0x80000003);
        assert_eq!(EXCEPTION_ACCESS_VIOLATION, 0xC0000005);
    }

    #[test]
    fn test_process_access() {
        assert_eq!(PROCESS_ALL_ACCESS, 0x1F0FFF);
        assert_eq!(PROCESS_VM_READ, 0x0010);
    }
}
