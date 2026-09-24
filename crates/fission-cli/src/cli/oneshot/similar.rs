//! Fuzzy function similarity (`fission_decompiler::similarity`).

use anyhow::{Context, Result, bail};
use fission_decompiler::similarity::{
    SIMILARITY_INDEX_VERSION, SimilarityCorpus, SimilarityIndex, SimilarityIndexDocument,
    SimilaritySearchHit, extract_function_features,
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
        let features = extract_function_features(&lifted.function);
        let key = format!("{}@{:#x}", func.name, decode_addr);
        if cross_index_mode {
            documents.push(SimilarityIndexDocument {
                binary_hash: binary.hash.clone(),
                binary_path: binary.path.clone(),
                function_address: decode_addr,
                function_name: func.name.clone(),
                aliases: Vec::new(),
                language_id: language_id.clone(),
                features,
            });
        } else {
            corpus.add(key.clone(), features);
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
        let results: Vec<_> = queries
            .into_iter()
            .map(|query| {
                let matches = index.query_top_k(
                    &query.features,
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
    matches: &[SimilaritySearchHit],
) -> Result<()> {
    for hit in matches {
        writeln!(
            stdout,
            "  {:.4}  {}@{:#x}  [{}]",
            hit.score, hit.function_name, hit.function_address, hit.binary_path
        )?;
    }
    Ok(())
}

fn print_index_json(
    stdout: &mut impl Write,
    query_hash: &str,
    query_path: &str,
    results: &[(&SimilarityIndexDocument, Vec<SimilaritySearchHit>)],
) -> Result<()> {
    let rows: Vec<_> = results
        .iter()
        .map(|(query, matches)| {
            json!({
                "function": query.function_name,
                "aliases": query.aliases,
                "address": query.function_address,
                "matches": matches.iter().map(|hit| json!({
                    "binary_hash": hit.binary_hash,
                    "binary_path": hit.binary_path,
                    "function": hit.function_name,
                    "aliases": hit.aliases,
                    "address": hit.function_address,
                    "language_id": hit.language_id,
                    "score": hit.score,
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
