use super::PostProcessor;
use crate::utils::patterns::*;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

impl PostProcessor {
    /// Try to recover a divisor from a magic number multiplication and shift
    /// Based on algorithms from RetDec and angr
    fn recover_divisor(&self, magic: u64, shift: u32, is_64bit_mul: bool) -> Option<u64> {
        let base_bits = if is_64bit_mul { 64 } else { 32 };

        let pow_val = (base_bits as u64) + (shift as u64);
        if pow_val >= 128 {
            return None;
        }

        let dividend = if pow_val < 64 {
            1u128 << pow_val
        } else {
            1u128 << pow_val
        };

        let divisor = (dividend / (magic as u128)) as u64;

        let test_cases = [100u64, 1000, 10000];
        for &x in &test_cases {
            let expected = x / (divisor.max(1));
            let actual = (((x as u128 * magic as u128) >> base_bits) >> shift) as u64;
            if expected != actual {
                if x / (divisor + 1) == actual {
                    return Some(divisor + 1);
                }
                return None;
            }
        }

        Some(divisor)
    }

    /// Apply arithmetic idiom recovery
    /// Simplifies common compiler-generated bit-twiddling patterns
    pub(super) fn apply_arithmetic_idioms(&self, code: &str) -> String {
        let mut result = code.to_string();

        result = SIGN_EXT_INT64_CONCAT
            .replace_all(&result, |caps: &regex::Captures| {
                let sign = &caps["s1"];
                let md = &caps["m1"];
                let low = &caps["low"];

                if sign == &caps["s2"]
                    && sign == &caps["s3"]
                    && sign == &caps["s4"]
                    && sign == &caps["s5"]
                    && sign == &caps["s6"]
                    && md == &caps["m2"]
                    && md == &caps["m3"]
                {
                    format!("return (longlong){} % 2;", low)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = SIGN_EXT_INT32
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let sign = &caps["s1"];
                let out = &caps["out"];

                if sign == &caps["s2"] && sign == &caps["s3"] {
                    format!("{} = abs({}); // sign: {}", out, val, sign)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = ALIGNED_DIV
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let sign = &caps["s1"];
                let shift = &caps["sh1"];

                if sign == &caps["s2"] && shift == &caps["sh2"] {
                    let mask: u64 = if caps["mask"].starts_with("0x") {
                        u64::from_str_radix(&caps["mask"][2..], 16).unwrap_or(0)
                    } else {
                        caps["mask"].parse().unwrap_or(0)
                    };
                    format!("({} % {})", val, mask + 1)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = SIGN_BIT
            .replace_all(&result, |caps: &regex::Captures| {
                format!("SIGN_EXTRACT({})", &caps[1])
            })
            .to_string();

        result = MAGIC_DIV
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let magic_str = &caps["magic"];
                let magic: u64 = u64::from_str_radix(&magic_str[2..], 16).unwrap_or(0);
                let shift: u32 = caps
                    .name("shift")
                    .map(|s| {
                        if s.as_str().starts_with("0x") {
                            u32::from_str_radix(&s.as_str()[2..], 16).unwrap_or(0)
                        } else {
                            s.as_str().parse().unwrap_or(0)
                        }
                    })
                    .unwrap_or(0);

                if let Some(divisor) = self.recover_divisor(magic, shift, false) {
                    format!("({} / {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = CONCAT44_SIGN
            .replace_all(&result, |caps: &regex::Captures| {
                let hi = &caps["hi"];
                let lo = &caps["lo"];
                if hi == lo {
                    format!("(longlong){}", lo)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = CONCAT44_ZERO
            .replace_all(&result, |caps: &regex::Captures| {
                let lo = caps["lo"].trim();
                format!("(ulonglong){}", lo)
            })
            .to_string();

        result = CONCAT_INPUT_FIRST
            .replace_all(&result, |caps: &regex::Captures| {
                caps["real1"].trim().to_string()
            })
            .to_string();
        result = CONCAT_INPUT_SECOND
            .replace_all(&result, |caps: &regex::Captures| {
                caps["real2"].trim().to_string()
            })
            .to_string();

        result = CONCAT_CAP_INPUT
            .replace_all(&result, |caps: &regex::Captures| {
                caps["lo_val"].trim().to_string()
            })
            .to_string();

        result = MODULO_TO_SUB
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let v2 = &caps["v2"];
                if val == v2 {
                    let divisor = &caps["divisor"];
                    format!("({} % {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = UNSIGNED_RSHIFT
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let sh: u32 = caps["sh"].parse().unwrap_or(0);
                if sh > 0 && sh < 32 {
                    let divisor = 1u64 << sh;
                    format!("({} / {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = LEFT_SHIFT
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let sh: u32 = caps["sh"].parse().unwrap_or(0);
                if sh > 0
                    && sh < 32
                    && val != "if"
                    && val != "while"
                    && val != "for"
                    && val != "return"
                    && val != "int"
                    && val != "uint"
                {
                    let multiplier = 1u64 << sh;
                    format!("{} * {}", val, multiplier)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = BITWISE_AND_MASK
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let mask_str = &caps["mask"];
                let mask = u64::from_str_radix(&mask_str[2..], 16).unwrap_or(0);
                let modulus = mask.wrapping_add(1);
                if mask > 0
                    && modulus > 0
                    && modulus.is_power_of_two()
                    && val != "if"
                    && val != "while"
                    && val != "return"
                {
                    format!("({} % {})", val, modulus)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        static SIGNED_MAGIC_DIV: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"\(int\)\s*\(\s*\(longlong\)\s*(?P<val>[\w\->\.\*]+)\s*\*\s*",
                r"(?P<magic>0x[0-9a-fA-F]+)\s*>>\s*0x20\s*\)\s*",
                r"(?:>>\s*(?P<shift>0x[0-9a-fA-F]+|\d+)\s*)?",
                r"\+\s*\(\s*(?P<v2>[\w\->\.\*]+)\s*>>\s*0x1[fF]\s*\)",
            ))
            .unwrap_or_else(|e| panic!("SIGNED_MAGIC_DIV regex should compile: {}", e))
        });

        result = SIGNED_MAGIC_DIV
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let v2 = &caps["v2"];
                if val != v2 {
                    return caps[0].to_string();
                }
                let magic_str = &caps["magic"];
                let magic = u64::from_str_radix(&magic_str[2..], 16).unwrap_or(0);
                let shift: u32 = caps
                    .name("shift")
                    .map(|s| {
                        let s = s.as_str();
                        if s.starts_with("0x") {
                            u32::from_str_radix(&s[2..], 16).unwrap_or(0)
                        } else {
                            s.parse().unwrap_or(0)
                        }
                    })
                    .unwrap_or(0);

                if let Some(divisor) = self.recover_divisor(magic, shift, false) {
                    format!("({} / {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        static SIGNED_MAGIC_DIV_FIXUP: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"\(\s*\(longlong\)\s*(?P<val>[\w\->\.\*]+)\s*\*\s*",
                r"(?P<magic>0x[0-9a-fA-F]+)\s*>>\s*0x20\s*",
                r"\+\s*(?P<v2>[\w\->\.\*]+)\s*\)\s*",
                r">>\s*(?P<shift>0x[0-9a-fA-F]+|\d+)\s*",
                r"\+\s*\(\s*(?P<v3>[\w\->\.\*]+)\s*>>\s*0x1[fF]\s*\)",
            ))
            .unwrap_or_else(|e| panic!("SIGNED_MAGIC_DIV_FIXUP regex should compile: {}", e))
        });

        result = SIGNED_MAGIC_DIV_FIXUP
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let v2 = &caps["v2"];
                let v3 = &caps["v3"];
                if val != v2 || val != v3 {
                    return caps[0].to_string();
                }
                let magic = u64::from_str_radix(&caps["magic"][2..], 16).unwrap_or(0);
                let shift: u32 = {
                    let s = &caps["shift"];
                    if s.starts_with("0x") {
                        u32::from_str_radix(&s[2..], 16).unwrap_or(0)
                    } else {
                        s.parse().unwrap_or(0)
                    }
                };

                let effective_magic = magic.wrapping_add(1u64 << 32);
                if let Some(divisor) = self.recover_divisor(effective_magic, shift, false) {
                    format!("({} / {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result = XOR_SIGN_BIT
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                format!("-{}", val)
            })
            .to_string();

        result
    }

    pub(super) fn apply_arithmetic_idioms_cow<'a>(&self, code: &'a str) -> Cow<'a, str> {
        if !code.contains("CONCAT")
            && !code.contains(">>")
            && !code.contains("<<")
            && !code.contains('&')
        {
            return Cow::Borrowed(code);
        }

        let output = self.apply_arithmetic_idioms(code);
        if output == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(output)
        }
    }

    /// B-9: Replace multiplication by power-of-2 with left shift, but ONLY when
    /// the line is in a bitwise context (contains >> or |).
    /// e.g. `x * 256 | y` → `x << 8 | y`, `x | y * 0x100` → `x | y << 8`
    /// Does NOT apply to pure arithmetic contexts like `x * 8 + x / 10`.
    pub(super) fn mul_pow2_to_shift(code: &str) -> String {
        fn parse_int(s: &str) -> Option<u64> {
            let s = s.trim();
            if s.starts_with("0x") || s.starts_with("0X") {
                u64::from_str_radix(&s[2..], 16).ok()
            } else {
                s.parse().ok()
            }
        }

        let mut changed = false;
        let result: String = code
            .lines()
            .map(|line| {
                // Only apply transformation in bitwise context
                if !BITWISE_CTX.is_match(line) {
                    return line.to_string();
                }
                let new_line = MULT_CONTEXT.replace_all(line, |caps: &regex::Captures| {
                    let numstr = caps[1].trim();
                    if let Some(v) = parse_int(numstr) {
                        // Only replace powers of 2 in range [4, 2^24]
                        if v >= 4 && v <= (1 << 24) && (v & (v - 1)) == 0 {
                            let shift = v.trailing_zeros();
                            changed = true;
                            return format!("<< {}", shift);
                        }
                    }
                    caps[0].to_string()
                });
                new_line.into_owned()
            })
            .collect::<Vec<_>>()
            .join("\n");

        if !changed {
            return code.to_string();
        }
        result
    }
}
