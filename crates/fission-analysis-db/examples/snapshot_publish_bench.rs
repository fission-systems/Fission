//! Machine-local micro-benchmark for snapshot publication and queries.
//!
//! ```text
//! cargo run --release -p fission-analysis-db --example snapshot_publish_bench -- <functions> <blocks>
//! ```
//!
//! Builds a synthetic `LoadedBinary` with `<functions>` functions spread over
//! `<blocks>` executable sections (plus `functions / 2` data symbols and
//! relocations), then reports, for the publication boundary
//! (`ProgramSnapshotBuilder::publish`), the explicit audit
//! (`ProgramSnapshot::integrity_report`), a one-row typed delta
//! (`ProgramSnapshot::apply_deltas`) and point/interval queries: median
//! thread CPU time (wall time alongside), bytes and calls allocated, and peak live heap above the phase start.
//! Allocation figures come from a counting global allocator, so they are exact
//! for this process. Numbers are machine-local and only comparable between
//! runs on the same machine and build profile.

use fission_analysis_db::{FunctionId, ProgramSnapshotBuilder, ProgramSnapshotDelta};
use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, RelocationEntry};
use fission_loader::{FunctionInfo, LoadedBinary, SectionInfo};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct Counting;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            record_alloc(new_size);
        }
        new
    }
}

fn record_alloc(size: usize) {
    ALLOCATED.fetch_add(size, Ordering::Relaxed);
    CALLS.fetch_add(1, Ordering::Relaxed);
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

unsafe extern "C" {
    fn clock_gettime(clock: i32, tp: *mut Timespec) -> i32;
}

#[cfg(target_os = "macos")]
const CLOCK_THREAD_CPUTIME_ID: i32 = 16;
#[cfg(not(target_os = "macos"))]
const CLOCK_THREAD_CPUTIME_ID: i32 = 3;

/// CPU time consumed by this thread. The wall clock is reported too, but on a
/// loaded machine only the CPU time is comparable between runs.
fn thread_cpu_time() -> Duration {
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable timespec for the duration of the call.
    let rc = unsafe { clock_gettime(CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    assert_eq!(rc, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

#[derive(Clone, Copy)]
struct Sample {
    cpu: Duration,
    time: Duration,
    bytes: usize,
    calls: usize,
    peak_above_start: usize,
}

fn measure<T>(f: impl FnOnce() -> T) -> (T, Sample) {
    let bytes0 = ALLOCATED.load(Ordering::Relaxed);
    let calls0 = CALLS.load(Ordering::Relaxed);
    let live0 = LIVE.load(Ordering::Relaxed);
    PEAK.store(live0, Ordering::Relaxed);
    let cpu0 = thread_cpu_time();
    let start = Instant::now();
    let value = f();
    let time = start.elapsed();
    let cpu = thread_cpu_time() - cpu0;
    let sample = Sample {
        cpu,
        time,
        bytes: ALLOCATED.load(Ordering::Relaxed) - bytes0,
        calls: CALLS.load(Ordering::Relaxed) - calls0,
        peak_above_start: PEAK.load(Ordering::Relaxed) - live0,
    };
    (value, sample)
}

fn median(mut samples: Vec<Sample>) -> Sample {
    samples.sort_by_key(|sample| sample.cpu);
    samples[samples.len() / 2]
}

fn report(label: &str, sample: Sample) {
    println!(
        "{label:<20} cpu {:>9.3} ms (wall {:>9.3})  alloc {:>10.2} MiB in {:>9} calls  peak +{:>8.2} MiB",
        sample.cpu.as_secs_f64() * 1e3,
        sample.time.as_secs_f64() * 1e3,
        sample.bytes as f64 / (1024.0 * 1024.0),
        sample.calls,
        sample.peak_above_start as f64 / (1024.0 * 1024.0),
    );
}

const BASE: u64 = 0x40_0000;
const STRIDE: u64 = 0x40;

fn synthetic_binary(functions: usize, blocks: usize) -> LoadedBinary {
    let per_block = functions.div_ceil(blocks).max(1) as u64;
    let block_span = per_block * STRIDE;
    let sections = (0..blocks).map(|index| SectionInfo {
        name: format!(".text{index}"),
        virtual_address: BASE + index as u64 * block_span,
        virtual_size: block_span,
        file_offset: 0x400 + index as u64 * block_span,
        file_size: block_span,
        is_executable: true,
        is_readable: true,
        is_writable: false,
    });
    let data_base = BASE + blocks as u64 * block_span;
    let data_span = (functions as u64 / 2 + 1) * 8;
    let data_section = SectionInfo {
        name: ".data".into(),
        virtual_address: data_base,
        virtual_size: data_span,
        file_offset: 0x400 + blocks as u64 * block_span,
        file_size: data_span,
        is_executable: false,
        is_readable: true,
        is_writable: true,
    };
    let function_rows = (0..functions).map(|index| {
        let entry = BASE + index as u64 * STRIDE;
        // Mix ordinary functions with zero-sized aliases and with ranges that
        // nest a neighbour, so the interval index sees overlaps.
        let size = match index % 17 {
            0 => 0,
            5 if index + 1 < functions && (index + 1) as u64 % per_block != 0 => 2 * STRIDE,
            _ => STRIDE - 0x10,
        };
        FunctionInfo {
            name: format!("fn_{index:08}"),
            address: entry,
            size,
            origin: Some("symbol_table".into()),
            ..FunctionInfo::default()
        }
    });
    let mut globals = HashMap::new();
    let mut global_sizes = HashMap::new();
    let mut relocations = Vec::new();
    for index in 0..functions / 2 {
        let address = data_base + index as u64 * 8;
        let name = format!("g_{index:08}");
        globals.insert(address, name.clone());
        global_sizes.insert(address, 8);
        relocations.push(RelocationEntry {
            address,
            r_type: 1,
            size: 8,
            addend: 0,
            symbol_name: Some(name),
            symbol_address: Some(address),
        });
    }
    LoadedBinaryBuilder::new("bench.bin".into(), DataBuffer::Heap(vec![0; 64]))
        .format("ELF")
        .image_base(BASE)
        .entry_point(BASE)
        .is_64bit(true)
        .add_sections(sections)
        .add_section(data_section)
        .add_functions(function_rows)
        .add_global_symbols(globals)
        .add_global_symbol_sizes(global_sizes)
        .add_relocations(relocations)
        .build()
        .expect("synthetic binary builds")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let functions: usize = args
        .next()
        .map_or(100_000, |v| v.parse().expect("functions"));
    let blocks: usize = args.next().map_or(16, |v| v.parse().expect("blocks"));
    let rounds: usize = args.next().map_or(7, |v| v.parse().expect("rounds"));
    let queries: usize = 1_000_000;

    let binary = synthetic_binary(functions, blocks);
    let staged = ProgramSnapshotBuilder::from_loaded_binary(&binary);
    println!(
        "functions={functions} blocks={blocks} (+1 data) data_symbols={} relocations={} rounds={rounds}",
        functions / 2,
        functions / 2
    );

    let mut publish_samples = Vec::with_capacity(rounds);
    let mut snapshot = None;
    for _ in 0..rounds {
        let builder = staged.clone();
        let (published, sample) = measure(|| builder.build());
        publish_samples.push(sample);
        snapshot = Some(published.expect("synthetic snapshot is valid"));
    }
    let snapshot = snapshot.expect("at least one round");
    report("publish", median(publish_samples));

    let mut audit_samples = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let (report, sample) = measure(|| snapshot.integrity_report());
        assert!(report.is_valid());
        audit_samples.push(sample);
    }
    report("integrity_report", median(audit_samples));

    let mut delta_samples = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let target = snapshot.functions()[1].clone();
        let (updated, sample) = measure(|| {
            snapshot.apply_deltas([ProgramSnapshotDelta::UpdateFunctionRange {
                id: FunctionId(1),
                entry: target.entry,
                size: target.size / 2,
            }])
        });
        assert!(updated.is_ok());
        delta_samples.push(sample);
    }
    report("apply_deltas(1 op)", median(delta_samples));

    let span = functions as u64 * STRIDE + 0x1000;
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let probes: Vec<u64> = (0..queries)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            BASE - 0x800 + state % span
        })
        .collect();
    let query = |label: &str, hit: &dyn Fn(u64) -> bool| {
        let mut samples = Vec::with_capacity(rounds);
        let mut hits = 0;
        for _ in 0..rounds {
            let (count, sample) =
                measure(|| probes.iter().filter(|&&address| hit(address)).count());
            hits = count;
            samples.push(sample);
        }
        let sample = median(samples);
        println!(
            "{label:<20} cpu {:>8.1} ns/query  ({hits} of {queries} hit)",
            sample.cpu.as_nanos() as f64 / queries as f64,
        );
    };
    query("function_containing", &|address| {
        black_box(snapshot.function_containing(address)).is_some()
    });
    query("function_at", &|address| {
        black_box(snapshot.function_at(address)).is_some()
    });
    query("memory_block_containing", &|address| {
        black_box(snapshot.memory_block_containing(address)).is_some()
    });
}
