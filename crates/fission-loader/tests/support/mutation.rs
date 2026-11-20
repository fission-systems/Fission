//! Deterministic header-mutation sweep shared by the malformed-input tests.
//!
//! `cargo fuzz` finds one crash per nightly run. This does the same job in a
//! test: take a real fixture, overwrite one header field at a time with values
//! chosen to overflow arithmetic or point outside the file, load the result,
//! and require three things of the loader:
//!
//! * it does not **panic** (collected by a panic hook, reported by location);
//! * it does not **hang** (a watchdog fails the test when one load makes no
//!   progress for [`HANG_AFTER`]);
//! * it does not ask for an **allocation** far beyond the size of its input
//!   (a counting global allocator; a `vec![0; n]` over an attacker-chosen span
//!   succeeds lazily and would otherwise go unnoticed).
//!
//! It runs under the test profile, where overflow checks are on, as the
//! fuzzer's build is.

#![allow(dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::panic;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One load that makes no progress for this long is a hang. A load of these
/// fixtures takes milliseconds, so this is generous even on a busy machine.
const HANG_AFTER: Duration = Duration::from_secs(40);

/// No fixture here is larger than a few tens of kilobytes; a single request
/// this large is the loader trusting a size it read from the file.
const ALLOCATION_LIMIT: usize = 256 << 20;

static LARGEST_REQUEST: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

// SAFETY: every method forwards to `System` unchanged; the only addition is a
// relaxed atomic maximum, which cannot allocate or unwind.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LARGEST_REQUEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        LARGEST_REQUEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LARGEST_REQUEST.fetch_max(new_size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

static PANICS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Overwrite `width` bytes at `at` with the low bytes of `value`.
pub fn put(d: &mut [u8], at: usize, width: usize, value: u64, big_endian: bool) {
    if let Some(slot) = at.checked_add(width).and_then(|end| d.get_mut(at..end)) {
        if big_endian {
            slot.copy_from_slice(&value.to_be_bytes()[8 - width..]);
        } else {
            slot.copy_from_slice(&value.to_le_bytes()[..width]);
        }
    }
}

pub fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

pub fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

pub fn u64_at(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// The hostile values for a file of `len` bytes: arithmetic overflow at 64 and
/// 32 bits, zero (divisors and loop strides), and either side of the end of
/// the file.
pub fn hostile_values(len: usize) -> Vec<u64> {
    let len = len as u64;
    vec![
        u64::MAX,
        u64::MAX - 7,
        1 << 63,
        0,
        0x8000_0000,
        0xffff_fff8,
        len.saturating_sub(1),
        len,
        len + 1,
    ]
}

/// What a sweep found. Empty means the loader refused or accepted every
/// mutated input without misbehaving.
#[derive(Default)]
pub struct Findings {
    /// Panic location -> (message, the mutation that first reached it).
    pub panics: BTreeMap<String, (String, String)>,
    /// The mutation a load never returned from.
    pub hang: Option<String>,
    /// Mutation -> largest single allocation request, in bytes.
    pub allocations: Vec<(String, usize)>,
}

impl Findings {
    pub fn is_empty(&self) -> bool {
        self.panics.is_empty() && self.hang.is_none() && self.allocations.is_empty()
    }

    pub fn describe(&self) -> String {
        let mut out = String::new();
        if !self.panics.is_empty() {
            out.push_str(&format!("{} distinct panic site(s):\n", self.panics.len()));
            for (at, (msg, mutation)) in &self.panics {
                out.push_str(&format!("  {at}: {msg}  [{mutation}]\n"));
            }
        }
        if let Some(mutation) = &self.hang {
            out.push_str(&format!("hang (no progress): [{mutation}]\n"));
        }
        for (mutation, size) in &self.allocations {
            out.push_str(&format!("allocation of {size} bytes: [{mutation}]\n"));
        }
        out
    }
}

/// Load every single-field mutation of `base` and report what went wrong.
///
/// `sites` are `(file offset, width)` pairs; each is overwritten with each of
/// [`hostile_values`] in turn. `load` is the loader entry point under test.
pub fn sweep<F>(base: &[u8], sites: &[(usize, usize)], big_endian: bool, load: F) -> Findings
where
    F: Fn(Vec<u8>) + Send + Sync + panic::RefUnwindSafe + 'static,
{
    let values = hostile_values(base.len());
    sweep_inputs(
        sites
            .iter()
            .flat_map(|&(at, width)| {
                let base = base.to_vec();
                // Narrow fields see several of the values as the same bytes.
                let mask = if width >= 8 {
                    u64::MAX
                } else {
                    (1u64 << (width * 8)) - 1
                };
                let mut narrowed: Vec<u64> = values.iter().map(|v| v & mask).collect();
                narrowed.sort_unstable();
                narrowed.dedup();
                narrowed.into_iter().map(move |value| {
                    let mut data = base.clone();
                    put(&mut data, at, width, value, big_endian);
                    (format!("offset {at:#x} width {width} = {value:#x}"), data)
                })
            })
            .collect(),
        load,
    )
}

/// Load each `(label, bytes)` input and report what went wrong.
pub fn sweep_inputs<F>(inputs: Vec<(String, Vec<u8>)>, load: F) -> Findings
where
    F: Fn(Vec<u8>) + Send + Sync + panic::RefUnwindSafe + 'static,
{
    panic::set_hook(Box::new(|info| {
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                info.payload()
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
            })
            .unwrap_or_default();
        PANICS.lock().unwrap().push((at, msg));
    }));

    let total = inputs.len();
    let labels: Arc<Vec<String>> = Arc::new(inputs.iter().map(|(l, _)| l.clone()).collect());
    let done = Arc::new(AtomicUsize::new(0));
    let results: Arc<Mutex<Findings>> = Arc::new(Mutex::new(Findings::default()));

    let worker = {
        let done = Arc::clone(&done);
        let results = Arc::clone(&results);
        std::thread::spawn(move || {
            for (label, data) in inputs {
                LARGEST_REQUEST.store(0, Ordering::Relaxed);
                let _ = panic::catch_unwind(|| load(data));
                let largest = LARGEST_REQUEST.load(Ordering::Relaxed);
                let mut findings = results.lock().unwrap();
                if largest > ALLOCATION_LIMIT {
                    findings.allocations.push((label.clone(), largest));
                }
                for (at, msg) in PANICS.lock().unwrap().drain(..) {
                    findings
                        .panics
                        .entry(at)
                        .or_insert_with(|| (msg, label.clone()));
                }
                drop(findings);
                done.fetch_add(1, Ordering::Relaxed);
            }
        })
    };

    // Watchdog: a load that never returns must fail the test, not time it out
    // with no indication of which input did it.
    let mut last = (0usize, Instant::now());
    let mut hang = None;
    loop {
        let now = done.load(Ordering::Relaxed);
        if now >= total {
            break;
        }
        if now != last.0 {
            last = (now, Instant::now());
        } else if last.1.elapsed() > HANG_AFTER {
            hang = Some(labels[now].clone());
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if hang.is_none() {
        let _ = worker.join();
    }
    let _ = panic::take_hook();

    let mut findings = std::mem::take(&mut *results.lock().unwrap());
    findings.hang = hang;
    findings
}

/// Fail with the full list unless the sweep found nothing.
pub fn assert_clean(what: &str, findings: &Findings) {
    assert!(findings.is_empty(), "{what}:\n{}", findings.describe());
}
