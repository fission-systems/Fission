//! Static DIE rule model.

use serde::{Deserialize, Serialize};

/// A single statically matchable rule within a DIE signature.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SignatureRule {
    #[serde(rename = "section_name")]
    SectionName { name: String },

    #[serde(rename = "string")]
    StringMatch { value: String },

    #[serde(rename = "ep_pattern")]
    EpPattern {
        arch: Option<String>,
        pattern: String,
        #[serde(default)]
        offset: Option<usize>,
    },

    #[serde(rename = "file_pattern")]
    FilePattern {
        pattern: String,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        from_end: bool,
    },

    #[serde(rename = "overlay_pattern")]
    OverlayPattern {
        pattern: String,
        #[serde(default)]
        offset: Option<usize>,
    },

    #[serde(rename = "overlay_present")]
    OverlayPresent { present: bool },

    #[serde(rename = "section_count")]
    SectionCount { op: CompareOp, value: usize },

    #[serde(rename = "section_numeric")]
    SectionNumeric {
        selector: SectionSelector,
        field: SectionNumericField,
        op: CompareOp,
        value: u64,
    },

    #[serde(rename = "section_entropy")]
    SectionEntropy {
        selector: SectionSelector,
        op: CompareOp,
        value: f64,
    },

    #[serde(rename = "overlay_entropy")]
    OverlayEntropy { op: CompareOp, value: f64 },

    #[serde(rename = "import")]
    Import { function: String },

    #[serde(rename = "rich_header")]
    RichHeader { present: bool },
}

/// A complete DIE signature after static primitive extraction.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Signature {
    pub name: String,
    #[serde(rename = "type")]
    pub sig_type: String,
    pub rules: Vec<SignatureRule>,
    #[serde(default)]
    pub source_format: Option<String>,
    #[serde(default)]
    pub source_file: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unsupported_rule_count: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SectionSelector {
    Index(usize),
    Last,
    Name(String),
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SectionNumericField {
    FileSize,
    VirtualSize,
    FileOffset,
}
