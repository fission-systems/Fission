//! Asset-independent experimental entry point. Canonical FIR remains the sole
//! semantic owner; unsupported bytes/contracts never route to a legacy engine.
use crate::{
    backend::{ArtifactBundle, BackendRegistry, OutputBackend, ProjectionInput, TargetProfile},
    x86::{X86Package, X86Program},
    FslError, OutputLayer,
};
use std::{fs, path::Path};

/// Fission-owned encoding and semantic definitions are compiled in memory.
/// No `.sla`, SLEIGH source, cspec, FPK or external decoder is consulted.
pub fn owned_x86_package() -> Result<X86Package, FslError> {
    X86Package::compile(
        include_str!("../specs/x86/scalar-immediate.fslx"),
        include_str!("../specs/x86/scalar-immediate-bodies.fsl"),
    )
}
#[derive(Debug)]
pub struct PipelineBundle {
    pub projection: ArtifactBundle,
    pub fir: String,
    pub profile: Vec<u8>,
    pub pipeline_report: String,
}
impl PipelineBundle {
    /// Refuse an existing directory. Emission/admission finish before publishing;
    /// an I/O failure can leave a partial new directory, not an atomic publish.
    pub fn write_new(&self, directory: &Path) -> std::io::Result<()> {
        fs::create_dir(directory)?;
        for source in &self.projection.sources {
            fs::write(directory.join(&source.name), &source.source)?;
        }
        fs::write(directory.join("output.fir"), &self.fir)?;
        fs::write(directory.join("profile.fslxc"), &self.profile)?;
        fs::write(directory.join("projection.txt"), &self.projection.report)?;
        fs::write(directory.join("pipeline.txt"), &self.pipeline_report)?;
        fs::write(
            directory.join("build.txt"),
            format!(
                "tool={}\narguments={:?}\nexecution=not-performed\n",
                self.projection.build.tool, self.projection.build.arguments
            ),
        )
    }
}
/// Selected executable/file-backed x86 span -> owned decode -> canonical FIR
/// -> checked source backend. Selection is caller supplied, not a recovered
/// function boundary or source signature. Loader handles file facts only.
pub fn project_x86_binary(
    input: &Path,
    address: u64,
    size: usize,
    backend_id: &str,
) -> Result<PipelineBundle, FslError> {
    project_x86(input, address, Some(size), backend_id)
}
/// Address-only conservative candidate selection for static corpus evaluation.
/// Straight-line to first near RET only; no source function/ABI claim is made.
pub fn project_x86_entry(
    input: &Path,
    address: u64,
    backend_id: &str,
) -> Result<PipelineBundle, FslError> {
    project_x86(input, address, None, backend_id)
}
fn project_x86(
    input: &Path,
    address: u64,
    size: Option<usize>,
    backend_id: &str,
) -> Result<PipelineBundle, FslError> {
    let registry = BackendRegistry::with_builtin_sources();
    let backend = registry
        .get(backend_id)
        .ok_or_else(|| FslError::at(1, 1, "unregistered source backend"))?;
    let package = owned_x86_package()?;
    let program = match size {
        Some(size) => X86Program::from_owned_binary(&package, input, address, size)?,
        None => X86Program::from_owned_entry(&package, input, address)?,
    };
    let selection = match size {
        Some(size) => format!("caller-supplied-executable-span address=0x{address:x} size={size}"),
        None => format!(
            "conservative-straight-line-entry address=0x{address:x} size={} bound=65536-bytes/4096-instructions terminal=near-ret; source-function-boundary=unproven",
            program.byte_length()
        ),
    };
    emit_program(&package, &program, backend, &selection)
}

/// Caller-supplied owned profile catalog -> canonical loader -> existing x86 FIR.
/// The profile pins the exact encoding/semantic package. No ABI is inferred.
pub fn project_profiled_x86(
    profiles: &crate::profiles::ProfilePackage,
    input: &Path,
    address: u64,
    size: usize,
    backend_id: &str,
) -> Result<PipelineBundle, FslError> {
    if !(1..=65536).contains(&size) {
        return Err(FslError::at(1, 1, "binary span size must be 1..65536"));
    }
    let registry = BackendRegistry::with_builtin_sources();
    let backend = registry
        .get(backend_id)
        .ok_or_else(|| FslError::at(1, 1, "unregistered source backend"))?;
    let loaded = fission_loader::LoadedBinary::from_file_with_catalog(input, &profiles.catalog)
        .map_err(|e| FslError::at(1, 1, format!("loader: {e}")))?;
    let package = owned_x86_package()?;
    let package_bytes = package.encode_binary()?;
    use sha2::{Digest, Sha256};
    let package_hash = crate::sequence::hex(&Sha256::digest(&package_bytes));
    if loaded.profile.decoder != package.decoder() || loaded.profile.package_sha256 != package_hash
    {
        return Err(FslError::at(
            1,
            1,
            "owned profile decoder/package binding mismatch",
        ));
    }
    let program = X86Program::from_loaded_binary(&package, &loaded.binary, address, size)?;
    let selection = format!("caller-supplied-executable-span address=0x{address:x} size={size}");
    let mut bundle = emit_program(&package, &program, backend, &selection)?;
    bundle.pipeline_report = format!("loader-profile-contract=1\nloader-route=owned-catalog\nselected-profile={}\ncatalog-sha256={}\nbound-package-sha256={}\nabi=unselected\ntarget-facts={:?}\n{}",
        loaded.profile.id,crate::sequence::hex(&Sha256::digest(profiles.encode_binary()?)),package_hash,loaded.target_facts,bundle.pipeline_report);
    Ok(bundle)
}

fn emit_program(
    package: &X86Package,
    program: &X86Program,
    backend: &dyn OutputBackend,
    selection: &str,
) -> Result<PipelineBundle, FslError> {
    let rules = package.rule_count();
    let bodies = package.bodies().instructions.len();
    let profile = package.encode_binary()?;
    let plan = backend
        .plan(
            ProjectionInput::X86Window(program),
            &TargetProfile::for_backend(backend),
        )
        .map_err(|e| FslError::at(1, 1, e.to_string()))?;
    let projection = backend
        .emit(&plan)
        .map_err(|e| FslError::at(1, 1, e.to_string()))?;
    let fir = program.emit(OutputLayer::Fir)?;
    let report=format!("pipeline-contract=1\nprofile=owned-x86-v2\nspec-input=embedded-fsl\nsemantic-owner=canonical-fir\nloader-architecture=ELF-machine-class-endian-facts; no legacy LoadSpec\nroute=file-loader -> owned-fsl-decode -> canonical-fir -> checked-source-backend\nlegacy-fallback=none\nbackend={}\nselection={selection}\ncoverage={rules} encoding rules; {bodies} width-specific semantic bodies; not full x86\nlimits=no general function discovery, source prototype or ABI recovery; bounded flat memory; no guest fault/concurrency model\ncompilation=not-performed equivalence=not-performed\n",backend.descriptor().id);
    Ok(PipelineBundle {
        projection,
        fir,
        profile,
        pipeline_report: report,
    })
}
