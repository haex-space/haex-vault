//! SSH sessions kept open between calls (spec 050 research R3, R12): building one takes several
//! round trips, and a provider like Hetzner allows only a few connections per account. Calls to
//! one connection share up to two sessions; one without a call for 60 seconds closes. A session
//! belongs to its endpoint, its confirmed host key and its login, so new credentials or a new host
//! key never reuse an old session: those are left to close.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::time::Instant;

use super::super::{Access, Credentials};
use super::connect::{Refused, Session};

/// Sessions one connection may hold at once.
pub(super) const PER_CONNECTION: usize = 2;

/// A session without a call for this long closes.
pub(super) const IDLE: Duration = Duration::from_secs(60);

struct Pooled {
    session: Arc<Session>,
    used: Instant,
}

#[derive(Default)]
struct Lane {
    sessions: Vec<Pooled>,
    /// Sessions being opened now; they count against [`PER_CONNECTION`].
    opening: usize,
}

/// The open sessions by connection.
#[derive(Default)]
pub(super) struct Pool {
    lanes: Arc<Mutex<HashMap<String, Lane>>>,
}

/// What a session is for: endpoint, folder-independent; the host key; the login and a digest of
/// its secret (never the secret itself).
fn lane_of(access: &Access) -> String {
    let location = &access.location;
    let mut digest = Sha256::new();
    let user = match &access.credentials {
        Credentials::Password(login) => {
            digest.update(login.password.as_bytes());
            login.username.as_str()
        }
        Credentials::SshKey(login) => {
            digest.update(login.private_key.as_bytes());
            if let Some(passphrase) = &login.passphrase {
                digest.update(passphrase.as_bytes());
            }
            login.username.as_str()
        }
        Credentials::S3(_) => "",
    };
    let secret: String = digest
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        "{}|{}|{user}|{secret}",
        location.endpoint,
        location.host_key.as_deref().unwrap_or_default()
    )
}

impl Pool {
    /// A session for `access`: an idle one of its lane, a new one while the lane has room
    /// (opened by `open`), else the least busy one.
    pub async fn session<F, Fut>(&self, access: &Access, open: F) -> Result<Arc<Session>, Refused>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Session, Refused>>,
    {
        let lane_key = lane_of(access);
        {
            let mut lanes = self.lanes.lock().expect("pool");
            Self::sweep(&mut lanes);
            let lane = lanes.entry(lane_key.clone()).or_default();
            let idle = lane
                .sessions
                .iter_mut()
                .find(|pooled| Arc::strong_count(&pooled.session) == 1);
            if let Some(pooled) = idle {
                pooled.used = Instant::now();
                return Ok(Arc::clone(&pooled.session));
            }
            if lane.sessions.len() + lane.opening >= PER_CONNECTION {
                if let Some(pooled) = lane
                    .sessions
                    .iter_mut()
                    .min_by_key(|pooled| Arc::strong_count(&pooled.session))
                {
                    pooled.used = Instant::now();
                    return Ok(Arc::clone(&pooled.session));
                }
            }
            lane.opening += 1;
        }
        let opened = open().await;
        let mut lanes = self.lanes.lock().expect("pool");
        let lane = lanes.entry(lane_key).or_default();
        lane.opening -= 1;
        let session = Arc::new(opened?);
        lane.sessions.push(Pooled {
            session: Arc::clone(&session),
            used: Instant::now(),
        });
        drop(lanes);
        self.close_when_idle();
        Ok(session)
    }

    /// How many sessions are open (tests).
    #[cfg(test)]
    pub fn open_sessions(&self) -> usize {
        let mut lanes = self.lanes.lock().expect("pool");
        Self::sweep(&mut lanes);
        lanes.values().map(|lane| lane.sessions.len()).sum()
    }

    /// Drops closed sessions and those idle for [`IDLE`] that no call holds.
    fn sweep(lanes: &mut HashMap<String, Lane>) {
        for lane in lanes.values_mut() {
            lane.sessions.retain(|pooled| {
                !pooled.session.is_closed()
                    && (Arc::strong_count(&pooled.session) > 1 || pooled.used.elapsed() < IDLE)
            });
        }
        lanes.retain(|_, lane| !lane.sessions.is_empty() || lane.opening > 0);
    }

    /// Sweeps again once the newest session could be idle, until none is left.
    fn close_when_idle(&self) {
        let lanes = Arc::downgrade(&self.lanes);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(IDLE + Duration::from_secs(1)).await;
                let Some(lanes) = lanes.upgrade() else { return };
                let mut lanes = lanes.lock().expect("pool");
                Self::sweep(&mut lanes);
                if lanes.is_empty() {
                    return;
                }
            }
        });
    }
}
