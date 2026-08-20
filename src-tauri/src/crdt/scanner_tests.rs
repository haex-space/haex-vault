use super::*;
use rusqlite::Connection;

/// Test-only helper: unscoped single-table scan. Production code must use
/// `scan_table_for_local_changes_scoped` (or the space-scoped whitelist
/// entry point `scan_space_scoped_tables_for_local_changes`) — an unscoped
/// scan over a table shared by multiple spaces leaks cross-space rows.
pub fn scan_table_for_local_changes(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    scan_table_for_local_changes_scoped(conn, table_name, after_hlc, device_id, None, None)
}

/// Helper: create an in-memory DB with a CRDT-enabled table and return the connection.
fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE test_items (
                id TEXT PRIMARY KEY,
                name TEXT,
                value INTEGER,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

fn insert_row(conn: &Connection, id: &str, name: &str, value: i64, hlc: &str) {
    let hlcs = format!("{{\"name\":\"{hlc}\",\"value\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, name, value, hlc, hlcs],
    )
    .unwrap();
}

#[test]
fn test_scan_empty_table_returns_no_changes() {
    let conn = setup_test_db();
    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();
    assert!(changes.is_empty());
}

#[test]
fn test_scan_full_returns_all_columns() {
    let conn = setup_test_db();
    insert_row(
        &conn,
        "row-1",
        "hello",
        42,
        "2025-01-01T00:00:00.000Z-0001-device1",
    );

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    // 2 data columns: name, value
    assert_eq!(changes.len(), 2);

    let names: Vec<&str> = changes.iter().map(|c| c.column_name.as_str()).collect();
    assert!(names.contains(&"name"));
    assert!(names.contains(&"value"));

    // Verify PK JSON
    for change in &changes {
        assert_eq!(change.table_name, "test_items");
        assert_eq!(change.device_id, "device-1");
        let pks: serde_json::Map<String, JsonValue> =
            serde_json::from_str(&change.row_pks).unwrap();
        assert_eq!(pks.get("id").unwrap(), "row-1");
    }
}

#[test]
fn test_scan_with_after_hlc_filters_old_rows() {
    let conn = setup_test_db();
    insert_row(&conn, "old", "old", 1, "1000000000000000000/aabbccdd");
    insert_row(&conn, "new", "new", 2, "3000000000000000000/aabbccdd");

    let changes = scan_table_for_local_changes(
        &conn,
        "test_items",
        Some("2000000000000000000/aabbccdd"),
        "device-1",
    )
    .unwrap();

    // Only the "new" row should be present (2 data columns: name, value)
    assert_eq!(changes.len(), 2);
    for change in &changes {
        let pks: serde_json::Map<String, JsonValue> =
            serde_json::from_str(&change.row_pks).unwrap();
        assert_eq!(pks.get("id").unwrap(), "new");
    }
}

#[test]
fn test_scan_excludes_metadata_columns() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE with_meta (
                id TEXT PRIMARY KEY,
                data TEXT,
                last_push_hlc_timestamp TEXT,
                last_pull_server_timestamp TEXT,
                updated_at TEXT,
                created_at TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();

    conn.execute(
        "INSERT INTO with_meta (id, data, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'test', '2025-01-01T00:00:00.000Z-0001-d1',
                     '{\"data\":\"2025-01-01T00:00:00.000Z-0001-d1\"}')",
        [],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "with_meta", None, "device-1").unwrap();

    let col_names: Vec<&str> = changes.iter().map(|c| c.column_name.as_str()).collect();
    // Only "data" should remain; all metadata/CRDT columns filtered out
    assert!(col_names.contains(&"data"));
    assert!(!col_names.contains(&"last_push_hlc_timestamp"));
    assert!(!col_names.contains(&"last_pull_server_timestamp"));
    assert!(!col_names.contains(&"updated_at"));
    assert!(!col_names.contains(&"created_at"));
    assert!(!col_names.contains(&"haex_hlc"));
    assert!(!col_names.contains(&"haex_column_hlcs"));
}

#[test]
fn test_scan_uses_row_hlc_as_fallback() {
    let conn = setup_test_db();
    // Insert a row where haex_column_hlcs is empty — row-level HLC should be used
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'test', 10, '2025-01-01T00:00:00.000Z-0001-d1', '{}')",
        [],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    // Both data columns should be emitted using the row-level HLC
    assert_eq!(changes.len(), 2);
    for change in &changes {
        assert_eq!(change.hlc_timestamp, "2025-01-01T00:00:00.000Z-0001-d1");
    }
}

#[test]
fn test_scan_empty_column_hlc_falls_back_to_row_hlc() {
    // Regression: a corrupt/legacy row can carry an empty-string per-column
    // HLC. It must be treated as absent (fall back to the row HLC), never
    // emitted as `hlc_timestamp = ""`. An empty HLC would feed
    // `compare_hlc_strings("")` on every apply (the `[HLC] cannot parse time
    // component of ""` flood) and could never converge (`"" > x` is false).
    let conn = setup_test_db();
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'test', 10, '2025-01-01T00:00:00.000Z-0001-d1', '{\"name\":\"\",\"value\":\"\"}')",
        [],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    assert_eq!(changes.len(), 2);
    for change in &changes {
        assert_eq!(
            change.hlc_timestamp, "2025-01-01T00:00:00.000Z-0001-d1",
            "empty per-column HLC must fall back to the row HLC, never stay \"\""
        );
        assert!(!change.hlc_timestamp.is_empty());
    }
}

#[test]
fn test_scan_skips_row_when_all_hlcs_empty() {
    // Regression: when BOTH the per-column HLC and the row HLC are empty the
    // column has no usable timestamp and must be skipped. Emitting `""` is what
    // produced the empty-HLC log flood and a row that never synced.
    let conn = setup_test_db();
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'test', 10, '', '{\"name\":\"\",\"value\":\"\"}')",
        [],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    assert!(
        changes.is_empty(),
        "rows with no usable HLC must not emit empty-string timestamps"
    );
}

#[test]
fn test_incremental_scan_admits_empty_row_hlc_with_valid_column_hlc() {
    // Regression: an incremental scan must not drop a corrupt/legacy row whose
    // row-level HLC is empty (`haex_hlc = ''`) but which still carries a valid,
    // newer per-column HLC. The SQL prefilter (`"haex_hlc" > after_hlc`) would
    // otherwise reject such a row before the per-column fallback could emit the
    // valid change, so the column would only ever converge on a full scan.
    let conn = setup_test_db();
    // Empty row HLC, but `name` has a per-column HLC newer than the cursor while
    // `value` stays at the old one.
    let hlcs = r#"{"name":"3000000000000000000/aabbccdd","value":"1000000000000000000/aabbccdd"}"#;
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'updated', 10, '', ?1)",
        [hlcs],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(
        &conn,
        "test_items",
        Some("2000000000000000000/aabbccdd"),
        "device-1",
    )
    .unwrap();

    // Only `name` passes the per-column threshold, and it is emitted despite the
    // empty row HLC.
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].column_name, "name");
    assert_eq!(changes[0].hlc_timestamp, "3000000000000000000/aabbccdd");
}

#[test]
fn test_column_level_hlc_filtering() {
    let conn = setup_test_db();
    // Insert a row where 'name' has a newer HLC but 'value' has an older one
    let hlcs = r#"{"name":"3000000000000000000/aabbccdd","value":"1000000000000000000/aabbccdd"}"#;
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'updated', 10, '3000000000000000000/aabbccdd', ?1)",
        [hlcs],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(
        &conn,
        "test_items",
        Some("2000000000000000000/aabbccdd"),
        "device-1",
    )
    .unwrap();

    // Only 'name' should pass the per-column HLC filter
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].column_name, "name");
}

#[test]
fn test_scan_composite_pk() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE composite_pk (
                group_id TEXT NOT NULL,
                item_id TEXT NOT NULL,
                data TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                PRIMARY KEY (group_id, item_id)
            );",
    )
    .unwrap();

    let hlcs = r#"{"data":"2025-01-01T00:00:00.000Z-0001-d1"}"#;
    conn.execute(
        "INSERT INTO composite_pk (group_id, item_id, data, haex_hlc, haex_column_hlcs)
             VALUES ('g1', 'i1', 'hello', '2025-01-01T00:00:00.000Z-0001-d1', ?1)",
        [hlcs],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "composite_pk", None, "device-1").unwrap();

    assert_eq!(changes.len(), 1); // data only

    let pks: serde_json::Map<String, JsonValue> =
        serde_json::from_str(&changes[0].row_pks).unwrap();
    assert_eq!(pks.get("group_id").unwrap(), "g1");
    assert_eq!(pks.get("item_id").unwrap(), "i1");
}

#[test]
fn test_scan_null_value() {
    let conn = setup_test_db();
    let hlcs =
        r#"{"name":"2025-01-01T00:00:00.000Z-0001-d1","value":"2025-01-01T00:00:00.000Z-0001-d1"}"#;
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', NULL, NULL, '2025-01-01T00:00:00.000Z-0001-d1', ?1)",
        [hlcs],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    // NULL values should still produce changes for both data columns
    assert_eq!(changes.len(), 2);
    let name_change = changes.iter().find(|c| c.column_name == "name").unwrap();
    assert_eq!(name_change.value, JsonValue::Null);
}

#[test]
fn test_scan_nonexistent_table_returns_empty() {
    let conn = Connection::open_in_memory().unwrap();
    let changes = scan_table_for_local_changes(&conn, "nonexistent", None, "device-1").unwrap();
    assert!(changes.is_empty());
}

#[test]
fn test_is_space_scoped_table_whitelist() {
    for t in SPACE_SCOPED_CRDT_TABLES {
        assert!(
            is_space_scoped_table(t),
            "whitelist member not recognised: {t}"
        );
    }
    // Private per-vault tables must NOT be space-scoped.
    assert!(!is_space_scoped_table("haex_identities"));
    assert!(!is_space_scoped_table("haex_ucan_tokens"));
    assert!(!is_space_scoped_table("haex_vault_settings"));
    assert!(!is_space_scoped_table("haex_sync_backends"));
    // Extension / unknown tables default to private.
    assert!(!is_space_scoped_table("some_extension_table"));
}

/// Doppel-Buchführungs-Test (ADR 0003).
///
/// **Sicherheits-Kontext:** `SPACE_SCOPED_CRDT_TABLES` (siehe
/// `crdt/scanner.rs:37-55`) entscheidet, welche `haex_*`-Tabelle Inhalt an
/// dritte Space-Member kreuzen darf. Ein versehentlich hinzugefügter Eintrag
/// verletzt Invariante I8 (Sync Safety) und I12 (Vault-Scoped-Table
/// Confinement) in `docs/security/invariants.md` — vault-privater Inhalt
/// flösse an dritte Peers.
///
/// Der Test spiegelt die Runtime-Konstante gegen eine im Test explizit
/// aufgeführte Erwartungs-Liste **mit Begründungs-Kommentar pro Eintrag**.
/// Wenn eine intendierte Änderung an der Runtime-Konstante nötig ist, muss
/// hier gleichzeitig die Erwartungs-Liste angepasst UND der Kommentar zur
/// Sicherheits-Rationale ergänzt werden. Diff auf beiden Seiten ist damit
/// review-sichtbar und Reviewer-erzwungen bewusst.
///
/// **Nicht** ersetzen durch `use super::SPACE_SCOPED_CRDT_TABLES` — die
/// wörtliche Duplikation IST der Enforcement-Mechanismus.
#[test]
fn space_scoped_crdt_tables_matches_documented_expectation() {
    // Wörtlich aufgezählte Erwartung — jede Änderung erfordert bewussten Edit
    // hier UND in `crdt/scanner.rs`. Reihenfolge muss deterministisch sein
    // (Slice-Vergleich, nicht Set-Vergleich), damit die Fehlermeldung bei
    // Diff präzise anzeigt, welcher Eintrag wo eingefügt oder entfernt wurde.
    const EXPECTED: &[&str] = &[
        // 5 Infrastruktur-Tabellen (ADR 0002 §0 Terminologie, Bootstrap-Klasse).
        "haex_space_devices", // Device-Registrierung eines Members im Space.
        "haex_space_members", // Membership-Roster.
        "haex_peer_shares",   // Vom Device angebotene Share-Endpunkte.
        "haex_mls_sync_keys", // MLS KeyPackages (damit andere an uns encrypten).
        "haex_device_mls_enrollments", // MLS-Enrollment-Artefakte.
        // Share-Register selbst (ADR 0002 §0, 6. Bootstrap-Tabelle).
        "haex_shared_space_sync", // Unshare = Register-DELETE; Members brauchen das Signal.
        // Delete-Propagation (ADR 0002 §6.5, Phase 3.a).
        "haex_shared_space_deleted_rows", // Per-Space Delete-Log (Hard-Delete + Unshare).
        "haex_space_compaction_anchors",  // Anti-Resurrection-Anchor.
    ];

    let actual: &[&str] = SPACE_SCOPED_CRDT_TABLES;
    assert_eq!(
        actual, EXPECTED,
        "\n\nSPACE_SCOPED_CRDT_TABLES ist in scanner.rs geändert worden, aber die \
         im Test dokumentierte Erwartungs-Liste nicht.\n\n\
         Wenn die Änderung beabsichtigt ist:\n\
         1. Passe EXPECTED in `space_scoped_crdt_tables_matches_documented_expectation` an,\n\
         2. Ergänze pro Eintrag einen Begründungs-Kommentar mit Sicherheits-Rationale,\n\
         3. Referenziere die ADR (0003 oder Folge-ADR), die den neuen Eintrag rechtfertigt.\n\n\
         Wenn die Änderung unbeabsichtigt ist: reverte den Edit in `crdt/scanner.rs`.\n\
         Hintergrund: docs/adr/0003-explicit-sync-policy.md § Entscheidung Punkt 3.\n\n"
    );
}

/// Doppel-Buchführungs-Test (ADR 0003) für `MEMBERSHIP_SYSTEM_TABLES`.
///
/// **Sicherheits-Kontext:** `MEMBERSHIP_SYSTEM_TABLES` (`crdt/scanner.rs:73-78`)
/// ist die Untermenge von `SPACE_SCOPED_CRDT_TABLES`, die auch ein
/// **read-only Member** pushen darf, weil die Zeilen seine eigene Existenz
/// im Space beschreiben. Ein versehentlich hinzugefügter Eintrag würde einem
/// read-only Member erlauben, User-Content zu publizieren — Bruch der
/// Read-Only-Semantik.
///
/// Wie bei `space_scoped_crdt_tables_matches_documented_expectation`:
/// wörtliche Duplikation ist der Enforcement-Mechanismus, kein `use`-Import.
#[test]
fn membership_system_tables_matches_documented_expectation() {
    const EXPECTED: &[&str] = &[
        // Membership-Roster + eigene Device-Registrierung sind Read-only-erlaubt.
        "haex_space_devices",
        "haex_space_members",
        // MLS KeyPackages + Enrollment: read-only Member muss diese pushen,
        // damit andere ihm encrypten und ihn in die MLS-Gruppe committen können.
        "haex_mls_sync_keys",
        "haex_device_mls_enrollments",
        // haex_peer_shares fehlt bewusst: read-only Member darf keine
        // Share-Endpunkte publizieren (echter User-Content).
        // haex_shared_space_sync / _deleted_rows / _compaction_anchors:
        // Register + Delete-Log = Write-Aktionen, kein Read-only-Recht.
    ];

    let actual: &[&str] = MEMBERSHIP_SYSTEM_TABLES;
    assert_eq!(
        actual, EXPECTED,
        "\n\nMEMBERSHIP_SYSTEM_TABLES ist in scanner.rs geändert worden, aber die \
         im Test dokumentierte Erwartungs-Liste nicht.\n\n\
         Sicherheits-Konsequenz einer stillen Änderung: ein read-only Member \
         könnte die neu-hinzugefügte Tabelle beschreiben und damit die \
         Read-Only-Semantik brechen.\n\n\
         Wenn beabsichtigt: EXPECTED anpassen + Begründungs-Kommentar + \
         ADR-Referenz. Wenn nicht: reverte in `crdt/scanner.rs`.\n\
         Hintergrund: docs/adr/0003-explicit-sync-policy.md § Entscheidung Punkt 3.\n\n"
    );
}

/// Schema-Präsenz-Check (ADR 0003).
///
/// Jeder Eintrag in `SPACE_SCOPED_CRDT_TABLES` und `MEMBERSHIP_SYSTEM_TABLES`
/// muss einer generierten Table-Name-Konstante in `crate::table_names`
/// entsprechen. Fängt zwei Drift-Klassen:
/// 1. Whitelist zeigt auf eine Tabelle, die im Schema-Generator umbenannt oder
///    entfernt wurde → still-defekter Sync-Path, statt lauter Test-Fail.
/// 2. Ein neuer Whitelist-Eintrag wurde als String-Literal statt via Konstante
///    hinzugefügt → hilft, den Table-Name-Konstanten-Chokepoint zu erzwingen.
#[test]
fn whitelisted_tables_exist_as_generated_constants() {
    use crate::table_names::*;

    // Alle im Schema-Generator registrierten Table-Name-Konstanten für die
    // beiden Whitelists. Wenn hier ein Eintrag fehlt, weil er unter anderem
    // Konstanten-Namen generiert wurde: passe im Test die verwendeten Namen
    // an, ergänze keine neue Konstante ad-hoc — siehe
    // `src-tauri/generator/table_names.rs`.
    let known: &[&str] = &[
        TABLE_SPACE_DEVICES,
        TABLE_SPACE_MEMBERS,
        TABLE_PEER_SHARES,
        TABLE_MLS_SYNC_KEYS,
        TABLE_DEVICE_MLS_ENROLLMENTS,
        TABLE_SHARED_SPACE_SYNC,
        TABLE_SHARED_SPACE_DELETED_ROWS,
        TABLE_SPACE_COMPACTION_ANCHORS,
    ];

    for t in SPACE_SCOPED_CRDT_TABLES {
        assert!(
            known.contains(t),
            "SPACE_SCOPED_CRDT_TABLES enthält {t:?}, aber diese Tabelle ist in \
             `crate::table_names` nicht als Konstante registriert. Entweder wurde \
             die Tabelle im Schema-Generator umbenannt/entfernt, oder der \
             Whitelist-Eintrag wurde als String-Literal hinzugefügt statt via \
             Konstante. Prüfe `src-tauri/generator/table_names.rs`."
        );
    }
    for t in MEMBERSHIP_SYSTEM_TABLES {
        assert!(
            known.contains(t),
            "MEMBERSHIP_SYSTEM_TABLES enthält {t:?} — selbe Diagnose wie oben."
        );
    }
}

/// Schema-Präsenz-Check Teil (b) (ADR 0003 § Entscheidung Punkt 3).
///
/// Teil (a) (`whitelisted_tables_exist_as_generated_constants`) prüft die
/// Snapshot-Strings gegen `crate::table_names`. Diese Konstanten werden aus
/// `src/database/tableNames.json` generiert (`src-tauri/generator/table_names.rs`)
/// — einer **handgepflegten** Registry, NICHT aus den Migrationen. Beide Quellen
/// können also auseinanderlaufen: eine Migration darf eine Tabelle droppen oder
/// umbenennen, während der `tableNames.json`-Eintrag stehen bleibt. Genau das
/// ist die "still-defekter Sync-Path"-Drift-Klasse, die (b) fangen soll.
///
/// Deshalb wird hier das echte Migrations-Schema aufgebaut: Journal lesen, alle
/// Drizzle-Migrationen in Journal-Reihenfolge auf eine In-Memory-DB anwenden
/// (Split auf `--> statement-breakpoint`, wie der Produktions-Runner), dann pro
/// Whitelist-Eintrag `get_table_schema` abfragen.
///
/// Die manuellen Migrationen (`database/migrations-manual`) werden bewusst
/// **nicht** angewendet: sie enthalten nur Trigger, legen keine Tabellen an, und
/// referenzieren die CRDT-Meta-Spalten, die der Produktions-Runner via
/// `CrdtTransformer` injiziert — dieser rohe Replay tut das nicht.
#[test]
fn whitelisted_tables_exist_in_the_migration_schema() {
    use crate::crdt::trigger::get_table_schema;
    use std::path::PathBuf;

    let mig_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("database/migrations");
    let journal_raw = std::fs::read_to_string(mig_dir.join("meta/_journal.json"))
        .expect("drizzle migration journal must exist");
    let journal: serde_json::Value =
        serde_json::from_str(&journal_raw).expect("migration journal must be valid JSON");
    let mut entries: Vec<(u64, String)> = journal["entries"]
        .as_array()
        .expect("journal.entries must be an array")
        .iter()
        .map(|e| {
            (
                e["idx"].as_u64().expect("journal entry needs idx"),
                e["tag"]
                    .as_str()
                    .expect("journal entry needs tag")
                    .to_string(),
            )
        })
        .collect();
    entries.sort_by_key(|(idx, _)| *idx);
    assert!(
        !entries.is_empty(),
        "migration journal must list at least one migration"
    );

    let conn = Connection::open_in_memory().unwrap();
    for (_, tag) in &entries {
        let sql = std::fs::read_to_string(mig_dir.join(format!("{tag}.sql")))
            .unwrap_or_else(|e| panic!("migration {tag} listed in the journal is unreadable: {e}"));
        for stmt in sql.split("--> statement-breakpoint") {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            conn.execute_batch(stmt)
                .unwrap_or_else(|e| panic!("migration {tag} failed to apply: {e}\nSQL:\n{stmt}"));
        }
    }

    for table in SPACE_SCOPED_CRDT_TABLES
        .iter()
        .chain(MEMBERSHIP_SYSTEM_TABLES)
    {
        let columns = get_table_schema(&conn, table)
            .unwrap_or_else(|e| panic!("PRAGMA table_info failed for {table}: {e}"));
        assert!(
            !columns.is_empty(),
            "Sync-Whitelist enthält {table:?}, aber nach dem vollständigen \
             Migrations-Replay existiert diese Tabelle nicht. Entweder hat eine \
             Migration sie gedroppt/umbenannt, ohne die Whitelist in \
             `crdt/scanner.rs` nachzuziehen (still-defekter Sync-Path), oder der \
             Whitelist-Eintrag ist ein Tippfehler. Hintergrund: \
             docs/adr/0003-explicit-sync-policy.md § Entscheidung Punkt 3 (b)."
        );
    }
}

#[test]
fn test_membership_system_tables_are_subset_of_space_scoped() {
    for t in MEMBERSHIP_SYSTEM_TABLES {
        assert!(
            is_space_scoped_table(t),
            "membership-system table not in sync whitelist: {t}"
        );
        assert!(
            is_membership_system_table(t),
            "membership-system table not recognised by helper: {t}"
        );
    }
    // peer_shares must NOT be in the membership-system set: it is
    // user-authored content (a device declaring it hosts a folder),
    // and a read-only member must not be able to push entries here.
    assert!(!is_membership_system_table("haex_peer_shares"));
    // Off-whitelist tables are obviously not membership-system either.
    assert!(!is_membership_system_table("haex_identities"));
    assert!(!is_membership_system_table("some_extension_table"));
}

/// Creates a CRDT table that carries a `space_id` discriminator, used to
/// exercise the scoped-filter path.
fn setup_scoped_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE scoped_items (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                data TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

fn insert_scoped_row(conn: &Connection, id: &str, space_id: &str, data: &str, hlc: &str) {
    let hlcs = format!("{{\"space_id\":\"{hlc}\",\"data\":\"{hlc}\"}}");
    let sigs = serde_json::json!({
        "space_id": {
            (space_id): {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        },
        "data": {
            (space_id): {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        },
    })
    .to_string();
    conn.execute(
        "INSERT INTO scoped_items
             (id, space_id, data, haex_hlc, haex_column_hlcs, haex_column_sigs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, space_id, data, hlc, hlcs, sigs],
    )
    .unwrap();
}

#[test]
fn test_scoped_filter_returns_only_matching_space() {
    let conn = setup_scoped_test_db();
    insert_scoped_row(
        &conn,
        "r1",
        "space-A",
        "hello",
        "2025-01-01T00:00:00.000Z-0001-d1",
    );
    insert_scoped_row(
        &conn,
        "r2",
        "space-A",
        "world",
        "2025-01-01T00:00:00.000Z-0002-d1",
    );
    insert_scoped_row(
        &conn,
        "r3",
        "space-B",
        "leak",
        "2025-01-01T00:00:00.000Z-0003-d1",
    );

    let changes = scan_table_for_local_changes_scoped(
        &conn,
        "scoped_items",
        None,
        "device-1",
        Some("space-A"),
        None,
    )
    .unwrap();

    // 2 matching rows × 2 data columns (space_id, data) = 4 changes.
    assert_eq!(changes.len(), 4);
    assert!(changes.iter().all(|change| change.sig.is_some()));
    assert!(
        changes
            .iter()
            .all(|change| change.column_name != "haex_column_sigs"),
        "signature metadata must never be emitted as user data"
    );

    // No row from space-B may appear — this is the leak gate.
    for change in &changes {
        let pks: serde_json::Map<String, JsonValue> =
            serde_json::from_str(&change.row_pks).unwrap();
        let id = pks.get("id").and_then(|v| v.as_str()).unwrap();
        assert!(
            id == "r1" || id == "r2",
            "leaked row from other space: {id}"
        );
    }
}

/// Creates a "vault-private-like" CRDT table that is NOT in
/// [`SPACE_SCOPED_CRDT_TABLES`] and carries no `space_id` column — the
/// shape of a per-vault private table (e.g. passwords). Used to prove the
/// owner scanner ships such tables, which a space-scoped scan never would.
fn setup_vault_private_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE haex_passwords (
                id TEXT PRIMARY KEY,
                secret TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

fn insert_private_row(conn: &Connection, id: &str, secret: &str, hlc: &str) {
    let hlcs = format!("{{\"secret\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO haex_passwords (id, secret, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, secret, hlc, hlcs],
    )
    .unwrap();
}

#[test]
fn scan_all_crdt_tables_for_owner_includes_vault_private_and_space_tables() {
    // One vault-private table (no space_id, off the space whitelist) and
    // one space-scoped-like table sharing the same connection.
    let conn = setup_vault_private_test_db();
    conn.execute_batch(
        "CREATE TABLE scoped_items (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                data TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();

    // Distinct HLC timestamps so we can assert global ordering. Use known
    // monotonically increasing logical-clock values (no secret literals).
    let secret_a: u64 = rand::random();
    let secret_b: u64 = rand::random();
    insert_private_row(
        &conn,
        "p1",
        &format!("v{secret_a}"),
        "1000000000000000000/aabbccdd",
    );
    insert_private_row(
        &conn,
        "p2",
        &format!("v{secret_b}"),
        "3000000000000000000/aabbccdd",
    );
    insert_scoped_row(
        &conn,
        "s1",
        "space-A",
        "hello",
        "2000000000000000000/aabbccdd",
    );

    let table_names = vec!["haex_passwords".to_string(), "scoped_items".to_string()];
    let changes =
        scan_all_crdt_tables_for_owner(&conn, &table_names, None, "device-1", None).unwrap();

    // Rows from BOTH tables must appear — proving no space filter is
    // applied. The vault-private table is the leak-relevant one: a
    // space-scoped scan would never return it.
    let tables: std::collections::HashSet<&str> =
        changes.iter().map(|c| c.table_name.as_str()).collect();
    assert!(
        tables.contains("haex_passwords"),
        "owner scan dropped vault-private table"
    );
    assert!(
        tables.contains("scoped_items"),
        "owner scan dropped space-scoped table"
    );

    // Result must be globally HLC-ordered (non-decreasing), mirroring the
    // sibling fn's global sort.
    for pair in changes.windows(2) {
        assert_ne!(
            crate::crdt::hlc::compare_hlc_strings(&pair[0].hlc_timestamp, &pair[1].hlc_timestamp,),
            std::cmp::Ordering::Greater,
            "owner scan result is not globally HLC-ordered"
        );
    }
}

#[test]
fn scan_all_crdt_tables_for_owner_empty_table_list_returns_empty() {
    let conn = setup_vault_private_test_db();
    insert_private_row(&conn, "p1", "x", "1000000000000000000/aabbccdd");

    let changes = scan_all_crdt_tables_for_owner(&conn, &[], None, "device-1", None).unwrap();
    assert!(changes.is_empty());
}

#[test]
fn test_scoped_filter_on_table_without_space_id_returns_empty() {
    // `test_items` (from setup_test_db) has no space_id column. A scoped
    // filter on such a table must return zero rows rather than the whole
    // table, otherwise vault-private CRDT tables would leak through any
    // peer SyncPull that misconfigures its filter.
    let conn = setup_test_db();
    insert_row(&conn, "r1", "hello", 42, "2025-01-01T00:00:00.000Z-0001-d1");

    let changes = scan_table_for_local_changes_scoped(
        &conn,
        "test_items",
        None,
        "device-1",
        Some("any-space"),
        None,
    )
    .unwrap();

    assert!(changes.is_empty());
}

#[test]
fn scan_single_column_for_owner_returns_only_requested_column() {
    // `test_items` has two data columns: `name` and `value`. Scanning for
    // `name` must never return a `value` change, and vice versa.
    let conn = setup_test_db();
    insert_row(&conn, "r1", "hello", 42, "1000000000000000000/aabbccdd");
    insert_row(&conn, "r2", "world", 99, "2000000000000000000/aabbccdd");

    let changes = scan_single_column_for_owner(&conn, "test_items", "name", "device-1").unwrap();

    // Two rows, one `name` change each — and nothing for `value`.
    assert_eq!(changes.len(), 2);
    assert!(changes.iter().all(|c| c.column_name == "name"));
    assert!(
        changes.iter().all(|c| c.table_name == "test_items"),
        "table name must be carried through"
    );
}

#[test]
fn scan_single_column_for_owner_full_dump_ignores_hlc_threshold() {
    // Recovery has no cursor: every row's value for the column must come back,
    // even rows whose HLC would be "old" relative to any threshold. There is
    // no `after_hlc` parameter, so all rows are returned regardless of age.
    let conn = setup_test_db();
    insert_row(&conn, "ancient", "a", 1, "1000000000000000000/aabbccdd");
    insert_row(&conn, "recent", "b", 2, "9000000000000000000/aabbccdd");

    let changes = scan_single_column_for_owner(&conn, "test_items", "value", "device-1").unwrap();

    // Both rows present — the "ancient" one is NOT filtered out.
    assert_eq!(changes.len(), 2);
    let pks: std::collections::HashSet<String> =
        changes.iter().map(|c| c.row_pks.clone()).collect();
    assert!(
        pks.contains("{\"id\":\"ancient\"}"),
        "full dump must include the old row"
    );
    assert!(
        pks.contains("{\"id\":\"recent\"}"),
        "full dump must include the new row"
    );
}

#[test]
fn scan_single_column_for_owner_does_not_origin_filter() {
    // Rows authored by OTHER devices carry a different node-id in the HLC
    // suffix. Recovery wants the COMPLETE column state, so those rows must
    // still be returned — the opposite of the origin-filtered push path.
    let conn = setup_test_db();
    // Two distinct HLC node-id suffixes => two distinct authoring nodes.
    insert_row(&conn, "mine", "x", 1, "1000000000000000000/aabbccdd");
    insert_row(&conn, "theirs", "y", 2, "2000000000000000000/11223344");

    let changes = scan_single_column_for_owner(&conn, "test_items", "name", "device-1").unwrap();

    // Sanity: the two rows genuinely carry different node-ids.
    let mine = crate::crdt::hlc::parse_hlc_node_hex("aabbccdd").unwrap();
    let theirs = crate::crdt::hlc::parse_hlc_node_hex("11223344").unwrap();
    assert_ne!(mine, theirs);

    // Both rows returned despite differing authoring nodes => no origin filter.
    assert_eq!(changes.len(), 2);
    let suffixes: std::collections::HashSet<Option<&str>> = changes
        .iter()
        .map(|c| crate::crdt::hlc::hlc_node_id_suffix(&c.hlc_timestamp))
        .collect();
    assert!(suffixes.contains(&Some("aabbccdd")));
    assert!(suffixes.contains(&Some("11223344")));
}

#[test]
fn space_scoped_crdt_tables_includes_register_delete_log_and_anchor() {
    // Guard: the three new shared-space infrastructure tables must be in the
    // Rust P2P whitelist so scan_space_scoped_tables_for_local_changes ships
    // them across a space-delivery stream. If any is missing, deletes will
    // still write local delete-log rows but never reach other members.
    assert!(
        SPACE_SCOPED_CRDT_TABLES.contains(&"haex_shared_space_sync"),
        "register itself must sync so unshare (register-DELETE without row-DELETE) reaches peers"
    );
    assert!(
        SPACE_SCOPED_CRDT_TABLES.contains(&"haex_shared_space_deleted_rows"),
        "per-space delete-log must sync so hard-delete + unshare propagate"
    );
    assert!(
        SPACE_SCOPED_CRDT_TABLES.contains(&"haex_space_compaction_anchors"),
        "anti-resurrection anchor must sync so a peer's push below anchor is rejectable"
    );
}

#[test]
fn ucan_grants_not_in_space_scoped_crdt_tables() {
    // Guard: `haex_space_ucan_grants_no_sync` is a deliberately local-only
    // UCAN grants store (Task A.2). If it were ever added to the P2P
    // whitelist below, it would start CRDT-syncing across space members,
    // defeating the point of keeping delegation bookkeeping vault-local.
    assert!(
        !SPACE_SCOPED_CRDT_TABLES
            .iter()
            .any(|t| *t == crate::table_names::TABLE_SPACE_UCAN_GRANTS_NO_SYNC),
        "haex_space_ucan_grants_no_sync must never be added to SPACE_SCOPED_CRDT_TABLES — it is a local-only grants store"
    );
    assert!(
        !MEMBERSHIP_SYSTEM_TABLES
            .iter()
            .any(|t| *t == crate::table_names::TABLE_SPACE_UCAN_GRANTS_NO_SYNC),
        "haex_space_ucan_grants_no_sync must never be added to MEMBERSHIP_SYSTEM_TABLES either"
    );
}

#[test]
fn scan_single_column_for_owner_nonexistent_table_or_column_is_empty() {
    let conn = setup_test_db();
    insert_row(&conn, "r1", "hello", 42, "1000000000000000000/aabbccdd");

    // Nonexistent table => empty, no error.
    let no_table =
        scan_single_column_for_owner(&conn, "does_not_exist", "name", "device-1").unwrap();
    assert!(no_table.is_empty());

    // Existing table, but a column no row has => empty, no error.
    let no_column =
        scan_single_column_for_owner(&conn, "test_items", "nonexistent_col", "device-1").unwrap();
    assert!(no_column.is_empty());
}

// -----------------------------------------------------------------------
// Registry-driven P2P scan coverage
// -----------------------------------------------------------------------
//
// `scan_space_scoped_tables_for_local_changes` runs two passes:
//
// * Pass 1 — static [`SPACE_SCOPED_CRDT_TABLES`] whitelist for the
//   membership-system tables scoped by their own `space_id` column.
// * Pass 2 — registry-driven scan of extension-owned content tables via
//   `haex_shared_space_sync`, which maps `(table_name, row_pks)` to a
//   `space_id` for tables that carry no `space_id` column of their own.
//   The register and the whitelist are additive; the guard in the pass 2
//   loop drops any register row referencing a whitelisted table so
//   control-plane data cannot be re-emitted through the ext path.
//
// The tests below lock the observable contract of that pipeline in:
//
// * `registered_extension_row_included_in_p2p_scan` — a registered
//   `ext_notes_v1` row appears in the scan output and a sibling
//   unregistered row does not.
// * `registered_row_in_different_space_not_included` — cross-space leak
//   guard: the registry-space filter matches on `registry.space_id`, so
//   a row registered for space B is invisible to a scan targeting
//   space A.
// * `control_plane_scan_still_works_without_registry_entries` —
//   regression guard: pass 1 continues to emit whitelisted rows even
//   when the registry holds nothing for the space.

/// Convention: extension-owned CRDT tables use an `ext_<name>_v<n>`
/// prefix. Matches the literal used in `inbound_sync_tests::
/// validate_and_attribute::EXT_TABLE`, so the receiver-side and
/// scanner-side tests exercise the same shape.
const EXT_TABLE: &str = "ext_notes_v1";

/// Wrap an in-memory `Connection` in a [`DbConnection`] the way the
/// production code expects. Mirrors the pattern in
/// `space_delivery::local::inbound_sync_tests::helpers::setup_authz_db`.
fn wrap_db(conn: Connection) -> crate::database::DbConnection {
    use std::sync::{Arc, Mutex};
    crate::database::DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// In-memory DB with the schemas the registry-driven scan needs:
/// the register itself, one whitelisted control-plane table
/// (`haex_space_members`), and one extension-owned content table
/// (`EXT_TABLE`). No CRDT triggers or migrations — the scanner reads
/// plain rows.
fn setup_registry_scan_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE haex_shared_space_sync (
                id TEXT PRIMARY KEY NOT NULL,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                space_id TEXT NOT NULL,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE haex_space_members (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                identity_id TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'read',
                authored_by_did TEXT,
                joined_at TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE ext_notes_v1 (
                id TEXT PRIMARY KEY,
                body TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

/// Seed one row into `EXT_TABLE` with a per-column signature keyed by
/// `sig_space_id` (which is how the W1 write path attaches sigs
/// through `execute_with_crdt`). `sig_space_id = None` writes no sig,
/// used by the leak guard to prove the scanner still emits scoped
/// data even in that shape.
fn insert_ext_row(conn: &Connection, id: &str, body: &str, hlc: &str, sig_space_id: Option<&str>) {
    let hlcs = format!("{{\"body\":\"{hlc}\"}}");
    let sigs = match sig_space_id {
        Some(space_id) => serde_json::json!({
            "body": {
                (space_id): {
                    "authorDid": "did:key:test",
                    "sig": "",
                    "storageClass": "text",
                }
            }
        })
        .to_string(),
        None => "{}".to_string(),
    };
    conn.execute(
        "INSERT INTO ext_notes_v1 (id, body, haex_hlc, haex_column_hlcs, haex_column_sigs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, body, hlc, hlcs, sigs],
    )
    .unwrap();
}

/// Seed a `(table, row_pks, space_id)` triple into the register.
///
/// Direct INSERT (not `core::execute_with_crdt`) — the scanner only
/// READS `haex_shared_space_sync`, so bypassing the register-fanout
/// trigger is fine. Same trade-off as
/// `inbound_sync_tests::helpers::insert_registered`.
fn insert_registry_entry(
    conn: &Connection,
    registry_row_id: &str,
    space_id: &str,
    table_name: &str,
    row_pks: &str,
) {
    // Give the register row a plausible HLC + per-column HLC map so the
    // scanner emits realistic `LocalColumnChange`s from the register
    // table itself (`haex_shared_space_sync` IS on the whitelist).
    // Without this, the register-row changes come out with
    // `hlc_timestamp = "haex_hlc"` (a literal-string fallback), which
    // is confusing when debugging failures on the ext-table assertions.
    let hlc = "1000000000000000000/aabbccdd";
    let hlcs = format!("{{\"table_name\":\"{hlc}\",\"row_pks\":\"{hlc}\",\"space_id\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO haex_shared_space_sync
             (id, table_name, row_pks, space_id, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![registry_row_id, table_name, row_pks, space_id, hlc, hlcs],
    )
    .unwrap();
}

/// Seed one row into `haex_space_members` with per-column sigs keyed by
/// `space_id`. Used by the regression guard test.
fn insert_member_row(conn: &Connection, id: &str, space_id: &str, identity_id: &str, hlc: &str) {
    let hlcs = format!("{{\"identity_id\":\"{hlc}\",\"role\":\"{hlc}\"}}");
    let sigs = serde_json::json!({
        "identity_id": {
            (space_id): {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        },
        "role": {
            (space_id): {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        }
    })
    .to_string();
    conn.execute(
        "INSERT INTO haex_space_members
             (id, space_id, identity_id, role, haex_hlc, haex_column_hlcs, haex_column_sigs)
             VALUES (?1, ?2, ?3, 'read', ?4, ?5, ?6)",
        rusqlite::params![id, space_id, identity_id, hlc, hlcs, sigs],
    )
    .unwrap();
}

#[test]
fn registered_extension_row_included_in_p2p_scan() {
    // Given `(space-A, EXT_TABLE, {"id":"row-1"})` is registered in
    // haex_shared_space_sync and the corresponding row exists in
    // EXT_TABLE, the space-scoped P2P scan returns it. A second row in
    // the same table that is NOT registered stays out of the output —
    // the register is the sole entry point for extension-owned rows
    // into pass 2 of `scan_space_scoped_tables_for_local_changes`.
    let conn = setup_registry_scan_db();
    insert_ext_row(
        &conn,
        "row-1",
        "hello",
        "1000000000000000000/aabbccdd",
        Some("space-A"),
    );
    insert_ext_row(
        &conn,
        "row-unregistered",
        "sekrit",
        "2000000000000000000/aabbccdd",
        Some("space-A"),
    );
    insert_registry_entry(&conn, "reg-1", "space-A", EXT_TABLE, r#"{"id":"row-1"}"#);

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        changes
            .iter()
            .any(|c| c.table_name == EXT_TABLE && c.row_pks.contains("row-1")),
        "registered extension row must be scanned for P2P push, got: {changes:?}",
    );
    assert!(
        !changes
            .iter()
            .any(|c| c.row_pks.contains("row-unregistered")),
        "unregistered rows must not leak into P2P scan, got: {changes:?}",
    );

    // Sig-forwarding (W1 → W2 contract). The receiver's W2 gate REJECTS
    // a registered extension row that arrives without a per-column sig
    // for the requested space, so the outbound scan MUST forward the
    // sig it read off the row. The fixture writes the sig via direct
    // INSERT (same shape `execute_with_crdt` would produce); if a
    // future refactor stops seeding sigs here, this assertion will
    // fail spuriously and needs re-inspection alongside the W1 write
    // path.
    let extension_changes: Vec<_> = changes
        .iter()
        .filter(|c| c.table_name == EXT_TABLE)
        .collect();
    assert!(
        !extension_changes.is_empty(),
        "at least one ext change is required for the sig-forwarding assertion to be meaningful",
    );
    assert!(
        extension_changes.iter().all(|c| c.sig.is_some()),
        "registered extension rows must carry a per-column signature (W1): {extension_changes:?}",
    );
}

#[test]
fn registered_row_in_different_space_not_included() {
    // Cross-space leak guard. The row is registered for SPACE_B but
    // the scan requests SPACE_A — the row must NOT appear. Passes
    // trivially today (ext tables are never scanned) and becomes
    // load-bearing once Task 5 iterates the register: a naive impl
    // that reads every register row without matching `space_id`
    // against the scan space would leak SPACE_B rows to a SPACE_A
    // peer.
    let conn = setup_registry_scan_db();
    insert_ext_row(
        &conn,
        "row-1",
        "hidden",
        "1000000000000000000/aabbccdd",
        Some("space-B"),
    );
    insert_registry_entry(&conn, "reg-b", "space-B", EXT_TABLE, r#"{"id":"row-1"}"#);

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        !changes.iter().any(|c| c.table_name == EXT_TABLE),
        "row registered in space-B must never appear in a space-A scan, got: {changes:?}",
    );
}

/// Composite-PK smoke test for the registry-driven pass.
///
/// Regression guard for the "PK JSON key ordering" concern (Task 5 code
/// review): the writer path (`extension_space_assign`) stores `row_pks`
/// verbatim from the caller, and TS extensions produce PK JSON via
/// `JSON.stringify` on object literals built in schema-PK-declaration
/// order (matching the TS reader `tableScanner.ts:446-449`). The outbound
/// scanner MUST produce the same schema-declaration-order form, or a
/// `HashSet::contains(pk_json)` filter would miss composite-PK rows and
/// silently drop them.
///
/// The scanner's canonical form is **schema-declaration order** — the
/// order PK columns are declared in the CREATE TABLE. This test locks
/// that contract in from the read side: a registry entry stored in
/// schema-declaration order (matching the writer wire form) MUST match.
#[test]
fn registered_composite_pk_row_matches_schema_order_wire_form() {
    let conn = setup_registry_scan_db();

    // Composite-PK extension table where schema order (b, a) != alphabetical (a, b).
    conn.execute_batch(
        "CREATE TABLE ext_composite_v1 (
            b TEXT NOT NULL,
            a TEXT NOT NULL,
            body TEXT,
            haex_hlc TEXT,
            haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
            haex_column_sigs TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (b, a)
        );",
    )
    .unwrap();

    let hlc = "1000000000000000000/aabbccdd";
    let hlcs = format!("{{\"body\":\"{hlc}\"}}");
    let sigs = serde_json::json!({
        "body": {
            "space-A": {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        }
    })
    .to_string();
    conn.execute(
        "INSERT INTO ext_composite_v1 (b, a, body, haex_hlc, haex_column_hlcs, haex_column_sigs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params!["yb", "xa", "hello", hlc, hlcs, sigs],
    )
    .unwrap();

    // Register with SCHEMA-DECLARATION key order — matches the writer wire
    // form (`JSON.stringify({b: ..., a: ...})` on a schema-order object
    // literal, which is what TS extensions produce and what
    // `extension_space_assign` stores verbatim).
    insert_registry_entry(
        &conn,
        "reg-schema-order",
        "space-A",
        "ext_composite_v1",
        r#"{"b":"yb","a":"xa"}"#,
    );

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    let composite_hits: Vec<_> = changes
        .iter()
        .filter(|c| c.table_name == "ext_composite_v1")
        .collect();
    assert!(
        !composite_hits.is_empty(),
        "composite-PK row registered in canonical (schema-declaration) key order must be \
         included in the space-scoped scan, got: {changes:?}",
    );
    // Sanity: the scanner emitted the row_pks in schema-declaration order too,
    // proving both writer and scanner agree on the canonical form.
    assert_eq!(
        composite_hits[0].row_pks, r#"{"b":"yb","a":"xa"}"#,
        "scanner must emit composite PKs in schema-declaration key order (matches writer wire form)"
    );
}

/// Negative counterpart: a register entry stored in alphabetical PK
/// order (non-canonical under the current wire form) MUST NOT match,
/// because the outbound scanner produces schema-declaration-order PK
/// JSON. If this test starts failing, someone likely changed the scanner
/// to emit alphabetical form (e.g. reintroduced `serde_json::Map`) —
/// see the doc on `is_registered_for_space` for the wire contract.
#[test]
fn registered_composite_pk_row_ignored_when_registry_key_order_differs() {
    let conn = setup_registry_scan_db();
    conn.execute_batch(
        "CREATE TABLE ext_composite_v1 (
            b TEXT NOT NULL,
            a TEXT NOT NULL,
            body TEXT,
            haex_hlc TEXT,
            haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
            haex_column_sigs TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (b, a)
        );",
    )
    .unwrap();
    let hlc = "1000000000000000000/aabbccdd";
    let hlcs = format!("{{\"body\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO ext_composite_v1 (b, a, body, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["yb", "xa", "hello", hlc, hlcs],
    )
    .unwrap();
    // Register with ALPHABETICAL (a, b) key order — NOT the canonical
    // schema-declaration form the scanner produces.
    insert_registry_entry(
        &conn,
        "reg-alphabetical",
        "space-A",
        "ext_composite_v1",
        r#"{"a":"xa","b":"yb"}"#,
    );

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        !changes.iter().any(|c| c.table_name == "ext_composite_v1"),
        "register entry using non-canonical (alphabetical) key order MUST NOT match — \
         the register writer is responsible for producing schema-declaration-order \
         PK JSON: {changes:?}",
    );
}

#[test]
fn control_plane_scan_still_works_without_registry_entries() {
    // Regression guard. Whitelisted control-plane tables must keep
    // being scanned even when `haex_shared_space_sync` holds no rows
    // for the scan space. Locks in that Task 5 does not accidentally
    // gate whitelisted tables behind a register lookup — the whitelist
    // and the register are additive, not one-or-the-other.
    let conn = setup_registry_scan_db();
    insert_member_row(
        &conn,
        "mem-1",
        "space-A",
        "id-alice",
        "1000000000000000000/aabbccdd",
    );

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        changes
            .iter()
            .any(|c| c.table_name == "haex_space_members"),
        "whitelisted control-plane row must be scanned even with an empty registry, got: {changes:?}",
    );
}

#[test]
fn read_only_push_excludes_registered_extension_rows() {
    // Regression guard. Read-only members push via
    // `scan_membership_tables_for_local_changes`, which filters to
    // `MEMBERSHIP_SYSTEM_TABLES` only. Registered extension rows are
    // content, not membership, and must never leak into that pipe —
    // pushing them with Read capability would make the leader reject
    // the whole batch and wedge the read-only push cursor at t=0.
    //
    // Passes trivially today (the whitelist filter can't contain an
    // extension table), but Task 5 taught `scan_space_scoped_...` to
    // consult the registry; this test locks the read-only pipe down
    // against a future refactor that accidentally routes extension
    // rows through it.
    let conn = setup_registry_scan_db();
    // Seed a registered extension row into space-A — same fixture
    // shape as `registered_extension_row_included_in_p2p_scan`.
    insert_ext_row(
        &conn,
        "row-1",
        "hello",
        "1000000000000000000/aabbccdd",
        Some("space-A"),
    );
    insert_registry_entry(&conn, "reg-1", "space-A", EXT_TABLE, r#"{"id":"row-1"}"#);
    // Also seed a whitelisted membership row so the assertion below is
    // non-vacuous — the test would trivially pass on "no changes at
    // all", we want it to confirm the read-only pipe still surfaces
    // whitelisted rows while dropping extension rows.
    insert_member_row(
        &conn,
        "mem-1",
        "space-A",
        "id-alice",
        "2000000000000000000/aabbccdd",
    );

    let db = wrap_db(conn);
    let changes =
        scan_membership_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        !changes.iter().any(|c| c.table_name == EXT_TABLE),
        "read-only push MUST NOT include registered extension rows: {changes:?}",
    );
    assert!(
        changes
            .iter()
            .any(|c| c.table_name == "haex_space_members"),
        "read-only push must still surface whitelisted MEMBERSHIP_SYSTEM_TABLES rows, got: {changes:?}",
    );
}
