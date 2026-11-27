# Plugin & Script Security Policy

Fission executes extensions across three explicit trust tiers defined in
`crates/fission-plugin/src/security.rs` (`PluginTrustTier`). These tiers are
strictly distinguished because in-process code cannot be capability-sandboxed by
the host.

## Trust Tiers

| Tier | Mechanism | Host Privileges | Isolation Contract |
|---|---|---|---|
| `TrustedNative` | Dynamic libraries (`.so`/`.dylib`/`.dll`) or host-registered Rust `FissionPlugin` | **Full host process privileges** (same address space and allocator contract) | **Not sandboxed.** Requires explicit operator opt-in (`PluginSecurityPolicy`), approved origin directory (never target binary dir), and verified manifest SHA-256 + `NATIVE_PLUGIN_ABI_VERSION`. |
| `ConstrainedScript` | Rhai runtime (`crates/fission-script`) using `Engine::new_raw` | Curated host API (`binary`, `emit`, optional `machine`) and integer/array/string/op/output limits (`set_max_modules(0)`) | **Constrained host bindings, not OS isolation.** Shares the host process. |
| `IsolatedExternalWorker` | External SDK child process (`crates/fission-plugin/src/worker.rs`) over bounded stdio JSON-RPC | Zero direct host memory access; all host requests mediated by `SessionAuthorizer` | **Process + resource separation.** Cleared environment, stdio-only transport, POSIX `setrlimit` caps (`RLIMIT_CPU`, `RLIMIT_NOFILE`, `RLIMIT_FSIZE`, optional `RLIMIT_AS`), bounded request/response frames, timeouts, and fail-closed `require_kernel_sandbox` option when kernel MAC confinement (seccomp/Landlock/Seatbelt) is absent. |

## Core Rules

1. **No implicit execution from analyzed binaries or metadata.**
   - `PluginManager::new()` defaults to `PluginSecurityPolicy::deny_all()`.
   - Default plugin discovery does not probe `./plugins` in the current working directory.
   - `PluginSecurityPolicy::forbid_target_binary_dir` explicitly blocks any path inside the target binary's directory, overriding `approved_origins`.

2. **SHA-256 is identity, not proof of trust.**
   - `PluginManifest` verification binds `(id, version, abi_version, trust_tier, sha256)` before calling `dlopen` (`libloading::Library::new`).
   - Optional `fission_plugin_abi_version` symbols are checked immediately upon open before calling `fission_plugin_create`.
   - `SessionAuthorizer::issue_grant` rejects `TrustedNative` because native code inside the host address space cannot be constrained by capability grants.

3. **Least-privilege capability separation & revocation.**
   - Capabilities are separated into `ReadAnalysis`, `EditProject`, and `MutateLiveExecution` (`CapabilitySet::none()` by default).
   - Every host RPC callback from an external worker is authorized at the host boundary against `(grant_id, active_session_id, required_capability)`. Claimed session mismatches (`wrong_session`), missing capabilities (`permission_denied`), and revoked grants (`revoked`) are denied before invoking host handlers.
   - Grants are automatically revoked when a worker session is cancelled, times out, crashes, overflows IPC bounds, or is dropped.

4. **Explicit reporting of unsupported OS isolation.**
   - `OsConfinementReport` explicitly records `kernel_sandbox_active: false` and lists `unsupported_protections` when kernel MAC confinement is unavailable on the platform/runner.
   - Callers that require kernel MAC confinement set `ExternalWorkerConfig { require_kernel_sandbox: true, .. }` to fail closed with `WorkerError::UnsupportedIsolation`.
