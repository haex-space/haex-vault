//! Inbound-SyncPush validation, authorisation and origin attribution.
//!
//! The leader receives a batch of column-level CRDT changes from a peer,
//! already authenticated by UCAN signature. This module is the **single
//! choke point** that decides whether such a batch may be applied to the
//! space DB. Every inbound CRDT push MUST flow through
//! [`authorize_inbound_sync_push`] — handlers should never call the
//! lower-level checks individually, otherwise the security model decays
//! into "everyone enforces their own subset".
//!
//! The pipeline (in this exact order — each step's success is a
//! precondition for the next):
//!
//! 1. **Capability gate.** The minimum capability is determined from the
//!    set of tables touched. Pushes that touch only the
//!    [membership-system tables][`crate::crdt::scanner::MEMBERSHIP_SYSTEM_TABLES`]
//!    require `Read`; any other table requires `Write`. This lets a
//!    read-only member publish their own membership / device / MLS
//!    KeyPackage rows while still blocking attempts to write user content
//!    like `haex_peer_shares`.
//! 2. **Membership gate.** The UCAN audience must still be an active
//!    (non-tombstoned) member of the space — admin removal is the
//!    revocation kill-switch.
//! 3. **Payload validation** (pure transform — see
//!    [`validate_and_attribute`]):
//!    - **Table whitelist.** Only rows for tables in
//!      [`crate::crdt::scanner::SPACE_SCOPED_CRDT_TABLES`] may cross the
//!      wire.
//!    - **`space_id` column scope.** Any change that writes the
//!      `space_id` column must set it to the request's `space_id`;
//!      anything else is a cross-space injection attempt.
//!    - **Origin attribution.** The client's claim about `authored_by_did`
//!      is *stripped* from the batch; the leader re-injects exactly one
//!      `authored_by_did` column-change per unique `(table, row)` with
//!      the value taken from the validated UCAN audience. A peer cannot
//!      forge authorship because the field is never read from the wire.
//! 4. **Per-row ownership** (see [`enforce_row_ownership`]). For the
//!    membership-system tables that any member may write, every modified
//!    row must already belong to the caller (or be a brand-new row whose
//!    declared owner is the caller). This stops Mallory from overwriting
//!    Bob's membership with `identity_id = mallory` or hijacking Bob's
//!    device endpoint registration.

use std::collections::HashMap;

use rusqlite::params_from_iter;
use serde_json::Value as JsonValue;

use crate::crdt::hlc::hlc_is_newer;
use crate::crdt::scanner::{is_membership_system_table, is_space_scoped_table, LocalColumnChange};
use crate::database::core::with_connection;
use crate::database::DbConnection;
use crate::ucan::{require_capability, CapabilityLevel, ValidatedUcan};

use super::error::DeliveryError;
use super::ucan::is_active_space_member;

/// The outcome of validating and attributing an inbound SyncPush batch.
#[derive(Debug)]
pub enum InboundSyncPushOutcome {
    /// All checks passed; `changes` has been stripped of client-supplied
    /// `authored_by_did` entries and one authoritative attribution per
    /// unique row has been injected.
    Accepted { changes: Vec<LocalColumnChange> },
    /// The batch was rejected; the reason is suitable for logging and
    /// returning to the peer as an error response.
    Rejected { reason: String },
}

/// Validate, scope-check, and attribute an inbound SyncPush batch.
///
/// See the module doc-comment for the contract. The `ucan_audience` is
/// expected to be the validated UCAN audience for the request — i.e. the
/// Space-Member-DID the leader already confirmed is an active member of
/// `space_id` via the membership check.
pub fn validate_and_attribute(
    space_id: &str,
    ucan_audience: &str,
    changes: Vec<LocalColumnChange>,
) -> InboundSyncPushOutcome {
    // --- (1) + (2): whitelist and space_id scope -------------------------
    for change in &changes {
        if !is_space_scoped_table(&change.table_name) {
            return InboundSyncPushOutcome::Rejected {
                reason: format!(
                    "Table {} is not allowed in space-scoped sync",
                    change.table_name
                ),
            };
        }

        if change.column_name == "space_id" {
            let inbound = change.value.as_str();
            if inbound != Some(space_id) {
                return InboundSyncPushOutcome::Rejected {
                    reason: format!(
                        "Row in {} sets space_id={:?} but request is for {}",
                        change.table_name, change.value, space_id
                    ),
                };
            }
        }
    }

    // --- (3): strip client-supplied authored_by_did ----------------------
    let mut stripped: Vec<LocalColumnChange> = changes
        .into_iter()
        .filter(|c| c.column_name != "authored_by_did")
        .collect();

    // --- (3): collect max HLC + device_id per unique (table, row) --------
    // The injected authored_by_did carries the max HLC seen in its row-
    // group so the CRDT merge treats it as the most recent authoritative
    // write for the column. Using the row's own device_id keeps the
    // scanner's (table, row, column, device)-dedup intact.
    let mut per_row: HashMap<(String, String), (String, String)> = HashMap::new();
    for change in &stripped {
        let key = (change.table_name.clone(), change.row_pks.clone());
        per_row
            .entry(key)
            .and_modify(|(hlc, device_id): &mut (String, String)| {
                if hlc_is_newer(&change.hlc_timestamp, hlc) {
                    *hlc = change.hlc_timestamp.clone();
                    *device_id = change.device_id.clone();
                }
            })
            .or_insert((change.hlc_timestamp.clone(), change.device_id.clone()));
    }

    // --- (3): inject exactly one authored_by_did per unique row ----------
    for ((table_name, row_pks), (hlc, device_id)) in per_row {
        stripped.push(LocalColumnChange {
            table_name,
            row_pks,
            column_name: "authored_by_did".to_string(),
            hlc_timestamp: hlc,
            value: JsonValue::String(ucan_audience.to_string()),
            device_id,
        });
    }

    InboundSyncPushOutcome::Accepted { changes: stripped }
}

/// Maps a membership-system table to the column whose value identifies the
/// row's owner. `None` means "no owner check" — currently used for
/// `haex_mls_sync_keys` where the row is a per-epoch derived value that all
/// members compute identically.
fn owner_column_for(table: &str) -> Option<&'static str> {
    match table {
        "haex_space_members" => Some("identity_id"),
        "haex_space_devices" => Some("device_endpoint_id"),
        "haex_device_mls_enrollments" => Some("device_id"),
        // Epoch-derived key, identical across all members of an epoch.
        // CRDT-LWW already lets any write member replace the value; the
        // membership-system whitelist does not widen that surface, so a
        // dedicated ownership column makes no sense here.
        "haex_mls_sync_keys" => None,
        _ => None,
    }
}

/// What identifies the *owner* of a row from the caller's perspective.
enum OwnerKind {
    /// Owner column holds an identity UUID; resolved against
    /// `haex_identities.id` and matched to the caller's audience DID.
    IdentityId,
    /// Owner column holds an Iroh node id; matched directly against the
    /// caller's QUIC peer endpoint id.
    EndpointId,
}

fn owner_kind_for(table: &str) -> Option<OwnerKind> {
    match table {
        "haex_space_members" => Some(OwnerKind::IdentityId),
        "haex_space_devices" | "haex_device_mls_enrollments" => Some(OwnerKind::EndpointId),
        _ => None,
    }
}

/// Reads a single column value from an existing CRDT row, identified by
/// its primary-key JSON (`row_pks`). Returns `Ok(None)` when the row does
/// not exist (a brand-new insert).
fn read_existing_column(
    db: &DbConnection,
    table: &str,
    row_pks_json: &str,
    column: &str,
) -> Result<Option<JsonValue>, DeliveryError> {
    let pks: HashMap<String, JsonValue> =
        serde_json::from_str(row_pks_json).map_err(|e| DeliveryError::ProtocolError {
            reason: format!("malformed row_pks JSON {row_pks_json:?}: {e}"),
        })?;
    if pks.is_empty() {
        return Err(DeliveryError::ProtocolError {
            reason: format!("empty row_pks JSON for table {table}"),
        });
    }

    // Validate identifiers — table and column come from the wire and must
    // not be interpolated unfiltered. Allow [a-z0-9_] only; everything in
    // the whitelist conforms to that.
    let safe = |s: &str| s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !safe(table) || !safe(column) || !pks.keys().all(|k| safe(k)) {
        return Err(DeliveryError::ProtocolError {
            reason: format!("identifier contains unsafe characters: table={table} column={column}"),
        });
    }

    let where_clause = pks
        .keys()
        .map(|k| format!("{k} = ?"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!("SELECT {column} FROM {table} WHERE {where_clause} LIMIT 1");

    let pk_values: Vec<JsonValue> = pks.values().cloned().collect();

    with_connection(db, |conn| {
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(pk_values.iter().map(|v| match v {
            JsonValue::String(s) => rusqlite::types::Value::Text(s.clone()),
            JsonValue::Number(n) if n.is_i64() => rusqlite::types::Value::Integer(n.as_i64().unwrap()),
            JsonValue::Number(n) => rusqlite::types::Value::Real(n.as_f64().unwrap_or_default()),
            JsonValue::Null => rusqlite::types::Value::Null,
            other => rusqlite::types::Value::Text(other.to_string()),
        })))?;

        if let Some(row) = rows.next()? {
            let raw: rusqlite::types::Value = row.get(0)?;
            let json = match raw {
                rusqlite::types::Value::Null => JsonValue::Null,
                rusqlite::types::Value::Integer(i) => JsonValue::Number(i.into()),
                rusqlite::types::Value::Real(r) => serde_json::Number::from_f64(r)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null),
                rusqlite::types::Value::Text(s) => JsonValue::String(s),
                rusqlite::types::Value::Blob(_) => JsonValue::Null,
            };
            Ok(Some(json))
        } else {
            Ok(None)
        }
    })
    .map_err(|e| DeliveryError::Database {
        reason: format!("read_existing_column({table}.{column}): {e}"),
    })
}

/// Resolve a DID to an identity UUID via `haex_identities`. Returns `None`
/// when no identity row matches — a state that should never arise for an
/// active space member (the leader inserts the identity during ClaimInvite).
fn resolve_identity_id_for_did(
    db: &DbConnection,
    did: &str,
) -> Result<Option<String>, DeliveryError> {
    let sql = "SELECT id FROM haex_identities WHERE did = ?1 LIMIT 1".to_string();
    let params = vec![JsonValue::String(did.to_string())];
    let rows = crate::database::core::select_with_crdt(sql, params, db).map_err(|e| {
        DeliveryError::Database {
            reason: format!("resolve_identity_id_for_did: {e}"),
        }
    })?;
    Ok(rows
        .first()
        .and_then(|row| row.first())
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

/// For every membership-system row in `changes`, verify that the row
/// already belongs to the caller — or, for inserts, that the change-set
/// itself declares the caller as owner. Rejects the whole batch on the
/// first violation.
///
/// `changes` is expected to be the output of [`validate_and_attribute`]
/// (i.e. table-whitelisted and `authored_by_did` already attributed).
pub fn enforce_row_ownership(
    db: &DbConnection,
    peer_endpoint_id: &str,
    audience_did: &str,
    changes: &[LocalColumnChange],
) -> Result<(), String> {
    // Group changes by (table, row_pks) and remember any owner-column
    // value the change set itself declares for the row.
    let mut per_row: HashMap<(String, String), Option<JsonValue>> = HashMap::new();
    for change in changes {
        if !is_membership_system_table(&change.table_name) {
            continue;
        }
        let key = (change.table_name.clone(), change.row_pks.clone());
        let owner_col = match owner_column_for(&change.table_name) {
            Some(c) => c,
            None => continue,
        };
        let entry = per_row.entry(key).or_insert(None);
        if change.column_name == owner_col {
            *entry = Some(change.value.clone());
        }
    }

    for ((table, row_pks), declared) in per_row {
        let kind = match owner_kind_for(&table) {
            Some(k) => k,
            None => continue,
        };
        let owner_col = owner_column_for(&table).expect("owner_kind_for ⇒ owner_column_for");

        // Always look up the *existing* owner first. If a row exists, its
        // owner column is the authoritative answer — peers cannot rewrite
        // it via push, otherwise a "set owner_col = me" change would let
        // anyone hijack any row. The change-set declaration only matters
        // for fresh inserts (no existing row).
        let existing = read_existing_column(db, &table, &row_pks, owner_col)
            .map_err(|e| format!("ownership lookup failed for {table} row {row_pks}: {e}"))?;

        let existing_owner: Option<String> = match existing {
            Some(JsonValue::String(s)) => Some(s),
            Some(JsonValue::Null) | None => None,
            Some(other) => {
                return Err(format!(
                    "row {table}/{row_pks}: existing owner column {owner_col} has non-string value {other}",
                ));
            }
        };

        let owner_value = match (&existing_owner, &declared) {
            // Existing row — declaration in push must either be absent or
            // identical. Any divergence is an ownership-rewrite attempt.
            (Some(existing), Some(JsonValue::String(decl))) => {
                if existing != decl {
                    return Err(format!(
                        "row {table}/{row_pks}: push attempts to change owner from {existing:?} to {decl:?}",
                    ));
                }
                existing.clone()
            }
            (Some(existing), None) => existing.clone(),
            (Some(_), Some(other)) => {
                return Err(format!(
                    "row {table}/{row_pks}: declared owner has non-string value {other}",
                ));
            }
            // Fresh insert — must declare a string owner.
            (None, Some(JsonValue::String(decl))) => decl.clone(),
            (None, None | Some(JsonValue::Null)) => {
                return Err(format!(
                    "row {table}/{row_pks}: owner column {owner_col} missing on insert",
                ));
            }
            (None, Some(other)) => {
                return Err(format!(
                    "row {table}/{row_pks}: declared owner has non-string value {other}",
                ));
            }
        };

        let ok = match kind {
            OwnerKind::EndpointId => owner_value == peer_endpoint_id,
            OwnerKind::IdentityId => match resolve_identity_id_for_did(db, audience_did) {
                Ok(Some(my_identity)) => owner_value == my_identity,
                Ok(None) => {
                    return Err(format!(
                        "caller DID {audience_did} has no identity row — cannot verify ownership of {table}",
                    ));
                }
                Err(e) => return Err(format!("identity lookup failed: {e}")),
            },
        };
        if !ok {
            return Err(format!(
                "row {table}/{row_pks}: owner {owner_value:?} does not match caller (endpoint={peer_endpoint_id}, did={audience_did})",
            ));
        }
    }

    Ok(())
}

/// Decide which capability level a push needs given the tables it touches.
/// A push that touches only [membership-system tables] is allowed for any
/// member (Read suffices). Any other table requires Write.
fn required_capability_for(changes: &[LocalColumnChange]) -> CapabilityLevel {
    if changes.iter().all(|c| is_membership_system_table(&c.table_name)) {
        CapabilityLevel::Read
    } else {
        CapabilityLevel::Write
    }
}

/// **The single authorisation entry point** for inbound CRDT pushes from
/// space peers. Every code path that wants to apply remote
/// `LocalColumnChange`s to the local DB MUST call this function and act
/// only on `Accepted { changes }`.
///
/// Pipeline (each step's success is a precondition for the next):
///
/// 1. Capability gate (per change-set, see [`required_capability_for`])
/// 2. Active-membership gate ([`is_active_space_member`])
/// 3. Payload validation + origin attribution ([`validate_and_attribute`])
/// 4. Per-row ownership for membership-system tables
///    ([`enforce_row_ownership`])
///
/// On success, the returned `Accepted` value carries the *sanitised*
/// change set: the client-supplied `authored_by_did` claims have been
/// replaced by leader-injected ones derived from the UCAN audience.
pub fn authorize_inbound_sync_push(
    db: &DbConnection,
    space_id: &str,
    peer_endpoint_id: &str,
    validated_ucan: &ValidatedUcan,
    raw_changes: Vec<LocalColumnChange>,
) -> InboundSyncPushOutcome {
    if raw_changes.is_empty() {
        return InboundSyncPushOutcome::Accepted { changes: vec![] };
    }

    // (1) Capability gate
    let required = required_capability_for(&raw_changes);
    if let Err(e) = require_capability(validated_ucan, space_id, required) {
        return InboundSyncPushOutcome::Rejected {
            reason: format!("Access denied: {e}"),
        };
    }

    // (2) Active-membership gate
    match is_active_space_member(db, space_id, &validated_ucan.audience) {
        Ok(true) => {}
        Ok(false) => {
            return InboundSyncPushOutcome::Rejected {
                reason: "Access denied: not an active member of this space".to_string(),
            };
        }
        Err(e) => {
            return InboundSyncPushOutcome::Rejected {
                reason: format!("Membership check failed: {e}"),
            };
        }
    }

    // (3) Payload validation + origin attribution
    let attributed = match validate_and_attribute(space_id, &validated_ucan.audience, raw_changes) {
        InboundSyncPushOutcome::Accepted { changes } => changes,
        rejected @ InboundSyncPushOutcome::Rejected { .. } => return rejected,
    };

    // (4) Per-row ownership for membership-system tables
    if let Err(reason) =
        enforce_row_ownership(db, peer_endpoint_id, &validated_ucan.audience, &attributed)
    {
        return InboundSyncPushOutcome::Rejected {
            reason: format!("Row ownership violation: {reason}"),
        };
    }

    InboundSyncPushOutcome::Accepted { changes: attributed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_change(
        table: &str,
        row_id: &str,
        column: &str,
        hlc: &str,
        value: JsonValue,
    ) -> LocalColumnChange {
        LocalColumnChange {
            table_name: table.to_string(),
            row_pks: format!(r#"{{"id":"{row_id}"}}"#),
            column_name: column.to_string(),
            hlc_timestamp: hlc.to_string(),
            value,
            device_id: "device-under-test".to_string(),
        }
    }

    fn expect_accepted(outcome: InboundSyncPushOutcome) -> Vec<LocalColumnChange> {
        match outcome {
            InboundSyncPushOutcome::Accepted { changes } => changes,
            InboundSyncPushOutcome::Rejected { reason } => {
                panic!("expected Accepted, got Rejected: {reason}")
            }
        }
    }

    fn expect_rejected(outcome: InboundSyncPushOutcome) -> String {
        match outcome {
            InboundSyncPushOutcome::Rejected { reason } => reason,
            InboundSyncPushOutcome::Accepted { .. } => panic!("expected Rejected, got Accepted"),
        }
    }

    #[test]
    fn rejects_non_whitelisted_table() {
        let changes = vec![make_change(
            "haex_identities",
            "row-1",
            "private_key",
            "1000/abcd",
            json!("leaked-key"),
        )];
        let reason = expect_rejected(validate_and_attribute(
            "space-A",
            "did:key:zAlice",
            changes,
        ));
        assert!(
            reason.contains("haex_identities"),
            "reason should name the bad table: {reason}"
        );
    }

    #[test]
    fn rejects_foreign_space_id_column_value() {
        let changes = vec![make_change(
            "haex_peer_shares",
            "row-1",
            "space_id",
            "1000/abcd",
            json!("space-B"),
        )];
        let reason = expect_rejected(validate_and_attribute(
            "space-A",
            "did:key:zAlice",
            changes,
        ));
        assert!(
            reason.contains("space-A") || reason.contains("space-B"),
            "reason should mention the space_id mismatch: {reason}"
        );
    }

    #[test]
    fn accepts_matching_space_id_column_value() {
        let changes = vec![make_change(
            "haex_peer_shares",
            "row-1",
            "space_id",
            "1000/abcd",
            json!("space-A"),
        )];
        let out = expect_accepted(validate_and_attribute(
            "space-A",
            "did:key:zAlice",
            changes,
        ));
        assert!(out.iter().any(|c| c.column_name == "space_id"));
    }

    #[test]
    fn strips_client_supplied_authored_by_did() {
        // Attacker pushes a row and tries to claim Bob wrote it.
        let changes = vec![
            make_change(
                "haex_peer_shares",
                "row-1",
                "space_id",
                "1000/abcd",
                json!("space-A"),
            ),
            make_change(
                "haex_peer_shares",
                "row-1",
                "name",
                "2000/abcd",
                json!("evil-share"),
            ),
            make_change(
                "haex_peer_shares",
                "row-1",
                "authored_by_did",
                "3000/abcd",
                json!("did:key:zBob"),
            ),
        ];
        let out = expect_accepted(validate_and_attribute(
            "space-A",
            "did:key:zMallory",
            changes,
        ));

        let author_changes: Vec<&LocalColumnChange> = out
            .iter()
            .filter(|c| c.column_name == "authored_by_did")
            .collect();
        assert_eq!(
            author_changes.len(),
            1,
            "exactly one authored_by_did change expected, got {author_changes:?}"
        );
        let author_value = author_changes[0].value.as_str().unwrap();
        assert_eq!(
            author_value, "did:key:zMallory",
            "origin must be the UCAN audience (Mallory), not the client claim (Bob)",
        );
    }

    #[test]
    fn injects_one_authored_by_did_per_unique_row() {
        let changes = vec![
            make_change(
                "haex_peer_shares",
                "row-1",
                "name",
                "1000/abcd",
                json!("share-one"),
            ),
            make_change(
                "haex_peer_shares",
                "row-1",
                "local_path",
                "2000/abcd",
                json!("/path/one"),
            ),
            make_change(
                "haex_peer_shares",
                "row-2",
                "name",
                "3000/abcd",
                json!("share-two"),
            ),
        ];
        let out = expect_accepted(validate_and_attribute(
            "space-A",
            "did:key:zAlice",
            changes,
        ));

        let mut author_rows: Vec<&str> = out
            .iter()
            .filter(|c| c.column_name == "authored_by_did")
            .map(|c| c.row_pks.as_str())
            .collect();
        author_rows.sort();
        assert_eq!(
            author_rows,
            vec![r#"{"id":"row-1"}"#, r#"{"id":"row-2"}"#],
            "exactly one authored_by_did per unique row expected",
        );
    }

    #[test]
    fn authored_by_did_uses_max_hlc_within_row_group() {
        // HLC string format is "<ntp_nanos>/<node_id_hex>" — compared
        // numerically by the time component. Pass them out of order to
        // prove the transform picks the real maximum, not the first-seen.
        let changes = vec![
            make_change(
                "haex_peer_shares",
                "row-1",
                "name",
                "1000/abcd",
                json!("a"),
            ),
            make_change(
                "haex_peer_shares",
                "row-1",
                "local_path",
                "9000/abcd",
                json!("z"),
            ),
            make_change(
                "haex_peer_shares",
                "row-1",
                "device_endpoint_id",
                "5000/abcd",
                json!("m"),
            ),
        ];
        let out = expect_accepted(validate_and_attribute(
            "space-A",
            "did:key:zAlice",
            changes,
        ));

        let author = out
            .iter()
            .find(|c| c.column_name == "authored_by_did")
            .expect("authored_by_did should be injected");
        assert_eq!(
            author.hlc_timestamp, "9000/abcd",
            "authored_by_did HLC should be the max HLC of the row-group",
        );
    }

    #[test]
    fn origin_always_comes_from_audience_never_from_payload() {
        // Even with no client-supplied authored_by_did, the leader sets one
        // from the audience — the UX contract is "every synced row has an
        // origin, and the origin is the authenticated identity".
        let changes = vec![make_change(
            "haex_space_members",
            "row-1",
            "role",
            "1000/abcd",
            json!("write"),
        )];
        let out = expect_accepted(validate_and_attribute(
            "space-A",
            "did:key:zAlice",
            changes,
        ));

        let author = out
            .iter()
            .find(|c| c.column_name == "authored_by_did")
            .expect("authored_by_did must be injected even without client input");
        assert_eq!(author.value.as_str(), Some("did:key:zAlice"));
    }

    #[test]
    fn empty_batch_stays_empty() {
        let out = expect_accepted(validate_and_attribute(
            "space-A",
            "did:key:zAlice",
            vec![],
        ));
        assert!(out.is_empty());
    }

    #[test]
    fn preserves_non_attribution_changes() {
        // Sanity: the transform must not swallow legitimate changes.
        let changes = vec![make_change(
            "haex_peer_shares",
            "row-1",
            "name",
            "1000/abcd",
            json!("my-share"),
        )];
        let out = expect_accepted(validate_and_attribute(
            "space-A",
            "did:key:zAlice",
            changes,
        ));
        assert!(
            out.iter().any(|c| c.column_name == "name"
                && c.value.as_str() == Some("my-share")),
            "original 'name' change must be preserved",
        );
    }

    // =========================================================================
    // Authorization pipeline tests (capability + membership + ownership)
    //
    // These exercise `authorize_inbound_sync_push`, the single choke point
    // every real handler must use. Each test sets up just enough DB state
    // (identities, member rows, device rows) for the scenario it covers,
    // constructs a `ValidatedUcan` directly (bypassing JWT signing — the
    // function trusts the validated audience by contract) and asserts on
    // the outcome.
    // =========================================================================

    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    use crate::crdt::hlc::HlcService;
    use crate::database::connection_context::ConnectionContext;
    use crate::database::core::{install_tx_hlc_hooks, register_current_hlc_udf};
    use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
    use crate::ucan::{CapabilityLevel, ValidatedUcan};

    /// In-memory DB with all schemas the authorization pipeline reads from.
    /// Schemas mirror production but skip CRDT triggers — these tests do
    /// not exercise the CRDT merge layer, only authorization decisions.
    fn setup_authz_db() -> DbConnection {
        let conn = Connection::open_in_memory().unwrap();
        let hlc = HlcService::new_for_testing("test-device");
        let ctx = ConnectionContext::new();
        register_current_hlc_udf(&conn, hlc, ctx.clone()).unwrap();
        install_tx_hlc_hooks(&conn, ctx).unwrap();

        conn.execute_batch(&format!(
            "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);",
            TABLE_CRDT_CONFIGS
        ))
        .unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
            TABLE_CRDT_DIRTY_TABLES
        ))
        .unwrap();

        conn.execute_batch(
            "CREATE TABLE haex_identities (
                id TEXT PRIMARY KEY,
                did TEXT NOT NULL UNIQUE,
                public_key TEXT,
                created_at TEXT
            );

            CREATE TABLE haex_spaces (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL DEFAULT 'local',
                status TEXT NOT NULL DEFAULT 'active',
                name TEXT NOT NULL
            );

            CREATE TABLE haex_space_members (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                identity_id TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'read',
                authored_by_did TEXT,
                joined_at TEXT
            );

            CREATE TABLE haex_space_devices (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                identity_id TEXT,
                device_endpoint_id TEXT NOT NULL,
                device_name TEXT NOT NULL,
                relay_url TEXT,
                authored_by_did TEXT
            );

            CREATE TABLE haex_device_mls_enrollments (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                key_package TEXT NOT NULL,
                welcome TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                authored_by_did TEXT
            );

            CREATE TABLE haex_mls_sync_keys (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                epoch INTEGER NOT NULL,
                key_data TEXT NOT NULL,
                authored_by_did TEXT
            );

            CREATE TABLE haex_peer_shares (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                device_endpoint_id TEXT NOT NULL,
                name TEXT NOT NULL,
                local_path TEXT NOT NULL,
                authored_by_did TEXT
            );",
        )
        .unwrap();

        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    fn insert_identity(db: &DbConnection, identity_id: &str, did: &str) {
        let guard = db.0.lock().unwrap();
        guard.as_ref().unwrap().execute(
            "INSERT INTO haex_identities (id, did) VALUES (?1, ?2)",
            rusqlite::params![identity_id, did],
        ).unwrap();
    }

    fn insert_member(
        db: &DbConnection,
        member_row_id: &str,
        space_id: &str,
        identity_id: &str,
        role: &str,
    ) {
        let guard = db.0.lock().unwrap();
        guard.as_ref().unwrap().execute(
            "INSERT INTO haex_space_members (id, space_id, identity_id, role) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![member_row_id, space_id, identity_id, role],
        ).unwrap();
    }

    fn insert_device(
        db: &DbConnection,
        device_row_id: &str,
        space_id: &str,
        identity_id: Option<&str>,
        endpoint_id: &str,
        name: &str,
    ) {
        let guard = db.0.lock().unwrap();
        guard.as_ref().unwrap().execute(
            "INSERT INTO haex_space_devices (id, space_id, identity_id, device_endpoint_id, device_name) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![device_row_id, space_id, identity_id, endpoint_id, name],
        ).unwrap();
    }

    fn make_ucan(audience: &str, space_id: &str, level: CapabilityLevel) -> ValidatedUcan {
        let mut capabilities = HashMap::new();
        capabilities.insert(space_id.to_string(), level);
        ValidatedUcan {
            issuer: "did:key:zIssuer".to_string(),
            audience: audience.to_string(),
            capabilities,
            expires_at: u64::MAX,
        }
    }

    fn change(
        table: &str,
        row_id: &str,
        column: &str,
        hlc: &str,
        value: JsonValue,
    ) -> LocalColumnChange {
        LocalColumnChange {
            table_name: table.to_string(),
            row_pks: format!(r#"{{"id":"{row_id}"}}"#),
            column_name: column.to_string(),
            hlc_timestamp: hlc.to_string(),
            value,
            device_id: "wire-device-id".to_string(),
        }
    }

    // -------------------------------------------------------------------------
    // Capability gate: read-only is enough for membership-system tables only
    // -------------------------------------------------------------------------

    #[test]
    fn authz_read_only_member_can_push_own_membership_update() {
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        // Updates "joined_at" on her own row — no identity_id in the change
        // set, so ownership falls back to the existing DB row (which is
        // hers).
        let changes = vec![change(
            "haex_space_members",
            "mem-mallory",
            "joined_at",
            "100/abcd",
            json!("2026-01-01"),
        )];

        let outcome = authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        );
        assert!(
            matches!(outcome, InboundSyncPushOutcome::Accepted { .. }),
            "read-only member must be able to update her own membership row, got: {outcome:?}",
        );
    }

    #[test]
    fn authz_read_only_member_cannot_push_peer_shares() {
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![change(
            "haex_peer_shares",
            "share-1",
            "name",
            "100/abcd",
            json!("malicious-share"),
        )];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.to_lowercase().contains("access denied") || reason.contains("Insufficient"),
            "expected capability rejection, got: {reason}",
        );
    }

    #[test]
    fn authz_write_member_can_push_peer_shares() {
        let db = setup_authz_db();
        insert_identity(&db, "id-alice", "did:key:zAlice");
        insert_member(&db, "mem-alice", "space-A", "id-alice", "write");

        let ucan = make_ucan("did:key:zAlice", "space-A", CapabilityLevel::Write);
        let changes = vec![
            change(
                "haex_peer_shares",
                "share-1",
                "device_endpoint_id",
                "100/abcd",
                json!("endpoint-alice"),
            ),
            change("haex_peer_shares", "share-1", "name", "100/abcd", json!("docs")),
            change(
                "haex_peer_shares",
                "share-1",
                "local_path",
                "100/abcd",
                json!("/home/alice/docs"),
            ),
        ];

        let outcome = authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-alice",
            &ucan,
            changes,
        );
        assert!(
            matches!(outcome, InboundSyncPushOutcome::Accepted { .. }),
            "write member must be able to push peer_shares, got: {outcome:?}",
        );
    }

    #[test]
    fn authz_mixed_push_with_user_table_requires_write() {
        // Membership row + peer_shares row in the same push: the mixed
        // batch escalates to Write because of peer_shares.
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![
            change(
                "haex_space_members",
                "mem-mallory",
                "joined_at",
                "100/abcd",
                json!("2026-01-01"),
            ),
            change(
                "haex_peer_shares",
                "share-1",
                "name",
                "100/abcd",
                json!("evil-share"),
            ),
        ];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.to_lowercase().contains("access denied"),
            "mixed push with peer_shares should fail capability check for read member: {reason}",
        );
    }

    #[test]
    fn authz_member_for_other_space_rejected() {
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        // Mallory is a member of space-B, not space-A
        insert_member(&db, "mem-mallory", "space-B", "id-mallory", "write");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Write);
        let changes = vec![change(
            "haex_space_members",
            "mem-mallory",
            "role",
            "100/abcd",
            json!("admin"),
        )];

        // require_capability passes (UCAN says space-A); but membership
        // check rejects (Mallory is not a member of space-A).
        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.contains("not an active member"),
            "non-member must be rejected, got: {reason}",
        );
    }

    // -------------------------------------------------------------------------
    // Per-row ownership: the new attack-surface check
    // -------------------------------------------------------------------------

    #[test]
    fn authz_read_only_cannot_overwrite_admin_membership_row() {
        // Classic privilege escalation attempt: Mallory (read) tries to
        // set Bob's membership identity_id to herself — which would let
        // her impersonate Bob. The ownership check must catch this.
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_identity(&db, "id-bob", "did:key:zBob");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
        insert_member(&db, "mem-bob", "space-A", "id-bob", "admin");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![change(
            "haex_space_members",
            "mem-bob",
            "identity_id",
            "100/abcd",
            json!("id-mallory"),
        )];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.contains("ownership") || reason.contains("does not match caller"),
            "Mallory must not be able to overwrite Bob's row, got: {reason}",
        );
    }

    #[test]
    fn authz_read_only_cannot_modify_foreign_member_role() {
        // Mallory pushes a `role=admin` change targeted at Bob's row but
        // does NOT include identity_id — the ownership check must pull
        // identity_id from the existing DB row and reject because Bob's
        // identity is not Mallory's.
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_identity(&db, "id-bob", "did:key:zBob");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
        insert_member(&db, "mem-bob", "space-A", "id-bob", "read");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![change(
            "haex_space_members",
            "mem-bob",
            "role",
            "100/abcd",
            json!("admin"),
        )];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.contains("ownership") || reason.contains("does not match caller"),
            "Mallory must not silently modify Bob's row, got: {reason}",
        );
    }

    #[test]
    fn authz_member_can_insert_own_new_membership_row() {
        // Mallory's identity exists, but the membership row is being
        // pushed for the first time (insert). The change set must declare
        // identity_id, and that identity must resolve to Mallory.
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        // Mallory is already an active member via existing row, but is
        // pushing a *new* membership row for herself (e.g. re-issued
        // after a rejoin). NOTE: validate_and_attribute is_active_space_member
        // requires Mallory to be a member already, so we set it up.
        insert_member(&db, "mem-mallory-old", "space-A", "id-mallory", "read");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![
            change(
                "haex_space_members",
                "mem-mallory-new",
                "space_id",
                "100/abcd",
                json!("space-A"),
            ),
            change(
                "haex_space_members",
                "mem-mallory-new",
                "identity_id",
                "100/abcd",
                json!("id-mallory"),
            ),
            change(
                "haex_space_members",
                "mem-mallory-new",
                "role",
                "100/abcd",
                json!("read"),
            ),
        ];

        let outcome = authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        );
        assert!(
            matches!(outcome, InboundSyncPushOutcome::Accepted { .. }),
            "self-insert must succeed, got: {outcome:?}",
        );
    }

    #[test]
    fn authz_member_cannot_insert_membership_with_others_identity() {
        // Mallory creates a fresh row (new uuid) but claims Bob's
        // identity_id — that would inject a fake "Bob is admin" row.
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_identity(&db, "id-bob", "did:key:zBob");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![
            change(
                "haex_space_members",
                "mem-fake",
                "identity_id",
                "100/abcd",
                json!("id-bob"),
            ),
            change(
                "haex_space_members",
                "mem-fake",
                "role",
                "100/abcd",
                json!("admin"),
            ),
        ];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.contains("ownership") || reason.contains("does not match caller"),
            "must not allow forging row for foreign identity, got: {reason}",
        );
    }

    #[test]
    fn authz_member_can_register_own_device() {
        let db = setup_authz_db();
        insert_identity(&db, "id-alice", "did:key:zAlice");
        insert_member(&db, "mem-alice", "space-A", "id-alice", "read");

        let ucan = make_ucan("did:key:zAlice", "space-A", CapabilityLevel::Read);
        let changes = vec![
            change(
                "haex_space_devices",
                "dev-alice",
                "space_id",
                "100/abcd",
                json!("space-A"),
            ),
            change(
                "haex_space_devices",
                "dev-alice",
                "device_endpoint_id",
                "100/abcd",
                json!("endpoint-alice"),
            ),
            change(
                "haex_space_devices",
                "dev-alice",
                "device_name",
                "100/abcd",
                json!("Alice's Laptop"),
            ),
        ];

        let outcome = authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-alice",
            &ucan,
            changes,
        );
        assert!(
            matches!(outcome, InboundSyncPushOutcome::Accepted { .. }),
            "Alice must be able to register her own device, got: {outcome:?}",
        );
    }

    #[test]
    fn authz_member_cannot_hijack_foreign_device_endpoint() {
        // Mallory pushes a haex_space_devices row claiming Bob's endpoint.
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_identity(&db, "id-bob", "did:key:zBob");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![
            change(
                "haex_space_devices",
                "dev-fake",
                "device_endpoint_id",
                "100/abcd",
                json!("endpoint-bob"),
            ),
            change(
                "haex_space_devices",
                "dev-fake",
                "device_name",
                "100/abcd",
                json!("Pretending to be Bob"),
            ),
        ];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.contains("ownership") || reason.contains("does not match caller"),
            "device endpoint hijack must be rejected, got: {reason}",
        );
    }

    #[test]
    fn authz_member_cannot_modify_foreign_device_row() {
        // Existing device row belongs to Bob; Mallory tries to update its
        // name without changing endpoint_id (so ownership comes from DB).
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_identity(&db, "id-bob", "did:key:zBob");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
        insert_device(
            &db,
            "dev-bob",
            "space-A",
            Some("id-bob"),
            "endpoint-bob",
            "Bob's Phone",
        );

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![change(
            "haex_space_devices",
            "dev-bob",
            "device_name",
            "100/abcd",
            json!("Hacked"),
        )];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.contains("ownership") || reason.contains("does not match caller"),
            "Mallory must not be able to alter Bob's device row, got: {reason}",
        );
    }

    #[test]
    fn authz_mixed_batch_one_foreign_row_rejects_whole_push() {
        // Whole-batch atomicity: a single bad row taints the whole push.
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_identity(&db, "id-bob", "did:key:zBob");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
        insert_member(&db, "mem-bob", "space-A", "id-bob", "read");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
        let changes = vec![
            // Legitimate self-update
            change(
                "haex_space_members",
                "mem-mallory",
                "joined_at",
                "100/abcd",
                json!("2026-01-01"),
            ),
            // …piggybacked with a foreign-row update
            change(
                "haex_space_members",
                "mem-bob",
                "role",
                "100/abcd",
                json!("admin"),
            ),
        ];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ));
        assert!(
            reason.contains("ownership") || reason.contains("does not match caller"),
            "the whole batch must be rejected even if one row is legit, got: {reason}",
        );
    }

    #[test]
    fn authz_cross_space_id_injection_blocked() {
        // Even with valid Write capability for space-A, attempting to set
        // space_id=space-B in the payload must fail. Defense-in-depth on
        // top of the per-row ownership check.
        let db = setup_authz_db();
        insert_identity(&db, "id-alice", "did:key:zAlice");
        insert_member(&db, "mem-alice", "space-A", "id-alice", "write");

        let ucan = make_ucan("did:key:zAlice", "space-A", CapabilityLevel::Write);
        let changes = vec![change(
            "haex_peer_shares",
            "share-1",
            "space_id",
            "100/abcd",
            json!("space-B"),
        )];

        let reason = expect_rejected(authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-alice",
            &ucan,
            changes,
        ));
        assert!(
            reason.contains("space-A") || reason.contains("space-B"),
            "cross-space injection must be blocked, got: {reason}",
        );
    }

    #[test]
    fn authz_authored_by_did_forge_attempt_is_rewritten() {
        // Confirms validate_and_attribute keeps working through the
        // central function: a client-supplied authored_by_did = Bob is
        // overwritten by the leader to = Mallory (the authenticated
        // audience), so post-attribution the row still attributes to
        // Mallory regardless of what the wire claimed.
        let db = setup_authz_db();
        insert_identity(&db, "id-mallory", "did:key:zMallory");
        insert_identity(&db, "id-bob", "did:key:zBob");
        insert_member(&db, "mem-mallory", "space-A", "id-mallory", "write");

        let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Write);
        let changes = vec![
            change(
                "haex_peer_shares",
                "share-mallory",
                "device_endpoint_id",
                "100/abcd",
                json!("endpoint-mallory"),
            ),
            change(
                "haex_peer_shares",
                "share-mallory",
                "name",
                "100/abcd",
                json!("share"),
            ),
            change(
                "haex_peer_shares",
                "share-mallory",
                "local_path",
                "100/abcd",
                json!("/m"),
            ),
            change(
                "haex_peer_shares",
                "share-mallory",
                "authored_by_did",
                "100/abcd",
                json!("did:key:zBob"),
            ),
        ];

        let out = match authorize_inbound_sync_push(
            &db,
            "space-A",
            "endpoint-mallory",
            &ucan,
            changes,
        ) {
            InboundSyncPushOutcome::Accepted { changes } => changes,
            InboundSyncPushOutcome::Rejected { reason } => {
                panic!("expected Accepted, got Rejected: {reason}")
            }
        };

        let author = out
            .iter()
            .find(|c| c.column_name == "authored_by_did")
            .expect("authored_by_did must be present");
        assert_eq!(
            author.value.as_str(),
            Some("did:key:zMallory"),
            "leader must overwrite forged authored_by_did with audience",
        );
    }
}
