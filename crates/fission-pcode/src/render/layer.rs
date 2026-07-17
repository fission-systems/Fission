//! NIR vs HIR pseudocode layer contracts.
//!
//! - **NIR**: semantic-faithful mechanical C (oracle / quality-loop input)
//! - **HIR**: human-readable presentation over the same structured tree
//!
//! Semantics live in normalize/structuring; this module only selects presentation.

use serde::{Deserialize, Serialize};

/// Which pseudocode surface to emit (CLI / JSON primary selection).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PseudocodeLayer {
    /// Semantic-faithful mechanical C (default primary `code` for benchmarks).
    #[default]
    Nir,
    /// Human-readable C presentation.
    Hir,
    /// Emit both layers (text sections + JSON dual fields).
    Both,
}

impl PseudocodeLayer {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "nir" | "faithful" | "semantic" => Some(Self::Nir),
            "hir" | "readable" | "pretty" => Some(Self::Hir),
            "both" | "all" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nir => "nir",
            Self::Hir => "hir",
            Self::Both => "both",
        }
    }
}

/// Printer presentation knobs (shared walk, different sugar).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrintProfile {
    /// Keep casts, temps, and structure visible.
    #[default]
    Nir,
    /// Prefer compact C: omit unused noise locals, elide some casts.
    Hir,
}

impl PrintProfile {
    pub fn from_layer(layer: PseudocodeLayer) -> Self {
        match layer {
            PseudocodeLayer::Hir => Self::Hir,
            PseudocodeLayer::Nir | PseudocodeLayer::Both => Self::Nir,
        }
    }
}

/// Dual-layer decompilation strings from one IR build.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LayeredPseudocode {
    /// Semantic-faithful mechanical C.
    pub nir: String,
    /// Human-readable presentation.
    pub hir: String,
}

impl LayeredPseudocode {
    pub fn primary(&self, layer: PseudocodeLayer) -> &str {
        match layer {
            PseudocodeLayer::Nir => &self.nir,
            PseudocodeLayer::Hir => &self.hir,
            // Prefer HIR for human dual view primary; JSON still carries both.
            PseudocodeLayer::Both => &self.hir,
        }
    }

    pub fn format_text(&self, layer: PseudocodeLayer, include_section_headers: bool) -> String {
        match layer {
            PseudocodeLayer::Nir => self.nir.clone(),
            PseudocodeLayer::Hir => self.hir.clone(),
            PseudocodeLayer::Both => {
                if include_section_headers {
                    format!(
                        "// === NIR (semantic-faithful) ===\n{}\n\n// === HIR (readable) ===\n{}",
                        self.nir.trim_end(),
                        self.hir.trim_end()
                    )
                } else {
                    format!("{}\n\n{}", self.nir.trim_end(), self.hir.trim_end())
                }
            }
        }
    }
}
