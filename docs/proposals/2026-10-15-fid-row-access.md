# Indexed access to immutable FPK payload rows

Mechanical performance follow-up to PR #205; no quality, FID-admission, naming, ordering, cache-key, runtime-dependency, or worker-policy change.

## Measured anchor and owner

Pinned qualification revision 07dd95dbe6214b8a78fc1e4a55961ab515986489. PR #205's measured release candidate (SHA256 4c0485b7da200135afbc2e0bfbdbb9de8a7058549a1f9a1d6af3e91c9c6dc320) is the new control: selected small medians 2.224/1.171/1.380 s and large control 103.820 s. A symbolized candidate diagnostic at 65 s shows FID matching in FpkReader::records_by_key line 351, text.lines().nth(row), with 1337 samples at that line in a representative worker stack. The 35 s diagnostic still shows native decode, so this is a distinct later phase. Native samples are diagnostics, not timing or output-parity runs; they are retained under benchmark/artifacts/performance/2026-10-14-kuna/startup-large-profile-*.txt.

The FPK reader already caches immutable decompressed payload blocks. Each hash locator names a row in such a block. Starting String::lines from the beginning for every locator repeats the same prefix scan. The previous rejected index-chunk experiment avoided decompression; it did not avoid this row scan. Keep that experiment excluded.

## Invariant and implementation boundary

Extend the existing per-reader payload cache entry with a lazily initialized line-range index. Construct ranges from newline-delimited chunks, removing exactly the same LF/optional CRLF terminators as str::lines. Empty input, empty interior lines, trailing newline, bare CR, Unicode and unterminated final rows must match str::lines exactly. Read one row by a bounds-checked string slice. Keep public block_at/block_for returning the original shared Arc<String>; direct block consumers need not build the row index.

Keep index-directory searches, decompression/validation, locator checks, matched record order, duplicate handling, full/specific hashes, FID thresholds and error messages unchanged. The row index is built synchronously from the already validated immutable string and does not call Rayon, I/O or the reader cache. Retain it only with that existing cached payload, with two usize offsets per line plus vector capacity overhead; do not add a parallel program-fact map, hidden runtime dependency or global cache. Preserve cold-cache publication behavior and do not cache failures.

## Verification and acceptance

- Compare indexed rows with str::lines across empty/LF/CRLF/bare-CR/Unicode/repeated rows and out-of-range indices.
- Assert public Arc ownership/warm identity remains unchanged; ordinary block reads leave line indexes uninitialized; repeated/concurrent hash reads return identical ordered records.
- Retain existing cross-chunk duplicate-key, payload format, malformed input and block-boundary tests. Run focused signatures nextest then signatures/static/decompiler/CLI suites, checks, release CLI and workspace formatting.
- Freeze a separate candidate binary. Measure five alternating small pairs and a sequential large control against PR #205's immutable measured binary. Fresh full 250-function stripped replay must preserve every non-time output field; no score or ranking change is claimed.
- Keep only an observed practical gain with output parity. Report the added row-index memory and any timing regression. The remaining Kuna gap is not assumed closed.

## Accepted local evidence

15 focused FPK tests and 398 related tests passed (one skipped), with crate checks, release CLI and workspace formatting passing. Five-pair small controls are unchanged on Linux/macOS and improve Windows 1.1763 -> 0.9614 s. The fresh sequential large control improves 112.924 -> 42.235 s (CPU 1044.333 -> 233.619 s), with unchanged six outputs and similar observed maximum RSS (3.624/3.598 GiB). Fresh 250-function replay changes no non-time records. Details and all limits are recorded in docs/validation/fid-row-access-performance.md and its compact JSON. The 22-function pinned Kuna comparison remains 4.17–15.47 times slower; Kuna-level performance is not achieved.
