# Reuse immutable FPK hash-index chunks (mechanical performance change)

Baseline main 6d6d8872a67a3beae2b1eb811d6240a5b6ac80be; immutable CLI SHA cad83688200d2aeca0f5b5b559d4df5ebf0b1c2a3cf218e7c83aa14b3fce20a4.

## Observed owner evidence
Optimized cfg(test) FactStore phases on a frozen real large input: matching median 2.1503 wall seconds / 28.12 cumulative CPU seconds. Separate product CLI paired controls will determine actual latency effect. FpkReader::records_by_key decompresses the same zstd hash-index chunk on every present or absent lookup; existing payload/candidate caches do not retain this index. No phase measurement attributes all matching time to decompression.

## Invariant and implementation
Each reader maps one immutable archive. Its directory and chunk count are fixed. Retain successful length-validated decoded index chunks in reader-owned OnceLock slots indexed by directory entry. Return borrowed bytes, without a shared lock or Arc increment on hits. Retained memory is bounded by archive chunks actually touched, independent of lookup count/miss keys. Failed decompress/length checks stay uncached and retry. Concurrent cold readers may decode twice, publishing only one immutable success. Keep all lookup ordering, duplicate-key boundary scans, payload checks, error strings, formats, candidate/matching policy and output unchanged. No production dependency or oracle data.

## Acceptance
Targeted index hit/miss/concurrency/duplicate-boundary/error tests; related nextest suite, checks/clippy/release/fmt/owner gate; full 1024 decode/error and 512 FID hash/resolver matrices. At least 2% native large median wall improvement in three alternating pairs and independent five inverted pairs (CPU no worse than 2%); small-input controls, fresh 250 complete output parity excluding exactly seven existing duration fields, fixed Kuna 1.808 comparisons. No decompiler quality or ranking claim. Rejected candidates are restored and not committed.
