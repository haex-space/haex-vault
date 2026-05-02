//! Peer discovery: query CRDT for space devices, probe reachability in parallel.

use crate::database::DbConnection;
use super::error::DeliveryError;
use super::protocol::ALPN;

/// A candidate device for leader election.
#[derive(Debug, Clone)]
pub struct DeviceCandidate {
    pub endpoint_id: String,
    pub relay_url: Option<String>,
    pub priority: i32,
}

/// Query all devices for a space from the CRDT, sorted by priority.
pub fn get_space_device_candidates(
    db: &DbConnection,
    space_id: &str,
) -> Result<Vec<DeviceCandidate>, DeliveryError> {
    let sql = "SELECT device_endpoint_id, relay_url, leader_priority \
               FROM haex_space_devices \
               WHERE space_id = ?1 \
               ORDER BY leader_priority ASC, device_endpoint_id ASC"
        .to_string();
    let params = vec![serde_json::Value::String(space_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, db)
        .map_err(|e| DeliveryError::Database {
            reason: e.to_string(),
        })?;

    Ok(rows
        .into_iter()
        .map(|row| DeviceCandidate {
            endpoint_id: row
                .get(0)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            relay_url: row.get(1).and_then(|v| v.as_str()).map(|s| s.to_string()),
            priority: row.get(2).and_then(|v| v.as_i64()).unwrap_or(10) as i32,
        })
        .collect())
}

/// Probe all candidates in parallel, return the reachable ones (preserving priority order).
/// Uses a 5-second timeout per probe.
/// Our own endpoint_id is always included as reachable (no self-probe needed).
pub async fn probe_reachable_candidates(
    iroh_endpoint: Option<iroh::Endpoint>,
    candidates: &[DeviceCandidate],
    own_endpoint_id: &str,
) -> Vec<DeviceCandidate> {
    use std::time::Duration;
    use tokio::time::timeout;

    let iroh_endpoint = match iroh_endpoint {
        Some(ep) => ep,
        None => {
            // Endpoint not running — only include self if present
            return candidates
                .iter()
                .filter(|c| c.endpoint_id == own_endpoint_id)
                .cloned()
                .collect();
        }
    };

    let mut join_set = tokio::task::JoinSet::<Option<DeviceCandidate>>::new();

    // Track insertion order so we can restore priority ordering after join_all
    let mut order: Vec<String> = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        order.push(candidate.endpoint_id.clone());

        if candidate.endpoint_id == own_endpoint_id {
            // Self is always reachable
            let c = candidate.clone();
            join_set.spawn(async move { Some(c) });
            continue;
        }

        let remote_id: iroh::EndpointId = match candidate.endpoint_id.parse() {
            Ok(id) => id,
            Err(_) => {
                // Invalid endpoint ID — skip
                join_set.spawn(async { None });
                continue;
            }
        };

        let relay = candidate
            .relay_url
            .as_deref()
            .and_then(|s| s.parse::<iroh::RelayUrl>().ok());

        let addr = match relay {
            Some(url) => iroh::EndpointAddr::new(remote_id).with_relay_url(url),
            None => iroh::EndpointAddr::new(remote_id),
        };

        let ep = iroh_endpoint.clone();
        let c = candidate.clone();

        join_set.spawn(async move {
            match timeout(Duration::from_secs(5), ep.connect(addr, ALPN)).await {
                Ok(Ok(conn)) => {
                    conn.close(0u32.into(), b"probe");
                    Some(c)
                }
                _ => None,
            }
        });
    }

    // Collect results as they complete
    let mut reachable = Vec::new();
    while let Some(result) = join_set.join_next().await {
        if let Ok(Some(candidate)) = result {
            reachable.push(candidate);
        }
    }

    // Restore original priority order (candidates came sorted from DB)
    reachable.sort_by_key(|c| {
        order
            .iter()
            .position(|id| *id == c.endpoint_id)
            .unwrap_or(usize::MAX)
    });

    reachable
}

/// Select the best leader from reachable candidates (lowest priority number).
pub fn select_leader(reachable: &[DeviceCandidate]) -> Option<&DeviceCandidate> {
    reachable.first() // Already sorted by priority ASC from DB query
}
