//! Lane manifests, target resolution, and per-lane runners.

pub mod nir_check;
pub mod source_semantic_check;

use crate::model::{LaneTarget, SourceMeta};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Manifest {
    lane: BTreeMap<String, LaneDefinition>,
}

#[derive(Debug, Deserialize)]
struct LaneDefinition {
    #[serde(default)]
    binaries: Vec<LaneBinary>,
    #[serde(default)]
    include_source_inventory_aligned_high: bool,
}

#[derive(Debug, Deserialize)]
struct LaneBinary {
    binary: String,
    path: String,
    role: String,
    default_functions_limit: Option<usize>,
    default_timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SourceInventoryFile {
    #[serde(default)]
    sources: Vec<SourceMeta>,
}

pub fn default_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("sentinel_sets.toml")
}

pub fn normalize_lane_name(lane_name: &str) -> (&str, bool) {
    match lane_name {
        "preview" => ("nir", true),
        other => (other, false),
    }
}

pub fn default_source_inventory_path() -> Option<PathBuf> {
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("preview_explicit_source_inventory.json");
    candidate.exists().then_some(candidate)
}

pub fn load_source_inventory(path: &Path) -> Result<BTreeMap<String, SourceMeta>> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("read source inventory {}", path.display()))?;
    let parsed: SourceInventoryFile = serde_json::from_str(&data)
        .with_context(|| format!("parse source inventory {}", path.display()))?;
    let mut inventory = BTreeMap::new();
    for source in parsed.sources {
        if !source.path.is_empty() {
            let resolved = PathBuf::from(&source.path)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(&source.path));
            inventory.insert(resolved.to_string_lossy().to_string(), source.clone());
            if let Some(name) = resolved.file_name().and_then(|v| v.to_str()) {
                inventory.insert(name.to_string(), source.clone());
            }
            if let Some(stem) = resolved.file_stem().and_then(|v| v.to_str()) {
                inventory.insert(stem.to_string(), source.clone());
            }
        }
        if !source.binary.is_empty() {
            inventory.insert(source.binary.clone(), source.clone());
            if let Some(stem) = Path::new(&source.binary)
                .file_stem()
                .and_then(|v| v.to_str())
            {
                inventory.insert(stem.to_string(), source.clone());
            }
        }
    }
    Ok(inventory)
}

pub fn resolve_source_meta<'a>(
    inventory: &'a BTreeMap<String, SourceMeta>,
    binary_path: &Path,
) -> Option<&'a SourceMeta> {
    let resolved = binary_path
        .canonicalize()
        .unwrap_or_else(|_| binary_path.to_path_buf())
        .to_string_lossy()
        .to_string();
    inventory
        .get(&resolved)
        .or_else(|| {
            binary_path
                .file_name()
                .and_then(|v| v.to_str())
                .and_then(|name| inventory.get(name))
        })
        .or_else(|| {
            binary_path
                .file_stem()
                .and_then(|v| v.to_str())
                .and_then(|stem| inventory.get(stem))
        })
}

fn is_high_aligned(source: &SourceMeta) -> bool {
    source.admission_alignment.as_deref() == Some("aligned")
        && matches!(source.rescan_priority.as_deref(), Some("high"))
}

pub fn resolve_lane_targets(
    root: &Path,
    manifest_path: &Path,
    lane_name: &str,
    source_inventory: Option<&BTreeMap<String, SourceMeta>>,
) -> Result<Vec<LaneTarget>> {
    let data = fs::read_to_string(manifest_path)
        .with_context(|| format!("read sentinel manifest {}", manifest_path.display()))?;
    let manifest: Manifest = toml::from_str(&data)
        .with_context(|| format!("parse sentinel manifest {}", manifest_path.display()))?;
    let (normalized_lane_name, _) = normalize_lane_name(lane_name);
    let lane = manifest
        .lane
        .get(normalized_lane_name)
        .with_context(|| format!("unknown lane `{lane_name}` in {}", manifest_path.display()))?;

    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();

    for binary in &lane.binaries {
        let path = root.join(&binary.path);
        let resolved = path.canonicalize().unwrap_or(path);
        let key = resolved.to_string_lossy().to_string();
        if !seen.insert(key) {
            continue;
        }
        targets.push(LaneTarget {
            binary: binary.binary.clone(),
            path: resolved,
            role: binary.role.clone(),
            default_functions_limit: binary.default_functions_limit,
            default_timeout_ms: binary.default_timeout_ms,
        });
    }

    if lane.include_source_inventory_aligned_high {
        if let Some(source_inventory) = source_inventory {
            let mut extras = Vec::new();
            for source in source_inventory.values() {
                if source.path.is_empty() || !is_high_aligned(source) {
                    continue;
                }
                let resolved = PathBuf::from(&source.path)
                    .canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(&source.path));
                let key = resolved.to_string_lossy().to_string();
                if !seen.insert(key) {
                    continue;
                }
                extras.push(LaneTarget {
                    binary: if source.binary.is_empty() {
                        resolved
                            .file_name()
                            .and_then(|v| v.to_str())
                            .unwrap_or("unknown")
                            .to_string()
                    } else {
                        source.binary.clone()
                    },
                    path: resolved,
                    role: "aligned_high_priority_source".to_string(),
                    default_functions_limit: Some(20),
                    default_timeout_ms: Some(1500),
                });
            }
            extras.sort_by(|a, b| a.binary.cmp(&b.binary));
            targets.extend(extras);
        }
    }

    Ok(targets)
}

/// Fail fast if any lane manifest binary path is missing (before long inventory runs).
pub fn validate_lane_target_paths(targets: &[LaneTarget]) -> Result<()> {
    for target in targets {
        if !target.path.exists() {
            anyhow::bail!(
                "lane manifest path does not exist: {} (binary `{}`)",
                target.path.display(),
                target.binary
            );
        }
        if !target.path.is_file() {
            anyhow::bail!(
                "lane manifest path is not a file: {} (binary `{}`)",
                target.path.display(),
                target.binary
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LaneTarget;

    #[test]
    fn normalize_lane_name_maps_preview_alias() {
        let (n, dep) = normalize_lane_name("preview");
        assert_eq!(n, "nir");
        assert!(dep);
        let (n2, dep2) = normalize_lane_name("nir");
        assert_eq!(n2, "nir");
        assert!(!dep2);
    }

    #[test]
    fn validate_lane_target_paths_rejects_missing_file() {
        let t = LaneTarget {
            binary: "b".into(),
            path: PathBuf::from("/nonexistent/path/that/does/not/exist.bin"),
            role: "r".into(),
            default_functions_limit: None,
            default_timeout_ms: None,
        };
        assert!(validate_lane_target_paths(&[t]).is_err());
    }
}
