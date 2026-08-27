//! Session store — per-client binary isolation with TTL eviction.
//!
//! Each client that uploads a binary receives a UUID session token.
//! All subsequent analysis requests are scoped to that session, so
//! multiple analysts can work on different binaries simultaneously
//! on the same server instance (Ghidra Server model).

use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::facts::FactStore;
use fission_static::analysis::xref_index::{XrefIndex, build_xref_index};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

// ── Session data ─────────────────────────────────────────────────────────────

pub struct SessionData {
    binary: RwLock<Arc<LoadedBinary>>,
    pub binary_name: String,
    /// `true` while CFG-based function discovery is still running on this
    /// session's binary in the background. The upload handler returns as
    /// soon as loader-only parsing is done and spawns discovery separately
    /// -- on a large binary it can take far longer than parsing, and
    /// nothing about it needs to hold the HTTP response open.
    analyzing: AtomicBool,
    /// Lazily built, cached on first use, invalidated (`None`) whenever
    /// `set_binary` changes what the binary's function set looks like.
    /// `FactStore::from_binary` isn't cheap -- it walks every function and
    /// symbol into a `ProgramSnapshot` and runs FID/signature-database
    /// matching -- and was previously rebuilt from scratch on every single
    /// decompile request in a session, even back-to-back requests against
    /// the exact same unchanged binary state.
    ///
    /// A `tokio::sync::Mutex`, not `RwLock`: held across the *entire*
    /// check-then-build-then-store sequence in `facts()` (tokio's `Mutex`
    /// is specifically designed to be held across an `.await`, unlike
    /// `std::sync::Mutex`). Two concurrent decompile requests racing on a
    /// cold cache used to both independently pay the full FID-matching
    /// cost -- same check-then-release-then-compute race as the global
    /// `control_flow_facts_for` cache had -- now the second just awaits
    /// the first's in-flight build instead of redoing it.
    facts: tokio::sync::Mutex<Option<Arc<FactStore>>>,
    /// Lazily built, cached on first use, same shape and rationale as
    /// `facts` above. `build_xref_index(.., true)` disassembles every
    /// executable section to find call/jump/data cross-references -- the
    /// handler used to pass `false` here (loader-derived xrefs only:
    /// imports/exports/relocations), which meant the Xrefs panel showed
    /// "no known callers/callees" for nearly every ordinary internal
    /// function, since the actual call-graph edges were never computed.
    /// `fission_cli`'s `xrefs` subcommand already defaults to `true`
    /// (`include_disasm = !cli.xref_no_disassembly`) -- this brings the
    /// GUI in line with that. Caching matters here for the same reason it
    /// did for `facts`: uncached, every single Xrefs-tab view of any
    /// function would redo the whole-binary disassembly sweep.
    xref_index: tokio::sync::Mutex<Option<Arc<XrefIndex>>>,
    last_used: RwLock<Instant>,
}

impl SessionData {
    pub fn new(binary: LoadedBinary, binary_name: String, analyzing: bool) -> Self {
        Self {
            binary: RwLock::new(Arc::new(binary)),
            binary_name,
            analyzing: AtomicBool::new(analyzing),
            facts: tokio::sync::Mutex::new(None),
            xref_index: tokio::sync::Mutex::new(None),
            last_used: RwLock::new(Instant::now()),
        }
    }

    pub async fn binary(&self) -> Arc<LoadedBinary> {
        self.binary.read().await.clone()
    }

    /// Swap in a new binary snapshot (e.g. once background discovery adds
    /// its functions to the loader-only set the session started with).
    /// Invalidates the cached `FactStore`, since it's built from the
    /// binary's current function/symbol set.
    pub async fn set_binary(&self, binary: LoadedBinary) {
        *self.binary.write().await = Arc::new(binary);
        *self.facts.lock().await = None;
        *self.xref_index.lock().await = None;
    }

    /// The session's cached facts, building them from the current binary
    /// on first use (or after `set_binary` invalidated a stale copy).
    ///
    /// The lock is held across the `spawn_blocking` build (not just the
    /// check), so a second caller that arrives while a build is already in
    /// flight awaits the same result instead of kicking off a redundant
    /// one -- and because it's a `tokio::sync::Mutex`, "awaits" here means
    /// yielding the executor, not blocking a worker thread.
    pub async fn facts(&self) -> Arc<FactStore> {
        let mut slot = self.facts.lock().await;
        if let Some(facts) = slot.as_ref() {
            return Arc::clone(facts);
        }
        let binary = self.binary().await;
        // `FactStore::from_binary` runs FID/signature-database matching
        // over every unnamed function in the binary -- genuinely CPU-heavy
        // (seconds to tens of seconds on a large binary). Calling it
        // directly here (as this used to) runs it synchronously on
        // whichever tokio worker thread is executing this task, blocking
        // that thread from servicing *any* other async work -- other
        // sessions' requests included -- for the whole duration, not just
        // slowing this one down. `spawn_blocking` moves it to tokio's
        // separate blocking-task thread pool instead.
        let built = tokio::task::spawn_blocking(move || Arc::new(FactStore::from_binary(&binary)))
            .await
            .expect("FactStore::from_binary panicked");
        *slot = Some(Arc::clone(&built));
        built
    }

    /// The session's cached disassembly-based xref index, building it from
    /// the current binary on first use. Same locking shape as `facts()`
    /// (lock held across the `spawn_blocking` build) for the same reason:
    /// avoid both blocking a tokio worker thread and a redundant-build race
    /// between concurrent xrefs requests.
    pub async fn xref_index(&self) -> Arc<XrefIndex> {
        let mut slot = self.xref_index.lock().await;
        if let Some(index) = slot.as_ref() {
            return Arc::clone(index);
        }
        let t0 = std::time::Instant::now();
        let binary = self.binary().await;
        let built = tokio::task::spawn_blocking(move || Arc::new(build_xref_index(&binary, true)))
            .await
            .expect("build_xref_index panicked");
        tracing::info!("[PERF] xref_index build: {:?}", t0.elapsed());
        *slot = Some(Arc::clone(&built));
        built
    }

    /// Persist a `FactStore` a decompile call learned new things into
    /// (`RustSleighDecompileResult::learned_facts`) as the session's
    /// current facts, so the *next* decompile call in this session
    /// (of this function or, more usefully, a caller/callee of it) starts
    /// from what was just discovered instead of the plain loader-derived
    /// facts every session starts with.
    pub async fn set_facts(&self, facts: FactStore) {
        *self.facts.lock().await = Some(Arc::new(facts));
    }

    pub fn is_analyzing(&self) -> bool {
        self.analyzing.load(Ordering::Acquire)
    }

    pub fn set_analyzing(&self, value: bool) {
        self.analyzing.store(value, Ordering::Release);
    }

    pub async fn touch(&self) {
        *self.last_used.write().await = Instant::now();
    }

    pub async fn idle_secs(&self) -> u64 {
        self.last_used.read().await.elapsed().as_secs()
    }
}

// ── Session store ─────────────────────────────────────────────────────────────

pub struct SessionStore {
    sessions: RwLock<HashMap<Uuid, Arc<SessionData>>>,
    pub max_sessions: usize,
    ttl: Duration,
}

impl SessionStore {
    pub fn new(max_sessions: usize, ttl_secs: u64) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            max_sessions,
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Create a new session for the given binary.
    /// Returns `Err` if the session cap is reached.
    pub async fn create(
        &self,
        binary: LoadedBinary,
        binary_name: String,
        analyzing: bool,
    ) -> Result<Uuid, &'static str> {
        let mut map = self.sessions.write().await;
        if map.len() >= self.max_sessions {
            return Err("server at capacity — try again later");
        }
        let id = Uuid::new_v4();
        map.insert(
            id,
            Arc::new(SessionData::new(binary, binary_name, analyzing)),
        );
        info!("session created: {id}  (total: {})", map.len());
        Ok(id)
    }

    /// Retrieve a session, touching its last-used timestamp.
    pub async fn get(&self, id: &Uuid) -> Option<Arc<SessionData>> {
        let map = self.sessions.read().await;
        let sess = map.get(id)?.clone();
        sess.touch().await;
        Some(sess)
    }

    /// Explicitly remove a session (client-driven cleanup).
    pub async fn remove(&self, id: &Uuid) -> bool {
        let mut map = self.sessions.write().await;
        let removed = map.remove(id).is_some();
        if removed {
            info!("session removed: {id}  (total: {})", map.len());
        }
        removed
    }

    /// Active session count.
    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Background task: sweep expired sessions every 60 seconds.
    pub async fn run_sweeper(self: Arc<Self>) {
        let sweep_interval = Duration::from_secs(60);
        loop {
            tokio::time::sleep(sweep_interval).await;
            self.sweep_expired().await;
        }
    }

    async fn sweep_expired(&self) {
        let mut map = self.sessions.write().await;
        let before = map.len();
        map.retain(|id, sess| {
            // We can't call async fn inside retain; use try_read instead.
            let idle = sess
                .last_used
                .try_read()
                .map(|t| t.elapsed())
                .unwrap_or(Duration::ZERO);
            let keep = idle < self.ttl;
            if !keep {
                debug!("session evicted (idle {:.0}s): {id}", idle.as_secs_f32());
            }
            keep
        });
        let evicted = before.saturating_sub(map.len());
        if evicted > 0 {
            info!(
                "TTL sweep: evicted {evicted} session(s)  (active: {})",
                map.len()
            );
        }
    }
}
