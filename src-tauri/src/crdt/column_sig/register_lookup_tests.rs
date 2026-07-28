use super::register_lookup::RegisterLookup;
use rusqlite::Connection;

/// Minimal in-memory DB with:
///   - `haex_space_members` — an infra table with `space_id`, used as the
///     stand-in for any of the 5 `SPACE_SCOPED_CRDT_TABLES` in the infra path.
///   - `haex_shared_space_sync` — the share register (no author column any
///     more: Runde 5 dropped `authored_by_did`; I2 lives at the caller now).
///
/// One extension-table row (`ext_calendar` / `{"id":"R"}`) is registered
/// into two spaces (`space_A`, `space_B`). Author identity is no longer
/// tracked on the register row itself — the caller (write-time signer)
/// intersects with the `SpaceKeyCache` to decide which of these spaces it
/// may legitimately sign for.
fn seed() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory");
    conn.execute_batch(
        "CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL
         );
         CREATE TABLE haex_shared_space_sync (
            id TEXT PRIMARY KEY NOT NULL,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            space_id TEXT NOT NULL
         );",
    )
    .expect("create schema");

    // Infra row: a `haex_space_members` row in space_A. Used by the
    // "infra table returns space_id from row column" test.
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-1", "space_A", "id-own-a"],
    )
    .unwrap();

    // Register: extension table `ext_calendar` row `{"id":"R"}` is shared
    // into space_A and space_B. Resolve returns BOTH — the caller's
    // key-cache filter decides which are actually ours to sign for.
    conn.execute(
        "INSERT INTO haex_shared_space_sync \
         (id, table_name, row_pks, space_id) VALUES (?1, ?2, ?3, ?4)",
        ["reg-a", "ext_calendar", r#"{"id":"R"}"#, "space_A"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_shared_space_sync \
         (id, table_name, row_pks, space_id) VALUES (?1, ?2, ?3, ?4)",
        ["reg-b", "ext_calendar", r#"{"id":"R"}"#, "space_B"],
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
fn extension_table_returns_every_space_the_register_maps_to() {
    let conn = seed();
    let lookup = RegisterLookup::new();
    let mut spaces = lookup
        .resolve(&conn, "ext_calendar", r#"{"id":"R"}"#)
        .expect("resolve");
    spaces.sort();
    // Runde 5: RegisterLookup is DID/key-agnostic — both register entries
    // survive. The caller (write-time signer) intersects with the
    // SpaceKeyCache to pick the spaces this vault can sign for.
    assert_eq!(spaces, vec!["space_A".to_string(), "space_B".to_string()]);
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
    assert_eq!(spaces, vec!["space_A".to_string(), "space_B".to_string()]);
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
