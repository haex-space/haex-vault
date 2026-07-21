use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use tokio::sync::RwLock;

use super::super::authorize_request;
use crate::crdt::hlc::HlcService;
use crate::database::DbConnection;
use crate::logging::LogSink;
use crate::space_delivery::local::dos_defence::config::DosDefenceConfig;
use crate::space_delivery::local::dos_defence::notifier::SingleSourceNotifier;
use crate::space_delivery::local::dos_defence::tracker::RejectRateTracker;
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::test_support::init_logs_db_inner_with_uri;
use crate::space_delivery::local::types::{ConnectedPeer, PeerClaim};
use crate::ucan::ValidatedUcan;

/// Wrapper around [`authorize_request`] that injects a fresh DoS-defence
/// tracker and the default config. Each test gets its own tracker, so
/// rate-limiting state never crosses test boundaries. With count=1 per
/// call, every reject stays well below the warn threshold (default 20),
/// so the gate continues to log every reject as Stage-1 / pre-rate-limit
/// behaviour.
pub(super) async fn authorize_default(
    request: &Request,
    verified_did: &str,
    peer_endpoint_id: &str,
    peers: &RwLock<HashMap<String, ConnectedPeer>>,
    db: &DbConnection,
    log_sink: Option<&LogSink>,
) -> Result<Option<ValidatedUcan>, Response> {
    let tracker = RejectRateTracker::new(Duration::from_secs(1));
    let cfg = DosDefenceConfig::defaults();
    let notifier = SingleSourceNotifier::new();
    authorize_request(
        request,
        verified_did,
        peer_endpoint_id,
        peers,
        db,
        &tracker,
        &cfg,
        &notifier,
        None,
        log_sink,
    )
    .await
}

/// In-memory DB without the membership tables, but with `haex_logs_no_sync`
/// + the HLC UDF + CRDT bookkeeping so `log_to_db` works for audit-row
/// assertions. Used by tests that short-circuit before the membership
/// check (stage 2 no-peer, stage 4 audience, stage 5 capability) and by
/// the DB-error test that wants `is_active_space_member` to fail on the
/// missing `haex_space_members` table.
///
/// Delegates the entire setup to `test_support::init_logs_db_inner` —
/// keeps this fixture byte-identical to `setup_membership_db` on every
/// shared knob (HLC, CRDT bookkeeping, the no-CRDT-column `_no_sync` log
/// table).
pub(super) fn empty_db() -> (DbConnection, Arc<Mutex<HlcService>>, LogSink) {
    let (conn, hlc_service, uri) = init_logs_db_inner_with_uri();
    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    let hlc = Arc::new(Mutex::new(hlc_service));

    // Second connection to the same shared-cache in-memory DB, wrapped
    // in a `LogSink`. Writes through the sink hit the same rows the
    // read-back SELECT (`select_audit_logs`) issues via `db`, matching
    // production's "two OS handles, one file" shape.
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_URI;
    let sink_conn =
        Connection::open_with_flags(&uri, flags).expect("open second URI conn for LogSink");
    let sink = LogSink::from_connection(Arc::new(Mutex::new(sink_conn)));

    (db, hlc, sink)
}

/// Build a `ConnectedPeer` whose cached `validated_ucan` is the one the
/// AuthGate's stage-2 lookup will resolve. The endpoint-id/audience-DID
/// pair is what stages 3-5 then check.
pub(super) fn make_peer(
    endpoint_id: &str,
    did: &str,
    validated_ucan: ValidatedUcan,
) -> ConnectedPeer {
    ConnectedPeer {
        endpoint_id: endpoint_id.to_string(),
        did: did.to_string(),
        label: None,
        claims: Vec::<PeerClaim>::new(),
        connected_at: "1970-01-01T00:00:00Z".to_string(),
        validated_ucan: Some(validated_ucan),
    }
}

/// Read all `haex_logs_no_sync` rows via the same `logging::query_logs` the
/// in-app log viewer uses. Going through the production query (rather than a
/// bespoke `SELECT level, source, message, metadata FROM haex_logs_no_sync`)
/// means any future change to `query_logs` — added column, JSON
/// normalisation, column-order change — gets exercised by these tests
/// automatically; a SQL drift between production and tests can no longer
/// pass silently.
///
/// (`select_with_crdt` is a no-op for `SELECT` statements in the
/// delete-log model — see `crdt::transformer::transform_query` — so we
/// do *not* get a hardened tombstone filter from this routing. The
/// motivation is purely the schema-drift coverage above.)
///
/// Ordering: `query_logs` returns newest first; today's assertions check
/// "exactly one row", so the order is moot. If a future test wants to
/// inspect multiple rows in temporal order, reverse the slice at the
/// callsite — don't reshape this helper.
pub(super) fn select_audit_logs(db: &DbConnection) -> Vec<crate::logging::LogEntry> {
    crate::logging::query_logs(
        db,
        &crate::logging::LogQueryParams {
            source: None,
            extension_id: None,
            level: None,
            since: None,
            until: None,
            device_id: None,
            limit: None,
            offset: None,
        },
    )
    .expect("query haex_logs_no_sync")
}

/// Assert that the gate wrote exactly one audit row at `expected_level`
/// (`"warn"` for peer-side rejects, `"error"` for internal vault failures),
/// tagged with the `request`'s [`Request::op_name`] and the structured
/// `subsystem` metadata field `expected_subsystem`, and whose message
/// contains `must_contain`.
///
/// Taking the actual `&Request` (rather than a hardcoded op-name string)
/// means a future rename of any `op_name` variant stays caught here — if
/// the production tag drifts the test fails for the right reason, never
/// "I edited only one of the two strings". The `subsystem` check pins the
/// metadata convention (always set to `"AuthGate"` for any reject row
/// this module emits) so operators can filter `haex_logs_no_sync` by subsystem
/// independent of the per-op `source` tag.
pub(super) fn assert_single_audit_row(
    db: &DbConnection,
    expected_level: &str,
    expected_subsystem: &str,
    request: &Request,
    must_contain: &str,
) {
    let expected_op = request.op_name();
    let rows = select_audit_logs(db);
    assert_eq!(
        rows.len(),
        1,
        "expected exactly one audit row for op={expected_op}, got: {rows:?}"
    );
    let row = &rows[0];
    assert_eq!(
        row.level, expected_level,
        "audit row level must be {expected_level}, got {}",
        row.level
    );
    assert_eq!(
        row.source, expected_op,
        "audit row source must be op_name={expected_op}, got {}",
        row.source
    );
    assert!(
        row.message.contains(must_contain),
        "audit row message must mention {must_contain:?}, got: {}",
        row.message
    );
    let metadata_str = row
        .metadata
        .as_deref()
        .expect("audit row must have metadata column populated (with subsystem field)");
    let metadata_json: serde_json::Value =
        serde_json::from_str(metadata_str).expect("audit row metadata must be valid JSON");
    assert_eq!(
        metadata_json.get("subsystem").and_then(|s| s.as_str()),
        Some(expected_subsystem),
        "audit row metadata.subsystem must be {expected_subsystem}, got: {metadata_str}"
    );
}
