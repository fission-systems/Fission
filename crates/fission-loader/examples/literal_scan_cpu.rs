//! Manual CPU comparator for the isolated Metal literal-search prototype.
//! This does not change or call an alternative production loader path.
use std::{error::Error, fs, time::Instant};

use rayon::prelude::*;
use regex::bytes::RegexSet;
use serde_json::json;

fn main() -> Result<(), Box<dyn Error>> {
    let total = Instant::now();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err("usage: literal_scan_cpu PATTERNS.json INPUT ITERATIONS [THREADS]".into());
    }
    let iterations: usize = args[2].to_str().ok_or("invalid iterations")?.parse()?;
    let threads: usize = args.get(3).map_or(Ok(1), |x| {
        x.to_str()
            .ok_or("invalid threads")?
            .parse()
            .map_err(|_| "invalid threads")
    })?;
    if iterations == 0 || threads == 0 || threads > 256 {
        return Err("positive iterations and 1..=256 threads required".into());
    }
    let read = Instant::now();
    let patterns: Vec<String> = serde_json::from_slice(&fs::read(&args[0])?)?;
    if patterns.iter().any(String::is_empty) {
        return Err(
            "empty literals must be filtered by the caller, as in fill_string_cache".into(),
        );
    }
    let data = fs::read(&args[1])?;
    let read_ms = read.elapsed().as_secs_f64() * 1000.0;
    let setup = Instant::now();
    // Same literal operator as DieMatcher::fill_string_cache. Resource loading,
    // signature filtering and detector evidence are outside this experiment.
    let escaped: Vec<_> = patterns.iter().map(|s| regex::escape(s)).collect();
    let set = RegexSet::new(&escaped)?;
    let pool = (threads > 1)
        .then(|| rayon::ThreadPoolBuilder::new().num_threads(threads).build())
        .transpose()?;
    let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
    let overlap = patterns
        .iter()
        .map(String::len)
        .max()
        .unwrap_or(1)
        .saturating_sub(1);
    let mut scans = Vec::new();
    for _ in 0..iterations {
        let start = Instant::now();
        let found: Vec<bool> = if let Some(pool) = &pool {
            let chunk_size = data.len().div_ceil(threads).max(1);
            pool.install(|| {
                data.par_chunks(chunk_size)
                    .enumerate()
                    .map(|(i, chunk)| {
                        let start = i * chunk_size;
                        let end = start
                            .saturating_add(chunk.len())
                            .saturating_add(overlap)
                            .min(data.len());
                        let matches = set.matches(&data[start..end]);
                        (0..patterns.len())
                            .map(|p| matches.matched(p))
                            .collect::<Vec<_>>()
                    })
                    .reduce(
                        || vec![false; patterns.len()],
                        |mut a, b| {
                            for (x, y) in a.iter_mut().zip(b) {
                                *x |= y;
                            }
                            a
                        },
                    )
            })
        } else {
            let matches = set.matches(&data);
            (0..patterns.len()).map(|p| matches.matched(p)).collect()
        };
        scans.push(json!({"scan_ms": start.elapsed().as_secs_f64()*1000.0, "found": found}));
    }
    let report = json!({"backend":"rust_regex", "threads":threads, "bytes":data.len(),
        "patterns":patterns.len(), "read_ms":read_ms, "setup_ms":setup_ms,
        "total_ms":total.elapsed().as_secs_f64()*1000.0, "scans":scans});
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
