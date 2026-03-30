//! mDNS discovery combined with CRDT-based leader priorities.

use crate::database::DbConnection;
use crate::peer_storage::endpoint::PeerEndpoint;
use super::error::DeliveryError;

/// A device that is a candidate for leader election.
#[derive(Debug, Clone)]
pub struct DeviceCandidate {
    pub endpoint_id: String,
    pub relay_url: Option<String>,
    pub priority: i32,
}

/// Query all device candidates for a space from the CRDT store.
pub fn get_space_device_candidates(
    _db: &DbConnection,
    _space_id: &str,
) -> Result<Vec<DeviceCandidate>, DeliveryError> {
    // TODO: implement in Task 1 (discovery)
    Ok(vec![])
}

/// Probe which candidates are reachable, in parallel with a timeout.
pub async fn probe_reachable_candidates(
    _endpoint: &PeerEndpoint,
    _candidates: &[DeviceCandidate],
    _own_endpoint_id: &str,
) -> Vec<DeviceCandidate> {
    // TODO: implement in Task 1 (discovery)
    vec![]
}

/// Select the leader from reachable candidates (lowest priority number wins).
pub fn select_leader(reachable: &[DeviceCandidate]) -> Option<&DeviceCandidate> {
    // TODO: implement in Task 1 (discovery)
    reachable.iter().min_by_key(|c| c.priority)
}
