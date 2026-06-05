use crate::loader::demangle::demangle;
use crate::loader::types::{
    DwarfFunctionInfo, DwarfLocation, DwarfParamInfo, LoadedBinary, PdbDebugInfo, PdbFunctionInfo,
};
use anyhow::{Context, Result};
use pdb::{FallibleIterator, IdIndex, SymbolData, TypeData, TypeIndex};
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

pub fn ingest_pdb_function_facts(binary: &mut LoadedBinary) -> Result<usize> {
    let Some(pdb_path) = resolve_pdb_sidecar_path(binary.inner(), Path::new(&binary.inner().path))
    else {
        return Ok(0);
    };

    let file = File::open(&pdb_path)
        .with_context(|| format!("open PDB sidecar {}", pdb_path.display()))?;
    let mut pdb = pdb::PDB::open(file)
        .with_context(|| format!("parse PDB sidecar {}", pdb_path.display()))?;

    let pdb_info = pdb.pdb_information().context("read PDB information")?;
    if !pdb_matches_image(binary.inner().pdb_debug_info.as_ref(), &pdb_info) {
        anyhow::bail!("PDB metadata does not match image debug record");
    }

    let address_map = pdb.address_map().context("build PDB address map")?;
    let type_information = pdb
        .type_information()
        .context("read PDB type information")?;
    let mut type_finder = type_information.finder();
    {
        let mut iter = type_information.iter();
        while let Some(_item) = iter.next().context("iterate PDB type information")? {
            type_finder.update(&iter);
        }
    }
    let id_information = pdb.id_information().ok();
    let id_finder = match id_information.as_ref() {
        Some(info) => {
            let mut finder = info.finder();
            let mut iter = info.iter();
            while let Some(_item) = iter.next().context("iterate PDB id information")? {
                finder.update(&iter);
            }
            Some(finder)
        }
        None => None,
    };

    let dbi = pdb
        .debug_information()
        .context("read PDB debug information")?;
    let mut modules = dbi.modules().context("read PDB module list")?;
    let mut functions = HashMap::<u64, PdbFunctionInfo>::new();

    while let Some(module) = modules.next().context("iterate PDB modules")? {
        let Some(module_info) = pdb
            .module_info(&module)
            .with_context(|| format!("read PDB module info {}", module.module_name()))?
        else {
            continue;
        };

        let mut symbols = module_info.symbols().context("read PDB module symbols")?;
        let mut current: Option<PendingFunction> = None;

        while let Some(symbol) = symbols.next().context("iterate PDB symbols")? {
            let symbol_index = symbol.index();
            let symbol_data = match symbol.parse() {
                Ok(data) => data,
                Err(_) => continue,
            };

            if let Some(mut pending) = current.take() {
                if symbol_index == pending.end {
                    pending.finalize();
                    merge_pdb_function(&mut functions, pending.info);
                } else {
                    pending.observe_symbol(
                        &symbol_data,
                        &type_information,
                        &type_finder,
                        id_information.as_ref(),
                        id_finder.as_ref(),
                    );
                    current = Some(pending);
                }
            }

            if current.is_none() {
                if let Some(pending) = PendingFunction::from_symbol(
                    binary.image_base,
                    &address_map,
                    &symbol,
                    &symbol_data,
                    &type_information,
                    &type_finder,
                    id_information.as_ref(),
                    id_finder.as_ref(),
                ) {
                    current = Some(pending);
                }
            }
        }

        if let Some(mut pending) = current.take() {
            pending.finalize();
            merge_pdb_function(&mut functions, pending.info);
        }
    }

    let count = functions.len();
    binary.pdb_functions.extend(functions);
    Ok(count)
}

#[derive(Debug)]
struct PendingFunction {
    end: pdb::SymbolIndex,
    info: PdbFunctionInfo,
    pending_param_names: Vec<String>,
}

impl PendingFunction {
    fn from_symbol(
        image_base: u64,
        address_map: &pdb::AddressMap<'_>,
        symbol: &pdb::Symbol<'_>,
        symbol_data: &SymbolData<'_>,
        type_information: &pdb::TypeInformation<'_>,
        type_finder: &pdb::TypeFinder<'_>,
        id_information: Option<&pdb::IdInformation<'_>>,
        id_finder: Option<&pdb::IdFinder<'_>>,
    ) -> Option<Self> {
        let SymbolData::Procedure(proc) = symbol_data else {
            return None;
        };
        let rva = proc.offset.to_rva(address_map)?;
        let address = image_base + u64::from(rva.0);
        let (return_type, param_type_names) = resolve_function_signature(
            symbol.raw_kind(),
            proc.type_index,
            type_information,
            type_finder,
            id_information,
            id_finder,
        );

        let name = demangle(&proc.name.to_string());
        let params = param_type_names
            .into_iter()
            .map(|type_name| DwarfParamInfo {
                name: String::new(),
                type_name,
                location: DwarfLocation::Unknown,
            })
            .collect();

        Some(Self {
            end: proc.end,
            info: DwarfFunctionInfo {
                address,
                name,
                return_type,
                params,
                local_vars: Vec::new(),
                size: u64::from(proc.len),
            },
            pending_param_names: Vec::new(),
        })
    }

    fn observe_symbol(
        &mut self,
        symbol_data: &SymbolData<'_>,
        type_information: &pdb::TypeInformation<'_>,
        type_finder: &pdb::TypeFinder<'_>,
        id_information: Option<&pdb::IdInformation<'_>>,
        id_finder: Option<&pdb::IdFinder<'_>>,
    ) {
        match symbol_data {
            SymbolData::Local(local)
                if local.flags.isparam
                    && !local.flags.isoptimizedout
                    && !local.name.to_string().trim().is_empty() =>
            {
                self.pending_param_names
                    .push(local.name.to_string().trim().to_string());
                if let Some(param) = self.info.params.get_mut(self.pending_param_names.len() - 1) {
                    if param.type_name.trim().is_empty() {
                        if let Some(type_name) = resolve_symbol_type_name(
                            local.type_index,
                            type_information,
                            type_finder,
                            id_information,
                            id_finder,
                        ) {
                            param.type_name = type_name;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn finalize(&mut self) {
        for (param, name) in self
            .info
            .params
            .iter_mut()
            .zip(self.pending_param_names.iter())
        {
            if param.name.trim().is_empty() {
                param.name = name.clone();
            }
        }
    }
}

fn merge_pdb_function(functions: &mut HashMap<u64, PdbFunctionInfo>, candidate: PdbFunctionInfo) {
    let score = score_function_info(&candidate);
    match functions.get(&candidate.address) {
        Some(existing) if score_function_info(existing) >= score => {}
        _ => {
            functions.insert(candidate.address, candidate);
        }
    }
}

fn score_function_info(info: &PdbFunctionInfo) -> usize {
    usize::from(!info.name.trim().is_empty())
        + usize::from(info.return_type.is_some())
        + info
            .params
            .iter()
            .map(|param| {
                usize::from(!param.name.trim().is_empty())
                    + usize::from(!param.type_name.trim().is_empty())
            })
            .sum::<usize>()
}

fn resolve_pdb_sidecar_path(
    info: &crate::loader::types::LoadedBinaryInner,
    binary_path: &Path,
) -> Option<PathBuf> {
    let debug = info.pdb_debug_info.as_ref()?;
    let mut candidates = Vec::new();

    if let Some(path_hint) = debug.path_hint.as_deref() {
        let hinted = PathBuf::from(path_hint);
        candidates.push(hinted.clone());
        if let Some(file_name) = hinted.file_name() {
            candidates.push(binary_path.with_file_name(file_name));
            if let Some(parent) = binary_path.parent() {
                candidates.push(parent.join(file_name));
            }
        }
    }

    candidates.push(binary_path.with_extension("pdb"));

    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .find(|path| path.is_file())
}

fn pdb_matches_image(debug: Option<&PdbDebugInfo>, pdb_info: &pdb::PDBInformation<'_>) -> bool {
    let Some(debug) = debug else {
        return true;
    };

    if let Some(expected_age) = debug.age {
        if pdb_info.age < expected_age {
            return false;
        }
    }

    true
}

fn resolve_function_signature(
    symbol_kind: pdb::SymbolKind,
    type_index: TypeIndex,
    type_information: &pdb::TypeInformation<'_>,
    type_finder: &pdb::TypeFinder<'_>,
    id_information: Option<&pdb::IdInformation<'_>>,
    id_finder: Option<&pdb::IdFinder<'_>>,
) -> (Option<String>, Vec<String>) {
    if let Some(signature) =
        resolve_signature_from_type_index(type_index, type_information, type_finder)
    {
        return signature;
    }

    if is_id_procedure_symbol(symbol_kind) {
        if let (Some(id_information), Some(id_finder)) = (id_information, id_finder) {
            if let Some(signature) = resolve_signature_from_id_index(
                IdIndex(type_index.0),
                id_information,
                id_finder,
                type_information,
                type_finder,
            ) {
                return signature;
            }
        }
    }

    (None, Vec::new())
}

fn is_id_procedure_symbol(kind: pdb::SymbolKind) -> bool {
    matches!(kind, 0x1147 | 0x1148 | 0x1156)
}

fn resolve_signature_from_id_index(
    id_index: IdIndex,
    _id_information: &pdb::IdInformation<'_>,
    id_finder: &pdb::IdFinder<'_>,
    type_information: &pdb::TypeInformation<'_>,
    type_finder: &pdb::TypeFinder<'_>,
) -> Option<(Option<String>, Vec<String>)> {
    let item = id_finder.find(id_index).ok()?;
    match item.parse().ok()? {
        pdb::IdData::Function(function) => {
            resolve_signature_from_type_index(function.function_type, type_information, type_finder)
        }
        pdb::IdData::MemberFunction(function) => {
            resolve_signature_from_type_index(function.function_type, type_information, type_finder)
        }
        _ => None,
    }
}

fn resolve_signature_from_type_index(
    type_index: TypeIndex,
    type_information: &pdb::TypeInformation<'_>,
    type_finder: &pdb::TypeFinder<'_>,
) -> Option<(Option<String>, Vec<String>)> {
    let item = type_finder.find(type_index).ok()?;
    match item.parse().ok()? {
        TypeData::Procedure(proc) => Some((
            proc.return_type.and_then(|index| {
                resolve_symbol_type_name(index, type_information, type_finder, None, None)
            }),
            resolve_argument_list(proc.argument_list, type_information, type_finder),
        )),
        TypeData::MemberFunction(proc) => Some((
            Some(resolve_symbol_type_name(
                proc.return_type,
                type_information,
                type_finder,
                None,
                None,
            )?),
            resolve_argument_list(proc.argument_list, type_information, type_finder),
        )),
        TypeData::Modifier(modifier) => resolve_signature_from_type_index(
            modifier.underlying_type,
            type_information,
            type_finder,
        ),
        _ => None,
    }
}

fn resolve_argument_list(
    argument_list: TypeIndex,
    type_information: &pdb::TypeInformation<'_>,
    type_finder: &pdb::TypeFinder<'_>,
) -> Vec<String> {
    let Ok(item) = type_finder.find(argument_list) else {
        return Vec::new();
    };
    let Ok(TypeData::ArgumentList(arguments)) = item.parse() else {
        return Vec::new();
    };
    arguments
        .arguments
        .iter()
        .filter_map(|index| {
            resolve_symbol_type_name(*index, type_information, type_finder, None, None)
        })
        .collect()
}

fn resolve_symbol_type_name(
    type_index: TypeIndex,
    type_information: &pdb::TypeInformation<'_>,
    type_finder: &pdb::TypeFinder<'_>,
    _id_information: Option<&pdb::IdInformation<'_>>,
    _id_finder: Option<&pdb::IdFinder<'_>>,
) -> Option<String> {
    let item = type_finder.find(type_index).ok()?;
    let data = item.parse().ok()?;
    resolve_type_data_name(data, type_information, type_finder)
}

fn resolve_type_data_name(
    data: TypeData<'_>,
    type_information: &pdb::TypeInformation<'_>,
    type_finder: &pdb::TypeFinder<'_>,
) -> Option<String> {
    match data {
        TypeData::Primitive(primitive) => Some(format!("{primitive:?}")),
        TypeData::Class(class) => Some(class.name.to_string().into_owned()),
        TypeData::Enumeration(enumeration) => Some(enumeration.name.to_string().into_owned()),
        TypeData::Union(union) => Some(union.name.to_string().into_owned()),
        TypeData::Pointer(pointer) => {
            let base = resolve_symbol_type_name(
                pointer.underlying_type,
                type_information,
                type_finder,
                None,
                None,
            )
            .unwrap_or_else(|| "void".to_string());
            Some(format!("{base}*"))
        }
        TypeData::Modifier(modifier) => resolve_symbol_type_name(
            modifier.underlying_type,
            type_information,
            type_finder,
            None,
            None,
        ),
        TypeData::Array(array) => {
            let base = resolve_symbol_type_name(
                array.element_type,
                type_information,
                type_finder,
                None,
                None,
            )
            .unwrap_or_else(|| "void".to_string());
            Some(format!("{base}[]"))
        }
        TypeData::Procedure(_) | TypeData::MemberFunction(_) => Some("fn".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::loader::LoadedBinary;

    #[test]
    fn loads_focused_pdb_function_facts_from_repo_sample() {
        let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../samples/other/binaries-master/tests/x86_64/windows/fauxware.exe");
        if !sample.exists() {
            eprintln!(
                "skipping PDB sidecar sample test; missing {}",
                sample.display()
            );
            return;
        }
        let binary = LoadedBinary::from_file(&sample).expect("load fauxware sample");

        assert!(
            binary.inner().pdb_debug_info.is_some(),
            "expected CodeView-backed PDB metadata"
        );
        assert!(
            !binary.pdb_functions.is_empty(),
            "expected focused PDB function facts to load"
        );
        assert!(
            binary
                .pdb_functions
                .values()
                .any(|func| func.return_type.is_some() || !func.params.is_empty()),
            "expected at least one PDB function signature"
        );
    }
}
