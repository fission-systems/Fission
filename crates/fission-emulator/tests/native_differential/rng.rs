//! A seeded generator, so a case can be rebuilt from `(seed, case id)` alone.

/// SplitMix64: small, fast, and every output depends on the whole seed.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// A generator for one case, independent of how many cases ran before it,
    /// so a longer nightly run replays every case a pull request ran.
    pub fn for_case(seed: u64, id: &str, index: u64) -> Self {
        let mut h = seed ^ 0x9E37_79B9_7F4A_7C15;
        for byte in id.bytes() {
            h = (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3);
        }
        let mut rng = Self(h ^ index.wrapping_mul(0xD1B5_4A32_D192_ED03));
        rng.next_u64();
        rng
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }

    pub fn fill(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let v = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
}
