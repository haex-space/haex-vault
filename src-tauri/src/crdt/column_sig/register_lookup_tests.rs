use super::register_lookup::{canonicalize_row_pks, is_register_target_forbidden, RegisterLookup};
use rusqlite::Connection;

/// Minimal in-memory DB with:
///   - `haex_space_members` — an infra table with `space_id`, used as the
///     stand-in for any of the 5 `SPACE_SCOPED_CRDT_TABLES` in the infra path.
///   - `haex_shared_space_sync` — the signed share register.
///   - `haex_identities` — distinguishes a locally-owned identity from a
///     foreign member identity.
///
/// One extension-table row (`ext_calendar` / `{"id":"R"}`) is registered
/// into two spaces (`space_A`, `space_B`). Only the space_A assignment was
/// authored by this vault; the space_B assignment is a relayed foreign share
/// and must not cause this vault to sign the referenced row.
fn seed() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory");
    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY NOT NULL,
            did TEXT NOT NULL,
            private_key TEXT
         );
         CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL
         );
         CREATE TABLE haex_shared_space_sync (
            id TEXT PRIMARY KEY NOT NULL,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            space_id TEXT NOT NULL,
            haex_column_sigs TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .expect("create schema");

    // Infra row: a `haex_space_members` row in space_A. Used by the
    // "infra table returns space_id from row column" test.
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        ["id-own-a", "did:key:own-a", "owned-key"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, NULL)",
        ["id-foreign-b", "did:key:foreign-b"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-1", "space_A", "id-own-a"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-2", "space_B", "id-foreign-b"],
    )
    .unwrap();

    // Register: extension table `ext_calendar` row `{"id":"R"}` is shared
    // into space_A and space_B. Only A's routing columns carry this vault's
    // own DID; B's carry a foreign member DID.
    conn.execute(
        "INSERT INTO haex_shared_space_sync \
         (id, table_name, row_pks, space_id, haex_column_sigs)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            "reg-a",
            "ext_calendar",
            r#"{"id":"R"}"#,
            "space_A",
            r#"{"table_name":{"space_A":{"authorDid":"did:key:own-a"}},"row_pks":{"space_A":{"authorDid":"did:key:own-a"}},"space_id":{"space_A":{"authorDid":"did:key:own-a"}}}"#,
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_shared_space_sync \
         (id, table_name, row_pks, space_id, haex_column_sigs)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            "reg-b",
            "ext_calendar",
            r#"{"id":"R"}"#,
            "space_B",
            r#"{"table_name":{"space_B":{"authorDid":"did:key:foreign-b"}},"row_pks":{"space_B":{"authorDid":"did:key:foreign-b"}},"space_id":{"space_B":{"authorDid":"did:key:foreign-b"}}}"#,
        ],
    )
    .unwrap();

    conn
}

#[test]
fn empty_row_pks_returns_empty_for_extension_table() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    // Extension row that has no register entry → empty vec.
    let spaces = lookup
        .resolve(&conn, "ext_calendar", r#"{"id":"unknown"}"#)
        .expect("resolve");
    assert!(spaces.is_empty());
}

#[test]
fn infra_table_returns_space_id_from_row_column() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    let spaces = lookup
        .resolve(&conn, "haex_space_members", r#"{"id":"mem-1"}"#)
        .expect("resolve");
    // Infra tables carry `space_id` inline — the row itself decides scope.
    assert_eq!(spaces, vec!["space_A".to_string()]);
}

#[test]
fn infra_table_missing_row_returns_empty() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    // Row does not exist in the infra table → no space (rather than a stray
    // DB error). The caller then simply signs into zero spaces.
    let spaces = lookup
        .resolve(&conn, "haex_space_members", r#"{"id":"does-not-exist"}"#)
        .expect("resolve");
    assert!(spaces.is_empty());
}

#[test]
fn extension_table_returns_only_self_authored_register_mappings() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    let mut spaces = lookup
        .resolve(&conn, "ext_calendar", r#"{"id":"R"}"#)
        .expect("resolve");
    spaces.sort();
    assert_eq!(spaces, vec!["space_A".to_string()]);
}

#[test]
fn per_transaction_cache_hits_on_repeated_lookup() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    // First call: miss → hit-counter stays at 0.
    lookup
        .resolve(&conn, "ext_calendar", r#"{"id":"R"}"#)
        .expect("first resolve");
    assert_eq!(lookup.cache_hits(), 0);
    // Second call with the same key: hit → counter bumps.
    let mut spaces = lookup
        .resolve(&conn, "ext_calendar", r#"{"id":"R"}"#)
        .expect("second resolve");
    spaces.sort();
    assert_eq!(spaces, vec!["space_A".to_string()]);
    assert_eq!(lookup.cache_hits(), 1);
}

#[test]
fn cache_is_scoped_by_table_and_row_pks() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    // Two distinct (table, row_pks) pairs must miss the cache independently.
    lookup
        .resolve(&conn, "ext_calendar", r#"{"id":"R"}"#)
        .expect("resolve 1");
    lookup
        .resolve(&conn, "haex_space_members", r#"{"id":"mem-1"}"#)
        .expect("resolve 2");
    // Both were misses.
    assert_eq!(lookup.cache_hits(), 0);
    // Repeat one of them → single hit.
    lookup
        .resolve(&conn, "ext_calendar", r#"{"id":"R"}"#)
        .expect("resolve 3");
    assert_eq!(lookup.cache_hits(), 1);
}

/// F#2 (Round-3 review): a partial PK on an infra table would otherwise
/// silently match a random row via `LIMIT 1`. Enforce full-PK coverage.
#[test]
fn infra_table_pk_mismatch_returns_error() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    // haex_space_members's PK is `id`; supplying `space_id` alone must fail.
    let err = lookup
        .resolve(&conn, "haex_space_members", r#"{"space_id":"space_A"}"#)
        .expect_err("expected PK mismatch error");
    match err {
        rusqlite::Error::InvalidParameterName(msg) => {
            assert!(
                msg.contains("PK mismatch") || msg.contains("has no primary key"),
                "expected PK mismatch message, got: {msg}"
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// F#2 (Round-3 review): extra PK-shaped keys beyond the table's actual
/// PK set are also refused (callers must submit exactly the PK columns).
#[test]
fn infra_table_extra_pk_column_returns_error() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    let err = lookup
        .resolve(
            &conn,
            "haex_space_members",
            r#"{"id":"mem-1","identity_id":"id-own-a"}"#,
        )
        .expect_err("expected PK mismatch error");
    assert!(matches!(err, rusqlite::Error::InvalidParameterName(_)));
}

/// F#3 (Runde-4 review): a legacy or malicious register row targeting a
/// forbidden `haex_*` system table must not cause the extension path to
/// sign for the target space. Return an empty vec — F2 rejects fresh
/// INSERTs of this shape with a hard I1 error at write time, so a read
/// hitting one is legacy garbage the sig layer refuses to trust.
#[test]
fn resolve_extension_row_ignores_forbidden_system_table_targets() {
    let conn = seed();
    // Plant a register row targeting `haex_identities` — this is exactly
    // the shape F2 blocks with I1RegisterTargetsSystemTable, but F1's
    // read path can still be handed such a row from historic writes.
    conn.execute(
        "INSERT INTO haex_shared_space_sync \
         (id, table_name, row_pks, space_id) VALUES (?1, ?2, ?3, ?4)",
        [
            "reg-forbidden",
            "haex_identities",
            r#"{"id":"id-own-a"}"#,
            "space_A",
        ],
    )
    .unwrap();

    let lookup = RegisterLookup::new();
    let spaces = lookup
        .resolve(&conn, "haex_identities", r#"{"id":"id-own-a"}"#)
        .expect("resolve");
    assert!(
        spaces.is_empty(),
        "forbidden system-table target must never produce a space, got: {:?}",
        spaces
    );
}

/// F#3 (Runde-4 review): the same rule applies to `_no_sync` cache tables
/// and other denylist entries — anything the F2 write path refuses must
/// also be silently ignored on the F1 read path.
#[test]
fn resolve_extension_row_ignores_no_sync_suffix_targets() {
    let conn = seed();
    conn.execute(
        "INSERT INTO haex_shared_space_sync \
         (id, table_name, row_pks, space_id) VALUES (?1, ?2, ?3, ?4)",
        [
            "reg-no-sync",
            "haex_workspaces_no_sync",
            r#"{"id":"ws-1"}"#,
            "space_A",
        ],
    )
    .unwrap();

    let lookup = RegisterLookup::new();
    let spaces = lookup
        .resolve(&conn, "haex_workspaces_no_sync", r#"{"id":"ws-1"}"#)
        .expect("resolve");
    assert!(spaces.is_empty());
}

#[test]
fn system_target_policy_is_fail_closed_with_scoped_storage_exception() {
    assert!(!is_register_target_forbidden("haex_s3_backends"));
    assert!(is_register_target_forbidden("haex_future_private_table"));
    assert!(is_register_target_forbidden("sqlite_sequence"));
    assert!(is_register_target_forbidden("ext_cache_no_sync"));
}

#[test]
fn ucan_grants_are_forbidden_register_targets() {
    // Defense-in-depth: even if `haex_space_ucan_grants_no_sync` (Task A.2,
    // local-only UCAN grants store) were somehow written into
    // `haex_shared_space_sync.table_name`, both the `haex_` prefix rule and
    // the `_no_sync` suffix rule must independently reject it as a register
    // target, so the F1/F2 register-driven sync path can never pick it up.
    assert!(is_register_target_forbidden(
        crate::table_names::TABLE_SPACE_UCAN_GRANTS_NO_SYNC
    ));
}

// ---------------------------------------------------------------------------
// `canonicalize_row_pks` — PR #741 finding 3 (CRITICAL)
//
// `persist_shared_backend` (remote_storage/share_command/mod.rs) writes
// `haex_shared_space_sync.row_pks` as a JSON ARRAY (`["<uuid>"]`), not the
// JSON OBJECT shape the CRDT scanner produces for every other table. Both
// shapes must canonicalise without error; only scalars/null are rejected.
// ---------------------------------------------------------------------------

#[test]
fn canonicalize_row_pks_accepts_object() {
    let canonical = canonicalize_row_pks(r#"{"id":"R"}"#).expect("object accepted");
    assert_eq!(canonical, r#"{"id":"R"}"#);
}

#[test]
fn canonicalize_row_pks_object_is_key_sorted() {
    let canonical = canonicalize_row_pks(r#"{"b":"2","a":"1"}"#).expect("object accepted");
    let a_pos = canonical.find("\"a\"").expect("has key a");
    let b_pos = canonical.find("\"b\"").expect("has key b");
    assert!(
        a_pos < b_pos,
        "expected key 'a' before 'b', got: {canonical}"
    );
}

#[test]
fn canonicalize_row_pks_accepts_array() {
    let canonical = canonicalize_row_pks(r#"["a","b"]"#).expect("array accepted");
    assert_eq!(canonical, r#"["a","b"]"#);
}

#[test]
fn canonicalize_row_pks_array_preserves_order() {
    // Arrays are positional, not sorted — unlike object keys.
    let canonical = canonicalize_row_pks(r#"["b","a"]"#).expect("array accepted");
    assert_eq!(canonical, r#"["b","a"]"#);
}

#[test]
fn canonicalize_row_pks_rejects_scalar_string() {
    canonicalize_row_pks(r#""foo""#).expect_err("bare string scalar must be rejected");
}

#[test]
fn canonicalize_row_pks_rejects_scalar_number() {
    canonicalize_row_pks("42").expect_err("bare number scalar must be rejected");
}

#[test]
fn canonicalize_row_pks_rejects_null() {
    canonicalize_row_pks("null").expect_err("JSON null must be rejected");
}

#[test]
fn canonicalize_row_pks_rejects_invalid_json() {
    canonicalize_row_pks("not json").expect_err("invalid JSON must be rejected");
}
