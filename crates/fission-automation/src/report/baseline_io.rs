//! Baseline JSON IO and `latest/` directory sync.

use crate::model::InventoryRow;
use crate::report::snapshot::AutomationSummary;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn load_baseline(path: &Path) -> Result<Option<AutomationSummary>> {
    if !path.exists() {
        return Ok(None);
    }
    let data =
        fs::read_to_string(path).with_context(|| format!("read baseline {}", path.display()))?;
    let summary = serde_json::from_str(&data)
        .with_context(|| format!("parse baseline {}", path.display()))?;
    Ok(Some(summary))
}
pub fn load_baseline_candidates(summary_path: &Path) -> Result<Option<Vec<InventoryRow>>> {
    let Some(parent) = summary_path.parent() else {
        return Ok(None);
    };
    let path = parent.join("nir_quality_candidates.json");
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))?;
    let candidates_value = value
        .get("candidates")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if candidates_value.is_null() {
        return Ok(Some(Vec::new()));
    }
    let candidates: Vec<InventoryRow> = serde_json::from_value(candidates_value)
        .with_context(|| format!("decode candidates {}", path.display()))?;
    Ok(Some(candidates))
}
pub fn update_latest(run_dir: &Path, latest_dir: &Path) -> Result<()> {
    if latest_dir.exists() {
        fs::remove_dir_all(latest_dir)
            .with_context(|| format!("remove {}", latest_dir.display()))?;
    }
    copy_dir_all(run_dir, latest_dir)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)
                .with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_baseline_rejects_invalid_json() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "fission-automation-baseline-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let mut f = std::fs::File::create(&path).expect("temp file");
        f.write_all(b"{not json").expect("write");
        drop(f);
        let err = load_baseline(&path).expect_err("expected parse error");
        let _ = std::fs::remove_file(&path);
        assert!(
            err.to_string().contains("parse baseline") || err.to_string().contains("parse"),
            "{err:?}"
        );
    }
}
