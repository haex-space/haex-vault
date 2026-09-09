//! Per-space column-signature verification, moved verbatim out of the old
//! monolithic apply loop in `db.rs`. The only real change from before this
//! cutover is that these are now called from inside
//! [`super::policy::VaultApplyPolicy::prepare_row`] / `after_row` with a real
//! `&Transaction` from the hook, instead of from inside the old per-row loop.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::{OptionalExtension, Transaction};
use serde_json::Value as JsonValue;

use crate::crdt::shared_space_trigger::ColumnInfo;
use crate::database::error::DatabaseError;

use super::super::helpers::json_values_to_sql_params;
use super::types::{ColumnSig, RemoteColumnChange};

/// Idempotently insert a stub `haex_identities` row for a DID we've never
/// seen before, so downstream FKs referencing `haex_identities.did` don't
/// fail when the first inbound row for a foreign author lands.
///
/// The stub uses the DID itself as its `id` (both columns are TEXT). Once
/// the real identity handshake arrives — e.g. the invite-claim flow — the
/// row is UPDATE-merged in place by the usual CRDT path; there is no
/// separate reconciliation step.
///
/// This function is the Rust replacement for the DB-trigger-based stub
/// creation that the older schema relied on (ADR 0002 §6, §D). It is called
/// from [`super::policy::VaultApplyPolicy::prepare_row`] for every column
/// change that carries a **valid** column signature. Invalid or missing sigs
/// must NOT create a stub — that would let a peer flood `haex_identities`
/// with attacker-picked DIDs.
///
/// `name` is written explicitly (with the DID as the placeholder label)
/// because the column is `TEXT NOT NULL` with **no default**: omitting it
/// makes SQLite raise a NOT NULL constraint violation that `OR IGNORE`
/// then swallows, so the stub is silently never created and the FK it was
/// meant to satisfy still dangles. `source` relies on its schema default
/// (`'contact'`), which is the correct provenance for a DID we only know
/// from an inbound signature.
pub(super) fn ensure_identity_stub(tx: &Transaction, did: &str) -> Result<(), DatabaseError> {
    tx.execute(
        "INSERT OR IGNORE INTO haex_identities (id, did, name) VALUES (?1, ?1, ?1)",
        [did],
    )
    .map_err(DatabaseError::from)?;
    Ok(())
}

/// Compute the `space_id` that a column-sig on this row must have been
/// signed under.
///
/// **The locally persisted `space_id` always wins.** The batch-supplied
/// `space_id` column change is only consulted when the row does not exist
/// locally yet (the INSERT path), and even then only if it agrees with
/// `expected_space_id` — the space the caller scoped this pull to.
///
/// Precedence is not cosmetic. `space_id` arrives as an ordinary,
/// unauthenticated column change off the wire, so trusting it over the
/// stored value hands the attacker the verification anchor: a peer holding
/// a signing key for its own space `S_evil` could push
/// `{space_id: "S_evil", <target column>: <value>}` at a row that locally
/// belongs to `S_victim`. Both changes then verify under `S_evil`, and the
/// column update lands on the victim row — defeating the space binding in
/// the preimage (ADR 0002 §4b) precisely because the verifier let the
/// attacker pick the binding.
///
/// Returns `Ok(None)` when no trustworthy anchor exists (table has no
/// `space_id` column, the row is new and the caller gave no
/// `expected_space_id`, or the batch's claimed space contradicts it).
/// `verify_change_sig` turns that into a per-change rejection, so an
/// unanchored row drops its signed changes rather than applying them
/// unverified.
pub(super) fn resolve_row_space_id_for_sig(
    tx: &Transaction,
    table_name: &str,
    pk_where_clause: &str,
    pk_values_for_query: &[JsonValue],
    row_change_list: &[RemoteColumnChange],
    schema: &[ColumnInfo],
    expected_space_id: Option<&str>,
) -> Result<Option<String>, DatabaseError> {
    // (1) Authoritative source: the row's own persisted space_id.
    if schema.iter().any(|c| c.name == "space_id") {
        let sql = format!(
            "SELECT space_id FROM \"{}\" WHERE {}",
            table_name, pk_where_clause
        );
        let mut stmt = tx.prepare(&sql).map_err(DatabaseError::from)?;
        let params = json_values_to_sql_params(pk_values_for_query)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let persisted: Option<Option<String>> = stmt
            .query_row(&*params_refs, |row| row.get::<_, Option<String>>(0))
            .optional()
            .map_err(DatabaseError::from)?;
        if let Some(space_id) = persisted.flatten() {
            return Ok(Some(space_id));
        }
    }

    // (2) INSERT path — the row is new, so there is nothing persisted to
    // anchor on. Fall back to the batch's claimed space_id, but only after
    // cross-checking it against the space this pull was scoped to. Without
    // an expected space there is no way to tell an honest claim from a
    // forged one, so we refuse to guess.
    let expected = match expected_space_id {
        Some(e) => e,
        None => return Ok(None),
    };
    let claimed = row_change_list
        .iter()
        .find(|c| c.column_name == "space_id")
        .and_then(|c| match &c.decrypted_value {
            JsonValue::String(s) => Some(s.as_str()),
            _ => None,
        });
    match claimed {
        // No claim in the batch: the pull scope is still a valid anchor —
        // every change in this batch was fetched for `expected`.
        None => Ok(Some(expected.to_string())),
        Some(c) if c == expected => Ok(Some(expected.to_string())),
        Some(c) => {
            eprintln!(
                "[SYNC RUST] Refusing sig anchor for new row in '{}': batch claims space_id '{}' but pull is scoped to '{}'",
                table_name, c, expected
            );
            Ok(None)
        }
    }
}

/// Verify a single `RemoteColumnChange`'s attached column signature against
/// the row's `space_id`. Returns `Ok(())` on a good sig, `Err(reason)` when
/// the sig is malformed, the space_id is unavailable, or the Ed25519 check
/// fails. The caller drops the change on `Err` and keeps the rest of the
/// batch flowing (row-scoped rejection — Phase-2 pattern from ADR 0002 §6).
pub(super) fn verify_change_sig(
    change: &RemoteColumnChange,
    sig: &ColumnSig,
    row_space_id: Option<&str>,
    table_name: &str,
    row_pks: &str,
) -> Result<(), String> {
    let space_id =
        row_space_id.ok_or_else(|| "space_id unavailable — cannot verify sig".to_string())?;
    if sig.sig.len() > 88 {
        return Err("signature exceeds the 64-byte Ed25519 wire size".to_string());
    }
    if sig.storage_class == crate::crdt::column_sig::value_bytes::StorageClass::Blob {
        let max = crate::crdt::column_sig::limits::MAX_VALUE_BYTES_LEN;
        if let Some(encoded) = change.decrypted_value.as_str() {
            if encoded.len() > max * 4 / 3 + 4 {
                return Err("BLOB value exceeds the column-signature size limit".to_string());
            }
        } else if let Some(array) = change.decrypted_value.as_array() {
            if array.len().saturating_add(1) > max {
                return Err("BLOB value exceeds the column-signature size limit".to_string());
            }
        }
    }
    let sql_value = sig.storage_class.restore(&change.decrypted_value)?;
    let value_bytes_vec = crate::crdt::column_sig::value_bytes::to_canonical_bytes(&sql_value);
    let sig_bytes = BASE64
        .decode(&sig.sig)
        .map_err(|e| format!("malformed sig base64: {e}"))?;
    crate::crdt::column_sig::verify::verify_column_sig(
        space_id.as_bytes(),
        table_name.as_bytes(),
        row_pks.as_bytes(),
        change.column_name.as_bytes(),
        change.hlc_timestamp.as_bytes(),
        &sig.author_did,
        &value_bytes_vec,
        &sig_bytes,
    )
    .map_err(|e| format!("verify_column_sig: {e:?}"))
}

#[cfg(test)]
#[path = "signatures_tests.rs"]
mod tests;
