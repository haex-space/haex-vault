//! Tests for the pure request-routing seam of the owner-serve handler.
//!
//! The full `handle_owner_sync_request` needs a live DB + AppHandle, so its
//! behavioral coverage (incl. the "foreign peer gets ZERO vault-private rows"
//! negative) is the Task-8 endpoint capstone. Here we lock down the pure
//! request-variant classifier: an owner-classified connection must only ever
//! act on SyncPull / SyncPush, never fall through to other request logic.

use super::{owner_request_action, OwnerRequestAction};
use crate::space_delivery::local::protocol::Request;

#[test]
fn sync_pull_routes_to_pull() {
    let req = Request::SyncPull {
        space_id: "vault-space".to_string(),
        after_timestamp: None,
        ucan_token: None,
    };
    assert_eq!(owner_request_action(&req), OwnerRequestAction::Pull);
}

#[test]
fn sync_push_routes_to_push() {
    let req = Request::SyncPush {
        space_id: "vault-space".to_string(),
        changes: serde_json::json!([]),
        ucan_token: None,
    };
    assert_eq!(owner_request_action(&req), OwnerRequestAction::Push);
}

/// Any non-sync request reaching the owner handler must be rejected, NOT
/// silently treated as a sync op or passed to space logic.
#[test]
fn non_sync_requests_are_rejected() {
    let welcomes = Request::MlsFetchWelcomes {
        space_id: "vault-space".to_string(),
    };
    assert_eq!(owner_request_action(&welcomes), OwnerRequestAction::Reject);

    let rejoin = Request::RequestRejoin {
        space_id: "vault-space".to_string(),
        ucan_token: None,
    };
    assert_eq!(owner_request_action(&rejoin), OwnerRequestAction::Reject);

    let kp_count = Request::MlsKeyPackageCount {
        space_id: "vault-space".to_string(),
    };
    assert_eq!(owner_request_action(&kp_count), OwnerRequestAction::Reject);
}
