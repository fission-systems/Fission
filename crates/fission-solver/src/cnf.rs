use crate::aig::AigLit;

/// A Literal in CNF is a signed integer. Positive means non-inverted, negative means inverted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lit(pub i32);

impl Lit {
    pub fn new(var: u32, inverted: bool) -> Self {
        if inverted {
            Lit(-(var as i32))
        } else {
            Lit(var as i32)
        }
    }

    pub fn not(self) -> Self {
        Lit(-self.0)
    }

    pub fn var(self) -> u32 {
        self.0.unsigned_abs()
    }

    /// Returns a 0-indexed integer for this literal: 2 * (var - 1) + sign
    pub fn index(self) -> usize {
        let v = self.var();
        debug_assert!(v > 0);
        let idx = (v - 1) * 2;
        if self.0 < 0 {
            (idx + 1) as usize
        } else {
            idx as usize
        }
    }
}

/// A Clause is a disjunction (OR) of literals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause(pub Vec<Lit>);

/// Converts AIG to CNF using the Tseitin transformation.
pub struct CnfBuilder {
    pub clauses: Vec<Clause>,
    /// Maps AigLit variable indices to CNF variable indices.
    var_map: Vec<u32>,
    next_var: u32,
}

impl Default for CnfBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CnfBuilder {
    pub fn new() -> Self {
        Self {
            clauses: vec![],
            var_map: vec![],
            next_var: 1, // 0 is unused in DIMACS CNF
        }
    }

    fn get_cnf_var(&mut self, aig_idx: u32) -> u32 {
        assert_ne!(
            aig_idx, 0,
            "AIG constant index 0 must not be allocated as a CNF variable"
        );
        while self.var_map.len() <= aig_idx as usize {
            self.var_map.push(0);
        }
        if self.var_map[aig_idx as usize] == 0 {
            self.var_map[aig_idx as usize] = self.next_var;
            self.next_var += 1;
        }
        self.var_map[aig_idx as usize]
    }

    /// Total number of allocated CNF variables (1-indexed variables `1..=num_vars()`).
    pub fn num_vars(&self) -> u32 {
        self.next_var.saturating_sub(1)
    }

    /// Public read-only accessor: returns the CNF variable index for a given AIG node index.
    /// Returns 0 if the AIG node has not been assigned a CNF variable yet.
    pub fn get_cnf_var_for_aig(&self, aig_idx: u32) -> u32 {
        self.var_map.get(aig_idx as usize).copied().unwrap_or(0)
    }

    pub fn get_cnf_lit(&mut self, aig_lit: AigLit) -> Lit {
        let var = self.get_cnf_var(aig_lit.index());
        Lit::new(var, aig_lit.is_inverted())
    }

    /// Add a unit clause asserting a literal is true.
    pub fn assert_lit(&mut self, lit: AigLit) {
        if lit == AigLit::TRUE {
            return;
        }
        if lit == AigLit::FALSE {
            // Trivially UNSAT: add an empty clause (∅ is always false)
            self.clauses.push(Clause(vec![]));
            return;
        }
        let cnf_lit = self.get_cnf_lit(lit);
        self.clauses.push(Clause(vec![cnf_lit]));
    }

    /// Adds constraints for an AND gate: out = a AND b
    /// Tseitin transformation:
    /// (out => a) AND (out => b) AND (a AND b => out)
    /// = (!out OR a) AND (!out OR b) AND (!a OR !b OR out)
    pub fn add_and_gate(&mut self, out_idx: u32, a_lit: AigLit, b_lit: AigLit) {
        let out = Lit::new(self.get_cnf_var(out_idx), false);
        let a = self.get_cnf_lit(a_lit);
        let b = self.get_cnf_lit(b_lit);

        self.clauses.push(Clause(vec![out.not(), a]));
        self.clauses.push(Clause(vec![out.not(), b]));
        self.clauses.push(Clause(vec![a.not(), b.not(), out]));
    }
}

/// Deterministic DIMACS CNF serialization (`p cnf <num_vars> <num_clauses>`).
pub fn to_dimacs_cnf(clauses: &[Clause], num_vars: u32) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "p cnf {num_vars} {}", clauses.len());
    for clause in clauses {
        for lit in &clause.0 {
            let _ = write!(out, "{} ", lit.0);
        }
        out.push_str("0\n");
    }
    out
}

/// Parse a DIMACS CNF string into `(num_vars, clauses)` without relying on any solver state.
pub fn parse_dimacs_cnf(dimacs: &str) -> Result<(u32, Vec<Clause>), String> {
    let mut header: Option<(u32, usize)> = None;
    let mut clauses = Vec::new();
    let mut current: Vec<Lit> = Vec::new();
    let mut in_clause = false;

    for (line_idx, raw_line) in dimacs.lines().enumerate() {
        let line_no = line_idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('c') {
            continue;
        }
        if line.starts_with('p') {
            if header.is_some() {
                return Err(format!("line {line_no}: duplicate problem header"));
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() != 4 || parts[0] != "p" || parts[1] != "cnf" {
                return Err(format!("line {line_no}: malformed problem header `{line}`"));
            }
            let num_vars = parts[2]
                .parse::<u32>()
                .map_err(|e| format!("line {line_no}: invalid variable count: {e}"))?;
            let num_clauses = parts[3]
                .parse::<usize>()
                .map_err(|e| format!("line {line_no}: invalid clause count: {e}"))?;
            header = Some((num_vars, num_clauses));
            clauses.reserve(num_clauses);
            continue;
        }
        let (num_vars, _) =
            header.ok_or_else(|| format!("line {line_no}: clause data before `p cnf` header"))?;
        for tok in line.split_whitespace() {
            let val = tok
                .parse::<i32>()
                .map_err(|e| format!("line {line_no}: invalid literal `{tok}`: {e}"))?;
            if val == 0 {
                clauses.push(Clause(std::mem::take(&mut current)));
                in_clause = false;
            } else {
                let var = val.unsigned_abs();
                if var > num_vars {
                    return Err(format!(
                        "line {line_no}: literal {val} exceeds declared num_vars {num_vars}"
                    ));
                }
                current.push(Lit(val));
                in_clause = true;
            }
        }
    }

    let (num_vars, expected_clauses) =
        header.ok_or_else(|| "missing `p cnf` header".to_string())?;
    if in_clause || !current.is_empty() {
        return Err("unterminated trailing clause without `0`".to_string());
    }
    if clauses.len() != expected_clauses {
        return Err(format!(
            "clause count mismatch: header declared {expected_clauses}, found {}",
            clauses.len()
        ));
    }
    Ok((num_vars, clauses))
}

/// FIPS 180-4 SHA-256 hex digest for binding CNF and proof certificates to exact bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(bytes.len() + 64);
    padded.extend_from_slice(bytes);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut hex = String::with_capacity(64);
    for word in h {
        use std::fmt::Write;
        let _ = write!(hex, "{word:08x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_nist_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn dimacs_roundtrip_and_malformed_rejection() {
        let clauses = vec![
            Clause(vec![Lit::new(1, false), Lit::new(2, true)]),
            Clause(vec![]),
            Clause(vec![Lit::new(3, false)]),
        ];
        let dimacs = to_dimacs_cnf(&clauses, 3);
        let (vars, parsed) = parse_dimacs_cnf(&dimacs).expect("roundtrip dimacs");
        assert_eq!(vars, 3);
        assert_eq!(parsed, clauses);

        let with_comments = "c header comment\np cnf 2 2\n1 -2 0\n2 0\n";
        let (vars2, parsed2) = parse_dimacs_cnf(with_comments).expect("comments dimacs");
        assert_eq!(vars2, 2);
        assert_eq!(
            parsed2,
            vec![
                Clause(vec![Lit::new(1, false), Lit::new(2, true)]),
                Clause(vec![Lit::new(2, false)]),
            ]
        );

        assert!(parse_dimacs_cnf("p cnf 1 1\n2 0\n").is_err());
        assert!(parse_dimacs_cnf("p cnf 1 2\n1 0\n").is_err());
        assert!(parse_dimacs_cnf("p cnf 1 1\n1\n").is_err());
    }
}
