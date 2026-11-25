#[cfg(feature = "interactive_runtime")]
use fission_plugin::plugin::PluginManager;
#[cfg(feature = "interactive_runtime")]
use fission_plugin::plugin::api::{PluginAPI, PluginInfo};
#[cfg(feature = "interactive_runtime")]
use fission_plugin::plugin::{FissionPlugin, PluginContext};
use fission_plugin::security::{
    AuthorizationError, CapabilitySet, NATIVE_PLUGIN_ABI_VERSION, PluginCapability, PluginManifest,
    PluginSecurityPolicy, PluginTrustTier, PolicyError, SessionAuthorizer, compute_file_sha256,
};
use fission_plugin::worker::{
    ExternalWorkerConfig, ExternalWorkerLimits, ExternalWorkerSession, HostOperation,
    HostOperationHandler, WorkerError,
};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn valid_native_manifest(path: &std::path::Path) -> PluginManifest {
    PluginManifest {
        id: "benign.native".into(),
        name: "Benign Native".into(),
        version: "0.4.0".into(),
        abi_version: NATIVE_PLUGIN_ABI_VERSION,
        trust_tier: PluginTrustTier::TrustedNative,
        sha256: compute_file_sha256(path).expect("sha256"),
        requested_capabilities: CapabilitySet::none(),
    }
}

#[test]
fn native_plugin_policy_enforces_opt_in_origins_and_manifest_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let approved_dir = tmp.path().join("approved");
    let target_dir = approved_dir.join("target_sample");
    let unapproved_dir = tmp.path().join("unapproved");
    fs::create_dir_all(&target_dir).unwrap();
    fs::create_dir_all(&unapproved_dir).unwrap();

    let plugin_in_approved = approved_dir.join("benign.dylib");
    let plugin_in_target = target_dir.join("side_by_side.dylib");
    let plugin_in_unapproved = unapproved_dir.join("other.dylib");
    fs::write(&plugin_in_approved, b"not-a-real-dylib-1").unwrap();
    fs::write(&plugin_in_target, b"not-a-real-dylib-2").unwrap();
    fs::write(&plugin_in_unapproved, b"not-a-real-dylib-3").unwrap();

    let mut manifest = valid_native_manifest(&plugin_in_approved);

    // 1. Default deny-all refuses native load even with a matching manifest.
    let deny = PluginSecurityPolicy::deny_all();
    assert_eq!(
        deny.authorize_native_load(&plugin_in_approved, Some(&manifest))
            .unwrap_err(),
        PolicyError::NativePluginsDisabled
    );

    // 2. Opted-in policy still denies unapproved origins and target binary directories.
    let mut policy = PluginSecurityPolicy::allow_trusted_native_origins([&approved_dir]);
    let target_binary = target_dir.join("sample.bin");
    policy.forbid_target_binary_dir(&target_binary);

    assert!(matches!(
        policy.authorize_native_load(&plugin_in_target, None),
        Err(PolicyError::DeniedOrigin(_))
    ));
    assert!(matches!(
        policy.authorize_native_load(&plugin_in_unapproved, None),
        Err(PolicyError::UnapprovedOrigin(_))
    ));

    // 3. Missing manifest rejected by default opt-in.
    assert_eq!(
        policy
            .authorize_native_load(&plugin_in_approved, None)
            .unwrap_err(),
        PolicyError::MissingManifest
    );

    // 4. Wrong trust tier rejected.
    manifest.trust_tier = PluginTrustTier::IsolatedExternalWorker;
    assert!(matches!(
        policy.authorize_native_load(&plugin_in_approved, Some(&manifest)),
        Err(PolicyError::WrongTrustTier { .. })
    ));
    manifest.trust_tier = PluginTrustTier::TrustedNative;

    // 5. ABI version mismatch rejected before dlopen.
    manifest.abi_version = NATIVE_PLUGIN_ABI_VERSION + 1;
    assert!(matches!(
        policy.authorize_native_load(&plugin_in_approved, Some(&manifest)),
        Err(PolicyError::AbiVersionMismatch { .. })
    ));
    manifest.abi_version = NATIVE_PLUGIN_ABI_VERSION;

    // 6. SHA-256 tamper detection rejected.
    let mut tampered_manifest = manifest.clone();
    tampered_manifest.sha256 = "f".repeat(64);
    assert!(matches!(
        policy.authorize_native_load(&plugin_in_approved, Some(&tampered_manifest)),
        Err(PolicyError::DigestMismatch { .. })
    ));

    // 7. Matching manifest passes pre-dlopen verification when digest is in allowlist.
    policy.add_approved_digest("0".repeat(64)).unwrap();
    assert!(matches!(
        policy.authorize_native_load(&plugin_in_approved, Some(&manifest)),
        Err(PolicyError::UnapprovedDigest(_))
    ));
    policy.add_approved_digest(&manifest.sha256).unwrap();
    let verified = policy
        .authorize_native_load(&plugin_in_approved, Some(&manifest))
        .expect("verified artifact");
    assert_eq!(verified.sha256, manifest.sha256);

    // 8. Native tier cannot be misrepresented as capability-governed.
    let mut auth = SessionAuthorizer::new();
    let sid = auth.new_session();
    assert_eq!(
        auth.issue_grant(
            sid,
            "benign.native",
            PluginTrustTier::TrustedNative,
            CapabilitySet::read_only(),
        )
        .unwrap_err(),
        AuthorizationError::UnsupportedTierForGrant(PluginTrustTier::TrustedNative)
    );
    assert!(!PluginTrustTier::TrustedNative.supports_capability_grants());
    assert!(!PluginTrustTier::ConstrainedScript.is_os_isolated());
    assert!(PluginTrustTier::IsolatedExternalWorker.is_os_isolated());
}

#[cfg(feature = "interactive_runtime")]
#[test]
fn trusted_native_in_process_registration_preserves_compatibility() {
    use fission_plugin::contracts::types::PluginType;

    struct BenignPlugin;
    impl FissionPlugin for BenignPlugin {
        fn id(&self) -> &str {
            "benign.inproc"
        }
        fn name(&self) -> &str {
            "Benign In-Process"
        }
    }

    let mut pm = PluginManager::new();
    pm.register_native_plugin(Box::new(BenignPlugin)).unwrap();
    let info: &PluginInfo = pm.get_plugin("benign.inproc").expect("registered");
    assert_eq!(info.plugin_type, PluginType::Native);
    assert_eq!(info.trust_tier, PluginTrustTier::TrustedNative);

    // Also keep Trait imports used cleanly.
    let _check_api_ref: fn(&dyn PluginAPI) = |_| {};
    let _check_ctx_ref: fn(&PluginContext) = |_| {};
}

#[derive(Default)]
struct RecordingHostHandler {
    read_calls: usize,
    edit_calls: usize,
    live_mutate_calls: usize,
}

impl HostOperationHandler for RecordingHostHandler {
    fn handle(&mut self, op: &HostOperation) -> Result<serde_json::Value, String> {
        match op {
            HostOperation::ReadAnalysis { query } => {
                self.read_calls += 1;
                Ok(serde_json::json!({ "query": query, "ok": true }))
            }
            HostOperation::EditProject { target, value } => {
                self.edit_calls += 1;
                Ok(serde_json::json!({ "edited": target, "value": value }))
            }
            HostOperation::MutateLiveExecution { action, address } => {
                self.live_mutate_calls += 1;
                Ok(serde_json::json!({ "action": action, "address": address }))
            }
        }
    }
}

fn find_python3() -> PathBuf {
    for candidate in ["python3", "/usr/bin/python3"] {
        if std::process::Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return PathBuf::from(candidate);
        }
    }
    panic!("python3 is required for external worker isolation integration tests");
}

fn write_worker_script(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, body).unwrap();
    p
}

const RPC_WORKER_PY: &str = r#"
import json
import os
import sys

def call_host(cid, op, claimed_session=None):
    msg = {"type": "host_call", "id": cid, "operation": op}
    if claimed_session is not None:
        msg["claimed_session"] = claimed_session
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()
    line = sys.stdin.readline()
    if not line:
        raise RuntimeError("EOF waiting for host_result")
    return json.loads(line)

for raw in sys.stdin:
    req = json.loads(raw)
    rid = req["id"]
    method = req["method"]
    params = req.get("params", {})

    if method == "probe_read":
        r = call_host(1, {"op": "read_analysis", "query": "entry_point"})
        resp = {"type": "response", "id": rid, "ok": True, "result": r}
    elif method == "probe_escalate_write_and_exec":
        r_read = call_host(1, {"op": "read_analysis", "query": "funcs"})
        r_edit = call_host(2, {"op": "edit_project", "target": "fn_0", "value": "renamed"})
        r_exec = call_host(3, {"op": "mutate_live_execution", "action": "write_mem", "address": 4096})
        resp = {
            "type": "response",
            "id": rid,
            "ok": True,
            "result": {"read": r_read, "edit": r_edit, "exec": r_exec},
        }
    elif method == "probe_wrong_session":
        spoof = params["spoof_session"]
        r = call_host(1, {"op": "read_analysis", "query": "secret"}, claimed_session=spoof)
        resp = {"type": "response", "id": rid, "ok": True, "result": r}
    elif method == "probe_env":
        val = os.environ.get("FISSION_HOST_SECRET")
        resp = {"type": "response", "id": rid, "ok": True, "result": {"secret_present": val is not None}}
    elif method == "crash_now":
        sys.exit(42)
    elif method == "hang_forever":
        import time
        time.sleep(30)
        resp = {"type": "response", "id": rid, "ok": True, "result": {}}
    elif method == "oversize_stdout":
        sys.stdout.write("A" * 4096)
        sys.stdout.flush()
        continue
    else:
        resp = {"type": "response", "id": rid, "ok": False, "error": f"unknown method {method}"}

    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#;

#[test]
fn external_worker_enforces_capabilities_sessions_revocation_env_crash_timeout_and_bounds() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_worker_script(tmp.path(), "worker.py", RPC_WORKER_PY);
    let python = find_python3();
    let authorizer = Arc::new(Mutex::new(SessionAuthorizer::new()));

    // 0. Requiring unavailable kernel MAC sandbox fails closed cleanly.
    let mut strict_cfg = ExternalWorkerConfig::new("ext.strict", &python);
    strict_cfg.args = vec!["-B".into(), script.display().to_string()];
    strict_cfg.working_dir = tmp.path().to_path_buf();
    strict_cfg.require_kernel_sandbox = true;
    match ExternalWorkerSession::spawn(strict_cfg, authorizer.clone()) {
        Err(WorkerError::UnsupportedIsolation(msg)) => {
            assert!(msg.contains("seccomp/Landlock/Seatbelt"), "{msg}");
        }
        Err(other) => panic!("expected UnsupportedIsolation, got {other:?}"),
        Ok(_) => panic!("require_kernel_sandbox=true must fail closed without kernel MAC"),
    }

    // 1. Read-only worker can read analysis, cannot edit project or mutate live execution,
    // and cannot read leaked host environment variables.
    // SAFETY: test runs serialized or checks unique secret var name passed ONLY via parent env.
    unsafe {
        std::env::set_var("FISSION_HOST_SECRET", "top_secret_token");
    }

    let mut cfg = ExternalWorkerConfig::new("ext.readonly", &python);
    cfg.args = vec!["-B".into(), script.display().to_string()];
    cfg.working_dir = tmp.path().to_path_buf();
    cfg.capabilities = CapabilitySet::read_only();
    let mut session = ExternalWorkerSession::spawn(cfg, authorizer.clone()).expect("spawn");
    assert!(!session.confinement_report().kernel_sandbox_active);
    assert!(
        !session
            .confinement_report()
            .unsupported_protections
            .is_empty()
    );

    let mut handler = RecordingHostHandler::default();
    let res = session
        .invoke(
            "probe_escalate_write_and_exec",
            serde_json::json!({}),
            &mut handler,
        )
        .expect("invoke");
    assert_eq!(res["read"]["ok"], true);
    assert_eq!(res["edit"]["ok"], false);
    assert_eq!(res["edit"]["code"], "permission_denied");
    assert_eq!(res["exec"]["ok"], false);
    assert_eq!(res["exec"]["code"], "permission_denied");
    assert_eq!(handler.read_calls, 1);
    assert_eq!(handler.edit_calls, 0);
    assert_eq!(handler.live_mutate_calls, 0);

    let env_res = session
        .invoke("probe_env", serde_json::json!({}), &mut handler)
        .expect("probe_env");
    assert_eq!(env_res["secret_present"], false);

    unsafe {
        std::env::remove_var("FISSION_HOST_SECRET");
    }

    // 2. Wrong-session spoofing is denied even though read_analysis IS granted.
    let other_session = authorizer.lock().unwrap().new_session().0;
    let spoof_res = session
        .invoke(
            "probe_wrong_session",
            serde_json::json!({ "spoof_session": other_session }),
            &mut handler,
        )
        .expect("wrong_session RPC envelope completes");
    assert_eq!(spoof_res["ok"], false);
    assert_eq!(spoof_res["code"], "wrong_session");
    assert_eq!(handler.read_calls, 1, "spoofed call must not hit handler");

    // 3. Explicit grant revocation denies subsequent requests, and Drop revokes automatically.
    let grant_id = session.grant_id();
    assert!(authorizer.lock().unwrap().is_grant_active(grant_id));
    authorizer.lock().unwrap().revoke_grant(grant_id);
    let rev_res = session
        .invoke("probe_read", serde_json::json!({}), &mut handler)
        .expect("revoked callback returns denied result");
    assert_eq!(rev_res["ok"], false);
    assert_eq!(rev_res["code"], "revoked");
    assert_eq!(handler.read_calls, 1);
    drop(session);
    assert!(!authorizer.lock().unwrap().is_grant_active(grant_id));

    // 4. Separate EditProject grant permits edit_project while denying mutate_live_execution.
    let mut edit_cfg = ExternalWorkerConfig::new("ext.editor", &python);
    edit_cfg.args = vec!["-B".into(), script.display().to_string()];
    edit_cfg.working_dir = tmp.path().to_path_buf();
    edit_cfg.capabilities = CapabilitySet::from_capabilities([
        PluginCapability::ReadAnalysis,
        PluginCapability::EditProject,
    ]);
    let mut edit_session =
        ExternalWorkerSession::spawn(edit_cfg, authorizer.clone()).expect("spawn");
    let edit_res = edit_session
        .invoke(
            "probe_escalate_write_and_exec",
            serde_json::json!({}),
            &mut handler,
        )
        .expect("invoke");
    assert_eq!(edit_res["read"]["ok"], true);
    assert_eq!(edit_res["edit"]["ok"], true);
    assert_eq!(edit_res["exec"]["ok"], false);
    assert_eq!(edit_res["exec"]["code"], "permission_denied");
    assert_eq!(handler.edit_calls, 1);
    assert_eq!(handler.live_mutate_calls, 0);
    drop(edit_session);

    // 5. Crash returns WorkerError::Crashed and revokes grant.
    let mut crash_cfg = ExternalWorkerConfig::new("ext.crash", &python);
    crash_cfg.args = vec!["-B".into(), script.display().to_string()];
    crash_cfg.working_dir = tmp.path().to_path_buf();
    let mut crash_session =
        ExternalWorkerSession::spawn(crash_cfg, authorizer.clone()).expect("spawn");
    let crash_grant = crash_session.grant_id();
    let crash_err = crash_session
        .invoke("crash_now", serde_json::json!({}), &mut handler)
        .unwrap_err();
    assert!(
        matches!(crash_err, WorkerError::Crashed(_)),
        "{crash_err:?}"
    );
    assert!(!authorizer.lock().unwrap().is_grant_active(crash_grant));

    // 6. Timeout terminates hanging worker promptly and revokes grant.
    let mut hang_cfg = ExternalWorkerConfig::new("ext.hang", &python);
    hang_cfg.args = vec!["-B".into(), script.display().to_string()];
    hang_cfg.working_dir = tmp.path().to_path_buf();
    hang_cfg.limits = ExternalWorkerLimits {
        request_timeout: Duration::from_millis(200),
        ..Default::default()
    };
    let mut hang_session =
        ExternalWorkerSession::spawn(hang_cfg, authorizer.clone()).expect("spawn");
    let hang_grant = hang_session.grant_id();
    let hang_err = hang_session
        .invoke("hang_forever", serde_json::json!({}), &mut handler)
        .unwrap_err();
    assert!(matches!(hang_err, WorkerError::Timeout(_)), "{hang_err:?}");
    assert!(!authorizer.lock().unwrap().is_grant_active(hang_grant));

    // 7. Oversized line without newline triggers WorkerError::PayloadTooLarge without OOM.
    let mut big_cfg = ExternalWorkerConfig::new("ext.big", &python);
    big_cfg.args = vec!["-B".into(), script.display().to_string()];
    big_cfg.working_dir = tmp.path().to_path_buf();
    big_cfg.limits = ExternalWorkerLimits {
        max_response_bytes: 1024,
        request_timeout: Duration::from_secs(2),
        ..Default::default()
    };
    let mut big_session = ExternalWorkerSession::spawn(big_cfg, authorizer.clone()).expect("spawn");
    let big_grant = big_session.grant_id();
    let big_err = big_session
        .invoke("oversize_stdout", serde_json::json!({}), &mut handler)
        .unwrap_err();
    assert!(
        matches!(big_err, WorkerError::PayloadTooLarge { .. }),
        "{big_err:?}"
    );
    assert!(!authorizer.lock().unwrap().is_grant_active(big_grant));
}
