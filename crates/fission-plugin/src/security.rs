//! Explicit trust tiers, manifest/origin verification, and session-scoped
//! capability authorization for Fission plugins (#1173).
//!
//! Trust boundary summary:
//! - [`PluginTrustTier::TrustedNative`]: In-process dynamic libraries / Rust trait objects
//!   run in the host address space with **full host process privileges**. Native loading
//!   requires explicit operator opt-in and verified origin/manifest identity; SHA-256 is
//!   identity, not proof of trust, and native plugins are never advertised as sandboxed.
//! - [`PluginTrustTier::ConstrainedScript`]: Embedded Rhai scripts (`fission-script`) run
//!   with `Engine::new_raw`, curated packages, and bounded host bindings; this constrains
//!   exposed APIs rather than providing OS kernel isolation.
//! - [`PluginTrustTier::IsolatedExternalWorker`]: Out-of-process external SDK workers
//!   communicate only over bounded stdio JSON-RPC (`crate::worker`) and are untrusted until
//!   the host authorizes each requested operation against an active [`SessionId`] grant.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Current native Rust-plugin ABI contract version checked before construction.
pub const NATIVE_PLUGIN_ABI_VERSION: u32 = 1;

/// Execution trust tier of a plugin or scripting extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginTrustTier {
    /// In-process native shared library or host-registered Rust plugin.
    /// Carries full host process privileges; capability grants do not apply.
    TrustedNative,
    /// Embedded Rhai scripting runtime with `Engine::new_raw` and resource caps.
    ConstrainedScript,
    /// Out-of-process external worker over bounded stdio RPC with host capability grants.
    IsolatedExternalWorker,
}

impl PluginTrustTier {
    /// Whether this tier executes inside a separate OS process boundary.
    ///
    /// Note: process separation + OS resource limits reduce blast radius, but do
    /// not constitute full kernel MAC sandboxing unless paired with platform confinement.
    #[must_use]
    pub const fn is_os_isolated(self) -> bool {
        matches!(self, Self::IsolatedExternalWorker)
    }

    /// Whether host-enforced per-operation [`PluginCapability`] grants apply at runtime.
    ///
    /// Always `false` for [`Self::TrustedNative`], because code mapped into the host
    /// address space can bypass host trait mediation.
    #[must_use]
    pub const fn supports_capability_grants(self) -> bool {
        !matches!(self, Self::TrustedNative)
    }

    /// Human-readable description of the security guarantee (and non-guarantee).
    #[must_use]
    pub const fn security_contract(self) -> &'static str {
        match self {
            Self::TrustedNative => {
                "trusted native in-process code: full host privileges, not sandboxed"
            }
            Self::ConstrainedScript => {
                "constrained Rhai host bindings: curated API + op limits, not OS-isolated"
            }
            Self::IsolatedExternalWorker => {
                "external worker process: deny-by-default host capability grants over bounded stdio RPC"
            }
        }
    }
}

/// Host capabilities separated along read-only vs mutating trust boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    /// Read-only inspection of loaded binary facts and analysis outputs.
    ReadAnalysis,
    /// Persistent edits to Fission project metadata / annotations.
    EditProject,
    /// Mutating live emulator/debugger execution state (steps, breakpoints, writes).
    MutateLiveExecution,
}

impl fmt::Display for PluginCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::ReadAnalysis => "read_analysis",
            Self::EditProject => "edit_project",
            Self::MutateLiveExecution => "mutate_live_execution",
        };
        f.write_str(s)
    }
}

/// Explicit deny-by-default set of [`PluginCapability`] grants.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySet(BTreeSet<PluginCapability>);

impl CapabilitySet {
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn read_only() -> Self {
        Self::from_capabilities([PluginCapability::ReadAnalysis])
    }

    #[must_use]
    pub fn from_capabilities(caps: impl IntoIterator<Item = PluginCapability>) -> Self {
        Self(caps.into_iter().collect())
    }

    #[must_use]
    pub fn contains(&self, cap: PluginCapability) -> bool {
        self.0.contains(&cap)
    }

    pub fn grant(&mut self, cap: PluginCapability) {
        self.0.insert(cap);
    }

    pub fn revoke(&mut self, cap: PluginCapability) -> bool {
        self.0.remove(&cap)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = PluginCapability> + '_ {
        self.0.iter().copied()
    }
}

/// Signed/declared plugin identity manifest validated prior to loading native artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub abi_version: u32,
    pub trust_tier: PluginTrustTier,
    pub sha256: String,
    #[serde(default)]
    pub requested_capabilities: CapabilitySet,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.id.trim().is_empty() {
            return Err(PolicyError::InvalidManifest(
                "manifest id must not be empty".into(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(PolicyError::InvalidManifest(
                "manifest version must not be empty".into(),
            ));
        }
        if !is_valid_sha256_hex(&self.sha256) {
            return Err(PolicyError::InvalidManifest(format!(
                "manifest sha256 must be 64 hex characters, got '{}'",
                self.sha256
            )));
        }
        Ok(())
    }
}

fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Compute lowercase hexadecimal SHA-256 digest of a file on disk.
pub fn compute_file_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Failure from enforcing [`PluginSecurityPolicy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    NativePluginsDisabled,
    ArtifactNotFound(PathBuf),
    NotARegularFile(PathBuf),
    DeniedOrigin(PathBuf),
    UnapprovedOrigin(PathBuf),
    MissingManifest,
    InvalidManifest(String),
    WrongTrustTier {
        expected: PluginTrustTier,
        actual: PluginTrustTier,
    },
    AbiVersionMismatch {
        expected: u32,
        actual: u32,
    },
    DigestMismatch {
        expected: String,
        actual: String,
    },
    UnapprovedDigest(String),
    IoError(String),
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NativePluginsDisabled => {
                write!(
                    f,
                    "native plugins are disabled by security policy (explicit opt-in required)"
                )
            }
            Self::ArtifactNotFound(p) => {
                write!(f, "plugin artifact does not exist: {}", p.display())
            }
            Self::NotARegularFile(p) => {
                write!(f, "plugin artifact is not a regular file: {}", p.display())
            }
            Self::DeniedOrigin(p) => {
                write!(
                    f,
                    "refusing to load plugin from denied origin (e.g. target binary directory): {}",
                    p.display()
                )
            }
            Self::UnapprovedOrigin(p) => {
                write!(
                    f,
                    "plugin path is outside approved plugin origins: {}",
                    p.display()
                )
            }
            Self::MissingManifest => {
                write!(
                    f,
                    "native plugin manifest is required by policy before loading"
                )
            }
            Self::InvalidManifest(msg) => write!(f, "invalid plugin manifest: {msg}"),
            Self::WrongTrustTier { expected, actual } => write!(
                f,
                "plugin manifest trust tier {:?} does not match expected {:?}",
                actual, expected
            ),
            Self::AbiVersionMismatch { expected, actual } => write!(
                f,
                "plugin ABI version {actual} does not match host contract {expected}"
            ),
            Self::DigestMismatch { expected, actual } => write!(
                f,
                "plugin SHA-256 mismatch (manifest {expected}, actual {actual})"
            ),
            Self::UnapprovedDigest(actual) => {
                write!(f, "plugin digest {actual} is not in the approved allowlist")
            }
            Self::IoError(msg) => write!(f, "plugin I/O error during verification: {msg}"),
        }
    }
}

impl std::error::Error for PolicyError {}

/// Evidence returned when a native library passed origin, manifest, and digest checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedNativeArtifact {
    pub canonical_path: PathBuf,
    pub sha256: String,
}

fn canonicalize_best_effort(path: &Path) -> PathBuf {
    if let Ok(canon) = std::fs::canonicalize(path) {
        return canon;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(canon_parent) = std::fs::canonicalize(parent) {
            return canon_parent.join(name);
        }
    }
    path.to_path_buf()
}

/// Host policy governing native plugin loading and origin restrictions.
///
/// Defaults to deny-all (`PluginSecurityPolicy::deny_all()` / `Default`):
/// - native dynamic libraries cannot load without explicit operator opt-in,
/// - auto-loading from discovered target directories or metadata is disabled,
/// - when native loading is opted in, manifests + approved origins are enforced by default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSecurityPolicy {
    pub allow_native_plugins: bool,
    pub require_native_manifest: bool,
    pub approved_origins: Vec<PathBuf>,
    pub denied_origins: Vec<PathBuf>,
    pub approved_native_digests: BTreeSet<String>,
    /// Never automatically execute code discovered alongside a target binary or metadata.
    pub auto_load_discovered: bool,
}

impl Default for PluginSecurityPolicy {
    fn default() -> Self {
        Self::deny_all()
    }
}

impl PluginSecurityPolicy {
    /// Fail-closed default policy: native loading disabled, no approved origins, no auto-loading.
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            allow_native_plugins: false,
            require_native_manifest: true,
            approved_origins: Vec::new(),
            denied_origins: Vec::new(),
            approved_native_digests: BTreeSet::new(),
            auto_load_discovered: false,
        }
    }

    /// Explicit operator opt-in for native dynamic plugins under approved origin directories,
    /// requiring a validated [`PluginManifest`].
    #[must_use]
    pub fn allow_trusted_native_origins<I, P>(origins: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let approved_origins = origins
            .into_iter()
            .map(|p| canonicalize_best_effort(p.as_ref()))
            .collect();
        Self {
            allow_native_plugins: true,
            require_native_manifest: true,
            approved_origins,
            denied_origins: Vec::new(),
            approved_native_digests: BTreeSet::new(),
            auto_load_discovered: false,
        }
    }

    /// Explicit opt-in for native plugins inside approved origin directories without
    /// requiring a sidecar manifest (still checks origin, denied directories, and digest allowlist).
    #[must_use]
    pub fn allow_trusted_native_without_manifest<I, P>(origins: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut policy = Self::allow_trusted_native_origins(origins);
        policy.require_native_manifest = false;
        policy
    }

    /// Block any plugin located inside `dir` (wins even over `approved_origins`).
    pub fn forbid_origin(&mut self, dir: impl AsRef<Path>) {
        let canon = canonicalize_best_effort(dir.as_ref());
        if !self.denied_origins.contains(&canon) {
            self.denied_origins.push(canon);
        }
    }

    /// Forbid loading plugins located alongside an analyzed target binary.
    pub fn forbid_target_binary_dir(&mut self, binary_path: &Path) {
        if let Some(parent) = binary_path.parent()
            && !parent.as_os_str().is_empty()
        {
            self.forbid_origin(parent);
        }
    }

    /// Restrict native plugin loads to an explicit allowlist of SHA-256 hex digests.
    pub fn add_approved_digest(&mut self, sha256: impl AsRef<str>) -> Result<(), PolicyError> {
        let normalized = sha256.as_ref().trim().to_ascii_lowercase();
        if !is_valid_sha256_hex(&normalized) {
            return Err(PolicyError::InvalidManifest(format!(
                "approved digest must be 64 hex chars, got '{}'",
                sha256.as_ref()
            )));
        }
        self.approved_native_digests.insert(normalized);
        Ok(())
    }

    /// Verify whether a native plugin artifact is authorized to be opened (`dlopen`).
    pub fn authorize_native_load(
        &self,
        path: &Path,
        manifest: Option<&PluginManifest>,
    ) -> Result<VerifiedNativeArtifact, PolicyError> {
        if !self.allow_native_plugins {
            return Err(PolicyError::NativePluginsDisabled);
        }

        let candidate = canonicalize_best_effort(path);
        for denied in &self.denied_origins {
            if candidate.starts_with(denied) || path.starts_with(denied) {
                return Err(PolicyError::DeniedOrigin(candidate));
            }
        }

        let in_approved = self
            .approved_origins
            .iter()
            .any(|root| candidate.starts_with(root) || path.starts_with(root));
        if !in_approved {
            return Err(PolicyError::UnapprovedOrigin(candidate));
        }

        let meta = match std::fs::metadata(&candidate) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(PolicyError::ArtifactNotFound(candidate));
            }
            Err(err) => return Err(PolicyError::IoError(err.to_string())),
        };
        if !meta.is_file() {
            return Err(PolicyError::NotARegularFile(candidate));
        }

        let actual_sha256 =
            compute_file_sha256(&candidate).map_err(|e| PolicyError::IoError(e.to_string()))?;

        match manifest {
            Some(m) => {
                m.validate()?;
                if m.trust_tier != PluginTrustTier::TrustedNative {
                    return Err(PolicyError::WrongTrustTier {
                        expected: PluginTrustTier::TrustedNative,
                        actual: m.trust_tier,
                    });
                }
                if m.abi_version != NATIVE_PLUGIN_ABI_VERSION {
                    return Err(PolicyError::AbiVersionMismatch {
                        expected: NATIVE_PLUGIN_ABI_VERSION,
                        actual: m.abi_version,
                    });
                }
                if !m.sha256.eq_ignore_ascii_case(&actual_sha256) {
                    return Err(PolicyError::DigestMismatch {
                        expected: m.sha256.to_ascii_lowercase(),
                        actual: actual_sha256,
                    });
                }
            }
            None if self.require_native_manifest => {
                return Err(PolicyError::MissingManifest);
            }
            None => {}
        }

        if !self.approved_native_digests.is_empty()
            && !self.approved_native_digests.contains(&actual_sha256)
        {
            return Err(PolicyError::UnapprovedDigest(actual_sha256));
        }

        Ok(VerifiedNativeArtifact {
            canonical_path: candidate,
            sha256: actual_sha256,
        })
    }
}

/// Host-issued identifier for one isolated analysis/plugin session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

/// Opaque capability grant identifier bound to one `(SessionId, PluginId)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GrantId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityGrant {
    pub id: GrantId,
    pub session_id: SessionId,
    pub plugin_id: String,
    pub trust_tier: PluginTrustTier,
    pub capabilities: CapabilitySet,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationError {
    UnsupportedTierForGrant(PluginTrustTier),
    UnknownGrant(GrantId),
    GrantRevoked(GrantId),
    WrongSession {
        grant_session: SessionId,
        request_session: SessionId,
    },
    PermissionDenied {
        plugin_id: String,
        capability: PluginCapability,
    },
}

impl AuthorizationError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedTierForGrant(_) => "unsupported_tier",
            Self::UnknownGrant(_) => "unknown_grant",
            Self::GrantRevoked(_) => "revoked",
            Self::WrongSession { .. } => "wrong_session",
            Self::PermissionDenied { .. } => "permission_denied",
        }
    }
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTierForGrant(tier) => write!(
                f,
                "trust tier {:?} runs in-process and cannot be governed by host capability grants",
                tier
            ),
            Self::UnknownGrant(id) => write!(f, "unknown capability grant {}", id.0),
            Self::GrantRevoked(id) => write!(f, "capability grant {} has been revoked", id.0),
            Self::WrongSession {
                grant_session,
                request_session,
            } => write!(
                f,
                "grant belongs to session {}, not active session {}",
                grant_session.0, request_session.0
            ),
            Self::PermissionDenied {
                plugin_id,
                capability,
            } => write!(
                f,
                "plugin '{plugin_id}' lacks required capability '{capability}'"
            ),
        }
    }
}

impl std::error::Error for AuthorizationError {}

/// Host-side authority enforcing least-privilege grants, session binding, and revocation.
#[derive(Debug, Default)]
pub struct SessionAuthorizer {
    next_session_id: u64,
    next_grant_id: u64,
    grants: BTreeMap<GrantId, CapabilityGrant>,
}

impl SessionAuthorizer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_session(&mut self) -> SessionId {
        self.next_session_id += 1;
        SessionId(self.next_session_id)
    }

    /// Issue a least-privilege grant bound to `session_id` for a non-native plugin tier.
    pub fn issue_grant(
        &mut self,
        session_id: SessionId,
        plugin_id: impl Into<String>,
        trust_tier: PluginTrustTier,
        capabilities: CapabilitySet,
    ) -> Result<GrantId, AuthorizationError> {
        if !trust_tier.supports_capability_grants() {
            return Err(AuthorizationError::UnsupportedTierForGrant(trust_tier));
        }
        self.next_grant_id += 1;
        let id = GrantId(self.next_grant_id);
        self.grants.insert(
            id,
            CapabilityGrant {
                id,
                session_id,
                plugin_id: plugin_id.into(),
                trust_tier,
                capabilities,
                revoked: false,
            },
        );
        Ok(id)
    }

    /// Authorize an operation using the host-held `grant_id` against `active_session_id`.
    pub fn authorize(
        &self,
        grant_id: GrantId,
        active_session_id: SessionId,
        required: PluginCapability,
    ) -> Result<(), AuthorizationError> {
        let grant = self
            .grants
            .get(&grant_id)
            .ok_or(AuthorizationError::UnknownGrant(grant_id))?;
        if grant.revoked {
            return Err(AuthorizationError::GrantRevoked(grant_id));
        }
        if grant.session_id != active_session_id {
            return Err(AuthorizationError::WrongSession {
                grant_session: grant.session_id,
                request_session: active_session_id,
            });
        }
        if !grant.capabilities.contains(required) {
            return Err(AuthorizationError::PermissionDenied {
                plugin_id: grant.plugin_id.clone(),
                capability: required,
            });
        }
        Ok(())
    }

    pub fn revoke_grant(&mut self, grant_id: GrantId) -> bool {
        if let Some(grant) = self.grants.get_mut(&grant_id) {
            grant.revoked = true;
            true
        } else {
            false
        }
    }

    pub fn revoke_session(&mut self, session_id: SessionId) -> usize {
        let mut count = 0;
        for grant in self.grants.values_mut() {
            if grant.session_id == session_id && !grant.revoked {
                grant.revoked = true;
                count += 1;
            }
        }
        count
    }

    #[must_use]
    pub fn is_grant_active(&self, grant_id: GrantId) -> bool {
        self.grants.get(&grant_id).is_some_and(|g| !g.revoked)
    }
}
