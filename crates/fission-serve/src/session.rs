//! Session store — per-client binary isolation with TTL eviction.
//!
//! Each client that uploads a binary receives a UUID session token.
//! All subsequent analysis requests are scoped to that session, so
//! multiple analysts can work on different binaries simultaneously
//! on the same server instance (Ghidra Server model).

use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::facts::FactStore;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{info, debug};
use uuid::Uuid;

// ── Session data ─────────────────────────────────────────────────────────────

pub struct SessionData {
    binary:          RwLock<Arc<LoadedBinary>>,
    pub binary_name: String,
    /// `true` while CFG-based function discovery is still running on this
    /// session's binary in the background. The upload handler returns as
    /// soon as loader-only parsing is done and spawns discovery separately
    /// -- on a large binary it can take far longer than parsing, and
    /// nothing about it needs to hold the HTTP response open.
    analyzing:       AtomicBool,
    /// Lazily built, cached on first use, invalidated (`None`) whenever
    /// `set_binary` changes what the binary's function set looks like.
    /// `FactStore::from_binary` isn't cheap -- it walks every function and
    /// symbol into a `ProgramSnapshot` and runs FID/signature-database
    /// matching -- and was previously rebuilt from scratch on every single
    /// decompile request in a session, even back-to-back requests against
    /// the exact same unchanged binary state.
    facts:           RwLock<Option<Arc<FactStore>>>,
    last_used:       RwLock<Instant>,
}

impl SessionData {
    pub fn new(binary: LoadedBinary, binary_name: String, analyzing: bool) -> Self {
        Self {
            binary:      RwLock::new(Arc::new(binary)),
            binary_name,
            analyzing:   AtomicBool::new(analyzing),
            facts:       RwLock::new(None),
            last_used:   RwLock::new(Instant::now()),
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
        *self.facts.write().await = None;
    }

    /// The session's cached facts, building them from the current binary
    /// on first use (or after `set_binary` invalidated a stale copy).
    pub async fn facts(&self) -> Arc<FactStore> {
        if let Some(facts) = self.facts.read().await.as_ref() {
            return Arc::clone(facts);
        }
        let mut slot = self.facts.write().await;
        // Another task may have built it while we were waiting for the
        // write lock -- check again before doing the work twice.
        if let Some(facts) = slot.as_ref() {
            return Arc::clone(facts);
        }
        let binary = self.binary().await;
        let built = Arc::new(FactStore::from_binary(&binary));
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
        *self.facts.write().await = Some(Arc::new(facts));
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
    sessions:     RwLock<HashMap<Uuid, Arc<SessionData>>>,
    pub max_sessions: usize,
    ttl:          Duration,
}

impl SessionStore {
    pub fn new(max_sessions: usize, ttl_secs: u64) -> Self {
        Self {
            sessions:     RwLock::new(HashMap::new()),
            max_sessions,
            ttl:          Duration::from_secs(ttl_secs),
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
        map.insert(id, Arc::new(SessionData::new(binary, binary_name, analyzing)));
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
            let idle = sess.last_used.try_read()
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
            info!("TTL sweep: evicted {evicted} session(s)  (active: {})", map.len());
        }
    }
}
