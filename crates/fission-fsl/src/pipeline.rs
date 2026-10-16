//! Asset-independent experimental entry point. Canonical FIR remains the sole
//! semantic owner; unsupported bytes/contracts never route to a legacy engine.
use crate::{
    backend::{ArtifactBundle, BackendRegistry, ProjectionInput, TargetProfile},
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
    let registry = BackendRegistry::with_builtin_sources();
    let backend = registry
        .get(backend_id)
        .ok_or_else(|| FslError::at(1, 1, "unregistered source backend"))?;
    let package = owned_x86_package()?;
    let rules = package.rule_count();
    let bodies = package.bodies().instructions.len();
    let profile = package.encode_binary()?;
    let program = X86Program::from_owned_binary(&package, input, address, size)?;
    let plan = backend
        .plan(
            ProjectionInput::X86Window(&program),
            &TargetProfile::for_backend(backend),
        )
        .map_err(|e| FslError::at(1, 1, e.to_string()))?;
    let projection = backend
        .emit(&plan)
        .map_err(|e| FslError::at(1, 1, e.to_string()))?;
    let fir = program.emit(OutputLayer::Fir)?;
    let report=format!("pipeline-contract=1\nprofile=owned-x86-v2\nspec-input=embedded-fsl\nsemantic-owner=canonical-fir\nloader-architecture=ELF-machine-class-endian-facts; no legacy LoadSpec\nroute=file-loader -> owned-fsl-decode -> canonical-fir -> checked-source-backend\nlegacy-fallback=none\nbackend={}\nselection=caller-supplied-executable-span address=0x{address:x} size={size}\ncoverage={rules} encoding rules; {bodies} width-specific semantic bodies; not full x86\nlimits=no general function discovery, source prototype or ABI recovery; bounded flat memory; no guest fault/concurrency model\ncompilation=not-performed equivalence=not-performed\n",backend.descriptor().id);
    Ok(PipelineBundle {
        projection,
        fir,
        profile,
        pipeline_report: report,
    })
}
