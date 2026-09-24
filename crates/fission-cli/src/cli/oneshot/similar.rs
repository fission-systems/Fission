//! Fuzzy function similarity (`fission_decompiler::similarity`).

use anyhow::{Context, Result, bail};
use fission_decompiler::similarity::{
    SIMILARITY_INDEX_VERSION, SimilarityCorpus, SimilarityFeatureFamily, SimilarityIndex,
    SimilarityIndexDocument, SimilaritySearchResult, extract_function_features,
    extract_function_features_with_provenance,
};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodeContract, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;
use fission_static::analysis::decode_context_for_address;
use serde_json::json;
use std::{fs, io::Write, path::Path};

use crate::cli::args::OneShotArgs;

const MAX_BYTES: usize = 1 << 16;
const INSTRUCTION_LIMIT: usize = 4000;

pub(super) fn run_similar(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    let load_spec = binary
        .load_spec()
        .context("similarity requires a resolved SLEIGH load spec for this binary")?;
    let frontend = RuntimeSleighFrontend::new_for_load_spec(load_spec)
        .context("failed to build a SLEIGH frontend for this binary")?;

    let mut corpus = SimilarityCorpus::new();
    let mut keys: Vec<(String, u64)> = Vec::new();
    let mut documents = Vec::new();
    let cross_index_mode = cli.similar_index.is_some() || cli.similar_update_index.is_some();
    let language_id = load_spec.pair.language_id.as_str().to_string();
    let query_one_function = cli.similar_index.is_some() && cli.similar_function.is_some();
    for func in &binary.functions {
        if func.is_import {
            continue;
        }
        if query_one_function
            && frontend
                .normalize_low_bit_code_address(func.address)
                .address
                != cli.similar_function.expect("checked above")
        {
            continue;
        }
        let Some((decode_addr, lifted)) = lift_for_similarity(binary, &frontend, func.address)
        else {
            continue;
        };
        let key = format!("{}@{:#x}", func.name, decode_addr);
        if cross_index_mode {
            let extracted = extract_function_features_with_provenance(&lifted.function);
            documents.push(SimilarityIndexDocument {
                binary_hash: binary.hash.clone(),
                binary_path: binary.path.clone(),
                function_address: decode_addr,
                function_name: func.name.clone(),
                aliases: Vec::new(),
                language_id: language_id.clone(),
                features: extracted.features,
                feature_provenance: Some(extracted.provenance),
            });
        } else {
            corpus.add(key.clone(), extract_function_features(&lifted.function));
        }
        keys.push((key, decode_addr));
    }

    let has_features = if cross_index_mode {
        documents
            .iter()
            .any(|document| !document.features.is_empty())
    } else {
        !corpus.is_empty()
    };
    if !has_features {
        bail!(
            "no functions could be lifted for similarity comparison (try --function-discovery-profile balanced)"
        );
    }

    let mut stdout = std::io::stdout().lock();

    if let Some(path) = &cli.similar_update_index {
        let mut index = read_index_or_new(path)?;
        ensure_compatible_index(&index, path)?;
        let (inserted, refreshed) = index.upsert_many(documents);
        write_index(path, &index)?;
        if cli.json {
            let payload = json!({
                "index_version": index.format_version(),
                "binary_hash": binary.hash,
                "binary_path": binary.path,
                "functions_added": inserted,
                "functions_refreshed": refreshed,
                "functions_total": index.len(),
            });
            writeln!(stdout, "{}", serde_json::to_string_pretty(&payload)?)?;
        } else {
            writeln!(
                stdout,
                "similar index: version={} added={} total={} path={}",
                index.format_version(),
                inserted,
                index.len(),
                path.display()
            )?;
        }
        return Ok(());
    }

    if let Some(path) = &cli.similar_index {
        let index = read_index(path)?;
        ensure_compatible_index(&index, path)?;
        if index.is_empty() {
            bail!("similarity index {} contains no functions", path.display());
        }
        let queries: Vec<_> = if let Some(address) = cli.similar_function {
            let Some(document) = documents
                .iter()
                .find(|document| document.function_address == address)
            else {
                bail!("0x{address:x} is not a known (non-import) function in this binary");
            };
            vec![document]
        } else {
            documents.iter().collect()
        };
        let prepared_index = index.prepare_search();
        let results: Vec<_> = queries
            .into_iter()
            .map(|query| {
                let matches = prepared_index.query_top_k_with_evidence(
                    &query.features,
                    query.feature_provenance.as_ref(),
                    Some((
                        &query.binary_hash,
                        &query.language_id,
                        query.function_address,
                    )),
                    cli.similar_top_k,
                );
                (query, matches)
            })
            .collect();
        if cli.json {
            print_index_json(&mut stdout, &binary.hash, &binary.path, &results)?;
        } else {
            for (query, matches) in &results {
                writeln!(
                    stdout,
                    "{}@{:#x}",
                    query.function_name, query.function_address
                )?;
                print_index_matches_text(&mut stdout, matches)?;
            }
        }
        return Ok(());
    }

    if let Some(address) = cli.similar_function {
        let Some((key, _)) = keys.iter().find(|(_, addr)| *addr == address) else {
            bail!("0x{address:x} is not a known (non-import) function in this binary");
        };
        let matches = corpus.most_similar_to(key, cli.similar_top_k);
        if cli.json {
            print_json(&mut stdout, &[(key.clone(), matches)])?;
        } else {
            writeln!(stdout, "{key}")?;
            print_matches_text(&mut stdout, &matches)?;
        }
        return Ok(());
    }

    let mut all_results: Vec<(String, Vec<(String, f64)>)> = Vec::with_capacity(keys.len());
    for (key, _) in &keys {
        all_results.push((key.clone(), corpus.most_similar_to(key, cli.similar_top_k)));
    }

    if cli.json {
        print_json(&mut stdout, &all_results)?;
        return Ok(());
    }

    writeln!(
        stdout,
        "similar: functions_indexed={} function_discovery_profile_applied={}",
        corpus.len(),
        cli.function_discovery_profile.is_some()
    )?;
    for (key, matches) in &all_results {
        if matches.is_empty() {
            continue;
        }
        writeln!(stdout, "{key}")?;
        print_matches_text(&mut stdout, matches)?;
    }
    Ok(())
}

fn read_index(path: &Path) -> Result<SimilarityIndex> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("read similarity index {}", path.display()))?;
    parse_index(&contents, path)
}

fn read_index_or_new(path: &Path) -> Result<SimilarityIndex> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_index(&contents, path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SimilarityIndex::new()),
        Err(error) => {
            Err(error).with_context(|| format!("read similarity index {}", path.display()))
        }
    }
}

fn parse_index(contents: &str, path: &Path) -> Result<SimilarityIndex> {
    let mut index: SimilarityIndex = serde_json::from_str(contents)
        .with_context(|| format!("parse similarity index {}", path.display()))?;
    index.canonicalize();
    Ok(index)
}

fn ensure_compatible_index(index: &SimilarityIndex, path: &Path) -> Result<()> {
    if !index.is_compatible() {
        bail!(
            "similarity index {} has format version {}; this CLI supports version {}",
            path.display(),
            index.format_version(),
            SIMILARITY_INDEX_VERSION
        );
    }
    if !index.has_unique_identities() {
        bail!(
            "similarity index {} contains duplicate binary-hash/function-address identities",
            path.display()
        );
    }
    Ok(())
}

fn write_index(path: &Path, index: &SimilarityIndex) -> Result<()> {
    let contents = serde_json::to_vec_pretty(index).context("serialize similarity index")?;
    fs::write(path, contents).with_context(|| format!("write similarity index {}", path.display()))
}

fn print_index_matches_text(
    stdout: &mut impl Write,
    matches: &[SimilaritySearchResult],
) -> Result<()> {
    if !matches.is_empty() {
        writeln!(
            stdout,
            "  evidence uses opaque structural fingerprints; family names identify extraction radius, not semantic categories"
        )?;
    }
    for result in matches {
        let hit = &result.hit;
        let explanation = &result.explanation;
        writeln!(
            stdout,
            "  {:.4}  {}@{:#x}  [{}]",
            hit.score, hit.function_name, hit.function_address, hit.binary_path
        )?;
        writeln!(
            stdout,
            "    evidence: formula=numerator/(query_l2*candidate_l2), normalization_defined={}, shared={} fingerprints, numerator={:.6}, query_l2={:.6}, candidate_l2={:.6}, provenance={}/{}",
            explanation.normalization_defined,
            explanation.shared_fingerprint_count,
            explanation.numerator,
            explanation.query_l2_norm,
            explanation.candidate_l2_norm,
            explanation.query_feature_provenance.as_str(),
            explanation.candidate_feature_provenance.as_str(),
        )?;
        for contributor in explanation.top_contributors.iter().take(3) {
            writeln!(
                stdout,
                "      0x{:08x}: numerator_contribution={:.6}, query_weight={:.6} [{}], candidate_weight={:.6} [{}]",
                contributor.fingerprint,
                contributor.numerator_contribution,
                contributor.query_weight,
                format_feature_families(&contributor.query_families),
                contributor.candidate_weight,
                format_feature_families(&contributor.candidate_families),
            )?;
        }
        if explanation.omitted_contributor_count > 0 {
            writeln!(
                stdout,
                "      ... {} more shared fingerprints (omitted numerator contribution={:.6})",
                explanation.omitted_contributor_count, explanation.omitted_numerator_contribution,
            )?;
        }
    }
    Ok(())
}

fn format_feature_families(families: &[SimilarityFeatureFamily]) -> String {
    if families.is_empty() {
        return "unavailable".to_string();
    }
    families
        .iter()
        .map(|family| family.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn print_index_json(
    stdout: &mut impl Write,
    query_hash: &str,
    query_path: &str,
    results: &[(&SimilarityIndexDocument, Vec<SimilaritySearchResult>)],
) -> Result<()> {
    let rows: Vec<_> = results
        .iter()
        .map(|(query, matches)| {
            json!({
                "function": query.function_name,
                "aliases": query.aliases,
                "address": query.function_address,
                "matches": matches.iter().map(|result| json!({
                    "binary_hash": result.hit.binary_hash,
                    "binary_path": result.hit.binary_path,
                    "function": result.hit.function_name,
                    "aliases": result.hit.aliases,
                    "address": result.hit.function_address,
                    "language_id": result.hit.language_id,
                    "score": result.hit.score,
                    "score_explanation": &result.explanation,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let payload = json!({
        "index_version": SIMILARITY_INDEX_VERSION,
        "query_binary_hash": query_hash,
        "query_binary_path": query_path,
        "results": rows,
    });
    let text = serde_json::to_string_pretty(&payload).context("serialize similarity JSON")?;
    writeln!(stdout, "{text}").context("write similarity JSON")?;
    Ok(())
}

fn lift_for_similarity(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    addr: u64,
) -> Option<(u64, fission_sleigh::runtime::DecodedPcodeFunction)> {
    let address_state = frontend.normalize_low_bit_code_address(addr);
    let decode_addr = address_state.address;
    let context_override =
        decode_context_for_address(binary, frontend, address_state.context_override);
    let max_bytes = binary
        .available_execution_bytes(decode_addr)
        .map(|available| MAX_BYTES.min(available).max(1))
        .unwrap_or(MAX_BYTES);
    let bytes = binary.view_bytes(decode_addr, max_bytes)?;
    let memory_context = decode_memory_context_for(binary, decode_addr, bytes.len());
    let contract = DecodeContract::decomp_function(INSTRUCTION_LIMIT);
    let lifted = frontend
        .lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_addr,
            contract,
            &memory_context,
            context_override,
        )
        .ok()?;
    Some((decode_addr, lifted))
}

fn print_matches_text(stdout: &mut impl Write, matches: &[(String, f64)]) -> Result<()> {
    for (name, score) in matches {
        writeln!(stdout, "  {score:.4}  {name}").context("write similar match")?;
    }
    Ok(())
}

fn print_json(stdout: &mut impl Write, results: &[(String, Vec<(String, f64)>)]) -> Result<()> {
    let nodes: Vec<_> = results
        .iter()
        .map(|(name, matches)| {
            json!({
                "function": name,
                "matches": matches.iter().map(|(m, score)| json!({
                    "name": m,
                    "score": score,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let payload = json!({ "results": nodes });
    let text = serde_json::to_string_pretty(&payload).context("serialize similar JSON")?;
    writeln!(stdout, "{text}").context("write similar JSON")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_decompiler::similarity::SimilarityFeatureProvenance;

    #[test]
    fn cross_binary_output_includes_score_evidence_in_json_and_text() {
        let provenance = SimilarityFeatureProvenance::for_features(
            &[1, 2],
            &[
                SimilarityFeatureFamily::LocalOperation,
                SimilarityFeatureFamily::LocalOperation,
            ],
        )
        .expect("provenance aligned with features");
        let mut candidate = SimilarityIndexDocument {
            binary_hash: "candidate-hash".into(),
            binary_path: "candidate.elf".into(),
            function_address: 0x2000,
            function_name: "candidate_fn".into(),
            aliases: Vec::new(),
            language_id: "x86:LE:64:default".into(),
            features: vec![1, 2],
            feature_provenance: Some(provenance.clone()),
        };
        let mut index = SimilarityIndex::new();
        index.upsert_many([candidate.clone()]);
        let matches =
            index
                .prepare_search()
                .query_top_k_with_evidence(&[1, 2], Some(&provenance), None, 1);
        candidate.binary_hash = "query-hash".into();
        candidate.binary_path = "query.elf".into();
        candidate.function_address = 0x1000;
        candidate.function_name = "query_fn".into();
        let query = candidate;

        let mut json_output = Vec::new();
        print_index_json(
            &mut json_output,
            "query-hash",
            "query.elf",
            &[(&query, matches.clone())],
        )
        .expect("render cross-binary JSON");
        let payload: serde_json::Value =
            serde_json::from_slice(&json_output).expect("parse cross-binary JSON");
        let explanation = &payload["results"][0]["matches"][0]["score_explanation"];
        assert_eq!(explanation["schema_version"], 1);
        assert_eq!(explanation["shared_fingerprint_count"], 2);
        assert_eq!(explanation["query_feature_provenance"], "complete");
        assert_eq!(explanation["candidate_feature_provenance"], "complete");
        assert_eq!(
            explanation["top_contributors"][0]["query_families"][0],
            "local_operation"
        );

        let mut text_output = Vec::new();
        print_index_matches_text(&mut text_output, &matches).expect("render match text");
        let text_output = String::from_utf8(text_output).expect("UTF-8 text output");
        assert!(text_output.contains("shared=2 fingerprints"));
        assert!(text_output.contains("0x00000001"));
        assert!(text_output.contains("local_operation"));
    }

    #[test]
    fn same_binary_json_shape_is_unchanged() {
        let mut output = Vec::new();
        print_json(
            &mut output,
            &[(
                "query@0x1000".into(),
                vec![("candidate@0x2000".into(), 0.5)],
            )],
        )
        .expect("render same-binary JSON");
        let payload: serde_json::Value =
            serde_json::from_slice(&output).expect("parse same-binary JSON");
        assert_eq!(
            payload,
            json!({
                "results": [{
                    "function": "query@0x1000",
                    "matches": [{
                        "name": "candidate@0x2000",
                        "score": 0.5,
                    }],
                }],
            })
        );
    }
}
