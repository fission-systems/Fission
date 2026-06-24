//! Summarize configured signature corpora via [`fission_core::resources::ResourceProvider`].

use super::model::IdentityResourceSummary;
use fission_core::path_config::PathConfig;
use fission_core::resources::ResourceProvider;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SG_FILES_ENUM: usize = 50_000;
const MAX_PATTERN_JSON_REPORTED: usize = 10_000;
const MAX_FID_BF_ENUM: usize = 50_000;

#[must_use]
pub(super) fn summarize_identity_resources() -> IdentityResourceSummary {
    use std::sync::OnceLock;
    static CACHED_SUMMARY: OnceLock<IdentityResourceSummary> = OnceLock::new();
    CACHED_SUMMARY
        .get_or_init(|| summarize_identity_resources_for(ResourceProvider::global().paths()))
        .clone()
}

#[must_use]
pub(super) fn summarize_identity_resources_for(paths: &PathConfig) -> IdentityResourceSummary {
    let die_pe_json_present = paths.get_die_signatures_path().is_some();

    let die_corpus_present =
        paths.die_dir.as_ref().is_some_and(|p| p.exists()) || die_pe_json_present;

    let die_corpus_root = paths
        .die_dir
        .as_ref()
        .and_then(|p| workspace_relative(paths, p))
        .or_else(|| {
            paths
                .get_die_signatures_path()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .and_then(|p| workspace_relative(paths, &p))
        });

    let die_sg_file_count = die_sg_file_count_bounded(paths);

    let patterns_present = paths.patterns_dir.as_ref().is_some_and(|p| p.exists());
    let pattern_json_count = paths.patterns_dir.as_ref().map(|_| {
        let n = paths.get_all_pattern_files().len();
        n.min(MAX_PATTERN_JSON_REPORTED)
    });

    let win_typeinfo_present = win_api_signatures_path(paths).is_some();

    let fid_present = paths.fid_dir.as_ref().is_some_and(|p| p.exists());
    let fid_bf_count = paths
        .fid_dir
        .as_ref()
        .map(|dir| fid_bf_count_bounded(dir.as_path()));

    IdentityResourceSummary {
        die_corpus_present,
        die_corpus_root,
        die_sg_file_count,
        die_pe_json_present,
        patterns_present,
        pattern_json_count,
        win_typeinfo_present,
        fid_present,
        fid_bf_count,
    }
}

fn workspace_relative(paths: &PathConfig, p: &Path) -> Option<String> {
    paths
        .workspace_root
        .as_ref()
        .and_then(|root| p.strip_prefix(root).ok())
        .map(|rel| rel.to_string_lossy().into_owned())
        .or_else(|| Some(p.to_string_lossy().into_owned()))
}

fn die_sg_file_count_bounded(paths: &PathConfig) -> Option<usize> {
    let mirror = paths.die_mirror_root()?;

    let mut acc = Vec::new();
    for child in ["db", "db_extra", "db_custom"] {
        if acc.len() >= MAX_SG_FILES_ENUM {
            break;
        }
        collect_sg_files_bounded(&mirror.join(child), &mut acc, MAX_SG_FILES_ENUM);
    }
    Some(acc.len())
}

fn collect_sg_files_bounded(dir: &Path, out: &mut Vec<PathBuf>, max_files: usize) {
    if out.len() >= max_files {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= max_files {
            break;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_sg_files_bounded(&path, out, max_files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("sg") {
            out.push(path);
        }
    }
}

fn win_api_signatures_path(paths: &PathConfig) -> Option<PathBuf> {
    paths.get_win_api_signatures_path()
}

fn fid_bf_count_bounded(fid_dir: &Path) -> usize {
    let mut n = 0usize;
    count_fidbf(fid_dir, &mut n);
    n.min(MAX_FID_BF_ENUM)
}

fn count_fidbf(dir: &Path, count: &mut usize) {
    if *count >= MAX_FID_BF_ENUM {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *count >= MAX_FID_BF_ENUM {
            break;
        }
        let path = entry.path();
        if path.is_dir() {
            count_fidbf(&path, count);
        } else if path.extension().and_then(|e| e.to_str()) == Some("fidbf") {
            *count += 1;
        }
    }
}
