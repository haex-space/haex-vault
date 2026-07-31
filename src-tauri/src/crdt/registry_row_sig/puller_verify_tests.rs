// Task B.4 — puller-side verify-on-apply for `haex_shared_space_sync`
// registry rows. Complements `tests.rs` (payload/sign/verify roundtrip) and
// `database::core_registry_row_sig_tests` (B.3 sign-on-write) with the
// peer-boundary enforcement point: an incoming change from another device is
// dropped unless its `row_sig` verifies AND (on UPDATE) its
// `authored_by_did` matches the row's existing value.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{SigningKey, VerifyingKey};

use super::payload::RegistryRowSigPayload;
use super::puller_verify::{
    verify_incoming_registry_change, IncomingRegistryChange, PersistedRegistryRow,
    RegistryVerifyError,
};
use super::sign::sign_registry_row;
use super::verify::VerifyRegistryRowSigError;
use crate::ucan::verify::did_key_from_public_key;

fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let pk = sk.verifying_key();
    (sk, pk)
}

/// Owned mirror of every `RegistryRowSigPayload` field, so a test can build
/// both the borrowing payload (to sign) and the owned
/// `IncomingRegistryChange` (this module's input type) from one set of
/// values without fighting borrow lifetimes.
struct SampleRow {
    id: String,
    space_id: String,
    table_name: String,
    row_pks: String,
    extension_public_key: Option<String>,
    extension_name: Option<String>,
    category: Option<String>,
    r#type: Option<String>,
    category_label: Option<String>,
    type_label: Option<String>,
    authored_by_did: String,
    created_at: String,
}

impl SampleRow {
    fn with_did(did: &str) -> Self {
        SampleRow {
            id: "row-1".to_string(),
            space_id: "space-1".to_string(),
            table_name: "ext_calendar_v1".to_string(),
            row_pks: r#"{"id":"evt-42"}"#.to_string(),
            extension_public_key: Some("epk".to_string()),
            extension_name: Some("calendar".to_string()),
            category: Some("work".to_string()),
            r#type: Some("event".to_string()),
            category_label: Some("Work Calendar".to_string()),
            type_label: Some("Termin".to_string()),
            authored_by_did: did.to_string(),
            created_at: "2026-07-31T00:00:00Z".to_string(),
        }
    }

    fn payload(&self) -> RegistryRowSigPayload<'_> {
        RegistryRowSigPayload {
            id: &self.id,
            space_id: &self.space_id,
            table_name: &self.table_name,
            row_pks: &self.row_pks,
            extension_public_key: self.extension_public_key.as_deref(),
            extension_name: self.extension_name.as_deref(),
            category: self.category.as_deref(),
            r#type: self.r#type.as_deref(),
            category_label: self.category_label.as_deref(),
            type_label: self.type_label.as_deref(),
            authored_by_did: &self.authored_by_did,
            created_at: &self.created_at,
        }
    }

    /// Sign this row's current field values with `sk` and package the result
    /// as the module's input type.
    fn signed_change(&self, sk: &SigningKey) -> IncomingRegistryChange {
        let sig = sign_registry_row(&self.payload(), sk);
        IncomingRegistryChange {
            id: self.id.clone(),
            space_id: self.space_id.clone(),
            table_name: self.table_name.clone(),
            row_pks: self.row_pks.clone(),
            extension_public_key: self.extension_public_key.clone(),
            extension_name: self.extension_name.clone(),
            category: self.category.clone(),
            r#type: self.r#type.clone(),
            category_label: self.category_label.clone(),
            type_label: self.type_label.clone(),
            authored_by_did: self.authored_by_did.clone(),
            created_at: self.created_at.clone(),
            row_sig: BASE64.encode(sig.to_bytes()),
        }
    }
}

/// A validly signed change from a freshly generated identity — the "happy
/// path" starting point most rejection tests mutate one field of.
fn valid_change() -> IncomingRegistryChange {
    let (sk, pk) = generate_keypair();
    let did = did_key_from_public_key(&pk);
    SampleRow::with_did(&did).signed_change(&sk)
}

#[test]
fn puller_accepts_registry_row_with_valid_sig() {
    let change = valid_change();
    let result = verify_incoming_registry_change(&change, None);
    assert!(matches!(result, Ok(())), "{result:?}");
}

#[test]
fn puller_rejects_registry_row_with_forged_authored_by_did() {
    // Alice signs a payload correctly attributed to herself. The wire change
    // is then relabelled as authored by Mallory (a second, unrelated
    // identity) without re-signing. `row_sig` still covers Alice's payload
    // (authored_by_did = Alice's DID), so rebuilding the payload with
    // Mallory's DID yields a canonical encoding Alice's signature never
    // covered — and it does not verify under Mallory's key either, since
    // Mallory never signed anything.
    let (sk_alice, pk_alice) = generate_keypair();
    let did_alice = did_key_from_public_key(&pk_alice);
    let (_sk_mallory, pk_mallory) = generate_keypair();
    let did_mallory = did_key_from_public_key(&pk_mallory);

    let row = SampleRow::with_did(&did_alice);
    let mut change = row.signed_change(&sk_alice);
    change.authored_by_did = did_mallory;

    let result = verify_incoming_registry_change(&change, None);
    assert!(
        matches!(result, Err(RegistryVerifyError::SignatureInvalid(_))),
        "{result:?}"
    );
}

#[test]
fn puller_rejects_registry_update_that_changes_authored_by_did() {
    let (sk_alice, pk_alice) = generate_keypair();
    let did_alice = did_key_from_public_key(&pk_alice);
    let row = SampleRow::with_did(&did_alice);
    let existing_change = row.signed_change(&sk_alice);
    let existing = PersistedRegistryRow {
        authored_by_did: existing_change.authored_by_did.clone(),
    };

    let (_sk_bob, pk_bob) = generate_keypair();
    let did_bob = did_key_from_public_key(&pk_bob);
    let mut update = existing_change.clone();
    update.authored_by_did = did_bob;

    let result = verify_incoming_registry_change(&update, Some(&existing));
    assert!(
        matches!(
            result,
            Err(RegistryVerifyError::AuthoredByDidImmutable { .. })
        ),
        "{result:?}"
    );
}

#[test]
fn puller_rejects_registry_row_with_empty_row_sig() {
    // A change with row_sig = "" — a pre-migration-0014 row replayed by a
    // hostile peer, or simply a peer running old code.
    let mut change = valid_change();
    change.row_sig = String::new();
    let result = verify_incoming_registry_change(&change, None);
    assert!(matches!(
        result,
        Err(RegistryVerifyError::RowSigMissingOrEmpty)
    ));
}

#[test]
fn puller_rejects_registry_row_with_malformed_row_sig_base64() {
    let mut change = valid_change();
    change.row_sig = "this is not base64!!!".to_string();
    let result = verify_incoming_registry_change(&change, None);
    assert!(
        matches!(
            result,
            Err(RegistryVerifyError::SignatureInvalid(
                VerifyRegistryRowSigError::MalformedSignatureBytes
            ))
        ),
        "{result:?}"
    );
}

#[test]
fn puller_rejects_registry_row_with_unresolvable_authored_by_did() {
    // authored_by_did isn't a valid did:key format at all.
    let mut change = valid_change();
    change.authored_by_did = "not-a-did-at-all".to_string();
    let result = verify_incoming_registry_change(&change, None);
    assert!(
        matches!(result, Err(RegistryVerifyError::UnknownAuthorDid(_))),
        "{result:?}"
    );
}

#[test]
fn puller_accepts_registry_update_from_same_author() {
    // Alice updates her own row (changes `category`, re-signs with her own
    // key) — must succeed since authorship did not change and the new sig
    // is valid.
    let (sk_alice, pk_alice) = generate_keypair();
    let did_alice = did_key_from_public_key(&pk_alice);
    let existing_change = SampleRow::with_did(&did_alice).signed_change(&sk_alice);
    let existing = PersistedRegistryRow {
        authored_by_did: existing_change.authored_by_did.clone(),
    };

    let mut updated_row = SampleRow::with_did(&did_alice);
    updated_row.category = Some("changed".to_string());
    let update = updated_row.signed_change(&sk_alice);

    let result = verify_incoming_registry_change(&update, Some(&existing));
    assert!(matches!(result, Ok(())), "{result:?}");
}

// -----------------------------------------------------------------------
// B.4 code-quality review gaps (filled as part of B.5's TDD cycle) — M1-M3
// -----------------------------------------------------------------------

/// M1: the signer produced the canonical (sorted-key) `row_pks` encoding,
/// but the wire change carries the SAME logical PKs with keys in a
/// different order. `verify_incoming_registry_change` must canonicalize
/// before rebuilding the payload — otherwise a peer's differently-ordered
/// (but semantically identical) JSON would spuriously fail verification.
#[test]
fn puller_accepts_row_pks_with_unsorted_json_keys() {
    let (sk, pk) = generate_keypair();
    let did = did_key_from_public_key(&pk);
    let mut row = SampleRow::with_did(&did);
    row.row_pks = r#"{"a":1,"b":2}"#.to_string(); // canonical (sorted) form
    let mut change = row.signed_change(&sk);
    change.row_pks = r#"{"b":2,"a":1}"#.to_string(); // peer sends unsorted

    let result = verify_incoming_registry_change(&change, None);
    assert!(matches!(result, Ok(())), "{result:?}");
}

/// M2: `extension_public_key` / `extension_name` are `None` (an infra-owned
/// registry row, not an extension-owned one — the DB enforces both-null-or-
/// both-present via `haex_shared_space_sync_extension_pair`). The optional
/// fields must round-trip through `canonical_encoding`'s presence tag
/// without breaking verification.
#[test]
fn puller_accepts_row_with_none_extension_fields() {
    let (sk, pk) = generate_keypair();
    let did = did_key_from_public_key(&pk);
    let mut row = SampleRow::with_did(&did);
    row.extension_public_key = None;
    row.extension_name = None;
    let change = row.signed_change(&sk);

    let result = verify_incoming_registry_change(&change, None);
    assert!(matches!(result, Ok(())), "{result:?}");
}

/// M3: an UPDATE claims the SAME `authored_by_did` as the persisted row
/// (immutability check passes) but mutates a signed field (`category`)
/// while replaying the OLD `row_sig` — which never covered the new value.
/// Immutability alone is not proof of integrity; the signature check must
/// still catch the tampered content.
#[test]
fn puller_rejects_update_with_matching_did_but_tampered_content() {
    let (sk_alice, pk_alice) = generate_keypair();
    let did_alice = did_key_from_public_key(&pk_alice);
    let row = SampleRow::with_did(&did_alice);
    let existing_change = row.signed_change(&sk_alice); // payload_v1, row_sig covers it
    let existing = PersistedRegistryRow {
        authored_by_did: existing_change.authored_by_did.clone(),
    };

    // Same authored_by_did, same row_sig (payload_v1's), but category is
    // mutated post-signing — as if a peer relabeled the field in transit
    // without re-signing.
    let mut tampered = existing_change.clone();
    tampered.category = Some("changed".to_string());

    let result = verify_incoming_registry_change(&tampered, Some(&existing));
    assert!(
        matches!(result, Err(RegistryVerifyError::SignatureInvalid(_))),
        "{result:?}"
    );
}
