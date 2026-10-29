//! On-disk layout of a compiled frontend that can be read on several threads.
//!
//! The x86-64 frontend is ~27 MB of bincode. Reading it as one stream cost
//! ~34 ms of every CLI process, almost all of it allocating and filling the
//! decode tables, and a single stream cannot use more than one core. The same
//! bytes split into independent sections can: the file holds the frontend with
//! its `subtables` removed, followed by the `subtables` entries cut into a few
//! contiguous, similarly sized runs. Each section is plain bincode and decodes
//! on its own, and the result is the same value the single stream produced.

use std::collections::BTreeMap;
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::thread::{Builder, ScopedJoinHandle};

use anyhow::{Context, Result};

use super::ir::{CompiledFrontend, CompiledSubtableDefinition};

const MAGIC: &[u8; 4] = b"FSC3";
/// Runs the `subtables` map is cut into. The head section (everything else)
/// is ~10 MB and `subtables` ~17 MB for x86-64, so four runs keep the largest
/// piece close to the head instead of making one run the long pole.
const SUBTABLE_RUNS: usize = 4;
/// Decode tables nest (pattern expressions, constructor templates) and bincode
/// recurses on them; a worker must not get less room than the thread that used
/// to do all of this. The pages are only committed if they are touched.
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

type SubtableRun = Vec<(String, CompiledSubtableDefinition)>;

/// Read a frontend written by [`write`]; `None` if the file is missing,
/// damaged or from another layout, in which case the caller recompiles.
pub(super) fn read(path: &Path) -> Option<CompiledFrontend> {
    let bytes = fs::read(path).ok()?;
    let sections = split_sections(&bytes)?;
    let (head, runs) = sections.split_first()?;

    std::thread::scope(|scope| {
        let pending: Vec<Pending<'_, SubtableRun>> = runs
            .iter()
            .map(|run| {
                match Builder::new()
                    .stack_size(WORKER_STACK_BYTES)
                    .spawn_scoped(scope, move || bincode::deserialize::<SubtableRun>(run).ok())
                {
                    Ok(handle) => Pending::Running(handle),
                    // Out of threads: do this run here rather than discard a cache
                    // that would take seconds to rebuild.
                    Err(_) => Pending::Done(bincode::deserialize::<SubtableRun>(run).ok()),
                }
            })
            .collect();

        let frontend = bincode::deserialize::<CompiledFrontend>(head).ok();
        // Join every worker before deciding, so a failure in one run does not
        // leave another unjoined (an unjoined panicking worker panics the scope).
        let runs: Vec<Option<SubtableRun>> = pending.into_iter().map(Pending::finish).collect();
        let mut frontend = frontend?;
        let mut subtables = BTreeMap::new();
        for run in runs {
            subtables.extend(run?);
        }
        frontend.subtables = subtables;
        Some(frontend)
    })
}

enum Pending<'scope, T> {
    Running(ScopedJoinHandle<'scope, Option<T>>),
    Done(Option<T>),
}

impl<T> Pending<'_, T> {
    fn finish(self) -> Option<T> {
        match self {
            Pending::Running(handle) => handle.join().ok()?,
            Pending::Done(value) => value,
        }
    }
}

/// Write `compiled` in the sectioned layout. `compiled` is left as it was.
pub(super) fn write(path: &Path, compiled: &mut CompiledFrontend) -> Result<()> {
    let subtables = std::mem::take(&mut compiled.subtables);
    let head = bincode::serialize(&*compiled);
    compiled.subtables = subtables;
    let head = head.context("serialize frontend head")?;

    let entries: Vec<(&String, &CompiledSubtableDefinition)> = compiled.subtables.iter().collect();
    let sizes = entries
        .iter()
        .map(|entry| bincode::serialized_size(entry).context("size subtable"))
        .collect::<Result<Vec<u64>>>()?;
    let mut sections = vec![head];
    for range in run_ranges(&sizes, SUBTABLE_RUNS) {
        sections
            .push(bincode::serialize(&entries[range].to_vec()).context("serialize subtable run")?);
    }

    let mut out = Vec::with_capacity(sections.iter().map(|s| s.len() + 8).sum::<usize>() + 8);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&u32::try_from(sections.len())?.to_le_bytes());
    for section in &sections {
        out.extend_from_slice(&(section.len() as u64).to_le_bytes());
    }
    for section in &sections {
        out.extend_from_slice(section);
    }

    // Beside the target and renamed into place, so a process reading while
    // another writes sees the old complete file or the new one.
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, &out).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(())
}

fn split_sections(bytes: &[u8]) -> Option<Vec<&[u8]>> {
    let rest = bytes.strip_prefix(MAGIC.as_slice())?;
    let (count, rest) = rest.split_first_chunk::<4>()?;
    let count = u32::from_le_bytes(*count) as usize;
    let header_len = count.checked_mul(8)?;
    let (lengths, mut body) = rest.split_at_checked(header_len)?;
    let mut sections = Vec::with_capacity(count);
    for length in lengths.chunks_exact(8) {
        let length = usize::try_from(u64::from_le_bytes(length.try_into().ok()?)).ok()?;
        let (section, tail) = body.split_at_checked(length)?;
        sections.push(section);
        body = tail;
    }
    body.is_empty().then_some(sections)
}

/// Cut `sizes` into at most `runs` contiguous ranges of similar total size.
/// Every index lands in exactly one range, in order.
fn run_ranges(sizes: &[u64], runs: usize) -> Vec<Range<usize>> {
    let total: u64 = sizes.iter().sum();
    let runs = runs.max(1);
    let mut ranges = Vec::with_capacity(runs);
    let mut start = 0;
    let mut seen = 0u64;
    for (index, size) in sizes.iter().enumerate() {
        seen += size;
        let boundary = total * (ranges.len() as u64 + 1) / runs as u64;
        if seen >= boundary && ranges.len() + 1 < runs && index + 1 < sizes.len() {
            ranges.push(start..index + 1);
            start = index + 1;
        }
    }
    if start < sizes.len() {
        ranges.push(start..sizes.len());
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers_exactly(ranges: &[Range<usize>], len: usize) -> bool {
        let mut next = 0;
        for range in ranges {
            if range.start != next || range.end <= range.start {
                return false;
            }
            next = range.end;
        }
        next == len
    }

    #[test]
    fn runs_partition_the_entries_in_order() {
        for sizes in [
            vec![],
            vec![5],
            vec![5, 5],
            vec![1, 1, 1, 1, 1, 1, 1],
            vec![100, 1, 1, 1],
            vec![1, 1, 1, 100],
            vec![0, 0, 0],
            vec![7; 40],
        ] {
            for runs in 1..=6 {
                let ranges = run_ranges(&sizes, runs);
                assert!(ranges.len() <= runs, "{sizes:?} into {runs}: {ranges:?}");
                assert!(
                    covers_exactly(&ranges, sizes.len()),
                    "{sizes:?} into {runs}: {ranges:?}"
                );
            }
        }
    }

    #[test]
    fn runs_are_similar_in_size_when_entries_are_uniform() {
        let ranges = run_ranges(&[10; 40], 4);
        assert_eq!(ranges, vec![0..10, 10..20, 20..30, 30..40]);
    }

    #[test]
    fn sections_reject_truncation_and_trailing_bytes() {
        let mut file = Vec::new();
        file.extend_from_slice(MAGIC);
        file.extend_from_slice(&2u32.to_le_bytes());
        file.extend_from_slice(&3u64.to_le_bytes());
        file.extend_from_slice(&2u64.to_le_bytes());
        file.extend_from_slice(b"abcde");
        assert_eq!(
            split_sections(&file),
            Some(vec![b"abc".as_slice(), b"de".as_slice()])
        );
        assert!(split_sections(&file[..file.len() - 1]).is_none());
        let mut longer = file.clone();
        longer.push(0);
        assert!(split_sections(&longer).is_none());
        assert!(split_sections(b"nope").is_none());
        let mut wrong_magic = file;
        wrong_magic[0] = b'X';
        assert!(split_sections(&wrong_magic).is_none());
    }

    #[test]
    fn a_real_frontend_reads_back_equal_to_what_was_written() {
        let mut compiled = crate::compiler::compile_x86_64_frontend().unwrap();
        let dir =
            std::env::temp_dir().join(format!("fission-frontend-cache-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x86.bin");
        write(&path, &mut compiled).unwrap();
        let reread = read(&path).expect("sectioned frontend must read back");
        assert!(
            reread == compiled,
            "reread frontend differs from the original"
        );

        // Damage anywhere is refused rather than half-read.
        let good = fs::read(&path).unwrap();
        fs::write(&path, &good[..good.len() / 2]).unwrap();
        assert!(read(&path).is_none());
        fs::write(&path, b"").unwrap();
        assert!(read(&path).is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
