//! The runtime trace artifact: what one execution reported, before any of it
//! is attributed to an image.
//!
//! A trace is the interchange form between a producer that ran something and a
//! consumer that analyses a binary. It stores addresses and the bytes seen at
//! them, so attribution happens at import and can be refused there.

use crate::digest::ContentDigest;
use crate::evidence::Producer;
use crate::identity::ImageIdentity;
use crate::location::DecodeContext;
use crate::runtime::{Mapping, RunId};
use serde::{Deserialize, Serialize};

pub const RUNTIME_TRACE_SCHEMA: &str = "fission-runtime-trace-v1";

/// Bytes written as lowercase hex.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexBytes(pub Vec<u8>);

impl TryFrom<String> for HexBytes {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let digit = |c: u8| match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        };
        let raw = text.as_bytes();
        if raw.len() % 2 != 0 {
            return Err(format!("odd number of hex digits: {text:?}"));
        }
        raw.chunks_exact(2)
            .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
            .collect::<Option<Vec<u8>>>()
            .map(Self)
            .ok_or_else(|| format!("not lowercase hex: {text:?}"))
    }
}

impl From<HexBytes> for String {
    fn from(bytes: HexBytes) -> Self {
        bytes.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    IndirectCall,
    IndirectJump,
}

/// One distinct indirect transfer and how often it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedTransfer {
    pub kind: TransferKind,
    pub process: u32,
    pub thread: u32,
    pub epoch: u64,
    /// Address of the transferring instruction, and its bytes.
    pub site: u64,
    pub site_bytes: HexBytes,
    /// Where control went, and the bytes seen there (empty if unreadable).
    pub target: u64,
    pub target_bytes: HexBytes,
    /// Decoder context of the site.
    #[serde(default)]
    pub decode: DecodeContext,
    pub count: u64,
}

/// Whether the trace holds everything its producer was asked to record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completeness {
    /// The guest ran to its own exit.
    pub guest_finished: bool,
    /// Why recording ended, in the producer's words.
    pub stop_reason: String,
    /// Transfers that ran but were not recorded because of a limit.
    pub dropped_transfers: u64,
}

impl Completeness {
    /// Even a complete trace covers one execution only: it never says which
    /// transfers *could* happen.
    pub fn is_complete(&self) -> bool {
        self.guest_finished && self.dropped_transfers == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTrace {
    pub schema: String,
    pub producer: Producer,
    /// The image the producer was asked to run.
    pub image: ImageIdentity,
    /// Digest of what the run was given: arguments, input and budget. The
    /// values themselves are not stored.
    pub inputs: ContentDigest,
    pub mappings: Vec<Mapping>,
    /// In order of first occurrence.
    pub transfers: Vec<ObservedTransfer>,
    pub completeness: Completeness,
}

#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    #[error("trace schema is {found:?}; this build reads {RUNTIME_TRACE_SCHEMA:?}")]
    Schema { found: String },
    #[error("trace is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl RuntimeTrace {
    /// The run this trace records, named by its whole content: replaying an
    /// execution that does the same things yields the same run.
    pub fn run_id(&self) -> RunId {
        RunId(ContentDigest::of_canonical(
            "fission-runtime-trace-run-v1",
            self,
        ))
    }

    pub fn to_json(&self) -> Result<String, TraceError> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }

    pub fn from_json(text: &str) -> Result<Self, TraceError> {
        #[derive(Deserialize)]
        struct Header {
            schema: String,
        }
        let header: Header = serde_json::from_str(text)?;
        if header.schema != RUNTIME_TRACE_SCHEMA {
            return Err(TraceError::Schema {
                found: header.schema,
            });
        }
        Ok(serde_json::from_str(text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_bytes_round_trip_and_reject_other_forms() {
        let bytes = HexBytes(vec![0x00, 0xff, 0x1a]);
        let text = String::from(bytes.clone());
        assert_eq!(text, "00ff1a");
        assert_eq!(HexBytes::try_from(text), Ok(bytes));
        for bad in ["0", "0G", "FF", "é0"] {
            assert!(HexBytes::try_from(bad.to_string()).is_err(), "{bad}");
        }
    }
}
