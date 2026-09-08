//! The sync whitelists and their drift guards (ADR 0003): membership against
//! the generated table-name constants and against the real migration schema.

use crate::crdt::space_scanner::{
    is_membership_system_table, is_space_scoped_table, MEMBERSHIP_SYSTEM_TABLES,
    SPACE_SCOPED_CRDT_TABLES,
};
use rusqlite::Connection;

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
/// `crdt/space_scanner/mod.rs`) entscheidet, welche `haex_*`-Tabelle Inhalt an
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
    // hier UND in `crdt/space_scanner/mod.rs`. Reihenfolge muss deterministisch sein
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
        // Phase 4 Round F1 (Plan: docs/plans/2026-08-27-phase4-sharing-content-addressable-iam.md).
        // Grant-Registry für Cross-Space-File-Sharing: eine Row =
        // "content_key X ist mit space_id Y geteilt". Members lernen
        // Grants via Space-CRDT-Push statt Bucket-LIST, cross-space-
        // Isolation über die `space_id`-Spalte.
        "haex_file_grants",
        // Phase 4 Round F1: per-space, per-member Scoped-S3-Credential-
        // Verteilung. `encrypted_cred` ist AEAD-gesealed unter dem
        // aktuellen MLS-Epoch-Key des Space; nur Members können es
        // extrahieren. Space-Scoping über `space_id` verhindert, dass
        // Cross-Space-Credentials auf der Wire liegen.
        "haex_s3_shared_access",
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
         Wenn die Änderung unbeabsichtigt ist: reverte den Edit in `crdt/space_scanner/mod.rs`.\n\
         Hintergrund: docs/adr/0003-explicit-sync-policy.md § Entscheidung Punkt 3.\n\n"
    );
}

/// Doppel-Buchführungs-Test (ADR 0003) für `MEMBERSHIP_SYSTEM_TABLES`.
///
/// **Sicherheits-Kontext:** `MEMBERSHIP_SYSTEM_TABLES` (`crdt/space_scanner/mod.rs`)
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
         ADR-Referenz. Wenn nicht: reverte in `crdt/space_scanner/mod.rs`.\n\
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
        TABLE_FILE_GRANTS,
        TABLE_S3_SHARED_ACCESS,
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
             `crdt/space_scanner/mod.rs` nachzuziehen (still-defekter Sync-Path), oder der \
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
