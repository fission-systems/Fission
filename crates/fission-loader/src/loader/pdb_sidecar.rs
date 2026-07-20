use crate::loader::demangle::demangle;
use crate::loader::pdb_registers;
use crate::loader::types::{
    DwarfFunctionInfo, DwarfLocalVar, DwarfLocation, DwarfParamInfo, InferredFieldInfo,
    InferredTypeInfo, LoadedBinary, PdbDebugInfo, PdbFunctionInfo,
};
use anyhow::{Context, Result};
use pdb::{FallibleIterator, IdIndex, SymbolData, TypeData, TypeIndex};
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

/// CodeView `S_FRAMEPROC` record kind. The `pdb` crate doesn't parse this
/// symbol into `SymbolData` at all (`.parse()` returns `Err`), but it's the
/// only source of the frame size and parameter/local frame-pointer register
/// info needed to classify `S_REGREL32` symbols -- see
/// `parse_frame_proc`/`classify_register_relative`.
const S_FRAMEPROC: u16 = 0x1012;

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

            if let Some(mut pending) = current.take() {
                if symbol_index == pending.end {
                    pending.finalize();
                    merge_pdb_function(&mut functions, pending.info);
                    // current stays None: falls through to the "start a new
                    // pending function" check below, same as the original
                    // control flow.
                } else if symbol.raw_kind() == S_FRAMEPROC {
                    // Not parseable via symbol.parse() at all (see the
                    // S_FRAMEPROC doc comment) -- read raw_bytes() directly.
                    pending.observe_frame_proc(symbol.raw_bytes());
                    current = Some(pending);
                } else if let Ok(symbol_data) = symbol.parse() {
                    pending.observe_symbol(
                        &symbol_data,
                        &type_information,
                        &type_finder,
                        id_information.as_ref(),
                        id_finder.as_ref(),
                    );
                    current = Some(pending);
                } else {
                    current = Some(pending);
                }
            }

            if current.is_none() {
                if let Ok(symbol_data) = symbol.parse() {
                    if let Some(pending) = PendingFunction::from_symbol(
                        binary.image_base,
                        binary.is_64bit,
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
        }

        if let Some(mut pending) = current.take() {
            pending.finalize();
            merge_pdb_function(&mut functions, pending.info);
        }
    }

    let count = functions.len();
    binary.pdb_functions.extend(functions);

    let struct_types = extract_pdb_struct_types(&type_information, &type_finder)
        .context("extract PDB struct/class layouts")?;
    binary.inner_mut().inferred_types.extend(struct_types);

    Ok(count)
}

/// Extract struct/class/interface layouts (name, kind, size, real field
/// name/type/offset) from every named `TypeData::Class` in the PDB's type
/// stream -- the PDB-side equivalent of DWARF's `DwarfAnalyzer::extract_type_info`.
/// Anonymous classes and forward declarations (`fields: None`, or a
/// `FieldList` this function can't resolve) are skipped rather than
/// producing an empty/partial entry.
fn extract_pdb_struct_types(
    type_information: &pdb::TypeInformation<'_>,
    type_finder: &pdb::TypeFinder<'_>,
) -> Result<Vec<InferredTypeInfo>> {
    let mut types = Vec::new();
    let mut iter = type_information.iter();
    while let Some(item) = iter.next().context("iterate PDB type information")? {
        let Ok(TypeData::Class(class)) = item.parse() else {
            continue;
        };
        let name = class.name.to_string().trim().to_string();
        if name.is_empty() {
            continue;
        }
        let Some(fields_index) = class.fields else {
            continue; // forward declaration only
        };
        let members = resolve_field_list_members(fields_index, type_information, type_finder);
        if members.is_empty() {
            // A real, laid-out class always has at least one LF_MEMBER in
            // its field list; an empty result here means either a
            // genuinely empty class or (more likely) an unresolvable
            // FieldList -- either way, not useful to surface.
            continue;
        }
        let kind = match class.kind {
            pdb::ClassKind::Class => "class",
            pdb::ClassKind::Struct => "struct",
            pdb::ClassKind::Interface => "interface",
        };
        types.push(InferredTypeInfo {
            name: name.clone(),
            mangled_name: name,
            kind: kind.to_string(),
            fields: members,
            size: u32::try_from(class.size).unwrap_or(0),
            metadata_address: 0,
        });
    }
    Ok(types)
}

/// Resolve a `TypeData::FieldList`'s `TypeData::Member` entries into
/// `InferredFieldInfo`, following the `continuation` chain PDB uses to
/// split large field lists across multiple linked records. Non-`Member`
/// entries (base classes, methods, vtable pointers, nested types) are
/// skipped -- `InferredFieldInfo` has no shape for them.
fn resolve_field_list_members(
    fields_index: TypeIndex,
    type_information: &pdb::TypeInformation<'_>,
    type_finder: &pdb::TypeFinder<'_>,
) -> Vec<InferredFieldInfo> {
    let mut members = Vec::new();
    let mut next = Some(fields_index);
    let mut visited = std::collections::HashSet::new();
    while let Some(index) = next {
        if !visited.insert(index) {
            break; // guard against a malformed cyclic continuation chain
        }
        let Ok(item) = type_finder.find(index) else {
            break;
        };
        let Ok(TypeData::FieldList(field_list)) = item.parse() else {
            break;
        };
        for field in &field_list.fields {
            let TypeData::Member(member) = field else {
                continue;
            };
            let name = member.name.to_string().trim().to_string();
            if name.is_empty() {
                continue;
            }
            let type_name = resolve_symbol_type_name(
                member.field_type,
                type_information,
                type_finder,
                None,
                None,
            )
            .unwrap_or_else(|| "unknown".to_string());
            members.push(InferredFieldInfo {
                name,
                type_name,
                offset: u32::try_from(member.offset).unwrap_or(0),
                size: 0, // PDB LF_MEMBER carries no per-field size (DWARF's
                         // extract_member_info leaves ordinary members at 0 too).
            });
        }
        next = field_list.continuation;
    }
    members
}

/// Parsed `S_FRAMEPROC` fields relevant to classifying `S_REGREL32`
/// symbols. Layout (`llvm::codeview::FrameProcSym` /
/// `SymbolRecordMapping.cpp`, since the `pdb` crate doesn't parse this
/// symbol kind at all): after the 2-byte record kind already consumed by
/// `raw_bytes()`'s caller,
/// `TotalFrameBytes: u32, PaddingFrameBytes: u32, OffsetToPadding: u32,
/// BytesOfCalleeSavedRegisters: u32, OffsetOfExceptionHandler: u32,
/// SectionIdOfExceptionHandler: u16, Flags: u32` -- `Flags` bits 14-15
/// encode the local-variable frame-pointer register, bits 16-17 the
/// parameter frame-pointer register, each a 2-bit `EncodedFramePtrReg`
/// (0=None, 1=StackPtr, 2=FramePtr, 3=BasePtr) resolved to a name via
/// `pdb_registers::frame_ptr_register_name`.
#[derive(Debug)]
struct FrameProcInfo {
    total_frame_bytes: u32,
    param_fp_reg: Option<&'static str>,
    local_fp_reg: Option<&'static str>,
}

fn parse_frame_proc(raw_bytes: &[u8], is_64bit: bool) -> Option<FrameProcInfo> {
    if raw_bytes.len() < 28 {
        return None;
    }
    let total_frame_bytes = u32::from_le_bytes(raw_bytes[2..6].try_into().ok()?);
    let flags = u32::from_le_bytes(raw_bytes[24..28].try_into().ok()?);
    let local_encoded = ((flags >> 14) & 0x3) as u8;
    let param_encoded = ((flags >> 16) & 0x3) as u8;
    Some(FrameProcInfo {
        total_frame_bytes,
        local_fp_reg: pdb_registers::frame_ptr_register_name(is_64bit, local_encoded),
        param_fp_reg: pdb_registers::frame_ptr_register_name(is_64bit, param_encoded),
    })
}

enum VariableRole {
    Param,
    Local,
    Unknown,
}

/// Classify an `S_REGREL32` symbol as a parameter or a local variable.
///
/// Old-style CodeView (no `S_LOCAL`/`S_DEFRANGE_*` pair -- confirmed via
/// `llvm-pdbutil dump --symbols` against a real PDB: some functions emit
/// *only* `S_REGREL32` for every parameter and local, with no `isparam`
/// flag anywhere) carries no explicit parameter/local marker on the symbol
/// itself. Ghidra's own PDB analyzer (`RegisterRelativeSymbolApplier.java`)
/// resolves this the same way real debuggers do: by where the offset falls
/// relative to the function's own frame.
///
/// - If the parameter and local frame-pointer registers differ (both
///   present in `S_FRAMEPROC`), the register the symbol is relative to
///   *is* the classification -- no arithmetic needed.
/// - If they're the same register (the common case for x64's RSP-relative,
///   frame-pointer-omitted frames -- confirmed in the same real PDB), fall
///   back to the standard convention: parameters live in the
///   caller-reserved home/shadow space *above* this function's own frame,
///   i.e. at `offset >= total_frame_bytes + return_address_size`; locals
///   live within the frame, below that threshold. Verified against a real
///   case: `printf`'s wrapper (frame size 80, x64) has `_Format`
///   (offset 96, a real parameter) above the threshold and `_Result`
///   (offset 32, a genuine local accumulator) below it.
/// - If no `S_FRAMEPROC` was observed for this function at all, or the
///   register doesn't match either known frame pointer, there's no
///   confident signal -- returns `Unknown`, which the caller treats as a
///   local (see `observe_symbol`'s `RegisterRelative` arm: the least
///   harmful assumption, since fabricating a phantom "parameter" that
///   doesn't match the type signature's argument count would be worse
///   than a real parameter merely being misfiled as a local).
fn classify_register_relative(
    frame_info: Option<&FrameProcInfo>,
    is_64bit: bool,
    reg_name: &str,
    offset: i32,
) -> VariableRole {
    let Some(info) = frame_info else {
        return VariableRole::Unknown;
    };
    let matches_param = info.param_fp_reg == Some(reg_name);
    let matches_local = info.local_fp_reg == Some(reg_name);
    let regs_differ = info.param_fp_reg.is_some()
        && info.local_fp_reg.is_some()
        && info.param_fp_reg != info.local_fp_reg;

    if regs_differ {
        if matches_param {
            return VariableRole::Param;
        }
        if matches_local {
            return VariableRole::Local;
        }
        return VariableRole::Unknown;
    }

    if matches_param || matches_local {
        let pointer_size: i32 = if is_64bit { 8 } else { 4 };
        let threshold = i32::try_from(info.total_frame_bytes).unwrap_or(i32::MAX) + pointer_size;
        return if offset >= threshold {
            VariableRole::Param
        } else {
            VariableRole::Local
        };
    }

    VariableRole::Unknown
}

#[derive(Debug)]
struct PendingFunction {
    end: pdb::SymbolIndex,
    info: PdbFunctionInfo,
    pending_param_names: Vec<String>,
    is_64bit: bool,
    /// Set once an `S_FRAMEPROC` symbol is observed for this function
    /// (real PDBs emit it once, immediately after the opening
    /// `S_GPROC32`/`S_LPROC32`, before any locals -- so this is populated
    /// well before any `S_REGREL32` needs it in practice).
    frame_info: Option<FrameProcInfo>,
}

impl PendingFunction {
    fn from_symbol(
        image_base: u64,
        is_64bit: bool,
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
            is_64bit,
            frame_info: None,
        })
    }

    fn observe_frame_proc(&mut self, raw_bytes: &[u8]) {
        if let Some(info) = parse_frame_proc(raw_bytes, self.is_64bit) {
            self.frame_info = Some(info);
        }
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
            // A genuine local (not a parameter): `S_LOCAL` without a
            // register/stack home means its location lives in a following
            // `S_DEFRANGE_*` record this crate doesn't parse, so at least
            // capture the name/type -- still strictly better than nothing.
            SymbolData::Local(local) if !local.flags.isparam && !local.flags.isoptimizedout => {
                let name = local.name.to_string().trim().to_string();
                if !name.is_empty() {
                    let type_name = resolve_symbol_type_name(
                        local.type_index,
                        type_information,
                        type_finder,
                        id_information,
                        id_finder,
                    )
                    .unwrap_or_default();
                    self.info.local_vars.push(DwarfLocalVar {
                        name,
                        type_name,
                        location: DwarfLocation::Unknown,
                        // PDB carries no lexical-block PC-range scoping in
                        // this crate's coverage.
                        scope: None,
                    });
                }
            }
            // Old-style CodeView: name, type, *and* a real fixed location
            // in one record, with no isparam-equivalent flag at all (see
            // `classify_register_relative`'s doc comment for why that
            // needs S_FRAMEPROC to resolve).
            SymbolData::RegisterRelative(rr) => {
                let name = rr.name.to_string().trim().to_string();
                if name.is_empty() {
                    return;
                }
                let type_name = resolve_symbol_type_name(
                    rr.type_index,
                    type_information,
                    type_finder,
                    id_information,
                    id_finder,
                )
                .unwrap_or_default();
                let location = DwarfLocation::StackOffset(i64::from(rr.offset));
                let role = match pdb_registers::register_name(self.is_64bit, rr.register.0) {
                    Some(reg_name) => classify_register_relative(
                        self.frame_info.as_ref(),
                        self.is_64bit,
                        reg_name,
                        rr.offset,
                    ),
                    None => VariableRole::Unknown,
                };
                match role {
                    VariableRole::Param => {
                        self.pending_param_names.push(name);
                        let idx = self.pending_param_names.len() - 1;
                        if let Some(param) = self.info.params.get_mut(idx) {
                            if param.type_name.trim().is_empty() {
                                param.type_name = type_name;
                            }
                            if matches!(param.location, DwarfLocation::Unknown) {
                                param.location = location;
                            }
                        }
                    }
                    VariableRole::Local | VariableRole::Unknown => {
                        self.info.local_vars.push(DwarfLocalVar {
                            name,
                            type_name,
                            location,
                            scope: None,
                        });
                    }
                }
            }
            // A variable resident in a single register for its whole
            // lifetime. Always treated as a local: unlike S_REGREL32,
            // there's no frame-relative offset to classify against a
            // parameter/local boundary, and this symbol kind didn't appear
            // even once in the real PDB this was validated against, so a
            // register-resident parameter (possible but rare) being
            // labeled a "local" here is a documented simplification, not
            // an observed regression.
            SymbolData::RegisterVariable(rv) => {
                let name = rv.name.to_string().trim().to_string();
                if name.is_empty() {
                    return;
                }
                let type_name = resolve_symbol_type_name(
                    rv.type_index,
                    type_information,
                    type_finder,
                    id_information,
                    id_finder,
                )
                .unwrap_or_default();
                let location = pdb_registers::register_name(self.is_64bit, rv.register.0)
                    .map(|n| DwarfLocation::Register(n.to_string()))
                    .unwrap_or(DwarfLocation::Unknown);
                self.info.local_vars.push(DwarfLocalVar {
                    name,
                    type_name,
                    location,
                    scope: None,
                });
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
    use super::*;
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

    /// Real `S_FRAMEPROC` record bytes for a statically-linked `printf`
    /// wrapper, read via `symbol.raw_bytes()` from a real MSVC-built PDB
    /// (`vendor/binaries/tests/x86_64/windows/fauxware.pdb`, not checked
    /// in) and hardcoded here so the byte-offset/bit-shift parsing has a
    /// ground-truth regression test independent of that large vendored
    /// file's presence. Cross-checked against `llvm-pdbutil dump
    /// --symbols`' own decode of the same record ("size = 80", "bytes of
    /// callee saved registers = 8", "local fp reg = RSP, param fp reg =
    /// RSP") before trusting the byte offsets in `parse_frame_proc`.
    #[test]
    fn parse_frame_proc_matches_llvm_pdbutil_decode() {
        let raw_bytes: [u8; 30] = [
            0x12, 0x10, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x42, 0x11, 0x10,
            0x00, 0x00,
        ];
        let info = parse_frame_proc(&raw_bytes, true).expect("parse real S_FRAMEPROC bytes");
        assert_eq!(info.total_frame_bytes, 80);
        assert_eq!(info.local_fp_reg, Some("rsp"));
        assert_eq!(info.param_fp_reg, Some("rsp"));
    }

    /// Real, observed case from the same `printf` wrapper (RSP-relative,
    /// no separate frame pointer -- the common x64 shape): `_Format`
    /// (offset 96) is genuinely the function's one declared parameter;
    /// `_Result` (offset 32, below the frame's own 80-byte size) is a
    /// compiler-synthesized local accumulator, not a parameter, despite
    /// both being plain `S_REGREL32` symbols with no other distinguishing
    /// flag.
    #[test]
    fn classify_register_relative_uses_frame_size_when_registers_match() {
        let info = FrameProcInfo {
            total_frame_bytes: 80,
            param_fp_reg: Some("rsp"),
            local_fp_reg: Some("rsp"),
        };
        assert!(matches!(
            classify_register_relative(Some(&info), true, "rsp", 96),
            VariableRole::Param
        ));
        assert!(matches!(
            classify_register_relative(Some(&info), true, "rsp", 32),
            VariableRole::Local
        ));
    }

    #[test]
    fn classify_register_relative_uses_register_identity_when_they_differ() {
        let info = FrameProcInfo {
            total_frame_bytes: 40,
            param_fp_reg: Some("rbp"),
            local_fp_reg: Some("fbp"),
        };
        // Different registers for param vs. local: the register alone
        // decides, regardless of offset -- a "local" register reference at
        // a large offset must not be misread as a parameter.
        assert!(matches!(
            classify_register_relative(Some(&info), true, "rbp", 8),
            VariableRole::Param
        ));
        assert!(matches!(
            classify_register_relative(Some(&info), true, "fbp", 200),
            VariableRole::Local
        ));
    }

    #[test]
    fn classify_register_relative_is_unknown_without_frame_info() {
        assert!(matches!(
            classify_register_relative(None, true, "rsp", 96),
            VariableRole::Unknown
        ));
    }

    /// `testdata/x64_pdb_struct_test.{exe,pdb}`: `clang-cl --target=
    /// x86_64-pc-windows-msvc /Zi /Od /GS- /c` + `lld-link /DEBUG`, from a
    /// source with `struct Point { int x; int y; long long z; };`. Small
    /// (2.5KB exe, 72KB pdb) and self-contained (`/nodefaultlib`, a custom
    /// `mainCRTStartup` entry point -- no CRT/Windows SDK needed to link),
    /// unlike the much larger real MSVC fauxware sample used above.
    /// clang-cl's own CodeView backend emits `S_LOCAL`/`S_DEFRANGE_*` for
    /// every local (not `S_REGREL32` -- a different compiler, a different
    /// choice, confirmed via `llvm-pdbutil dump --symbols`), so this
    /// exercises struct/class *type* extraction specifically, not
    /// `classify_register_relative` (covered above with real MSVC bytes
    /// instead, since type records are the same format regardless of which
    /// compiler's *symbol* encoding style produced them).
    #[test]
    fn analyze_pdb_extracts_real_struct_layout() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_pdb_struct_test.exe");
        assert!(path.is_file(), "missing fixture {}", path.display());
        let binary = LoadedBinary::from_file(&path).expect("load struct test PE");

        assert!(
            binary.inner().pdb_debug_info.is_some(),
            "expected CodeView-backed PDB metadata"
        );
        let point = binary
            .inferred_types
            .iter()
            .find(|t| t.name == "Point")
            .unwrap_or_else(|| {
                panic!("expected a \"Point\" struct in {:?}", binary.inferred_types)
            });
        assert_eq!(point.kind, "struct");
        assert_eq!(point.size, 16);

        let field = |name: &str| {
            point
                .fields
                .iter()
                .find(|f| f.name == name)
                .unwrap_or_else(|| panic!("expected field {name} in {:?}", point.fields))
        };
        assert_eq!(field("x").offset, 0);
        assert_eq!(field("y").offset, 4);
        assert_eq!(field("z").offset, 8);
    }
}
