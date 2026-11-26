//! Content digests, the only identity in these contracts that is not a name.

use serde::{Deserialize, Serialize};

const PREFIX: &str = "blake3:";
const HEX_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DigestError {
    #[error("digest must be `blake3:` followed by 64 lowercase hex digits, got {0:?}")]
    Malformed(String),
}

/// A BLAKE3 digest written as `blake3:<64 lowercase hex digits>`.
///
/// The algorithm is part of the value so that a stored digest is never compared
/// with one made by another function.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContentDigest(String);

impl ContentDigest {
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{PREFIX}{}", blake3::hash(bytes).to_hex()))
    }

    /// Wrap a bare BLAKE3 hex string, the form the loader records for a binary.
    pub fn from_blake3_hex(hex: &str) -> Result<Self, DigestError> {
        Self::try_from(format!("{PREFIX}{hex}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Digest of a contract value's canonical JSON, separated by `domain` so
    /// that two kinds of value with the same encoding cannot collide.
    ///
    /// Canonical because every contract type serializes its fields in declared
    /// order and holds only ordered collections.
    pub(crate) fn of_canonical<T: Serialize>(domain: &str, value: &T) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(domain.as_bytes());
        hasher.update(&[0]);
        // The contract types hold strings, integers and ordered collections, so
        // serialization into a hasher cannot fail.
        serde_json::to_writer(&mut hasher, value).expect("contract values serialize");
        Self(format!("{PREFIX}{}", hasher.finalize().to_hex()))
    }
}

impl TryFrom<String> for ContentDigest {
    type Error = DigestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let well_formed = value.strip_prefix(PREFIX).is_some_and(|hex| {
            hex.len() == HEX_LEN && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        });
        if well_formed {
            Ok(Self(value))
        } else {
            Err(DigestError::Malformed(value))
        }
    }
}

impl From<ContentDigest> for String {
    fn from(value: ContentDigest) -> Self {
        value.0
    }
}

impl std::fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_digest_is_rejected_on_input() {
        let good = ContentDigest::of(b"x");
        assert_eq!(ContentDigest::try_from(good.as_str().to_string()), Ok(good));
        for bad in ["", "blake3:", "blake3:ABC", "sha256:00", "abc"] {
            assert!(
                serde_json::from_value::<ContentDigest>(bad.into()).is_err(),
                "{bad}"
            );
        }
        let upper = format!("{PREFIX}{}", "A".repeat(HEX_LEN));
        assert!(ContentDigest::try_from(upper).is_err());
    }
}
