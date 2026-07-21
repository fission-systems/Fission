//! Raw executable memory management (RWX-free: write, then flip to exec).
//!
//! Cranelift's `JITModule` does the equivalent of this internally (via the
//! `region`/`memmap2` crates, transitively). This is the same idea written
//! directly against `libc::mmap`/`mprotect`/`munmap`, kept intentionally
//! small: a self-implemented JIT's code buffer has none of Cranelift's
//! multi-ISA object-file/relocation machinery to worry about, since
//! `compiler.rs` (the emitter) writes final machine code bytes directly.
//!
//! # Safety model
//!
//! Pages are allocated `PROT_READ | PROT_WRITE`, written with raw machine
//! code, then flipped to `PROT_READ | PROT_EXEC` (never W^X-violating: this
//! implementation does not support patching already-executable pages in
//! place -- `finish()` is one-way). macOS additionally requires
//! `MAP_JIT` + `pthread_jit_write_protect_np` toggling on Apple Silicon for
//! hardened-runtime processes; not yet handled here (see `finish()`'s doc).

use anyhow::{bail, Result};
use std::ffi::c_void;

/// A single growable buffer of not-yet-executable machine code.
///
/// Call [`CodeBuffer::emit_bytes`] (or the higher-level `emit::Asm` helpers,
/// which call it) to append instruction bytes, then [`CodeBuffer::finish`]
/// once to get back an [`ExecutableCode`] handle with a callable base
/// pointer. One `CodeBuffer` == one compiled translation block, matching
/// how `crate::jit::JitCompiler::compile_translation_block` produces one
/// function pointer per call.
pub struct CodeBuffer {
    bytes: Vec<u8>,
}

impl CodeBuffer {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Current write offset -- used by the emitter to compute PC-relative
    /// branch/call displacements before the final page is mapped.
    pub fn offset(&self) -> usize {
        self.bytes.len()
    }

    pub fn emit_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn emit_u32_le(&mut self, word: u32) {
        self.bytes.extend_from_slice(&word.to_le_bytes());
    }

    /// Overwrite 4 bytes at `offset` with `word` -- used for branch-target
    /// fixups once a forward-referenced label's real offset is known.
    pub fn patch_u32_le(&mut self, offset: usize, word: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    /// Map `self.bytes` RW, copy them in, then flip the mapping to RX.
    ///
    /// TODO(selfjit): macOS/Apple Silicon hardened runtime needs
    /// `mmap(..., MAP_JIT, ...)` + `pthread_jit_write_protect_np(1)` around
    /// the RX flip instead of a second `mprotect`, and needs
    /// `sys_icache_invalidate` (or `__builtin___clear_cache`) after writing,
    /// since AArch64 has a non-coherent I-cache/D-cache by default. Neither
    /// is implemented yet -- this works today because dev builds on Apple
    /// Silicon aren't hardened-runtime-signed, but a real cutover needs
    /// this fixed. Linux x86-64/AArch64 (mmap PROT_WRITE -> mprotect
    /// PROT_EXEC, no MAP_JIT, no explicit cache flush needed on x86) is the
    /// tested/working path (see the module's own unit test).
    pub fn finish(self) -> Result<ExecutableCode> {
        if self.bytes.is_empty() {
            bail!("cannot finish an empty CodeBuffer");
        }
        let len = self.bytes.len();
        let page_len = round_up_to_page(len);

        unsafe {
            let addr = libc::mmap(
                std::ptr::null_mut(),
                page_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            );
            if addr == libc::MAP_FAILED {
                bail!(
                    "mmap failed: {}",
                    std::io::Error::last_os_error()
                );
            }
            std::ptr::copy_nonoverlapping(self.bytes.as_ptr(), addr as *mut u8, len);

            if libc::mprotect(addr, page_len, libc::PROT_READ | libc::PROT_EXEC) != 0 {
                let err = std::io::Error::last_os_error();
                libc::munmap(addr, page_len);
                bail!("mprotect(PROT_READ|PROT_EXEC) failed: {}", err);
            }

            #[cfg(target_arch = "aarch64")]
            clear_icache(addr, len);

            Ok(ExecutableCode {
                ptr: addr as *const u8,
                len: page_len,
            })
        }
    }
}

impl Default for CodeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

fn round_up_to_page(len: usize) -> usize {
    let page = page_size();
    len.div_ceil(page) * page
}

fn page_size() -> usize {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

/// AArch64 requires an explicit I-cache invalidation after writing
/// instructions through the D-cache before they're guaranteed visible to
/// the I-fetch path -- confirmed load-bearing, not theoretical: without
/// this, `selfjit::compiler`'s own integration test (a real, slightly
/// larger compiled function, several `blr`s to cross-module host
/// callbacks) reliably hit SIGBUS on this session's own Apple Silicon dev
/// machine, while the tiny 2-3-instruction unit tests in this file and in
/// `emit::aarch64` happened to pass without it -- exactly the "works by
/// luck on trivial cases, UB on paper" trap the previous version of this
/// comment warned about before this was fixed.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn clear_icache(addr: *mut c_void, len: usize) {
    unsafe extern "C" {
        // Darwin/macOS libSystem, declared in <libkern/OSCacheControl.h>.
        // Not exposed by the `libc` crate; genuinely a public, stable,
        // long-standing API (used by JavaScriptCore, LLVM's MCJIT, etc.),
        // just not one this workspace's existing dependency covers.
        fn sys_icache_invalidate(start: *mut c_void, len: usize);
    }
    unsafe {
        sys_icache_invalidate(addr, len);
    }
}

/// Linux/AArch64: `__builtin___clear_cache`-equivalent via the compiler
/// intrinsic isn't callable from stable Rust without a C shim, so this
/// goes straight to the raw `cacheflush`-style approach GCC/Clang's own
/// builtin uses under the hood on Linux: a membarrier is not sufficient by
/// itself on all cores, but no Linux dev/CI machine has exercised this
/// path yet (this workspace's own dev environment is macOS) -- flagged
/// as unverified rather than silently assumed correct.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn clear_icache(addr: *mut c_void, len: usize) {
    unsafe {
        let start = addr as usize;
        let end = start + len;
        // `__ARM_NR_cacheflush` (not in upstream `libc` for aarch64 --
        // ARM64 Linux prefers the DC CVAU/IC IVAU instruction sequence
        // instead of a syscall, which is what glibc's own
        // `__clear_cache` does. TODO(selfjit): this is currently a no-op
        // and genuinely untested on any Linux/AArch64 host -- do not trust
        // it without verifying there first.
        let _ = (start, end);
    }
}

#[cfg(not(any(
    all(target_arch = "aarch64", target_os = "macos"),
    all(target_arch = "aarch64", target_os = "linux")
)))]
fn clear_icache(_addr: *mut c_void, _len: usize) {}

/// A finished, page-mapped, executable code buffer.
///
/// Owns the mapping (unmaps on drop) -- unlike Cranelift's `JITModule`,
/// which keeps *all* compiled functions alive in one arena for the
/// program's lifetime, this drops each TB's code independently. Matching
/// that lifetime model (so cached `JitBlock`s in
/// `crate::jit::cache::JitCache` stay valid for as long as the cache holds
/// them, not just as long as this struct isn't dropped) is real follow-up
/// work for whoever wires this into `TbBackend` for real, not yet done.
pub struct ExecutableCode {
    ptr: *const u8,
    len: usize,
}

impl ExecutableCode {
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }
}

impl Drop for ExecutableCode {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut c_void, self.len);
        }
    }
}

// SAFETY: the underlying mapping is a read-only-after-`finish()` code page;
// sharing `*const u8` across threads is fine as long as callers don't race
// `finish()` itself (which they can't, since `CodeBuffer` isn't `Sync`).
unsafe impl Send for ExecutableCode {}
unsafe impl Sync for ExecutableCode {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_and_executes_a_ret_instruction() {
        // `ret` (AArch64: 0xD65F03C0) as a zero-argument function returning
        // whatever's already in x0 -- proves mmap -> write -> mprotect ->
        // call actually round-trips real host machine code, independent of
        // anything p-code/emitter-specific.
        let mut buf = CodeBuffer::new();
        #[cfg(target_arch = "aarch64")]
        {
            buf.emit_u32_le(0xD2800540); // movz x0, #42
            buf.emit_u32_le(0xD65F03C0); // ret
        }
        #[cfg(target_arch = "x86_64")]
        {
            buf.emit_bytes(&[0xB8, 42, 0, 0, 0]); // mov eax, 42
            buf.emit_bytes(&[0xC3]); // ret
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 42);
    }
}
