//! Tests for [`super::auth_gate::authorize_request`] — the unified
//! pre-dispatch authorisation gate.
//!
//! Coverage starts with the no-Announce reject path (Phase 3 T3); the
//! full table (bypass, audience, capability, membership, cache hit) lands
//! in T4 alongside the rich setup_authz_db helper from inbound_sync_tests.

#![cfg(test)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tokio::sync::RwLock;

use super::authorize_request;
use crate::database::DbConnection;
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::types::ConnectedPeer;

/// Bare in-memory `DbConnection`. The no-Announce reject path short-circuits
/// at the cache-lookup step before any SQL runs, so we deliberately do **not**
/// reach for the heavier `setup_authz_db` helper here.
fn empty_db() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

#[tokio::test]
async fn rejects_request_without_prior_announce() {
    let db = empty_db();
    let peers: RwLock<HashMap<String, ConnectedPeer>> = RwLock::new(HashMap::new());

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
    };

    let result = authorize_request(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
    )
    .await;

    match result {
        Err(Response::Error { message }) => {
            assert!(message.contains("Announce"), "got: {message}")
        }
        other => panic!("expected reject, got {other:?}"),
    }
}
