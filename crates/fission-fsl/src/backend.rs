//! Source projections over borrowed canonical FIR. A plan admits an explicit
//! execution contract; it is not a compilation result or equivalence proof.
use crate::{CompiledInstruction, FirOp, FslError, OutputLayer, StackContract};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionContext {
    VmStackInstruction,
    RegisterInstruction,
    WaveInstruction,
    X86BoundedWindow,
}
const SOURCE_CONTEXTS: &[ProjectionContext] = &[
    ProjectionContext::VmStackInstruction,
    ProjectionContext::RegisterInstruction,
    ProjectionContext::WaveInstruction,
    #[cfg(feature = "x86")]
    ProjectionContext::X86BoundedWindow,
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendDescriptor {
    pub id: &'static str,
    pub revision: u32,
    pub dialect: &'static str,
    pub extension: &'static str,
    pub contexts: &'static [ProjectionContext],
}
impl BackendDescriptor {
    fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.revision != 0
            && !self.dialect.is_empty()
            && !self.extension.is_empty()
            && !self.contexts.is_empty()
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            && self
                .dialect
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'+'))
            && self.extension.bytes().all(|b| b.is_ascii_alphanumeric())
            && self
                .contexts
                .iter()
                .enumerate()
                .all(|(i, c)| !self.contexts[..i].contains(c))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimePolicy {
    AllowEmbedded,
    ForbidEmbedded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryModel {
    BoundedFlatBytes,
    ArchitecturalFaults,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetProfile {
    pub dialect: String,
    pub runtime: RuntimePolicy,
    pub memory: MemoryModel,
}
impl TargetProfile {
    pub fn for_backend(backend: &(impl OutputBackend + ?Sized)) -> Self {
        Self {
            dialect: backend.descriptor().dialect.into(),
            runtime: RuntimePolicy::AllowEmbedded,
            memory: MemoryModel::BoundedFlatBytes,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum ProjectionInput<'a> {
    Instruction {
        body: &'a CompiledInstruction,
        symbol: &'a str,
    },
    #[cfg(feature = "x86")]
    X86Window(&'a crate::x86::X86Program),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Requirement {
    BitVectorSlots,
    BodyControl,
    VmStack,
    RegistersAndFlags,
    MaskedLanes,
    GuestPc,
    ReadOnlyByteMemory,
    TransactionalByteMemory,
    X86Window,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectionCode {
    ProfileMismatch,
    RuntimeForbidden,
    MemoryModelUnsupported,
    InvalidSymbol,
    UnsupportedSemantics,
    BackendMismatch,
    ContextUnsupported,
    InvalidFir,
    InvalidDescriptor,
    BackendAlreadyRegistered,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedReport {
    pub backend: &'static str,
    pub code: RejectionCode,
    pub body: Option<String>,
    pub operation: Option<usize>,
    pub message: String,
}
impl fmt::Display for UnsupportedReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "backend={} code={:?} body={:?} operation={:?}: {}",
            self.backend, self.code, self.body, self.operation, self.message
        )
    }
}
impl std::error::Error for UnsupportedReport {}
/// Fields are private: a plan can only be made through checked admission. Its
/// borrow prevents editing the canonical input between planning and emission.
#[derive(Clone, Debug)]
pub struct ProjectionPlan<'a> {
    descriptor: BackendDescriptor,
    input: ProjectionInput<'a>,
    target: TargetProfile,
    requirements: Vec<Requirement>,
}
impl<'a> ProjectionPlan<'a> {
    /// Shared admission for the currently supported instruction/window contexts.
    /// A new backend may reuse these contexts; new semantic domains need their
    /// own canonical contracts before this admission can be expanded.
    pub fn checked(
        descriptor: BackendDescriptor,
        input: ProjectionInput<'a>,
        target: &TargetProfile,
    ) -> Result<Self, UnsupportedReport> {
        let reject = |code, message: &str| UnsupportedReport {
            backend: descriptor.id,
            code,
            body: None,
            operation: None,
            message: message.into(),
        };
        if !descriptor.valid() {
            return Err(reject(
                RejectionCode::InvalidDescriptor,
                "invalid backend descriptor",
            ));
        }
        if target.dialect != descriptor.dialect {
            return Err(reject(
                RejectionCode::ProfileMismatch,
                "requested dialect is not implemented by this backend",
            ));
        }
        if target.runtime == RuntimePolicy::ForbidEmbedded {
            return Err(reject(
                RejectionCode::RuntimeForbidden,
                "current projections require embedded state/guard implementations",
            ));
        }
        if target.memory != MemoryModel::BoundedFlatBytes {
            return Err(reject(
                RejectionCode::MemoryModelUnsupported,
                "architectural faults and concurrent memory are not modeled by current projections",
            ));
        }
        let context = match input {
            ProjectionInput::Instruction { body, .. } => {
                body.validate().map_err(|e| UnsupportedReport {
                    backend: descriptor.id,
                    code: RejectionCode::InvalidFir,
                    body: Some(body.name.clone()),
                    operation: None,
                    message: e.to_string(),
                })?;
                if body.ops.iter().any(FirOp::requires_lane_version) {
                    ProjectionContext::WaveInstruction
                } else if body.ops.iter().any(FirOp::requires_state_version) {
                    ProjectionContext::RegisterInstruction
                } else {
                    ProjectionContext::VmStackInstruction
                }
            }
            #[cfg(feature = "x86")]
            ProjectionInput::X86Window(_) => ProjectionContext::X86BoundedWindow,
        };
        if !descriptor.contexts.contains(&context) {
            return Err(reject(
                RejectionCode::ContextUnsupported,
                "backend does not declare this FIR input context",
            ));
        }
        let mut requirements = BTreeSet::from([Requirement::BitVectorSlots]);
        match input {
            ProjectionInput::Instruction { body, symbol } => {
                if !symbol.starts_with("fsl_")
                    || !symbol
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_')
                {
                    return Err(reject(
                        RejectionCode::InvalidSymbol,
                        "symbol must be an ASCII identifier starting with fsl_",
                    ));
                }
                let admission = if body.ops.iter().any(FirOp::requires_lane_version) {
                    crate::WaveContract::for_instruction(body).map(|_| ())
                } else if body.ops.iter().any(FirOp::requires_state_version) {
                    crate::state::admit(body)
                } else {
                    StackContract::for_instruction(body).map(|_| ())
                };
                admission.map_err(|e| body_rejection(descriptor, body, e))?;
                collect(body, &mut requirements);
            }
            #[cfg(feature = "x86")]
            ProjectionInput::X86Window(program) => {
                requirements.insert(Requirement::X86Window);
                for body in program.projection_bodies() {
                    crate::state::admit_context(body, true, true)
                        .map_err(|e| body_rejection(descriptor, body, e))?;
                    collect(body, &mut requirements);
                }
            }
        }
        Ok(Self {
            descriptor,
            input,
            target: target.clone(),
            requirements: requirements.into_iter().collect(),
        })
    }
    pub fn descriptor(&self) -> BackendDescriptor {
        self.descriptor
    }
    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }
    pub fn input(&self) -> ProjectionInput<'a> {
        self.input
    }
    pub fn target(&self) -> &TargetProfile {
        &self.target
    }
    pub fn describe(&self) -> String {
        format!("projection-contract=1\nbackend={} revision={} dialect={}\nsupport=admitted-under-explicit-contract\nrequirements={:?}\nruntime=embedded-in-source memory=bounded-flat-bytes\nlimits=no guest fault/concurrency model; no source prototype or full ABI recovery\ncompilation=not-performed equivalence=not-performed\n",self.descriptor.id,self.descriptor.revision,self.descriptor.dialect,self.requirements)
    }
}
fn body_rejection(
    descriptor: BackendDescriptor,
    body: &CompiledInstruction,
    e: FslError,
) -> UnsupportedReport {
    UnsupportedReport {
        backend: descriptor.id,
        code: RejectionCode::UnsupportedSemantics,
        body: Some(body.name.clone()),
        operation: body.ops.iter().position(|op| {
            matches!(op, FirOp::Unsupported)
                || e.message.starts_with("guest PC effects require")
                    && matches!(
                        op,
                        FirOp::GuestPcRead { .. } | FirOp::GuestNextPcWrite { .. }
                    )
                || e.message.starts_with("memory FIR requires")
                    && matches!(
                        op,
                        FirOp::MemoryLoadLittle { .. } | FirOp::MemoryStoreLittle { .. }
                    )
        }),
        message: e.to_string(),
    }
}
fn collect(body: &CompiledInstruction, out: &mut BTreeSet<Requirement>) {
    if crate::control::has_control(body) {
        out.insert(Requirement::BodyControl);
    }
    for op in &body.ops {
        let feature = match op {
            FirOp::VmStackPop { .. } | FirOp::VmStackPush { .. } => Some(Requirement::VmStack),
            FirOp::RegisterRead { .. }
            | FirOp::RegisterWrite { .. }
            | FirOp::FlagRead { .. }
            | FirOp::FlagWrite { .. } => Some(Requirement::RegistersAndFlags),
            FirOp::LaneRead { .. } | FirOp::LaneWrite { .. } | FirOp::LaneMaskRead { .. } => {
                Some(Requirement::MaskedLanes)
            }
            FirOp::GuestPcRead { .. } | FirOp::GuestNextPcWrite { .. } => {
                Some(Requirement::GuestPc)
            }
            FirOp::MemoryLoadLittle { .. } => Some(Requirement::ReadOnlyByteMemory),
            FirOp::MemoryStoreLittle { .. } => Some(Requirement::TransactionalByteMemory),
            FirOp::IntConstant { .. }
            | FirOp::IntCompare { .. }
            | FirOp::IntConvert { .. }
            | FirOp::IntBinary { .. }
            | FirOp::FieldRead { .. }
            | FirOp::IntAddWrap { .. }
            | FirOp::IntAddCarry { .. }
            | FirOp::IntAddCarryIn { .. }
            | FirOp::IntAddWrapCarry { .. }
            | FirOp::Unsupported => None,
        };
        if let Some(feature) = feature {
            out.insert(feature);
        }
    }
    if out.contains(&Requirement::TransactionalByteMemory) {
        out.remove(&Requirement::ReadOnlyByteMemory);
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceArtifact {
    pub name: String,
    pub source: String,
}
/// Symbol-level association only; statement-level source maps are future work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginAssociation {
    pub body: String,
    pub source_symbol: String,
    pub guest_address: Option<u64>,
    pub raw_hex: Option<String>,
    pub input_offset: Option<u32>,
    pub byte_length: Option<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRecipe {
    pub tool: &'static str,
    pub arguments: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct ArtifactBundle {
    pub descriptor: BackendDescriptor,
    pub sources: Vec<SourceArtifact>,
    pub build: BuildRecipe,
    pub origins: Vec<OriginAssociation>,
    /// Admission/evidence metadata, not a semantic certification.
    pub report: String,
}
pub trait OutputBackend {
    fn descriptor(&self) -> BackendDescriptor;
    fn plan<'a>(
        &self,
        input: ProjectionInput<'a>,
        target: &TargetProfile,
    ) -> Result<ProjectionPlan<'a>, UnsupportedReport> {
        ProjectionPlan::checked(self.descriptor(), input, target)
    }
    fn emit(&self, plan: &ProjectionPlan<'_>) -> Result<ArtifactBundle, UnsupportedReport>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceBackend {
    C,
    Rust,
}
impl SourceBackend {
    pub fn from_layer(layer: OutputLayer) -> Option<Self> {
        match layer {
            OutputLayer::C => Some(Self::C),
            OutputLayer::Rust => Some(Self::Rust),
            _ => None,
        }
    }
    fn layer(self) -> OutputLayer {
        match self {
            Self::C => OutputLayer::C,
            Self::Rust => OutputLayer::Rust,
        }
    }
}
impl OutputBackend for SourceBackend {
    fn descriptor(&self) -> BackendDescriptor {
        match self {
            Self::C => BackendDescriptor {
                id: "c",
                revision: 1,
                dialect: "c11",
                extension: "c",
                contexts: SOURCE_CONTEXTS,
            },
            Self::Rust => BackendDescriptor {
                id: "rust",
                revision: 1,
                dialect: "rust2021",
                extension: "rs",
                contexts: SOURCE_CONTEXTS,
            },
        }
    }
    fn emit(&self, plan: &ProjectionPlan<'_>) -> Result<ArtifactBundle, UnsupportedReport> {
        let descriptor = self.descriptor();
        if descriptor != plan.descriptor {
            return Err(UnsupportedReport {
                backend: descriptor.id,
                code: RejectionCode::BackendMismatch,
                body: None,
                operation: None,
                message: "plan belongs to a different backend/revision".into(),
            });
        }
        let (source, origins) = match plan.input {
            ProjectionInput::Instruction { body, symbol } => (
                crate::output::emit_instruction_implementation(body, self.layer(), symbol)
                    .map_err(|e| body_rejection(descriptor, body, e))?,
                vec![OriginAssociation {
                    body: body.name.clone(),
                    source_symbol: symbol.into(),
                    guest_address: None,
                    raw_hex: None,
                    input_offset: None,
                    byte_length: None,
                }],
            ),
            #[cfg(feature = "x86")]
            ProjectionInput::X86Window(program) => (
                program
                    .emit_implementation(self.layer())
                    .map_err(|e| UnsupportedReport {
                        backend: descriptor.id,
                        code: RejectionCode::UnsupportedSemantics,
                        body: None,
                        operation: None,
                        message: e.to_string(),
                    })?,
                program.projection_origins(),
            ),
        };
        let name = format!("output.{}", descriptor.extension);
        let build = match self {
            Self::C => BuildRecipe {
                tool: "cc",
                arguments: vec![
                    "-std=c11".into(),
                    "-c".into(),
                    name.clone(),
                    "-o".into(),
                    "output.o".into(),
                ],
            },
            Self::Rust => BuildRecipe {
                tool: "rustc",
                arguments: vec![
                    "--edition=2021".into(),
                    "--crate-type=lib".into(),
                    name.clone(),
                    "-o".into(),
                    "output.rlib".into(),
                ],
            },
        };
        let mut report = plan.describe();
        report.push_str("origin-mapping=symbol-level; statement positions unavailable\n");
        for origin in &origins {
            report.push_str(&format!(
                "origin body={:?} source-symbol={:?} guest-address={:?} raw={:?} input-offset={:?} byte-length={:?}\n",
                origin.body, origin.source_symbol, origin.guest_address, origin.raw_hex, origin.input_offset, origin.byte_length
            ));
        }
        Ok(ArtifactBundle {
            descriptor,
            sources: vec![SourceArtifact { name, source }],
            build,
            origins,
            report,
        })
    }
}

/// In-process extension point. No dynamic libraries or external dependencies are
/// loaded. Duplicate IDs refuse instead of replacing a registered provider.
#[derive(Default)]
pub struct BackendRegistry {
    providers: BTreeMap<&'static str, Box<dyn OutputBackend>>,
}
impl BackendRegistry {
    pub fn with_builtin_sources() -> Self {
        let mut value = Self::default();
        value
            .register(Box::new(SourceBackend::C))
            .expect("valid C descriptor");
        value
            .register(Box::new(SourceBackend::Rust))
            .expect("valid Rust descriptor");
        value
    }
    pub fn register(&mut self, provider: Box<dyn OutputBackend>) -> Result<(), UnsupportedReport> {
        let d = provider.descriptor();
        let reject = |code, message: &str| UnsupportedReport {
            backend: d.id,
            code,
            body: None,
            operation: None,
            message: message.into(),
        };
        if !d.valid() {
            return Err(reject(
                RejectionCode::InvalidDescriptor,
                "invalid backend descriptor",
            ));
        }
        if self.providers.contains_key(d.id) {
            return Err(reject(
                RejectionCode::BackendAlreadyRegistered,
                "backend ID is already registered",
            ));
        }
        self.providers.insert(d.id, provider);
        Ok(())
    }
    pub fn get(&self, id: &str) -> Option<&dyn OutputBackend> {
        self.providers.get(id).map(|provider| provider.as_ref())
    }
    pub fn descriptors(&self) -> Vec<BackendDescriptor> {
        self.providers
            .values()
            .map(|provider| provider.descriptor())
            .collect()
    }
}
