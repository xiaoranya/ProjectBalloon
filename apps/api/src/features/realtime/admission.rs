//! Admission control for Server-Sent Events subscribers.
//!
//! Every SSE response pins a task, a broadcast receiver and a socket until the
//! client goes away. The public endpoint is reachable anonymously, so without a
//! cap a single client could open connections until the process ran out of
//! file descriptors — taking the judging API down with it.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Process-wide ceiling on concurrent SSE responses.
const DEFAULT_MAX_CONNECTIONS: usize = 4_096;
/// Ceiling per client address. Deliberately generous: a venue behind one NAT
/// address legitimately runs hundreds of screens, but one host must not be able
/// to occupy the whole process budget.
const DEFAULT_MAX_CONNECTIONS_PER_IP: usize = 512;
/// Bound on the per-address bookkeeping itself.
const MAX_TRACKED_IPS: usize = 8_192;

pub(crate) struct SseAdmission {
    total: Arc<Semaphore>,
    per_ip_limit: usize,
    max_connections: usize,
    per_ip: Mutex<HashMap<IpAddr, usize>>,
    active: AtomicUsize,
}

/// Holds one SSE slot until dropped, i.e. until the stream ends.
pub(crate) struct SsePermit {
    admission: Arc<SseAdmission>,
    ip: IpAddr,
    /// Released by the field's own `Drop`, after the counters above are fixed.
    _slot: OwnedSemaphorePermit,
}

impl SseAdmission {
    #[must_use]
    pub(crate) fn new(max_connections: usize, per_ip_limit: usize) -> Arc<Self> {
        let max_connections = max_connections.max(1);
        let per_ip_limit = per_ip_limit.clamp(1, max_connections);
        Arc::new(Self {
            total: Arc::new(Semaphore::new(max_connections)),
            per_ip_limit,
            max_connections,
            per_ip: Mutex::new(HashMap::new()),
            active: AtomicUsize::new(0),
        })
    }

    /// Builds the admission controller from the environment, falling back to the
    /// defaults for anything missing or unparsable.
    #[must_use]
    pub(crate) fn from_env() -> Arc<Self> {
        let read = |name: &str, default: usize| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(default)
        };
        Self::new(
            read("PROJECT_BALLOON_SSE_MAX_CONNECTIONS", DEFAULT_MAX_CONNECTIONS),
            read("PROJECT_BALLOON_SSE_MAX_CONNECTIONS_PER_IP", DEFAULT_MAX_CONNECTIONS_PER_IP),
        )
    }

    /// Reserves a slot for `ip`, or `None` when either limit is reached.
    pub(crate) fn admit(self: &Arc<Self>, ip: IpAddr) -> Option<SsePermit> {
        let slot = self.total.clone().try_acquire_owned().ok()?;
        {
            let mut per_ip = self.per_ip.lock().expect("sse admission lock");
            let count = per_ip.entry(ip).or_insert(0);
            if *count >= self.per_ip_limit {
                // `slot` goes out of scope and returns the global permit.
                return None;
            }
            *count += 1;
            // Keep the map itself bounded even if a client sprays source
            // addresses; every entry here is one live connection, so this only
            // trips far beyond the global cap.
            if per_ip.len() > MAX_TRACKED_IPS {
                per_ip.retain(|_, count| *count > 0);
            }
        }
        self.active.fetch_add(1, Ordering::Relaxed);
        Some(SsePermit { admission: self.clone(), ip, _slot: slot })
    }

    #[must_use]
    pub(crate) fn active(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }

    #[must_use]
    pub(crate) fn max_connections(&self) -> usize {
        self.max_connections
    }
}

impl Drop for SsePermit {
    fn drop(&mut self) {
        self.admission.active.fetch_sub(1, Ordering::Relaxed);
        let mut per_ip = self.admission.per_ip.lock().expect("sse admission lock");
        if let Some(count) = per_ip.get_mut(&self.ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                per_ip.remove(&self.ip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SseAdmission;

    #[test]
    fn global_and_per_address_limits_are_enforced_and_released() {
        let admission = SseAdmission::new(3, 2);
        let first = std::net::IpAddr::from([10, 0, 0, 1]);
        let second = std::net::IpAddr::from([10, 0, 0, 2]);

        let a = admission.admit(first).expect("first slot");
        let b = admission.admit(first).expect("second slot");
        assert!(admission.admit(first).is_none(), "per-address limit is two");
        let c = admission.admit(second).expect("another address still fits");
        assert!(admission.admit(second).is_none(), "global limit is three");
        assert_eq!(admission.active(), 3);

        drop(b);
        assert_eq!(admission.active(), 2);
        assert!(admission.admit(first).is_some(), "a released slot is reusable");
        drop((a, c));
    }
}
