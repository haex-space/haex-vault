//! Priority-based leader election.
//!
//! Flow:
//! 1. Query all devices for a space from CRDT
//! 2. Probe all in parallel (5s timeout)
//! 3. Select lowest-priority reachable device as leader
//! 4. If we ARE the leader → caller starts leader mode
//! 5. If someone else is leader → caller connects as peer

use crate::database::DbConnection;
use crate::peer_storage::endpoint::PeerEndpoint;
use super::discovery;
use super::error::DeliveryError;

/// Result of a leader election round.
#[derive(Debug)]
pub enum ElectionResult {
    /// This device should be the leader.
    SelfIsLeader,
    /// Another device is the leader — connect to it.
    RemoteLeader {
        endpoint_id: String,
        relay_url: Option<String>,
        priority: i32,
    },
    /// No devices are reachable (we might be alone or offline).
    NoLeaderFound,
}

/// Run a leader election for a local space.
///
/// Queries all devices, probes reachability in parallel,
/// and determines who should be leader based on priority.
pub async fn elect_leader(
    db: &DbConnection,
    endpoint: &PeerEndpoint,
    space_id: &str,
    own_endpoint_id: &str,
) -> Result<ElectionResult, DeliveryError> {
    // 1. Get all candidates from CRDT
    let candidates = discovery::get_space_device_candidates(db, space_id)?;

    if candidates.is_empty() {
        return Ok(ElectionResult::NoLeaderFound);
    }

    // 2. Probe all in parallel
    let reachable = discovery::probe_reachable_candidates(endpoint, &candidates, own_endpoint_id).await;

    if reachable.is_empty() {
        return Ok(ElectionResult::NoLeaderFound);
    }

    // 3. Select leader (lowest priority number)
    match discovery::select_leader(&reachable) {
        Some(candidate) if candidate.endpoint_id == own_endpoint_id => {
            Ok(ElectionResult::SelfIsLeader)
        }
        Some(candidate) => Ok(ElectionResult::RemoteLeader {
            endpoint_id: candidate.endpoint_id.clone(),
            relay_url: candidate.relay_url.clone(),
            priority: candidate.priority,
        }),
        None => Ok(ElectionResult::NoLeaderFound),
    }
}
