//! Session store — per-client binary isolation with TTL eviction.
//!
//! Each client that uploads a binary receives a UUID session token.
//! All subsequent analysis requests are scoped to that session, so
//! multiple analysts can work on different binaries simultaneously
//! on the same server instance (Ghidra Server model).

use fission_loader::loader::LoadedBinary;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{info, debug};
use uuid::Uuid;

// ── Session data ─────────────────────────────────────────────────────────────

pub struct SessionData {
    pub binary:      Arc<LoadedBinary>,
    pub binary_name: String,
    last_used:       RwLock<Instant>,
}

impl SessionData {
    pub fn new(binary: LoadedBinary, binary_name: String) -> Self {
        Self {
            binary:      Arc::new(binary),
            binary_name,
            last_used:   RwLock::new(Instant::now()),
        }
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
    ) -> Result<Uuid, &'static str> {
        let mut map = self.sessions.write().await;
        if map.len() >= self.max_sessions {
            return Err("server at capacity — try again later");
        }
        let id = Uuid::new_v4();
        map.insert(id, Arc::new(SessionData::new(binary, binary_name)));
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
