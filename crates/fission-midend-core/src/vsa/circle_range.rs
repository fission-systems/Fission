/// Wrapping-interval (circle) range domain for Value Set Analysis.
///
/// A `CircleRange` over n-bit integers represents a set of values as a
/// contiguous arc on the modular number line Z / 2^n Z:
///
/// ```text
/// [lo, hi)  =  { lo, lo+1, …, hi-1 }  (all arithmetic mod 2^n)
/// ```
///
/// Special cases:
/// - **Top** (all values): represented as `lo == hi == 0`, `size == 0`, `is_top == true`
///   (distinguished from the empty/bottom set via the `is_top` flag)
/// - **Bottom** (no values): `lo == hi == 0`, `is_top == false`
/// - **Singleton** {k}: `lo == k`, `hi == k+1`
///
/// References:
/// - Ghidra `rangeutil.hh`: `CircleRange`, `ValueSet`, `ValueSetSolver`
/// - Reps et al. "Precise Interprocedural Dataflow Analysis via Graph Reachability"
/// - Balakrishnan & Reps "Analyzing Memory Accesses in x86 Executables"

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CircleRange {
    /// Inclusive lower bound (mod 2^bits).
    lo: u64,
    /// Exclusive upper bound (mod 2^bits).  When `lo == hi` AND `is_top`,
    /// the range covers all 2^bits values.
    hi: u64,
    /// Number of bits in the integer domain (8, 16, 32, or 64).
    bits: u32,
    /// True iff this represents the universal set (top).
    is_top: bool,
}

impl CircleRange {
    // ── Constructors ───────────────────────────────────────────────────────

    /// The "no information" element — all values possible.
    #[inline]
    pub fn top(bits: u32) -> Self {
        Self {
            lo: 0,
            hi: 0,
            bits,
            is_top: true,
        }
    }

    /// The empty set — dead / unreachable code.
    #[inline]
    pub fn bottom(bits: u32) -> Self {
        Self {
            lo: 0,
            hi: 0,
            bits,
            is_top: false,
        }
    }

    /// Exactly one value.
    #[inline]
    pub fn singleton(value: u64, bits: u32) -> Self {
        let mask = Self::mask(bits);
        let lo = value & mask;
        let hi = lo.wrapping_add(1) & mask;
        Self {
            lo,
            hi,
            bits,
            is_top: false,
        }
    }

    /// An unsigned interval `[lo, hi)` (mod 2^bits).
    pub fn interval(lo: u64, hi: u64, bits: u32) -> Self {
        let mask = Self::mask(bits);
        let lo = lo & mask;
        let hi = hi & mask;
        if lo == hi {
            // Would be the full range, use top.
            return Self::top(bits);
        }
        Self {
            lo,
            hi,
            bits,
            is_top: false,
        }
    }

    // ── Predicates ─────────────────────────────────────────────────────────

    #[inline]
    pub fn is_top(&self) -> bool {
        self.is_top
    }

    #[inline]
    pub fn is_bottom(&self) -> bool {
        !self.is_top && self.lo == self.hi
    }

    /// True if this range contains exactly one value.
    #[inline]
    pub fn is_singleton(&self) -> bool {
        if self.is_top || self.is_bottom() {
            return false;
        }
        // [lo, lo+1 mod 2^n) is a singleton.
        let mask = Self::mask(self.bits);
        self.hi == (self.lo.wrapping_add(1) & mask)
    }

    /// Return the singleton value if this range has exactly one element.
    pub fn singleton_value(&self) -> Option<u64> {
        if self.is_singleton() {
            Some(self.lo)
        } else {
            None
        }
    }

    /// Number of elements in the range (0 = bottom, 2^n = top).
    pub fn count(&self) -> u128 {
        if self.is_top {
            return 1u128 << self.bits;
        }
        if self.is_bottom() {
            return 0;
        }
        let mask = Self::mask(self.bits);
        let c = self.hi.wrapping_sub(self.lo) & mask;
        c as u128
    }

    /// True if the range is non-empty and has a finite upper bound.
    pub fn upper_bound(&self) -> Option<u64> {
        if self.is_top || self.is_bottom() {
            return None;
        }
        // hi is exclusive; hi - 1 (mod 2^n) is the last value.
        let mask = Self::mask(self.bits);
        Some(self.hi.wrapping_sub(1) & mask)
    }

    pub fn lo(&self) -> u64 {
        self.lo
    }
    pub fn hi(&self) -> u64 {
        self.hi
    }
    pub fn bits(&self) -> u32 {
        self.bits
    }

    // ── Lattice operations ─────────────────────────────────────────────────

    /// Join (union, least upper bound).  Used at phi-nodes and join points.
    pub fn join(&self, other: &Self) -> Self {
        let bits = self.bits.max(other.bits);
        let a = self.with_bits(bits);
        let b = other.with_bits(bits);
        if a.is_top || b.is_top {
            return Self::top(bits);
        }
        if a.is_bottom() {
            return b;
        }
        if b.is_bottom() {
            return a;
        }
        if a == b {
            return a;
        }

        // Compute the minimal arc covering both intervals.
        // Try both orderings and pick the smaller arc.
        let mask = Self::mask(bits);
        let candidate1 = Self::arc_cover(a.lo, a.hi, b.lo, b.hi, mask, bits);
        let candidate2 = Self::arc_cover(b.lo, b.hi, a.lo, a.hi, mask, bits);
        // Pick the smaller; if both are "full", return top.
        let c1 = candidate1.count();
        let c2 = candidate2.count();
        if c1 <= c2 { candidate1 } else { candidate2 }
    }

    /// Meet (intersection, greatest lower bound).  Used for branch narrowing.
    pub fn meet(&self, other: &Self) -> Self {
        let bits = self.bits.max(other.bits);
        let a = self.with_bits(bits);
        let b = other.with_bits(bits);
        if a.is_bottom() || b.is_bottom() {
            return Self::bottom(bits);
        }
        if a.is_top {
            return b;
        }
        if b.is_top {
            return a;
        }
        if a == b {
            return a;
        }

        // Intersect two arcs on the circle.
        let mask = Self::mask(bits);
        // Check if a.lo is in b and if b.lo is in a.
        let a_lo_in_b = b.contains_mod(a.lo, mask);
        let b_lo_in_a = a.contains_mod(b.lo, mask);
        match (a_lo_in_b, b_lo_in_a) {
            (false, false) => Self::bottom(bits), // disjoint
            (true, true) => {
                // Overlapping; result = max(lo)..min(hi).
                // We pick whichever start produces the smaller arc.
                let r1 = Self::interval(a.lo, b.hi, bits);
                let r2 = Self::interval(b.lo, a.hi, bits);
                if r1.count() <= r2.count() { r1 } else { r2 }
            }
            (true, false) => {
                // b.lo not in a but a.lo in b → result is [a.lo, b.hi)
                Self::interval(a.lo, b.hi, bits)
            }
            (false, true) => {
                // a.lo not in b but b.lo in a → result is [b.lo, a.hi)
                Self::interval(b.lo, a.hi, bits)
            }
        }
    }

    /// Widening operator: if the new range is strictly larger than the old
    /// (lattice height increased), jump to top to guarantee termination.
    pub fn widen(&self, prev: &Self) -> Self {
        if prev.is_top {
            return *prev;
        }
        if prev.is_bottom() {
            return *self;
        }
        if self.count() <= prev.count() {
            return *self;
        }
        // The set grew — widen to top.
        Self::top(self.bits)
    }

    // ── Arithmetic transfer functions ──────────────────────────────────────

    /// `a + b` (unsigned, wrapping).
    pub fn add(&self, other: &Self) -> Self {
        if self.is_top || other.is_top {
            return Self::top(self.bits);
        }
        if self.is_bottom() || other.is_bottom() {
            return Self::bottom(self.bits);
        }
        let mask = Self::mask(self.bits);
        let lo = self.lo.wrapping_add(other.lo) & mask;
        let hi = self.hi.wrapping_add(other.hi).wrapping_sub(1) & mask;
        // If lo == hi after wrapping, conservative top.
        if lo == hi {
            Self::top(self.bits)
        } else {
            Self::interval(lo, hi.wrapping_add(1) & mask, self.bits)
        }
    }

    /// `a - b` (unsigned, wrapping).
    pub fn sub(&self, other: &Self) -> Self {
        if self.is_top || other.is_top {
            return Self::top(self.bits);
        }
        if self.is_bottom() || other.is_bottom() {
            return Self::bottom(self.bits);
        }
        let mask = Self::mask(self.bits);
        let lo = self.lo.wrapping_sub(other.hi).wrapping_add(1) & mask;
        let hi = self.hi.wrapping_sub(other.lo) & mask;
        if lo == hi {
            Self::top(self.bits)
        } else {
            Self::interval(lo, hi, self.bits)
        }
    }

    /// Logical right-shift by a constant `k`.
    pub fn shr_const(&self, k: u32) -> Self {
        if self.is_top {
            return Self::top(self.bits);
        }
        if self.is_bottom() {
            return Self::bottom(self.bits);
        }
        if k >= self.bits {
            return Self::singleton(0, self.bits);
        }
        let mask = Self::mask(self.bits);
        // Conservative: shift lo and hi.
        let lo = self.lo >> k;
        let hi = (self.hi.wrapping_sub(1) & mask) >> k;
        if lo <= hi {
            Self::interval(lo, hi.wrapping_add(1), self.bits)
        } else {
            Self::top(self.bits) // wrapped — conservative
        }
    }

    /// Bitwise AND with a constant mask.
    pub fn and_const(&self, mask_val: u64) -> Self {
        if self.is_bottom() {
            return Self::bottom(self.bits);
        }
        let mask = Self::mask(self.bits);
        let m = mask_val & mask;
        // Result is always in [0, m+1) — a conservative upper bound.
        Self::interval(0, m.wrapping_add(1) & mask.wrapping_add(1), self.bits)
    }

    /// Zero-extend or truncate to `new_bits`.
    pub fn cast_unsigned(&self, new_bits: u32) -> Self {
        if self.is_top {
            return Self::top(new_bits);
        }
        if self.is_bottom() {
            return Self::bottom(new_bits);
        }
        let new_mask = Self::mask(new_bits);
        // If this range fits in the new width without wrapping, preserve it.
        let lo_fits = self.lo <= new_mask;
        let hi_fits = {
            let hi_minus1 = self.hi.wrapping_sub(1) & Self::mask(self.bits);
            hi_minus1 <= new_mask
        };
        if lo_fits && hi_fits && self.lo <= (self.hi.wrapping_sub(1) & Self::mask(self.bits)) {
            Self::interval(self.lo, self.hi, new_bits)
        } else {
            Self::top(new_bits)
        }
    }

    // ── Helpers ────────────────────────────────────────────────────────────

    fn mask(bits: u32) -> u64 {
        if bits >= 64 {
            u64::MAX
        } else {
            (1u64 << bits).wrapping_sub(1)
        }
    }

    fn with_bits(&self, bits: u32) -> Self {
        if bits == self.bits {
            return *self;
        }
        if self.is_top {
            return Self::top(bits);
        }
        if self.is_bottom() {
            return Self::bottom(bits);
        }
        // Reinterpret lo/hi in the new width.
        let mask = Self::mask(bits);
        Self {
            lo: self.lo & mask,
            hi: self.hi & mask,
            bits,
            is_top: false,
        }
    }

    /// True if `v` is in the arc `[lo, hi)` mod 2^n.
    fn contains_mod(&self, v: u64, mask: u64) -> bool {
        let lo = self.lo & mask;
        let hi = self.hi & mask;
        let v = v & mask;
        if lo <= hi {
            lo <= v && v < hi
        } else {
            // Wrapped arc: lo..MAX ∪ 0..hi
            v >= lo || v < hi
        }
    }

    /// Smallest arc covering both `[a_lo, a_hi)` and `[b_lo, b_hi)` starting at `a_lo`.
    fn arc_cover(a_lo: u64, a_hi: u64, b_lo: u64, b_hi: u64, mask: u64, bits: u32) -> Self {
        // If b_hi is "further" from a_lo than a_hi, extend to b_hi.
        let a_span = a_hi.wrapping_sub(a_lo) & mask;
        let b_end_from_a = b_hi.wrapping_sub(a_lo) & mask;
        let new_hi = if b_end_from_a > a_span { b_hi } else { a_hi };
        let new_span = new_hi.wrapping_sub(a_lo) & mask;
        // Also check b_lo is inside.
        let b_start_from_a = b_lo.wrapping_sub(a_lo) & mask;
        let final_span = new_span.max(b_start_from_a.wrapping_add(1) & mask);
        let final_hi = a_lo.wrapping_add(final_span) & mask;
        if final_span == 0 {
            return Self::top(bits);
        }
        Self::interval(a_lo, final_hi, bits)
    }
}
