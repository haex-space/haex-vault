//! Notification fan-out helpers used by the dispatcher and ClaimInvite flow.

use super::super::protocol::Notification;
use super::LeaderState;

/// Broadcast an MLS notification to all connected peers.
pub(super) async fn notify_all_mls(state: &LeaderState, space_id: &str, message_type: &str) {
    let senders = state.notification_senders.read().await;
    for (_, sender) in senders.iter() {
        let _ = sender.try_send(Notification::Mls {
            space_id: space_id.to_string(),
            message_type: message_type.to_string(),
        });
    }
}

/// Broadcast a sync notification to all peers except the sender.
pub(super) async fn notify_others_sync(
    state: &LeaderState,
    space_id: &str,
    tables: &[String],
    exclude_endpoint: &str,
) {
    let senders = state.notification_senders.read().await;
    for (endpoint_id, sender) in senders.iter() {
        if endpoint_id != exclude_endpoint {
            let _ = sender.try_send(Notification::Sync {
                space_id: space_id.to_string(),
                tables: tables.to_vec(),
            });
        }
    }
}
