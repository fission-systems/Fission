//! `fission_cli languages`: the SLEIGH language IDs this build can lift --
//! the values `--language` accepts. Most of them describe raw firmware with
//! no container header to detect them from, so a caller has to pick one;
//! this is the list to pick from.

use crate::cli::args::LanguagesArgs;
use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct LanguageRow {
    pub id: String,
    pub processor: String,
    pub endian: String,
    pub size: u32,
    pub variant: String,
}

/// Split an ID into its four parts. An ID that does not have them is still
/// listed, with the whole ID as the processor, rather than dropped.
pub(crate) fn language_row(id: &str) -> LanguageRow {
    let parts: Vec<&str> = id.split(':').collect();
    match parts[..] {
        [processor, endian, size, variant] => LanguageRow {
            id: id.to_string(),
            processor: processor.to_string(),
            endian: match endian {
                "LE" | "le" => "little",
                "BE" | "be" => "big",
                other => other,
            }
            .to_string(),
            size: size.parse().unwrap_or(0),
            variant: variant.to_string(),
        },
        _ => LanguageRow {
            id: id.to_string(),
            processor: id.to_string(),
            endian: String::new(),
            size: 0,
            variant: String::new(),
        },
    }
}

pub(crate) fn rows(
    ids: impl IntoIterator<Item = String>,
    processor: Option<&str>,
) -> Vec<LanguageRow> {
    let needle = processor.map(str::to_ascii_lowercase);
    let mut rows: Vec<LanguageRow> = ids
        .into_iter()
        .map(|id| language_row(&id))
        .filter(|row| {
            needle
                .as_deref()
                .is_none_or(|n| row.processor.to_ascii_lowercase().contains(n))
        })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows
}

pub(crate) fn run(args: &LanguagesArgs) -> Result<()> {
    let ids = fission_core::architecture::known_language_ids()
        .map_err(|e| anyhow::anyhow!("failed to load SLEIGH language manifest: {e}"))?;
    let rows = rows(ids, args.processor.as_deref());
    if args.json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        println!("Languages ({}):", rows.len());
        for row in &rows {
            println!("  {}", row.id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_splits_into_its_parts() {
        let row = language_row("ARM:BE:32:v8");
        assert_eq!(row.processor, "ARM");
        assert_eq!(row.endian, "big");
        assert_eq!(row.size, 32);
        assert_eq!(row.variant, "v8");
    }

    #[test]
    fn the_processor_filter_is_a_case_insensitive_substring() {
        let ids = ["x86:LE:64:default", "ARM:LE:32:v8", "AARCH64:LE:64:v8A"].map(String::from);
        let arm = rows(ids.clone(), Some("arm"));
        assert_eq!(
            arm.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["ARM:LE:32:v8"]
        );
        assert_eq!(rows(ids, None).len(), 3);
    }
}
