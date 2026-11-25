//! Memory - Process memory operations
//!
//! Provides unified memory read/write/mapping operations across platforms.
//! Uses platform-specific implementations via the [`PlatformMemory`] trait.
//!
//! # Exact vs. Partial vs. Sparse Contracts
//!
//! - **Exact reads ([`MemoryManager::read`], [`MemoryManager::read_exact`], scalar
//!   readers `read_u8`/`read_u16`/`read_u32`/`read_u64`)**: Intended for headers,
//!   pointer tables, inline operands, breakpoints, and register-sized scalars where
//!   every requested byte must be observed. Any short transfer fails with
//!   [`MemoryError::ShortRead`] rather than returning trailing zero padding.
//! - **Explicit partial reads ([`MemoryManager::read_partial`], [`MemoryManager::read_into`])**:
//!   Intended for probing near page/segment boundaries where a prefix transfer may
//!   stop early. Returns only the bytes actually observed (`0..=requested`) paired
//!   with an explicit [`PartialRead::trailing_gap`] describing the unread tail.
//! - **Sparse acquisitions ([`MemoryManager::read_sparse`])**: Intended for dumps,
//!   scans, and forensic snapshots spanning multiple pages/regions. Preserves
//!   genuinely observed `0x00` bytes inside [`ObservedMemoryRun`] while recording
//!   every unread interval as an explicit [`MemoryGap`] tagged with a
//!   [`MemoryGapReason`] (`unmapped`, `denied`, `partial`, `process_exited`,
//!   `budget`, or `read_error`) and summarized in [`SparseCoverageSummary`]
//!   (`observed_bytes + missing_bytes == requested_bytes`).

use super::platform::{PlatformMemory, PlatformMemoryImpl};
use serde::Serialize;
use thiserror::Error;

/// Default hard upper bound for a single exact or partial read request (64 MiB).
pub const DEFAULT_MAX_READ_BYTES: usize = 64 * 1024 * 1024;

/// Default page step used by sparse memory acquisitions.
pub const DEFAULT_SPARSE_PAGE_SIZE: usize = 4096;

/// Memory operation errors.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    #[error("Failed to read memory at {address:#x}: {reason}")]
    ReadFailed { address: u64, reason: String },

    #[error("Short memory read at {address:#x}: expected {expected} bytes, got {actual}")]
    ShortRead {
        address: u64,
        expected: usize,
        actual: usize,
    },

    #[error(
        "Backend reported oversized transfer at {address:#x}: requested {requested} bytes, backend claimed {reported}"
    )]
    OversizedTransfer {
        address: u64,
        requested: usize,
        reported: usize,
    },

    #[error("Failed to write memory at {address:#x}: {reason}")]
    WriteFailed { address: u64, reason: String },

    #[error("Invalid memory region: {address:#x} ({size:#x} bytes)")]
    InvalidRegion { address: u64, size: usize },

    #[error("Unmapped memory at {address:#x} ({size:#x} bytes)")]
    Unmapped { address: u64, size: usize },

    #[error("Access denied at {address:#x}")]
    AccessDenied { address: u64 },

    #[error("No process attached")]
    NoProcess,

    #[error("Target process exited before reading {address:#x}")]
    ProcessExited { address: u64 },

    #[error("Memory read of {requested} bytes exceeds budget limit of {limit} bytes")]
    BudgetExceeded { requested: usize, limit: usize },

    #[error("Invalid sparse memory capture: {0}")]
    InvalidSparseCapture(#[from] SparseCaptureError),
}

/// Memory protection flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryProtection {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl MemoryProtection {
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const RWX: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
    pub const NONE: Self = Self {
        read: false,
        write: false,
        execute: false,
    };
}

/// Represents a memory region in the target process.
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Start address of the region.
    pub base_address: u64,

    /// Size of the region in bytes.
    pub size: usize,

    /// Memory protection flags.
    pub protection: MemoryProtection,

    /// Optional name (e.g., module name, "[stack]", "[heap]").
    pub name: Option<String>,
}

/// Reason why an address range inside a partial or sparse acquisition could not
/// be observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryGapReason {
    /// No memory mapping covers the address range.
    Unmapped,
    /// Mapping exists, or target policy denied read access.
    Denied,
    /// Backend completed only a shorter prefix or returned 0 bytes cleanly.
    Partial,
    /// Target process exited or is no longer attached.
    ProcessExited,
    /// Acquisition stopped due to configured byte/span budget.
    Budget,
    /// Other explicit read failure reported by the backend.
    ReadError { detail: String },
}

impl MemoryGapReason {
    /// Classify a [`MemoryError`] into an explicit sparse gap reason.
    pub fn from_memory_error(error: &MemoryError) -> Self {
        match error {
            MemoryError::Unmapped { .. } => Self::Unmapped,
            MemoryError::AccessDenied { .. } => Self::Denied,
            MemoryError::ShortRead { .. } => Self::Partial,
            MemoryError::ProcessExited { .. } | MemoryError::NoProcess => Self::ProcessExited,
            MemoryError::BudgetExceeded { .. } => Self::Budget,
            other => Self::ReadError {
                detail: other.to_string(),
            },
        }
    }

    /// Convert a gap reason back into a typed [`MemoryError`] for exact slice queries.
    pub fn into_memory_error(&self, address: u64, size: usize) -> MemoryError {
        match self {
            Self::Unmapped => MemoryError::Unmapped { address, size },
            Self::Denied => MemoryError::AccessDenied { address },
            Self::Partial => MemoryError::ShortRead {
                address,
                expected: size,
                actual: 0,
            },
            Self::ProcessExited => MemoryError::ProcessExited { address },
            Self::Budget => MemoryError::BudgetExceeded {
                requested: size,
                limit: 0,
            },
            Self::ReadError { detail } => MemoryError::ReadFailed {
                address,
                reason: detail.clone(),
            },
        }
    }
}

/// Contiguous unread interval `[start, start + len)` with its failure reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryGap {
    pub start: u64,
    pub len: u64,
    pub reason: MemoryGapReason,
}

impl MemoryGap {
    /// Exclusive end address. Saturates for a free-standing gap whose range
    /// overflows; a gap held by a [`SparseMemoryCapture`] never does.
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.len)
    }
}

/// Contiguous observed bytes read from `[start, start + bytes.len())`.
///
/// Any `0x00` byte present inside `bytes` was genuinely observed from target memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservedMemoryRun {
    pub start: u64,
    pub bytes: Vec<u8>,
}

impl ObservedMemoryRun {
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Exclusive end address. Saturates for a free-standing run whose range
    /// overflows; a run held by a [`SparseMemoryCapture`] never does.
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.bytes.len() as u64)
    }
}

/// Ordered element inside a [`SparseMemoryCapture`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MemorySegment {
    Observed(ObservedMemoryRun),
    Gap(MemoryGap),
}

impl MemorySegment {
    /// First address covered by this segment.
    pub fn start(&self) -> u64 {
        match self {
            Self::Observed(run) => run.start,
            Self::Gap(gap) => gap.start,
        }
    }

    /// Number of addresses covered by this segment.
    pub fn len(&self) -> u64 {
        match self {
            Self::Observed(run) => run.bytes.len() as u64,
            Self::Gap(gap) => gap.len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Exclusive end address (saturating; see [`MemoryGap::end`]).
    pub fn end(&self) -> u64 {
        self.start().saturating_add(self.len())
    }
}

/// Explicit partial read result containing only genuinely observed prefix bytes
/// and an optional [`MemoryGap`] for any unread tail.
///
/// Built only by [`MemoryManager::read_partial`]; fields are private so the
/// invariant `bytes.len() + trailing_gap.len == requested`, with the gap (if
/// any) starting at `address + bytes.len()`, cannot be broken after construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartialRead {
    address: u64,
    requested: usize,
    bytes: Vec<u8>,
    trailing_gap: Option<MemoryGap>,
}

impl PartialRead {
    /// Address of the first requested byte.
    pub fn address(&self) -> u64 {
        self.address
    }

    /// Number of bytes requested.
    pub fn requested(&self) -> usize {
        self.requested
    }

    /// The genuinely transferred prefix (never zero-padded).
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consume the read and return the transferred prefix.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The unread tail, if the transfer stopped early.
    pub fn trailing_gap(&self) -> Option<&MemoryGap> {
        self.trailing_gap.as_ref()
    }

    pub fn observed_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn missing_len(&self) -> usize {
        self.requested - self.bytes.len()
    }

    pub fn is_complete(&self) -> bool {
        self.trailing_gap.is_none()
    }
}

/// Result of querying a single byte inside a [`SparseMemoryCapture`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseByte<'a> {
    Observed(u8),
    Missing(&'a MemoryGapReason),
}

/// Configuration and resource bounds for [`MemoryManager::read_sparse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseReadOptions {
    /// Maximum number of bytes allowed to be observed before stopping with [`MemoryGapReason::Budget`].
    pub max_bytes: usize,
    /// Chunk / page size used when advancing past boundaries and holes.
    pub page_size: usize,
    /// Maximum virtual address span to probe across holes before recording a trailing [`MemoryGapReason::Budget`].
    pub max_span_bytes: usize,
}

impl Default for SparseReadOptions {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_READ_BYTES,
            page_size: DEFAULT_SPARSE_PAGE_SIZE,
            max_span_bytes: DEFAULT_MAX_READ_BYTES,
        }
    }
}

impl SparseReadOptions {
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            max_span_bytes: DEFAULT_MAX_READ_BYTES.max(max_bytes),
            ..Self::default()
        }
    }
}

/// Accounting summary for a sparse memory acquisition.
///
/// Derived by [`SparseMemoryCapture::from_segments`] from a validated
/// partition, so `observed_bytes + missing_bytes == requested_bytes` and
/// `complete == (missing_bytes == 0)` always hold for a summary obtained from
/// [`SparseMemoryCapture::coverage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SparseCoverageSummary {
    pub requested_bytes: u64,
    pub observed_bytes: u64,
    pub missing_bytes: u64,
    pub observed_runs: usize,
    pub gap_count: usize,
    pub complete: bool,
}

/// Why a list of [`MemorySegment`]s is not an exact partition of a
/// [`SparseMemoryCapture`]'s requested range.
///
/// A capture is evidence: `complete == true` must mean every requested address
/// was observed exactly once. The constructor therefore rejects any segment
/// list that is not an ordered, non-overlapping, gap-free cover of
/// `[start, start + requested_len)` instead of trusting aggregate byte counts.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SparseCaptureError {
    #[error("sparse capture range overflows the address space: start {start:#x}, len {len:#x}")]
    RangeOverflow { start: u64, len: u64 },

    #[error("sparse segment {index} at {start:#x} is empty")]
    EmptySegment { index: usize, start: u64 },

    #[error("sparse segment {index} at {start:#x} (len {len:#x}) overflows the address space")]
    SegmentOverflow { index: usize, start: u64, len: u64 },

    #[error(
        "sparse segment {index} [{start:#x}, {end:#x}) lies outside the capture [{capture_start:#x}, {capture_end:#x})"
    )]
    OutOfRange {
        index: usize,
        start: u64,
        end: u64,
        capture_start: u64,
        capture_end: u64,
    },

    #[error(
        "sparse segment {index} starts at {start:#x} but [{expected:#x}, {start:#x}) is not covered by any segment"
    )]
    Hole {
        index: usize,
        expected: u64,
        start: u64,
    },

    #[error(
        "sparse segment {index} starts at {start:#x}, before {expected:#x} where the previous segment ended (overlapping, duplicate or out-of-order segment)"
    )]
    Overlap {
        index: usize,
        expected: u64,
        start: u64,
    },

    #[error(
        "sparse segments cover [{capture_start:#x}, {covered_end:#x}) but the capture requests [{capture_start:#x}, {capture_end:#x})"
    )]
    IncompleteCoverage {
        capture_start: u64,
        covered_end: u64,
        capture_end: u64,
    },
}

/// Ordered sparse memory acquisition preserving observed runs and unread gaps.
///
/// # Invariant (checked by [`SparseMemoryCapture::from_segments`] in every build profile)
///
/// The segments are an exact partition of `[start, start + requested_len)`:
/// every segment is non-empty, the first starts at `start`, each next one starts
/// exactly where the previous one ends, the last ends at
/// `start + requested_len`, and no endpoint overflows `u64`. Adjacent observed
/// runs, and adjacent gaps with the same reason, are coalesced. Hence
/// `observed_bytes + missing_bytes == requested_bytes`, and `complete` means
/// every requested address was observed exactly once. Fields are private so the
/// invariant cannot be broken after construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SparseMemoryCapture {
    start: u64,
    requested_len: u64,
    segments: Vec<MemorySegment>,
    coverage: SparseCoverageSummary,
}

impl SparseMemoryCapture {
    /// Validate that `raw_segments` exactly partitions
    /// `[start, start + requested_len)` and build a canonical capture,
    /// coalescing contiguous observed runs and adjacent gaps with matching
    /// reasons.
    ///
    /// Rejects (in release builds too) an overflowing capture range, empty
    /// segments, segments whose end overflows, segments outside the range,
    /// holes, overlaps (including duplicates and out-of-order segments) and a
    /// cover that stops short of the requested end.
    pub fn from_segments(
        start: u64,
        requested_len: u64,
        raw_segments: Vec<MemorySegment>,
    ) -> Result<Self, SparseCaptureError> {
        let capture_end =
            start
                .checked_add(requested_len)
                .ok_or(SparseCaptureError::RangeOverflow {
                    start,
                    len: requested_len,
                })?;

        let mut cursor = start;
        for (index, segment) in raw_segments.iter().enumerate() {
            let seg_start = segment.start();
            let seg_len = match segment {
                MemorySegment::Observed(run) => u64::try_from(run.bytes.len()).map_err(|_| {
                    SparseCaptureError::SegmentOverflow {
                        index,
                        start: seg_start,
                        len: u64::MAX,
                    }
                })?,
                MemorySegment::Gap(gap) => gap.len,
            };
            if seg_len == 0 {
                return Err(SparseCaptureError::EmptySegment {
                    index,
                    start: seg_start,
                });
            }
            let seg_end =
                seg_start
                    .checked_add(seg_len)
                    .ok_or(SparseCaptureError::SegmentOverflow {
                        index,
                        start: seg_start,
                        len: seg_len,
                    })?;
            if seg_start < start || seg_end > capture_end {
                return Err(SparseCaptureError::OutOfRange {
                    index,
                    start: seg_start,
                    end: seg_end,
                    capture_start: start,
                    capture_end,
                });
            }
            if seg_start < cursor {
                return Err(SparseCaptureError::Overlap {
                    index,
                    expected: cursor,
                    start: seg_start,
                });
            }
            if seg_start > cursor {
                return Err(SparseCaptureError::Hole {
                    index,
                    expected: cursor,
                    start: seg_start,
                });
            }
            cursor = seg_end;
        }
        if cursor != capture_end {
            return Err(SparseCaptureError::IncompleteCoverage {
                capture_start: start,
                covered_end: cursor,
                capture_end,
            });
        }

        // The input is an exact partition, so every sum below is bounded by
        // `requested_len` and coalescing only merges true neighbours.
        let mut segments: Vec<MemorySegment> = Vec::with_capacity(raw_segments.len());
        for segment in raw_segments {
            match (segments.last_mut(), segment) {
                (Some(MemorySegment::Observed(prev)), MemorySegment::Observed(run)) => {
                    prev.bytes.extend_from_slice(&run.bytes);
                }
                (Some(MemorySegment::Gap(prev)), MemorySegment::Gap(gap))
                    if prev.reason == gap.reason =>
                {
                    prev.len += gap.len;
                }
                (_, segment) => segments.push(segment),
            }
        }

        let mut observed_bytes = 0u64;
        let mut missing_bytes = 0u64;
        let mut observed_runs = 0usize;
        let mut gap_count = 0usize;
        for segment in &segments {
            match segment {
                MemorySegment::Observed(run) => {
                    observed_bytes += run.bytes.len() as u64;
                    observed_runs += 1;
                }
                MemorySegment::Gap(gap) => {
                    missing_bytes += gap.len;
                    gap_count += 1;
                }
            }
        }

        let coverage = SparseCoverageSummary {
            requested_bytes: requested_len,
            observed_bytes,
            missing_bytes,
            observed_runs,
            gap_count,
            complete: missing_bytes == 0,
        };

        Ok(Self {
            start,
            requested_len,
            segments,
            coverage,
        })
    }

    /// First requested address.
    pub fn start(&self) -> u64 {
        self.start
    }

    /// Number of requested addresses.
    pub fn requested_len(&self) -> u64 {
        self.requested_len
    }

    /// Exclusive end of the requested range (never overflows; checked at construction).
    pub fn end(&self) -> u64 {
        self.start + self.requested_len
    }

    /// The canonical, ordered partition of the requested range.
    pub fn segments(&self) -> &[MemorySegment] {
        &self.segments
    }

    /// Coverage accounting derived from [`Self::segments`].
    pub fn coverage(&self) -> SparseCoverageSummary {
        self.coverage
    }

    /// `true` iff every requested address was observed.
    pub fn is_complete(&self) -> bool {
        self.coverage.complete
    }

    pub fn observed_runs(&self) -> impl Iterator<Item = &ObservedMemoryRun> {
        self.segments.iter().filter_map(|seg| match seg {
            MemorySegment::Observed(run) => Some(run),
            MemorySegment::Gap(_) => None,
        })
    }

    pub fn gaps(&self) -> impl Iterator<Item = &MemoryGap> {
        self.segments.iter().filter_map(|seg| match seg {
            MemorySegment::Gap(gap) => Some(gap),
            MemorySegment::Observed(_) => None,
        })
    }

    /// Index of the segment containing `address`; `address` must lie inside
    /// `[start, end)`. Binary search is valid because segments are ordered and
    /// contiguous.
    fn segment_index(&self, address: u64) -> usize {
        self.segments.partition_point(|seg| seg.end() <= address)
    }

    /// Query one byte at `address` inside this capture, distinguishing a genuinely
    /// observed `0x00` (`Some(SparseByte::Observed(0))`) from an unread gap
    /// (`Some(SparseByte::Missing(reason))`), or `None` if `address` is outside `[start, start + requested_len)`.
    pub fn byte_at(&self, address: u64) -> Option<SparseByte<'_>> {
        if address < self.start || address >= self.end() {
            return None;
        }
        match self.segments.get(self.segment_index(address))? {
            MemorySegment::Observed(run) => {
                let offset = usize::try_from(address - run.start).ok()?;
                run.bytes.get(offset).copied().map(SparseByte::Observed)
            }
            MemorySegment::Gap(gap) => Some(SparseByte::Missing(&gap.reason)),
        }
    }

    /// Extract an exact byte slice `[address, address + len)` only if every byte
    /// in that range was genuinely observed. Fails on any intersecting gap instead
    /// of padding zeros.
    pub fn read_exact_slice(&self, address: u64, len: usize) -> Result<Vec<u8>, MemoryError> {
        let invalid = || MemoryError::InvalidRegion { address, size: len };
        let len_u64 = u64::try_from(len).map_err(|_| invalid())?;
        let slice_end = address.checked_add(len_u64).ok_or_else(invalid)?;
        if address < self.start || slice_end > self.end() {
            return Err(MemoryError::Unmapped { address, size: len });
        }
        if len == 0 {
            return Ok(Vec::new());
        }

        // A fully observed slice cannot exceed the observed byte count, so this
        // bounds the allocation by bytes actually held, not by the (possibly
        // huge) requested span of gap metadata.
        let capacity = usize::try_from(self.coverage.observed_bytes)
            .unwrap_or(usize::MAX)
            .min(len);
        let mut out = Vec::with_capacity(capacity);
        let mut cursor = address;
        for segment in &self.segments[self.segment_index(address)..] {
            if cursor >= slice_end {
                break;
            }
            match segment {
                MemorySegment::Observed(run) => {
                    let take_end = run.end().min(slice_end);
                    let start_off = (cursor - run.start) as usize;
                    let end_off = (take_end - run.start) as usize;
                    out.extend_from_slice(&run.bytes[start_off..end_off]);
                    cursor = take_end;
                }
                MemorySegment::Gap(gap) => {
                    let err_len =
                        usize::try_from(slice_end.min(gap.end()) - cursor).unwrap_or(usize::MAX);
                    return Err(gap.reason.into_memory_error(cursor, err_len));
                }
            }
        }
        if out.len() != len {
            return Err(MemoryError::ShortRead {
                address,
                expected: len,
                actual: out.len(),
            });
        }
        Ok(out)
    }

    /// Export this sparse capture with hex-formatted addresses and per-segment metadata.
    pub fn to_json_value(&self) -> serde_json::Value {
        let segments: Vec<serde_json::Value> = self
            .segments
            .iter()
            .map(|seg| match seg {
                MemorySegment::Observed(run) => serde_json::json!({
                    "state": "observed",
                    "start": format!("0x{:x}", run.start),
                    "end": format!("0x{:x}", run.end()),
                    "len": run.len(),
                    "bytes_hex": run.bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                }),
                MemorySegment::Gap(gap) => serde_json::json!({
                    "state": "gap",
                    "start": format!("0x{:x}", gap.start),
                    "end": format!("0x{:x}", gap.end()),
                    "len": gap.len,
                    "reason": gap.reason,
                }),
            })
            .collect();

        serde_json::json!({
            "start": format!("0x{:x}", self.start),
            "requested_len": self.requested_len,
            "coverage": self.coverage,
            "segments": segments,
        })
    }
}

pub(crate) fn validate_read_range(
    address: u64,
    size: usize,
    max_bytes: usize,
) -> Result<(), MemoryError> {
    if size > max_bytes {
        return Err(MemoryError::BudgetExceeded {
            requested: size,
            limit: max_bytes,
        });
    }
    let size_u64 = u64::try_from(size).map_err(|_| MemoryError::InvalidRegion { address, size })?;
    if address.checked_add(size_u64).is_none() {
        return Err(MemoryError::InvalidRegion { address, size });
    }
    Ok(())
}

pub(crate) fn checked_transfer_len(
    address: u64,
    requested: usize,
    reported: usize,
) -> Result<usize, MemoryError> {
    if reported > requested {
        return Err(MemoryError::OversizedTransfer {
            address,
            requested,
            reported,
        });
    }
    Ok(reported)
}

/// Memory manager for reading/writing process memory.
///
/// Wraps a [`PlatformMemory`] backend (defaulting to [`PlatformMemoryImpl`]) and
/// enforces exact-read, explicit-partial, and bounded sparse acquisition contracts.
pub struct MemoryManager<M: PlatformMemory = PlatformMemoryImpl> {
    /// Platform-specific (or mock) memory implementation.
    platform: M,
    /// Cached memory regions.
    regions: Vec<MemoryRegion>,
    /// Hard limit on single exact or partial read size.
    max_read_bytes: usize,
}

impl MemoryManager<PlatformMemoryImpl> {
    /// Create a new memory manager using the host OS backend.
    pub fn new() -> Self {
        Self::with_backend(PlatformMemoryImpl::new())
    }
}

impl<M: PlatformMemory> MemoryManager<M> {
    /// Create a memory manager over an arbitrary [`PlatformMemory`] backend.
    pub fn with_backend(platform: M) -> Self {
        Self {
            platform,
            regions: Vec::new(),
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
        }
    }

    /// Override the maximum single-read byte budget.
    pub fn with_max_read_bytes(mut self, max_read_bytes: usize) -> Self {
        self.max_read_bytes = max_read_bytes;
        self
    }

    /// Return the configured maximum single-read byte budget.
    pub fn max_read_bytes(&self) -> usize {
        self.max_read_bytes
    }

    /// Access the underlying backend.
    pub fn backend(&self) -> &M {
        &self.platform
    }

    /// Mutably access the underlying backend.
    pub fn backend_mut(&mut self) -> &mut M {
        &mut self.platform
    }

    /// Open a process for memory operations.
    pub fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        self.platform.open_process(pid)
    }

    /// Check if a process is currently open.
    pub fn is_open(&self) -> bool {
        self.platform.is_open()
    }

    /// Exact memory read: returns `Ok(bytes)` of length `size` only if every
    /// requested byte was transferred. Fails with [`MemoryError::ShortRead`] on
    /// short or zero-byte transfers instead of returning trailing zeros.
    pub fn read(&self, address: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        self.read_exact(address, size)
    }

    /// Exact memory read requiring all `size` bytes to be observed.
    pub fn read_exact(&self, address: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        validate_read_range(address, size, self.max_read_bytes)?;
        if !self.platform.is_open() {
            return Err(MemoryError::NoProcess);
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        let mut buffer = vec![0u8; size];
        let actual = self.read_into(address, &mut buffer)?;
        if actual < size {
            return Err(MemoryError::ShortRead {
                address,
                expected: size,
                actual,
            });
        }
        Ok(buffer)
    }

    /// Explicit partial memory read: returns only the bytes actually transferred
    /// without padding missing tail bytes with zeros, alongside an optional
    /// [`MemoryGap`] explaining any unread tail.
    pub fn read_partial(&self, address: u64, size: usize) -> Result<PartialRead, MemoryError> {
        validate_read_range(address, size, self.max_read_bytes)?;
        if !self.platform.is_open() {
            return Err(MemoryError::NoProcess);
        }
        if size == 0 {
            return Ok(PartialRead {
                address,
                requested: 0,
                bytes: Vec::new(),
                trailing_gap: None,
            });
        }

        let mut buffer = vec![0u8; size];
        let actual = self.read_into(address, &mut buffer)?;
        buffer.truncate(actual);
        let trailing_gap = (actual < size).then(|| MemoryGap {
            start: address.saturating_add(actual as u64),
            len: (size - actual) as u64,
            reason: MemoryGapReason::Partial,
        });
        Ok(PartialRead {
            address,
            requested: size,
            bytes: buffer,
            trailing_gap,
        })
    }

    /// Bounded sparse acquisition over `[address, address + size)`.
    ///
    /// Walks page-aligned chunks across the requested range, preserving genuinely
    /// observed `0x00` bytes inside [`ObservedMemoryRun`] while recording every
    /// unmapped/denied/partial/exited/budget gap as an explicit [`MemoryGap`].
    pub fn read_sparse(
        &self,
        address: u64,
        size: usize,
        options: SparseReadOptions,
    ) -> Result<SparseMemoryCapture, MemoryError> {
        let size_u64 =
            u64::try_from(size).map_err(|_| MemoryError::InvalidRegion { address, size })?;
        let Some(end) = address.checked_add(size_u64) else {
            return Err(MemoryError::InvalidRegion { address, size });
        };
        if options.page_size == 0 {
            return Err(MemoryError::InvalidRegion { address, size: 0 });
        }
        if !self.platform.is_open() {
            return Err(MemoryError::NoProcess);
        }
        if size == 0 {
            return Ok(SparseMemoryCapture::from_segments(address, 0, Vec::new())?);
        }

        let page_size_u64 = options.page_size as u64;
        let mut cursor = address;
        let mut observed_total = 0usize;
        let mut probed_span = 0usize;
        let mut segments = Vec::new();

        while cursor < end {
            if observed_total >= options.max_bytes || probed_span >= options.max_span_bytes {
                segments.push(MemorySegment::Gap(MemoryGap {
                    start: cursor,
                    len: end - cursor,
                    reason: MemoryGapReason::Budget,
                }));
                break;
            }

            let offset_in_page = (cursor % page_size_u64) as usize;
            let to_page_end = options.page_size - offset_in_page;
            let remaining_req = usize::try_from(end - cursor).unwrap_or(usize::MAX);
            let remaining_obs_budget = options.max_bytes.saturating_sub(observed_total);
            let remaining_span_budget = options.max_span_bytes.saturating_sub(probed_span);
            let chunk_len = to_page_end
                .min(remaining_req)
                .min(remaining_obs_budget)
                .min(remaining_span_budget)
                .min(self.max_read_bytes);

            if chunk_len == 0 {
                segments.push(MemorySegment::Gap(MemoryGap {
                    start: cursor,
                    len: end - cursor,
                    reason: MemoryGapReason::Budget,
                }));
                break;
            }

            let mut buf = vec![0u8; chunk_len];
            match self.read_into(cursor, &mut buf) {
                Ok(0) => {
                    segments.push(MemorySegment::Gap(MemoryGap {
                        start: cursor,
                        len: chunk_len as u64,
                        reason: MemoryGapReason::Partial,
                    }));
                    cursor = cursor.saturating_add(chunk_len as u64);
                    probed_span = probed_span.saturating_add(chunk_len);
                }
                Ok(n) => {
                    buf.truncate(n);
                    observed_total = observed_total.saturating_add(n);
                    segments.push(MemorySegment::Observed(ObservedMemoryRun {
                        start: cursor,
                        bytes: buf,
                    }));
                    if n < chunk_len {
                        segments.push(MemorySegment::Gap(MemoryGap {
                            start: cursor.saturating_add(n as u64),
                            len: (chunk_len - n) as u64,
                            reason: MemoryGapReason::Partial,
                        }));
                    }
                    cursor = cursor.saturating_add(chunk_len as u64);
                    probed_span = probed_span.saturating_add(chunk_len);
                }
                Err(err @ MemoryError::OversizedTransfer { .. }) => return Err(err),
                Err(MemoryError::ProcessExited { .. } | MemoryError::NoProcess) => {
                    segments.push(MemorySegment::Gap(MemoryGap {
                        start: cursor,
                        len: end - cursor,
                        reason: MemoryGapReason::ProcessExited,
                    }));
                    break;
                }
                Err(err) => {
                    segments.push(MemorySegment::Gap(MemoryGap {
                        start: cursor,
                        len: chunk_len as u64,
                        reason: MemoryGapReason::from_memory_error(&err),
                    }));
                    cursor = cursor.saturating_add(chunk_len as u64);
                    probed_span = probed_span.saturating_add(chunk_len);
                }
            }
        }

        Ok(SparseMemoryCapture::from_segments(
            address, size_u64, segments,
        )?)
    }

    /// Read memory into an existing buffer, validating address bounds, budget,
    /// open state, and ensuring the backend never reports more bytes than `buffer.len()`.
    pub fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        validate_read_range(address, buffer.len(), self.max_read_bytes)?;
        if !self.platform.is_open() {
            return Err(MemoryError::NoProcess);
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        let reported = self.platform.read_into(address, buffer)?;
        checked_transfer_len(address, buffer.len(), reported)
    }

    /// Write memory to the target process.
    pub fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        self.platform.write(address, data)
    }

    /// Get memory regions of the target process.
    pub fn query_regions(&mut self) -> Result<&[MemoryRegion], MemoryError> {
        self.regions = self.platform.query_regions()?;
        Ok(&self.regions)
    }

    /// Read a null-terminated string from memory.
    pub fn read_string(&self, address: u64, max_len: usize) -> Result<String, MemoryError> {
        validate_read_range(address, max_len, self.max_read_bytes)?;
        read_string_chunked(address, max_len, |addr, buf| self.read_into(addr, buf))
    }

    /// Read a u64 value from memory (little-endian). Fails on short transfers.
    pub fn read_u64(&self, address: u64) -> Result<u64, MemoryError> {
        let data = self.read_exact(address, 8)?;
        let bytes: [u8; 8] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u64".to_string(),
        })?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Read a u32 value from memory (little-endian). Fails on short transfers.
    pub fn read_u32(&self, address: u64) -> Result<u32, MemoryError> {
        let data = self.read_exact(address, 4)?;
        let bytes: [u8; 4] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u32".to_string(),
        })?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read a u16 value from memory (little-endian). Fails on short transfers.
    pub fn read_u16(&self, address: u64) -> Result<u16, MemoryError> {
        let data = self.read_exact(address, 2)?;
        let bytes: [u8; 2] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u16".to_string(),
        })?;
        Ok(u16::from_le_bytes(bytes))
    }

    /// Read a u8 value from memory. Fails on short transfers.
    pub fn read_u8(&self, address: u64) -> Result<u8, MemoryError> {
        let data = self.read_exact(address, 1)?;
        Ok(data[0])
    }
}

impl<M: PlatformMemory + Default> Default for MemoryManager<M> {
    fn default() -> Self {
        Self::with_backend(M::default())
    }
}

pub(crate) fn read_string_chunked<F>(
    address: u64,
    max_len: usize,
    mut read_into: F,
) -> Result<String, MemoryError>
where
    F: FnMut(u64, &mut [u8]) -> Result<usize, MemoryError>,
{
    if max_len == 0 {
        return Ok(String::new());
    }

    let mut current_addr = address;
    let mut bytes = Vec::new();

    while bytes.len() < max_len {
        let offset_in_page = (current_addr % 4096) as usize;
        let bytes_to_page_end = 4096 - offset_in_page;
        let remaining = max_len - bytes.len();
        let chunk_size = bytes_to_page_end.min(remaining);

        let mut chunk = vec![0u8; chunk_size];
        let reported = read_into(current_addr, &mut chunk)?;
        let bytes_read = checked_transfer_len(current_addr, chunk_size, reported)?;
        if bytes_read == 0 {
            return Err(MemoryError::ShortRead {
                address: current_addr,
                expected: 1,
                actual: 0,
            });
        }

        let valid = &chunk[..bytes_read];
        if let Some(null_pos) = valid.iter().position(|&b| b == 0) {
            bytes.extend_from_slice(&valid[..null_pos]);
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }

        bytes.extend_from_slice(valid);
        if bytes_read < chunk_size {
            return Err(MemoryError::ShortRead {
                address: current_addr.saturating_add(bytes_read as u64),
                expected: chunk_size - bytes_read,
                actual: 0,
            });
        }

        match current_addr.checked_add(bytes_read as u64) {
            Some(next) => current_addr = next,
            None => break,
        }
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_read_string_stops_before_page_boundary() {
        // String located near end of page: 0x1ff8..0x1ffe (6 bytes: "hello\0")
        // Page boundary is 0x2000. Reading past 0x2000 would fault.
        let mut read_calls = Vec::new();
        let result = read_string_chunked(0x1ff8, 256, |addr, buf| {
            read_calls.push((addr, buf.len()));
            assert!(
                addr + buf.len() as u64 <= 0x2000,
                "Read request crossed page boundary!"
            );
            let s = b"hello\0";
            let copy_len = s.len().min(buf.len());
            buf[..copy_len].copy_from_slice(&s[..copy_len]);
            Ok(copy_len)
        });

        assert_eq!(result.unwrap(), "hello");
        assert_eq!(read_calls.len(), 1);
        assert_eq!(read_calls[0], (0x1ff8, 8)); // 4096 - (0x1ff8 % 4096) = 8
    }

    #[test]
    fn test_read_string_crosses_page_boundaries_in_chunks() {
        // String starts at 0x1ffe, spans 4 bytes: "ab" on page 1, "cd\0" on page 2.
        let mut read_calls = Vec::new();
        let result = read_string_chunked(0x1ffe, 256, |addr, buf| {
            read_calls.push((addr, buf.len()));
            if addr == 0x1ffe {
                assert_eq!(buf.len(), 2);
                buf[0] = b'a';
                buf[1] = b'b';
                Ok(2)
            } else if addr == 0x2000 {
                buf[0] = b'c';
                buf[1] = b'd';
                buf[2] = 0;
                Ok(3)
            } else {
                panic!("Unexpected read address: {:#x}", addr);
            }
        });

        assert_eq!(result.unwrap(), "abcd");
        assert_eq!(read_calls.len(), 2);
        assert_eq!(read_calls[0], (0x1ffe, 2));
        assert_eq!(read_calls[1], (0x2000, 254));
    }

    #[test]
    fn test_read_string_lossy_utf8() {
        let result = read_string_chunked(0x1000, 16, |_addr, buf| {
            buf[0] = 0xff; // Invalid UTF-8
            buf[1] = 0xfe;
            buf[2] = 0;
            Ok(3)
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_string_rejects_short_unterminated_read() {
        let err = read_string_chunked(0x1000, 16, |_addr, buf| {
            buf[0] = b'a';
            buf[1] = b'b';
            Ok(2)
        })
        .expect_err("short unterminated string read must fail instead of returning partial");
        assert!(matches!(err, MemoryError::ShortRead { .. }), "{err:?}");
    }

    #[derive(Clone)]
    enum MockAction {
        Bytes(Vec<u8>),
        Short { prefix: Vec<u8>, reported: usize },
        OversizedCount(usize),
        Error(MemoryError),
    }

    struct MockPlatformMemory {
        open: bool,
        responses: BTreeMap<(u64, usize), MockAction>,
        prefix_fallback: bool,
        page_map: BTreeMap<u64, Result<Vec<u8>, MemoryError>>,
    }

    impl MockPlatformMemory {
        fn new() -> Self {
            Self {
                open: true,
                responses: BTreeMap::new(),
                prefix_fallback: false,
                page_map: BTreeMap::new(),
            }
        }
    }

    impl PlatformMemory for MockPlatformMemory {
        fn open_process(&mut self, _pid: u32) -> Result<(), MemoryError> {
            self.open = true;
            Ok(())
        }

        fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
            if !self.open {
                return Err(MemoryError::NoProcess);
            }
            if let Some(action) = self.responses.get(&(address, buffer.len())) {
                return match action {
                    MockAction::Bytes(data) => {
                        let copy_len = data.len().min(buffer.len());
                        buffer[..copy_len].copy_from_slice(&data[..copy_len]);
                        Ok(data.len())
                    }
                    MockAction::Short { prefix, reported } => {
                        let copy_len = prefix.len().min(buffer.len());
                        buffer[..copy_len].copy_from_slice(&prefix[..copy_len]);
                        Ok(*reported)
                    }
                    MockAction::OversizedCount(n) => Ok(*n),
                    MockAction::Error(err) => Err(err.clone()),
                };
            }
            if let Some(entry) = self.page_map.get(&(address & !0xfff)) {
                return match entry {
                    Ok(page) => {
                        let off = (address & 0xfff) as usize;
                        if off >= page.len() {
                            return Ok(0);
                        }
                        let available = (page.len() - off).min(buffer.len());
                        buffer[..available].copy_from_slice(&page[off..off + available]);
                        Ok(available)
                    }
                    Err(err) => Err(err.clone()),
                };
            }
            if self.prefix_fallback {
                return Ok(0);
            }
            Err(MemoryError::Unmapped {
                address,
                size: buffer.len(),
            })
        }

        fn write(&self, _address: u64, data: &[u8]) -> Result<usize, MemoryError> {
            Ok(data.len())
        }

        fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError> {
            Ok(Vec::new())
        }

        fn is_open(&self) -> bool {
            self.open
        }
    }

    #[test]
    fn exact_read_and_scalars_reject_zero_and_short_transfers() {
        let mut mock = MockPlatformMemory::new();
        mock.responses.insert(
            (0x1000, 1),
            MockAction::Short {
                prefix: vec![],
                reported: 0,
            },
        );
        mock.responses.insert(
            (0x1000, 2),
            MockAction::Short {
                prefix: vec![0xaa],
                reported: 1,
            },
        );
        mock.responses.insert(
            (0x1000, 4),
            MockAction::Short {
                prefix: vec![0x11, 0x22],
                reported: 2,
            },
        );
        mock.responses.insert(
            (0x1000, 8),
            MockAction::Short {
                prefix: vec![0x01, 0x02, 0x03],
                reported: 3,
            },
        );
        mock.responses.insert(
            (0x2000, 8),
            MockAction::Short {
                prefix: vec![],
                reported: 0,
            },
        );

        let mem = MemoryManager::with_backend(mock);

        assert_eq!(
            mem.read(0x1000, 1).unwrap_err(),
            MemoryError::ShortRead {
                address: 0x1000,
                expected: 1,
                actual: 0,
            }
        );
        assert!(matches!(
            mem.read_u8(0x1000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 1,
                actual: 0,
                ..
            }
        ));
        assert!(matches!(
            mem.read_u16(0x1000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 2,
                actual: 1,
                ..
            }
        ));
        assert!(matches!(
            mem.read_u32(0x1000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 4,
                actual: 2,
                ..
            }
        ));
        assert!(matches!(
            mem.read_u64(0x1000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 8,
                actual: 3,
                ..
            }
        ));
        assert!(matches!(
            mem.read_u64(0x2000).unwrap_err(),
            MemoryError::ShortRead {
                expected: 8,
                actual: 0,
                ..
            }
        ));
    }

    #[test]
    fn exact_read_preserves_genuinely_observed_zeros_and_nonzeros() {
        let mut mock = MockPlatformMemory::new();
        mock.responses
            .insert((0x3000, 4), MockAction::Bytes(vec![0x00, 0x00, 0x00, 0x00]));
        mock.responses
            .insert((0x3004, 4), MockAction::Bytes(vec![0x00, 0x11, 0x00, 0x22]));
        mock.responses
            .insert((0x3008, 8), MockAction::Bytes(vec![0x00; 8]));

        let mem = MemoryManager::with_backend(mock);

        assert_eq!(mem.read(0x3000, 4).unwrap(), [0, 0, 0, 0]);
        assert_eq!(mem.read_u32(0x3000).unwrap(), 0);
        assert_eq!(mem.read_u32(0x3004).unwrap(), 0x2200_1100);
        assert_eq!(mem.read_u64(0x3008).unwrap(), 0);
    }

    #[test]
    fn oversized_transfer_count_and_oversized_or_wrapping_requests_are_rejected() {
        let mut mock = MockPlatformMemory::new();
        mock.responses
            .insert((0x4000, 4), MockAction::OversizedCount(8));
        mock.responses
            .insert((0x4000, 1024), MockAction::OversizedCount(2048));

        let mem = MemoryManager::with_backend(mock).with_max_read_bytes(1024);

        let mut buf = [0u8; 4];
        assert_eq!(
            mem.read_into(0x4000, &mut buf).unwrap_err(),
            MemoryError::OversizedTransfer {
                address: 0x4000,
                requested: 4,
                reported: 8,
            }
        );
        assert!(matches!(
            mem.read_exact(0x4000, 4).unwrap_err(),
            MemoryError::OversizedTransfer { .. }
        ));
        assert!(matches!(
            mem.read_partial(0x4000, 4).unwrap_err(),
            MemoryError::OversizedTransfer { .. }
        ));
        assert!(matches!(
            mem.read_sparse(0x4000, 4096, SparseReadOptions::default())
                .unwrap_err(),
            MemoryError::OversizedTransfer {
                address: 0x4000,
                requested: 1024,
                reported: 2048,
            }
        ));

        assert_eq!(
            mem.read_exact(0x4000, 1025).unwrap_err(),
            MemoryError::BudgetExceeded {
                requested: 1025,
                limit: 1024,
            }
        );
        assert!(matches!(
            mem.read_exact(u64::MAX - 1, 4).unwrap_err(),
            MemoryError::InvalidRegion { size: 4, .. }
        ));
        assert!(matches!(
            mem.read_sparse(u64::MAX - 1, 4, SparseReadOptions::default())
                .unwrap_err(),
            MemoryError::InvalidRegion { size: 4, .. }
        ));
    }

    #[test]
    fn partial_read_returns_only_observed_bytes_with_explicit_trailing_gap() {
        let mut mock = MockPlatformMemory::new();
        mock.responses.insert(
            (0x5000, 8),
            MockAction::Short {
                prefix: vec![0xaa, 0x00, 0xbb],
                reported: 3,
            },
        );
        mock.responses.insert(
            (0x5010, 4),
            MockAction::Short {
                prefix: vec![],
                reported: 0,
            },
        );
        mock.responses.insert(
            (0x5020, 4),
            MockAction::Error(MemoryError::AccessDenied { address: 0x5020 }),
        );

        let mem = MemoryManager::with_backend(mock);
        assert!(matches!(
            mem.read_partial(0x5020, 4).unwrap_err(),
            MemoryError::AccessDenied { address: 0x5020 }
        ));
        let partial = mem.read_partial(0x5000, 8).unwrap();
        assert_eq!(partial.bytes, vec![0xaa, 0x00, 0xbb]);
        assert_eq!(partial.observed_len(), 3);
        assert_eq!(partial.missing_len(), 5);
        assert!(!partial.is_complete());
        assert_eq!(
            partial.trailing_gap,
            Some(MemoryGap {
                start: 0x5003,
                len: 5,
                reason: MemoryGapReason::Partial,
            })
        );

        let empty_partial = mem.read_partial(0x5010, 4).unwrap();
        assert!(empty_partial.bytes.is_empty());
        assert_eq!(
            empty_partial.trailing_gap,
            Some(MemoryGap {
                start: 0x5010,
                len: 4,
                reason: MemoryGapReason::Partial,
            })
        );
    }

    #[test]
    fn sparse_capture_distinguishes_observed_zeros_from_all_gap_reasons() {
        let mut mock = MockPlatformMemory::new();
        // Page 0 (0x1000..0x2000): genuine zero bytes followed by nonzero marker.
        let mut page0 = vec![0u8; 4096];
        page0[4] = 0xde;
        page0[5] = 0xad;
        mock.page_map.insert(0x1000, Ok(page0));
        // Page 1 (0x2000..0x3000): unmapped hole.
        mock.page_map.insert(
            0x2000,
            Err(MemoryError::Unmapped {
                address: 0x2000,
                size: 4096,
            }),
        );
        // Page 2 (0x3000..0x4000): access denied.
        mock.page_map
            .insert(0x3000, Err(MemoryError::AccessDenied { address: 0x3000 }));
        // Page 3 (0x4000..0x5000): partial transfer (only 128 bytes).
        mock.page_map.insert(0x4000, Ok(vec![0x42u8; 128]));
        // Page 4 (0x5000..0x6000): full page after gaps, starting with 0x00.
        let mut page4 = vec![0x77u8; 4096];
        page4[0] = 0x00;
        mock.page_map.insert(0x5000, Ok(page4));

        let mem = MemoryManager::with_backend(mock);
        let capture = mem
            .read_sparse(0x1000, 5 * 4096, SparseReadOptions::default())
            .unwrap();

        assert!(!capture.is_complete());
        assert_eq!(capture.coverage.requested_bytes, 5 * 4096);
        assert_eq!(capture.coverage.observed_bytes, 4096 + 128 + 4096);
        assert_eq!(capture.coverage.missing_bytes, 4096 + 4096 + (4096 - 128));
        assert_eq!(
            capture.coverage.observed_bytes + capture.coverage.missing_bytes,
            capture.coverage.requested_bytes
        );

        // Genuinely observed 0x00 bytes are indistinguishable from ordinary data.
        assert_eq!(capture.byte_at(0x1000), Some(SparseByte::Observed(0x00)));
        assert_eq!(capture.byte_at(0x1004), Some(SparseByte::Observed(0xde)));
        assert_eq!(capture.byte_at(0x5000), Some(SparseByte::Observed(0x00)));

        // Unread bytes record exact reasons rather than zeros.
        assert_eq!(
            capture.byte_at(0x2000),
            Some(SparseByte::Missing(&MemoryGapReason::Unmapped))
        );
        assert_eq!(
            capture.byte_at(0x3000),
            Some(SparseByte::Missing(&MemoryGapReason::Denied))
        );
        assert_eq!(
            capture.byte_at(0x4080),
            Some(SparseByte::Missing(&MemoryGapReason::Partial))
        );

        assert_eq!(
            capture.read_exact_slice(0x1000, 6).unwrap(),
            [0x00, 0x00, 0x00, 0x00, 0xde, 0xad]
        );
        assert!(matches!(
            capture.read_exact_slice(0x2000, 4).unwrap_err(),
            MemoryError::Unmapped { .. }
        ));
        assert!(matches!(
            capture.read_exact_slice(0x1ffe, 8).unwrap_err(),
            MemoryError::Unmapped { .. }
        ));
        assert!(matches!(
            capture.read_exact_slice(0x3000, 4).unwrap_err(),
            MemoryError::AccessDenied { .. }
        ));

        let exported = capture.to_json_value();
        assert_eq!(exported["coverage"]["observed_bytes"], 4096 + 128 + 4096);
        assert_eq!(exported["segments"][1]["reason"]["kind"], "unmapped");
        assert_eq!(exported["segments"][2]["reason"]["kind"], "denied");
    }

    #[test]
    fn sparse_capture_records_budget_and_process_exit_gaps() {
        let mut mock = MockPlatformMemory::new();
        mock.page_map.insert(0x1000, Ok(vec![0x11u8; 4096]));
        mock.page_map.insert(0x2000, Ok(vec![0x22u8; 4096]));
        mock.page_map
            .insert(0x3000, Err(MemoryError::ProcessExited { address: 0x3000 }));
        mock.page_map.insert(0x4000, Ok(vec![0x44u8; 4096]));

        let mem = MemoryManager::with_backend(mock);

        // Tight budget stops cleanly and tags trailing tail as Budget.
        let budgeted = mem
            .read_sparse(0x1000, 3 * 4096, SparseReadOptions::with_max_bytes(4096))
            .unwrap();
        assert_eq!(budgeted.coverage.observed_bytes, 4096);
        assert_eq!(budgeted.coverage.missing_bytes, 2 * 4096);
        assert_eq!(
            budgeted.byte_at(0x2000),
            Some(SparseByte::Missing(&MemoryGapReason::Budget))
        );
        assert!(matches!(
            budgeted.read_exact_slice(0x2000, 4).unwrap_err(),
            MemoryError::BudgetExceeded { .. }
        ));

        // Process exit marks the remainder of the capture as ProcessExited.
        let exited = mem
            .read_sparse(0x2000, 3 * 4096, SparseReadOptions::default())
            .unwrap();
        assert_eq!(exited.coverage.observed_bytes, 4096);
        assert_eq!(exited.coverage.missing_bytes, 2 * 4096);
        assert_eq!(
            exited.byte_at(0x3000),
            Some(SparseByte::Missing(&MemoryGapReason::ProcessExited))
        );
        assert_eq!(
            exited.byte_at(0x4500),
            Some(SparseByte::Missing(&MemoryGapReason::ProcessExited))
        );
    }

    // ---------------------------------------------------------------------
    // SparseMemoryCapture partition validation (#1324 review).
    //
    // No property-test crate is a workspace dependency, so the properties below
    // use a hand-rolled deterministic enumerator (every composition of small
    // lengths) plus a seeded xorshift generator for larger lengths and address
    // extremes. Both are reproducible: a failure names the exact case.
    // ---------------------------------------------------------------------

    fn obs(start: u64, bytes: &[u8]) -> MemorySegment {
        MemorySegment::Observed(ObservedMemoryRun {
            start,
            bytes: bytes.to_vec(),
        })
    }

    fn gap(start: u64, len: u64, reason: MemoryGapReason) -> MemorySegment {
        MemorySegment::Gap(MemoryGap { start, len, reason })
    }

    #[test]
    fn sparse_capture_rejects_two_runs_at_the_same_address() {
        // Review counterexample: two one-byte runs at 0x1000 sum to the requested
        // two bytes, but 0x1001 was never observed.
        let err = SparseMemoryCapture::from_segments(
            0x1000,
            2,
            vec![obs(0x1000, &[0xaa]), obs(0x1000, &[0xbb])],
        )
        .expect_err("duplicate runs must not form a complete capture");
        assert_eq!(
            err,
            SparseCaptureError::Overlap {
                index: 1,
                expected: 0x1001,
                start: 0x1000,
            }
        );
    }

    #[test]
    fn sparse_capture_rejects_every_malformed_partition_shape() {
        use MemoryGapReason::Unmapped;
        type Case = (&'static str, u64, u64, Vec<MemorySegment>);
        let cases: Vec<Case> = vec![
            (
                "reversed order",
                0x1000,
                2,
                vec![obs(0x1001, &[2]), obs(0x1000, &[1])],
            ),
            (
                "hole between runs",
                0x1000,
                3,
                vec![obs(0x1000, &[1]), obs(0x1002, &[3])],
            ),
            ("hole at the start", 0x1000, 2, vec![obs(0x1001, &[2])]),
            (
                "overlapping run and gap",
                0x1000,
                4,
                vec![obs(0x1000, &[1, 2, 3]), gap(0x1002, 2, Unmapped)],
            ),
            (
                "segment below the capture",
                0x1000,
                2,
                vec![obs(0x0fff, &[0, 1, 2])],
            ),
            (
                "segment past the capture end",
                0x1000,
                2,
                vec![obs(0x1000, &[1]), gap(0x1001, 5, Unmapped)],
            ),
            (
                "cover stops short (byte sum mismatch)",
                0x1000,
                4,
                vec![obs(0x1000, &[1])],
            ),
            ("no segments for a non-empty request", 0x1000, 4, vec![]),
            (
                "empty observed run",
                0x1000,
                1,
                vec![obs(0x1000, &[]), obs(0x1000, &[1])],
            ),
            (
                "empty gap",
                0x1000,
                1,
                vec![gap(0x1000, 0, Unmapped), obs(0x1000, &[1])],
            ),
            (
                "capture range overflows u64",
                u64::MAX - 1,
                2,
                vec![obs(u64::MAX - 1, &[1]), obs(u64::MAX, &[2])],
            ),
            (
                "gap end overflows u64",
                u64::MAX - 4,
                4,
                vec![gap(u64::MAX - 4, u64::MAX, Unmapped)],
            ),
            (
                "duplicate gap covering the same bytes twice",
                0x1000,
                4,
                vec![gap(0x1000, 2, Unmapped), gap(0x1000, 2, Unmapped)],
            ),
        ];
        for (name, start, len, segments) in cases {
            let result = SparseMemoryCapture::from_segments(start, len, segments);
            assert!(result.is_err(), "{name}: accepted {result:?}");
        }

        // The last byte below u64::MAX is representable; only a range whose
        // exclusive end would be 2^64 is rejected.
        let edge = SparseMemoryCapture::from_segments(
            u64::MAX - 2,
            2,
            vec![obs(u64::MAX - 2, &[7]), gap(u64::MAX - 1, 1, Unmapped)],
        )
        .expect("range ending exactly at u64::MAX is valid");
        assert_eq!(edge.end(), u64::MAX);
        assert_eq!(edge.byte_at(u64::MAX - 2), Some(SparseByte::Observed(7)));
        assert_eq!(
            edge.byte_at(u64::MAX - 1),
            Some(SparseByte::Missing(&Unmapped))
        );
        assert_eq!(edge.byte_at(u64::MAX), None);
    }

    #[test]
    fn sparse_exact_slice_over_huge_gap_fails_without_allocating_the_span() {
        // Gaps are metadata: a capture may describe a span far larger than any
        // allocation. Asking for an exact slice across it must report the gap,
        // not reserve `len` bytes up front.
        let span = 1u64 << 62;
        let capture = SparseMemoryCapture::from_segments(
            0x1000,
            span,
            vec![
                obs(0x1000, &[0xaa; 16]),
                gap(0x1010, span - 16, MemoryGapReason::Unmapped),
            ],
        )
        .unwrap();
        let err = capture
            .read_exact_slice(0x1000, span as usize)
            .expect_err("slice crosses an unmapped gap");
        assert!(
            matches!(
                err,
                MemoryError::Unmapped {
                    address: 0x1010,
                    ..
                }
            ),
            "{err:?}"
        );
        assert_eq!(capture.read_exact_slice(0x1004, 4).unwrap(), [0xaa; 4]);
    }

    /// Per-address reference model of a capture.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum RefByte {
        Observed(u8),
        Missing(MemoryGapReason),
    }

    const REASONS: [MemoryGapReason; 3] = [
        MemoryGapReason::Unmapped,
        MemoryGapReason::Denied,
        MemoryGapReason::Partial,
    ];

    /// Build segments for consecutive `parts` (lengths) with per-part `kinds`
    /// (0 = observed, 1.. = gap reason index + 1), filling observed bytes from a
    /// position-derived pattern that includes genuine zeros.
    fn build_partition(
        start: u64,
        parts: &[u64],
        kinds: &[usize],
    ) -> (Vec<MemorySegment>, Vec<RefByte>) {
        let mut segments = Vec::new();
        let mut reference = Vec::new();
        let mut at = start;
        for (&len, &kind) in parts.iter().zip(kinds) {
            if kind == 0 {
                let bytes: Vec<u8> = (0..len)
                    .map(|i| ((at.wrapping_add(i)).wrapping_mul(37) % 5) as u8)
                    .collect();
                reference.extend(bytes.iter().map(|&b| RefByte::Observed(b)));
                segments.push(obs(at, &bytes));
            } else {
                let reason = REASONS[kind - 1].clone();
                reference.extend((0..len).map(|_| RefByte::Missing(reason.clone())));
                segments.push(gap(at, len, reason));
            }
            at += len;
        }
        (segments, reference)
    }

    /// Check every observable property of a capture against the reference model.
    fn assert_capture_matches_reference(
        case: &str,
        capture: &SparseMemoryCapture,
        start: u64,
        reference: &[RefByte],
    ) {
        let len = reference.len() as u64;
        assert_eq!(capture.start(), start, "{case}");
        assert_eq!(capture.requested_len(), len, "{case}");
        let observed = reference
            .iter()
            .filter(|b| matches!(b, RefByte::Observed(_)))
            .count() as u64;
        let cov = capture.coverage();
        assert_eq!(cov.requested_bytes, len, "{case}");
        assert_eq!(cov.observed_bytes, observed, "{case}");
        assert_eq!(cov.missing_bytes, len - observed, "{case}");
        assert_eq!(cov.complete, observed == len, "{case}");
        assert_eq!(capture.is_complete(), observed == len, "{case}");
        assert_eq!(cov.observed_runs, capture.observed_runs().count(), "{case}");
        assert_eq!(cov.gap_count, capture.gaps().count(), "{case}");

        // Canonical: contiguous, non-empty, maximally coalesced.
        let mut at = start;
        for pair in capture.segments().windows(2) {
            match (&pair[0], &pair[1]) {
                (MemorySegment::Observed(_), MemorySegment::Observed(_)) => {
                    panic!("{case}: adjacent observed runs not coalesced")
                }
                (MemorySegment::Gap(a), MemorySegment::Gap(b)) if a.reason == b.reason => {
                    panic!("{case}: adjacent equal-reason gaps not coalesced")
                }
                _ => {}
            }
        }
        for seg in capture.segments() {
            assert_eq!(seg.start(), at, "{case}: segment not contiguous");
            assert!(!seg.is_empty(), "{case}: empty segment kept");
            at = seg.end();
        }
        assert_eq!(at, capture.end(), "{case}");

        // Per-address queries, including one address on each side.
        if let Some(before) = start.checked_sub(1) {
            assert_eq!(capture.byte_at(before), None, "{case}");
        }
        assert_eq!(capture.byte_at(capture.end()), None, "{case}");
        for (i, expected) in reference.iter().enumerate() {
            let got = capture.byte_at(start + i as u64);
            match expected {
                RefByte::Observed(b) => {
                    assert_eq!(got, Some(SparseByte::Observed(*b)), "{case} @{i}")
                }
                RefByte::Missing(r) => assert_eq!(got, Some(SparseByte::Missing(r)), "{case} @{i}"),
            }
        }

        // Every sub-slice: exact bytes iff fully observed, else a gap error.
        for a in 0..=reference.len() {
            for b in a..=reference.len() {
                let got = capture.read_exact_slice(start + a as u64, b - a);
                let window = &reference[a..b];
                if window.iter().all(|x| matches!(x, RefByte::Observed(_))) {
                    let want: Vec<u8> = window
                        .iter()
                        .map(|x| match x {
                            RefByte::Observed(b) => *b,
                            RefByte::Missing(_) => unreachable!(),
                        })
                        .collect();
                    assert_eq!(got.as_deref(), Ok(&want[..]), "{case} slice {a}..{b}");
                } else {
                    assert!(got.is_err(), "{case} slice {a}..{b} returned {got:?}");
                }
            }
        }

        // Rebuilding from the canonical segments is the identity.
        let rebuilt =
            SparseMemoryCapture::from_segments(start, len, capture.segments().to_vec()).unwrap();
        assert_eq!(&rebuilt, capture, "{case}: not idempotent");
    }

    /// Every single-step corruption of a valid, non-empty partition. Each one is
    /// guaranteed to break the exact-partition invariant.
    fn mutations(segments: &[MemorySegment]) -> Vec<(String, Vec<MemorySegment>)> {
        let mut out = Vec::new();
        for i in 0..segments.len() {
            let mut dup = segments.to_vec();
            dup.insert(i + 1, segments[i].clone());
            out.push((format!("duplicate {i}"), dup));

            let mut drop = segments.to_vec();
            drop.remove(i);
            out.push((format!("drop {i}"), drop));

            let mut grow = segments.to_vec();
            match &mut grow[i] {
                MemorySegment::Observed(run) => run.bytes.push(0),
                MemorySegment::Gap(g) => g.len += 1,
            }
            out.push((format!("grow {i}"), grow));

            for delta in [-1i64, 1] {
                let mut shift = segments.to_vec();
                match &mut shift[i] {
                    MemorySegment::Observed(run) => {
                        run.start = run.start.wrapping_add_signed(delta)
                    }
                    MemorySegment::Gap(g) => g.start = g.start.wrapping_add_signed(delta),
                }
                out.push((format!("shift {i} by {delta}"), shift));
            }

            let mut empty = segments.to_vec();
            empty.insert(i, obs(segments[i].start(), &[]));
            out.push((format!("empty before {i}"), empty));

            if i + 1 < segments.len() {
                let mut swap = segments.to_vec();
                swap.swap(i, i + 1);
                out.push((format!("swap {i},{}", i + 1), swap));
            }
        }
        // Shrinking the last segment leaves the requested end uncovered.
        let last = segments.len() - 1;
        let mut shrink = segments.to_vec();
        let shrinks = match &mut shrink[last] {
            MemorySegment::Observed(run) if run.bytes.len() > 1 => {
                run.bytes.pop();
                true
            }
            MemorySegment::Gap(g) if g.len > 1 => {
                g.len -= 1;
                true
            }
            _ => false,
        };
        if shrinks {
            out.push(("shrink last".to_string(), shrink));
        }
        out
    }

    /// All compositions of `n` into positive parts, in a fixed order.
    fn compositions(n: u64) -> Vec<Vec<u64>> {
        if n == 0 {
            return vec![Vec::new()];
        }
        let mut out = Vec::new();
        for first in 1..=n {
            for mut rest in compositions(n - first) {
                rest.insert(0, first);
                out.push(rest);
            }
        }
        out
    }

    #[test]
    fn sparse_capture_exhaustive_small_partitions_match_reference_and_reject_mutations() {
        let starts = [0u64, 0x1000, 0x1fff, u64::MAX - 6];
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        for &start in &starts {
            for n in 0..=6u64 {
                for parts in compositions(n) {
                    // Every assignment of {observed, 3 gap reasons} to the parts.
                    let k = parts.len() as u32;
                    for code in 0..4usize.pow(k) {
                        let kinds: Vec<usize> =
                            (0..k).map(|j| (code / 4usize.pow(j)) % 4).collect();
                        let (segments, reference) = build_partition(start, &parts, &kinds);
                        let case = format!("start={start:#x} parts={parts:?} kinds={kinds:?}");
                        let capture =
                            SparseMemoryCapture::from_segments(start, n, segments.clone())
                                .unwrap_or_else(|e| {
                                    panic!("{case}: valid partition rejected: {e}")
                                });
                        assert_capture_matches_reference(&case, &capture, start, &reference);
                        accepted += 1;

                        if segments.is_empty() {
                            continue;
                        }
                        for (what, bad) in mutations(&segments) {
                            let result = SparseMemoryCapture::from_segments(start, n, bad.clone());
                            assert!(
                                result.is_err(),
                                "{case}: mutation `{what}` accepted: {bad:?} -> {result:?}"
                            );
                            rejected += 1;
                        }
                    }
                }
            }
        }
        // Guard against the enumerator silently shrinking to nothing.
        assert!(accepted > 4 * 10_000, "accepted only {accepted}");
        assert!(rejected > 4 * 100_000, "rejected only {rejected}");
    }

    /// Deterministic xorshift64* so randomized cases are reproducible.
    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    #[test]
    fn sparse_capture_randomized_large_partitions_match_reference_and_reject_mutations() {
        let mut rng = XorShift(0x1324_5eed_cafe_f00d);
        for iter in 0..400 {
            let n = 1 + rng.below(2048);
            let start = match iter % 4 {
                0 => rng.below(1 << 48),
                1 => u64::MAX - n - rng.below(4),
                2 => 0,
                _ => (rng.below(1 << 36)) << 12,
            };
            let mut parts = Vec::new();
            let mut left = n;
            while left > 0 {
                let p = 1 + rng.below(left.min(300));
                parts.push(p);
                left -= p;
            }
            let kinds: Vec<usize> = parts.iter().map(|_| rng.below(4) as usize).collect();
            let (segments, reference) = build_partition(start, &parts, &kinds);
            let case = format!("iter={iter} start={start:#x} n={n} parts={}", parts.len());
            let capture = SparseMemoryCapture::from_segments(start, n, segments.clone())
                .unwrap_or_else(|e| panic!("{case}: valid partition rejected: {e}"));

            // Full sub-slice enumeration is quadratic; sample it instead.
            assert_eq!(
                capture.coverage().observed_bytes + capture.coverage().missing_bytes,
                n
            );
            for _ in 0..64 {
                let i = rng.below(n) as usize;
                let got = capture.byte_at(start + i as u64);
                match &reference[i] {
                    RefByte::Observed(b) => {
                        assert_eq!(got, Some(SparseByte::Observed(*b)), "{case} @{i}")
                    }
                    RefByte::Missing(r) => {
                        assert_eq!(got, Some(SparseByte::Missing(r)), "{case} @{i}")
                    }
                }
                let j = i + rng.below((n as usize - i) as u64 + 1) as usize;
                let window = &reference[i..j];
                let got = capture.read_exact_slice(start + i as u64, j - i);
                assert_eq!(
                    got.is_ok(),
                    window.iter().all(|x| matches!(x, RefByte::Observed(_))),
                    "{case} slice {i}..{j}"
                );
            }
            for (what, bad) in mutations(&segments) {
                assert!(
                    SparseMemoryCapture::from_segments(start, n, bad).is_err(),
                    "{case}: mutation `{what}` accepted"
                );
            }
        }
    }

    #[test]
    fn read_sparse_over_randomized_backend_failures_yields_exact_partitions() {
        // Property over the producer, not just the constructor: whatever mix of
        // full pages, short transfers, zero transfers, unmapped/denied pages and
        // process exit the backend reports, every observed byte equals target
        // memory, every unobservable byte is a gap, and coverage adds up.
        let mut rng = XorShift(0x0bad_5eed_1324_0001);
        for iter in 0..200 {
            let mut mock = MockPlatformMemory::new();
            let pages = 6u64;
            let base = 0x10_000u64;
            // expected[addr - base] = Some(byte) if a correct backend can observe it.
            let mut expected: Vec<Option<u8>> = Vec::new();
            let mut exited_at: Option<u64> = None;
            for p in 0..pages {
                let page_addr = base + p * 4096;
                match rng.below(6) {
                    0 | 1 => {
                        let page: Vec<u8> = (0..4096)
                            .map(|i| if i % 3 == 0 { 0 } else { (i % 251) as u8 })
                            .collect();
                        expected.extend(page.iter().map(|&b| Some(b)));
                        mock.page_map.insert(page_addr, Ok(page));
                    }
                    2 => {
                        let short = rng.below(4096) as usize;
                        let page = vec![0u8; short];
                        expected.extend((0..4096).map(|i| (i < short).then_some(0u8)));
                        mock.page_map.insert(page_addr, Ok(page));
                    }
                    3 => {
                        expected.extend(std::iter::repeat_n(None, 4096));
                        mock.page_map.insert(
                            page_addr,
                            Err(MemoryError::Unmapped {
                                address: page_addr,
                                size: 4096,
                            }),
                        );
                    }
                    4 => {
                        expected.extend(std::iter::repeat_n(None, 4096));
                        mock.page_map.insert(
                            page_addr,
                            Err(MemoryError::AccessDenied { address: page_addr }),
                        );
                    }
                    _ => {
                        expected.extend(std::iter::repeat_n(None, 4096));
                        mock.page_map.insert(
                            page_addr,
                            Err(MemoryError::ProcessExited { address: page_addr }),
                        );
                    }
                }
            }
            let total = (pages * 4096) as usize;
            let start_off = rng.below(4096) as usize;
            let len = rng.below((total - start_off) as u64) as usize + 1;
            let start = base + start_off as u64;
            // Once the process exits, nothing after it is observable.
            for (i, slot) in expected.iter_mut().enumerate().skip(start_off) {
                if exited_at.is_some() {
                    *slot = None;
                    continue;
                }
                let addr = base + i as u64;
                if let Some(Err(MemoryError::ProcessExited { .. })) =
                    mock.page_map.get(&(addr & !0xfff))
                {
                    exited_at = Some(addr);
                    *slot = None;
                }
            }

            let mem = MemoryManager::with_backend(mock);
            let capture = mem
                .read_sparse(start, len, SparseReadOptions::default())
                .unwrap_or_else(|e| panic!("iter {iter}: read_sparse failed: {e}"));
            let case = format!("iter={iter} start={start:#x} len={len}");
            assert_eq!(capture.requested_len(), len as u64, "{case}");
            let cov = capture.coverage();
            assert_eq!(cov.observed_bytes + cov.missing_bytes, len as u64, "{case}");
            let mut observed = 0u64;
            for i in 0..len {
                let addr = start + i as u64;
                match (capture.byte_at(addr), expected[start_off + i]) {
                    (Some(SparseByte::Observed(b)), Some(want)) => {
                        assert_eq!(b, want, "{case} @{addr:#x}");
                        observed += 1;
                    }
                    (Some(SparseByte::Missing(_)), None) => {}
                    (got, want) => panic!("{case} @{addr:#x}: got {got:?}, want {want:?}"),
                }
            }
            assert_eq!(cov.observed_bytes, observed, "{case}");
            assert_eq!(capture.is_complete(), observed == len as u64, "{case}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_self_process_read_and_sparse_capture_distinguish_zeros_holes_and_denied_pages() {
        unsafe extern "C" {
            fn mmap(
                addr: *mut std::ffi::c_void,
                len: usize,
                prot: i32,
                flags: i32,
                fd: i32,
                offset: i64,
            ) -> *mut std::ffi::c_void;
            fn munmap(addr: *mut std::ffi::c_void, len: usize) -> i32;
            fn mprotect(addr: *mut std::ffi::c_void, len: usize, prot: i32) -> i32;
            fn getpagesize() -> i32;
        }
        const PROT_NONE: i32 = 0;
        const PROT_READ: i32 = 1;
        const PROT_WRITE: i32 = 2;
        const MAP_PRIVATE: i32 = 0x0002;
        const MAP_ANON: i32 = 0x1000;
        const MAP_FAILED: *mut std::ffi::c_void = !0usize as *mut std::ffi::c_void;

        let page = unsafe { getpagesize() } as usize;
        assert!(page >= 4096);

        // Allocate 4 contiguous RW pages:
        // page 0: genuine zeros + sentinel bytes at tail
        // page 1: munmap -> unmapped hole
        // page 2: mprotect(PROT_NONE) -> access denied
        // page 3: nonzero pattern readable after gaps
        let base_ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                page * 4,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANON,
                -1,
                0,
            )
        };
        assert_ne!(base_ptr, MAP_FAILED, "mmap failed");

        struct MmapGuard {
            base: usize,
            page: usize,
            page1_unmapped: bool,
        }
        impl Drop for MmapGuard {
            fn drop(&mut self) {
                unsafe {
                    munmap(self.base as *mut _, self.page);
                    if !self.page1_unmapped {
                        munmap((self.base + self.page) as *mut _, self.page);
                    }
                    munmap((self.base + self.page * 2) as *mut _, self.page);
                    munmap((self.base + self.page * 3) as *mut _, self.page);
                }
            }
        }

        let base = base_ptr as u64;
        let mut guard = MmapGuard {
            base: base_ptr as usize,
            page,
            page1_unmapped: false,
        };

        unsafe {
            let p0 = std::slice::from_raw_parts_mut(base_ptr as *mut u8, page);
            p0.fill(0x00);
            p0[page - 4..].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);

            let p3_ptr = (base_ptr as usize + page * 3) as *mut u8;
            let p3 = std::slice::from_raw_parts_mut(p3_ptr, page);
            p3.fill(0x5a);
            p3[0] = 0x00;

            assert_eq!(munmap((base_ptr as usize + page) as *mut _, page), 0);
            guard.page1_unmapped = true;

            assert_eq!(
                mprotect((base_ptr as usize + page * 2) as *mut _, page, PROT_NONE),
                0
            );
        }

        let mut mem = MemoryManager::new();
        mem.open_process(std::process::id())
            .expect("open self task port");

        // Exact read on genuinely zeroed memory succeeds and preserves zeros.
        let zeros = mem.read(base, 16).expect("read genuine zeros on page 0");
        assert_eq!(zeros, vec![0u8; 16]);
        assert_eq!(mem.read_u32(base).unwrap(), 0);

        // Exact read crossing page 0 -> unmapped page 1 must fail cleanly (no zero padding).
        let boundary_err = mem
            .read(base + page as u64 - 4, 8)
            .expect_err("crossing into unmapped page must fail");
        assert!(
            matches!(
                boundary_err,
                MemoryError::Unmapped { .. }
                    | MemoryError::ShortRead { .. }
                    | MemoryError::ReadFailed { .. }
            ),
            "{boundary_err:?}"
        );

        // Sparse acquisition captures page 0 + page 3 and tags page 1 & page 2 gaps accurately.
        let capture = mem
            .read_sparse(
                base,
                page * 4,
                SparseReadOptions {
                    page_size: page,
                    ..SparseReadOptions::default()
                },
            )
            .expect("sparse read self memory");

        assert_eq!(capture.coverage.requested_bytes, (page * 4) as u64);
        assert_eq!(capture.coverage.observed_bytes, (page * 2) as u64);
        assert_eq!(capture.coverage.missing_bytes, (page * 2) as u64);
        assert_eq!(capture.byte_at(base), Some(SparseByte::Observed(0x00)));
        assert_eq!(
            capture.byte_at(base + page as u64 - 1),
            Some(SparseByte::Observed(0xef))
        );
        assert_eq!(
            capture.byte_at(base + page as u64),
            Some(SparseByte::Missing(&MemoryGapReason::Unmapped))
        );
        assert_eq!(
            capture.byte_at(base + (page * 2) as u64),
            Some(SparseByte::Missing(&MemoryGapReason::Denied))
        );
        assert_eq!(
            capture.byte_at(base + (page * 3) as u64),
            Some(SparseByte::Observed(0x00))
        );
        assert_eq!(
            capture.byte_at(base + (page * 3) as u64 + 1),
            Some(SparseByte::Observed(0x5a))
        );
    }
}

#[cfg(test)]
mod error_message_tests {
    use super::MemoryError;

    #[test]
    fn an_invalid_region_names_its_size_not_its_address_twice() {
        let message = MemoryError::InvalidRegion {
            address: 0x1000,
            size: 0x20,
        }
        .to_string();
        assert!(message.contains("0x1000"), "{message}");
        assert!(message.contains("0x20"), "the size is missing: {message}");
        assert_ne!(
            message.matches("0x1000").count(),
            2,
            "the address is printed twice: {message}"
        );
    }
}
