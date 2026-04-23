//! Inbound-SyncPush validation and origin attribution.
//!
//! The leader receives a batch of column-level CRDT changes from a peer,
//! already authenticated by UCAN signature and authorised for the declared
//! `space_id` with Write capability. This module performs the remaining
//! *payload-level* checks and the authoritative origin tagging that the
//! handler can no longer trust to the client:
//!
//! 1. **Table whitelist.** Only rows for tables in
//!    [`crate::crdt::scanner::SPACE_SCOPED_CRDT_TABLES`] may cross the wire.
//! 2. **`space_id` column scope.** Any change that writes the `space_id`
//!    column must set it to the request's `space_id`; anything else is a
//!    cross-space injection attempt.
//! 3. **Origin attribution.** The client's claim about `authored_by_did`
//!    is *stripped* from the batch; the leader re-injects exactly one
//!    `authored_by_did` column-change per unique `(table, row)` with the
//!    value taken from the validated UCAN audience. A peer cannot forge
//!    authorship because the field is never read from the wire.
//!
//! The transform is pure: no I/O, no clocks, no randomness. Callers wrap
//! it with a DB-write step.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::crdt::hlc::hlc_is_newer;
use crate::crdt::scanner::{is_space_scoped_table, LocalColumnChange};

/// The outcome of validating and attributing an inbound SyncPush batch.
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
}
