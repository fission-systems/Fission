//! Shannon entropy helpers for identity hardening signals.

pub fn shannon_entropy(bytes: &[u8]) -> f32 {
    if bytes.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let len = bytes.len() as f64;
    let mut h = 0.0_f64;
    for &c in &counts {
        if c == 0 {
            continue;
        }
        let p = (c as f64) / len;
        h -= p * p.log2();
    }
    h as f32
}

#[must_use]
pub fn classify_executable_entropy(entropy: f32) -> &'static str {
    if entropy >= 7.2 {
        "high_entropy"
    } else if entropy >= 6.8 {
        "elevated_entropy"
    } else {
        "normal_code"
    }
}
