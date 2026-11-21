---
description: 'Tasks für Spec 024: Vault-Identität, Geräteliste und Datensync zwischen eigenen Geräten'
---

# Tasks: Vault-Identität, Geräteliste und Datensync zwischen eigenen Geräten

**Input**: Design-Dokumente in `specs/024-own-device-sync/` (plan.md, spec.md, research.md,
data-model.md, contracts/, quickstart.md)

**Tests**: verlangt. Die Constitution fordert für nicht-triviale Logik einen ausführbaren Check,
die Spec verlangt E2E-Szenarien (SC-011), und quickstart.md nennt die automatischen Prüfungen
A1–A9. Tests stehen in eigenen Dateien (`*_tests.rs`, `src-tauri/tests/`, `scripts/e2e/scenarios/`)
und warten nie mit festen Pausen, sondern auf Ereignisse mit Zeitgrenze.

**Organisation**: nach User Stories. Phase 2 (Schreibweg, Schlüssel, Geräteliste, Sync-Dienst) blockiert
alle Stories. Danach die P1-Stories US1, US2, US3, US5, dann die P2-Stories US4, US6, US7.

**Maßgebliche API von haex-crdt** (gemergt in #34/#35/#36): `Database::write(|tx| …)` mit
`CrdtTransaction::{execute, query_map, query_row}` (Parameter `&[&dyn ToSql]`, also `params![…]`;
`_no_sync`-Tabellen laufen ohne Stempel durch), `Database::read(|conn| …)` mit
`ReadOnlyConnection::{query_row, query_map}`, `DatabaseConfig.max_transaction_bytes`,
`Error::Database(DatabaseError)`, `Error::Consumer`, `Error::sqlite_error()` und
`serialized_parameter_bytes`. Wo research R19 oder `contracts/haex-crdt-upstream.md` noch
`execute_local`, `select` oder Hooks nennt, gilt diese API (T007 zieht die Doku nach).

## Format: `[ID] [P?] [Story] Beschreibung`

- **[P]**: parallel möglich (andere Dateien, keine offene Abhängigkeit)
- **[Story]**: US1 … US7 aus spec.md
- Pfade relativ zur Repository-Wurzel; Rust unter `src-tauri/`, Oberfläche unter `src/`

---

## Phase 1: Setup (haex-crdt pinnen, Fehler typisiert, Abhängigkeiten)

**Purpose**: neue haex-crdt-Revision einbinden (Plan-Schritt 0) und die Textprüfungen von Fehlern
ablösen, bevor irgendetwas anderes darauf aufbaut.

- [x] T001 haex-crdt in `src-tauri/Cargo.toml` auf `b8194662ac8f55f4c2b01453606e83bdb3f783d2` pinnen (Merge von haexmas/haex-crdt#36, enthält #34/#35); Kommentar über dem Eintrag auf #34–#36 aktualisieren; `Cargo.lock` mitziehen
- [x] T002 `max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES` im `DatabaseConfig` in `src-tauri/src/instances/vault_config.rs` setzen (neues Pflichtfeld aus haex-crdt#34)
- [x] T003 `From<CrdtError> for HaexError` in `src-tauri/src/error.rs` auf die typisierten Varianten umstellen: `CrdtError::Database(DatabaseError::VaultAlreadyOpenElsewhere { .. })` → `HaexError::VaultAlreadyOpenElsewhere`, `DatabaseError::TransactionTooLarge { bytes, limit }` → neue Variante `HaexError::TransactionTooLarge { bytes, limit }` (Text in `src/i18n/locales/de.json` und `en.json`), `CrdtError::Consumer(e)` → per `downcast` zurück zum eigenen `HaexError`, sonst `CrdtInit`; übrige `Database`-Varianten wie bisher auf `CrdtInit { reason }`; Tests in `src-tauri/src/error_tests.rs` (anlegen, falls nicht vorhanden)
- [x] T004 In `src-tauri/src/instances/open.rs` `is_locked_elsewhere` durch einen Match auf `CrdtError::Database(DatabaseError::VaultAlreadyOpenElsewhere { .. })` ersetzen und `is_wrong_passphrase` über `err.sqlite_error().and_then(rusqlite::Error::sqlite_error_code) == Some(ErrorCode::NotADatabase)` ohne Text-Fallback bilden; den erklärenden Kommentar in `src-tauri/src/instances/lock_retry.rs` (Zeilen 4–9) auf die typisierte Prüfung ändern; bestehende Tests in `src-tauri/src/instances/lock_retry_tests.rs` und `src-tauri/tests/vault_single_session.rs` laufen unverändert grün
- [x] T005 [P] (verschoben nach 2b: die Crates kommen mit dem ersten Task, der sie nutzt, T022) Neue Crates laut research R1 in `src-tauri/Cargo.toml` aufnehmen: `iroh = { version = "1.2", default-features = false, features = ["tls-ring"] }`, `nostr = { version = "0.45", features = ["nip44", "nip59"] }`, `nostr-sdk = "0.45"` (+ `features = ["local-relay"]` nur unter `[dev-dependencies]`), `secp256k1 = "0.30"`, `hkdf = "0.13"`, `chacha20poly1305 = { version = "0.10", features = ["std"] }`, `postcard = { version = "1", features = ["use-std"] }`, `qrcode = { version = "0.14", features = ["svg"] }`; MSRV ≥ 1.91 prüfen
- [x] T006 [P] Begriffe in `CONTEXT.md` ergänzen: Hauptgerät, verknüpftes Gerät, Geräteliste, Ursprungsgerät, Fortschrittsstand, Transaktionsgruppe, Nostr-Relay, iroh-Relay, Sync-Server (nie „Relay“ allein)
- [x] T007 [P] research R19 in `specs/024-own-device-sync/research.md` und `specs/024-own-device-sync/contracts/haex-crdt-upstream.md` an die gemergte API anpassen (`execute`/`query_map`/`query_row`, `Database::read` mit `ReadOnlyConnection`, kein `execute_local`, keine Hooks, `serialized_parameter_bytes`, typisierte Fehler, entfernte `DbConnection`-Schicht); in `plan.md` und research R19 die veraltete Bestandsaufnahme ersetzen: aktuell sind es 56 `with_connection`-Aufrufe in 15 Produktionsdateien; zusätzlich gibt es 14 Integrationstestdateien mit solchen Aufrufen

**Checkpoint**: `cargo test --manifest-path src-tauri/Cargo.toml`, `pnpm lint:rust` grün; haex-vault baut gegen die neue haex-crdt-Revision.

---

## Phase 2: Foundational (blockiert alle Stories)

### 2a Schreibweg über `Database::write` (Plan-Schritt 1, R19, R20) — ohne sichtbare Änderung

- [x] T008 Kompatibilitätstest anlegen in `src-tauri/tests/storage_transformer_compat.rs`: jede heutige INSERT-/UPDATE-/DELETE-Anweisung der Speicher-Module (`storage/chat_messages.rs`, `chat_threads.rs`, `known_devices.rs`, `models.rs`, `preferences.rs`, `providers.rs`, `wm_session.rs`, `adapters/cli_delegate/autonomy.rs`, `chat/send_admission.rs`, `providers/local.rs`), ohne die Zuweisung `haex_hlc_no_sync = current_hlc()`, läuft über `Database::write` gegen eine frisch migrierte Vault; erwartet: Zeilen-HLC gesetzt, Spalten-HLCs gefüllt, Tabelle als geändert markiert; `ON CONFLICT … DO UPDATE` eingeschlossen
- [x] T009 `src-tauri/src/storage/vault_db.rs` anlegen: `VaultDb` um `Arc<haex_crdt::Database>` mit `async fn read<R>(f: impl FnOnce(&ReadOnlyConnection) -> haex_crdt::Result<R>)` und `async fn write<R>(f: impl FnOnce(&mut CrdtTransaction) -> haex_crdt::Result<R>)`, beide über `spawn_blocking`, Fehler einheitlich nach `HaexError`; nach jedem erfolgreichen `write` `sync_notify.notify_one()` (`tokio::sync::Notify`, zunächst ohne Abnehmer); Tests in `src-tauri/src/storage/vault_db_tests.rs` (Rollback bei `Err`, eine HLC je `write`, Anstoß nach Commit, kein Anstoß nach Rollback)
- [x] T010 [P] Einstellungen umstellen: Schreibfunktionen in `src-tauri/src/storage/preferences.rs` nehmen `&mut CrdtTransaction`, Lesefunktionen `&ReadOnlyConnection`, alle `current_hlc()`-Zuweisungen entfallen; Aufrufer in `src-tauri/src/storage/preferences_commands.rs` und `src-tauri/src/voice.rs` auf `VaultDb`; Tests in `preferences_tests.rs`, `preferences_commands_tests.rs`, `src-tauri/tests/preferences_roundtrip.rs` nachziehen
- [x] T011 [P] Sitzung umstellen: `src-tauri/src/storage/wm_session.rs` und `src-tauri/src/storage/wm_session_commands.rs` auf `VaultDb`; mehrteilige Schreibungen in einem `write`; Tests in `wm_session_tests.rs`, `wm_session_commands_tests.rs`
- [x] T012 [P] Geräte umstellen: `src-tauri/src/storage/known_devices.rs` (`update_alias` ohne `current_hlc()`) und `src-tauri/src/device/commands.rs` auf `VaultDb`; Tests in `known_devices_tests.rs`
- [x] T013 [P] Freigaben der CLI-Delegation umstellen: `src-tauri/src/adapters/cli_delegate/autonomy.rs` und `src-tauri/src/adapters/cli_delegate/approval_bridge.rs` auf `VaultDb`; Tests in `approval_bridge_tests.rs`, `src-tauri/tests/cli_delegate_autonomy_*.rs`
- [x] T014 Provider umstellen: `src-tauri/src/storage/providers.rs`, `src-tauri/src/providers/mod.rs`, `src-tauri/src/providers/connect.rs`, `src-tauri/src/providers/local.rs` auf `VaultDb`; Tests in `providers_tests.rs`, `src-tauri/tests/provider_models.rs`
- [x] T015 Modelle umstellen: `src-tauri/src/storage/models.rs` (11 von Hand gesetzte HLCs), `src-tauri/src/models/commands.rs`, `src-tauri/src/chat/model_loading.rs`, `src-tauri/src/chat/default_model.rs` auf `VaultDb`; Tests in `models_tests.rs`, `models/commands_tests.rs`, `src-tauri/tests/huggingface_models.rs`, `model_capabilities_storage.rs`, `model_import.rs`
- [x] T016 Chat umstellen (nach T010, T015, weil `chat/commands.rs` Modelle und Einstellungen liest): `src-tauri/src/storage/chat_messages.rs`, `src-tauri/src/storage/chat_threads.rs`, `src-tauri/src/chat/commands.rs`, `src-tauri/src/chat/thread_commands.rs` (etwa `rename_thread`: Lesen und Schreiben in einem `write`), `src-tauri/src/chat/turn/persist.rs`, `src-tauri/src/chat/turn/tool_round.rs`, `src-tauri/src/chat/send_admission.rs` auf `VaultDb`; Tests in `chat_messages_tests.rs`, `persist_tests.rs`, `src-tauri/tests/chat_*.rs`, `src-tauri/tests/common/tool_loop_fixture.rs`
- [x] T017 Wartung umstellen: in `src-tauri/src/storage/maintenance.rs` `fold_scoped_preferences` über `VaultDb::write`; `PRAGMA secure_delete` und `VACUUM` bleiben auf `with_connection` mit `#[allow(clippy::disallowed_methods)]` und Begründung; Tests in `maintenance_tests.rs`
- [x] T018 Die Beschreibung in `src-tauri/src/storage/mod.rs` („Etappe-0 finding #2“) durch den neuen Grundsatz ersetzen (Schreiben nur über `VaultDb::write`, HLC setzt der Transformer); verbliebene Aufrufe von `with_connection` in den Integrationstests (`src-tauri/tests/*.rs`, 14 Dateien) auf `db.read`/`db.write` umstellen oder mit begründetem `allow` versehen
- [x] T019 `src-tauri/clippy.toml` anlegen mit `disallowed-methods = [{ path = "haex_crdt::Database::with_connection", reason = "über storage::vault_db schreiben und lesen (research R19)" }]`; einzige begründete Ausnahmen: `storage/maintenance.rs` (PRAGMA, VACUUM), `identity/bootstrap.rs` (noch kein HLC), Tests; `pnpm lint:rust` grün in beiden Feature-Sätzen
- [x] T020 Löschvermerke begrenzen (R20): nach dem Öffnen im Wartungsablauf `src-tauri/src/storage/maintenance.rs` `Database::cleanup_deleted_rows(RetentionPolicy::TimeBasedDays { days: 90 }, …)` aufrufen; Test in `maintenance_tests.rs` (Vermerk älter als 90 Tage weg, jüngerer bleibt)

### 2b Schlüssel und Geräteliste ohne Netz (Plan-Schritt 2, FR-001 bis FR-006, FR-015, FR-038)

- [x] T021 Migrationen in `src-tauri/src/identity/migrations.rs` nach `0020_wm_session_no_sync` anlegen: `vault_identity` neu aufbauen (nur `id INTEGER PK` immer `1`, `pubkey BLOB(32)` „x-only secp256k1, unveränderlich nach dem Anlegen“), `vault_identity_secret_no_sync(id, privkey)`, `device_keys_no_sync(installation_uuid TEXT PK, device_secret, device_pubkey, endpoint_secret, endpoint_id, created_at)`, `device_lists(list_hash BLOB(32) PK, generation INTEGER ≥ 1, payload BLOB, signature BLOB(64))` „nur Einfügen“, `vault_key_generations(key_id BLOB(16) PK, scope, generation, created_by, created_at, device_list_hash, authorization)`, `vault_key_envelopes(key_id, recipient, sender, envelope, device_list_hash, authorization, PK(key_id, recipient))`, `vault_content_keys_no_sync(key_id PK, generation, key)`, `sync_progress_no_sync(origin TEXT PK, max_hlc TEXT)`, `pending_links_no_sync(link_id BLOB(32) PK, peer_device_pubkey, new_list_hash, new_list_payload, new_list_signature, role, resume_secret, state, created_at)`, `admission_requests(device_pubkey BLOB(32) PK, vault_device_uuid, endpoint_id, name, requested_at, signature, state)`, `device_presence_no_sync(device_pubkey PK, last_seen, endpoint_addr, problem TEXT NULL)`; vor dem Entfernen der alten `vault_identity.privkey` den vorhandenen 32-Byte-Placeholder per idempotentem `INSERT ... SELECT` in `vault_identity_secret_no_sync` übernehmen, niemals neu erzeugen; `HAEX_TRIGGER_VERSION` erhöhen, damit die neuen CRDT-Tabellen Trigger bekommen; Tests in `src-tauri/src/identity/migrations_tests.rs`
- [x] T022 [P] `src-tauri/src/sync/keys.rs` + `keys_tests.rs`: Vault-Identität erzeugen (secp256k1 aus CSPRNG), Ableitung aus dem Platzhalter nach R3 (`HKDF-SHA256(ikm = platzhalter_privkey, salt = "haex", info = "haex/vault-identity/v1" ‖ zähler)` bis gültiger Skalar), Geräte- und iroh-Schlüssel je Installations-UUID laden oder erzeugen (fremde Zeilen nie lesen oder löschen, FR-006), alle Geheimnisse in `Zeroizing`, `Debug` geschwärzt; Tests: zwei Kopien desselben Platzhalters → dieselbe Identität, neue Installation → neue Schlüssel, Zeile des Quellgeräts bleibt unverändert
- [x] T023 [P] `src-tauri/src/sync/signing.rs` + `signing_tests.rs`: `sign(domain_tag, bytes)` / `verify` als BIP-340 über `SHA-256(tag ‖ postcard-bytes)` mit den Tags `haex-device-auth/v1`, `haex-device-list/v1`, `haex-key-generation/v1`, `haex-key-envelope/v1`, `haex-admission/v1`, `haex-link/v1`, `haex-link-resume/v1` (R10); Längenpräfix-Helfer `lp`
- [x] T024 `src-tauri/src/sync/device_list.rs` + `device_list_tests.rs`: `DeviceList`-Payload laut data-model.md (`vault`, `generation`, `devices[]`, `removed[]` mit `limit_hlc`, `issued_by`, `issued_at`, `base_list_hash`), kanonische Kodierung, Prüfregeln wörtlich: „Signatur der Vault-Identität über den ganzen Datensatz; `base_list_hash` verweist, außer bei der ersten Liste, auf eine gültige Liste mit niedrigerer Generation; die Liste führt alle Entfernungen ihrer Basisliste weiter; kein Gerät steht zugleich in `devices` und `removed`; kein Geräteschlüssel und keine `vault_device_uuid` doppelt; mindestens ein Gerät mit Rolle `main`“; `issued_by` nicht prüfen; geltende Liste = höchste Generation, bei Gleichstand kleinster Hash, der allein Geräte und Entfernungen bestimmt; Zusammenführung zur nächsten Generation führt die geltenden Entfernungen weiter; Speichern/Laden in `device_lists` über `VaultDb`; Tests: Generation, kleinster Hash, gegenseitiges Entfernen (genau ein Hauptgerät bleibt, SC-013), fremde Signatur verworfen, Liste ohne Hauptgerät ungültig
- [x] T025 `src-tauri/src/sync/content_keys.rs` + `content_keys_tests.rs`: Inhaltsschlüssel (32 Byte, `key_id = SHA-256("haex-key-id/v1" ‖ key)[0..16]`), Generation gebunden an `device_list_hash`, `authorization` durch ein in dieser Liste eingetragenes Hauptgerät, Umschläge NIP-44 v2 an jeden Geräteschlüssel der Liste, Annahme nur bei gültiger Autorisierung, eigenen Umschlag entpacken nach `vault_content_keys_no_sync`; Namen in der Geräteliste mit XChaCha20-Poly1305 unter `HKDF(key, "haex/device-name/v1")`, zusätzliche Daten `generation ‖ device_pubkey`; höchste Generation, die an kein entferntes Gerät verpackt ist; Tests inkl. NIP-44-Testvektoren und „verknüpftes Gerät kann keine Generation ausstellen“
- [x] T026 Bootstrap umbauen: `src-tauri/src/identity/bootstrap.rs` erzeugt für neue Vaults eine echte Vault-Identität (kein Platzhalter mehr, FR-001), legt den privaten Schlüssel in `vault_identity_secret_no_sync`; nach dem Öffnen schreibt ein idempotenter Schritt in `src-tauri/src/instances/open.rs` und `create.rs` über `VaultDb::write` `vault_identity.pubkey` (erster HLC, data-model.md), bei alten Vaults aus dem von T021 übernommenen Placeholder nach R3 abgeleitet, und stellt die erste Geräteliste (Generation 1, dieses Gerät als Hauptgerät) und die erste Inhaltsschlüssel-Generation aus; Tests in `src-tauri/tests/bootstrap.rs` und `vault_upgrade.rs` (A2 aus quickstart.md); außerdem die vom Bootstrap ohne HLC angelegten Zeilen in `known_devices` nachstempeln: ihre Spalten-HLC-Map bleibt heute `NULL`, weil der Update-Trigger `json_set(NULL, …)` schreibt (Befund aus T008, `tests/storage_transformer_compat.rs` `device_alias_update`); ohne Spalten-HLCs liefert der Sync die Zeilen nicht
- [x] T027 Sync-Dienst anlegen: `src-tauri/src/sync/mod.rs` (`SyncService`, gestartet nach dem Öffnen in `src-tauri/src/instances/open.rs`, hängt am Abbruch-Token und Task-Tracker des Vault-Gates, beendet sich innerhalb der Fristen von Spec 013, FR-031) und `src-tauri/src/sync/events.rs` (`sync-devices-changed`, `sync-data-changed { tables }`, getrennte `link-host-state-changed`/`link-join-state-changed`); den `Notify` aus `VaultDb` abonnieren; Modul in `src-tauri/src/lib.rs` einhängen

**Checkpoint**: Eine neue und eine alte Vault öffnen sich, haben eine echte Vault-Identität, eine
signierte Geräteliste der Generation 1 und einen Inhaltsschlüssel; alle bisherigen Tests grün.

---

## Phase 3: User Story 1 - Zwei eigene Geräte halten die Vault gleich (Priority: P1) 🎯 MVP

**Goal**: Zwei Geräte derselben Geräteliste finden sich, verbinden sich und gleichen jede Änderung
in beide Richtungen ab, auch nach einer Offline-Zeit.

**Independent Test**: zwei In-Prozess-Geräte auf derselben Geräteliste (Test-Helfer stellt sie
aus); auf jedem etwas ändern → erscheint auf dem anderen; eines beenden, weiterarbeiten, wieder
öffnen → holt alles nach (quickstart A3, M2).

### Tests für User Story 1

- [x] T028 [P] [US1] Fixture `src-tauri/tests/common/sync_fixture.rs`: n Geräte im selben Prozess, je eigene Vault-Datei, gemeinsame Geräteliste, iroh mit `presets::Minimal`, `RelayMode::Disabled`, `MemoryLookup` über Loopback, `nostr_sdk::local_relay::MockRelay` als Nostr-Relay; Warten nur auf Ereignisse mit Zeitgrenze
- [x] T029 [P] [US1] Integrationstest `src-tauri/tests/sync_devices.rs` für US1: Szenarien 1–7 aus spec.md (verbinden ohne Zutun, anlegen/ändern/löschen erscheint, Nachholen nach Offline, gleiches Feld offline geändert → jüngere gilt, gerätelokale Daten reisen nie, Hauptgerät ↔ verknüpftes Gerät ohne privaten Schlüssel, Ein-Gerät-Vault beginnt Präsenz sobald zweites Gerät auf der Liste steht)

### Implementierung für User Story 1

- [x] T030 [P] [US1] `src-tauri/src/sync/progress.rs` + `progress_tests.rs`: Fortschrittsstand je Ursprungsgerät aus `sync_progress_no_sync` („höchster HLC, bis zu dem alle Änderungen dieses Ursprungs angewendet oder nach R5 abgelehnt“), eigener Stand aus dem eigenen jüngsten HLC, „nichts“ für unbekannte Ursprünge, Schreiben nur nach oben und nur nach dem Commit einer vollständigen Gruppe
- [x] T031 [P] [US1] `src-tauri/src/sync/outbound.rs` + `outbound_tests.rs`: jede CRDT-Tabelle und `haex_deleted_rows` einmal ab dem kleinsten Cursor scannen (`scan_table_for_local_changes`), je Spalte nach dem Cursor ihres Ursprungs filtern (Ursprung aus `hlc_timestamp`, nie `device_id`), über alle Ursprünge nach HLC sortieren, in Seiten ≤ 4 MiB packen, die nie eine Transaktionsgruppe teilen; gerätelokale Tabellen nie
- [x] T032 [US1] `src-tauri/src/sync/inbound.rs` + `inbound_tests.rs`: Seiten nach Transaktions-HLC gruppieren, gültige Gruppen über `Database::apply_remote_changes` anwenden, danach Fortschritt schreiben; unbekannte Tabelle → Pull abbrechen ohne Fortschritt; Ursprung ohne Listeneintrag annehmen, wenn ein geprüftes eigenes Gerät liefert (R5); die berührten Tabellen für `sync-data-changed { tables }` zurückgeben (das Senden übernimmt T036)
- [x] T033 [P] [US1] `src-tauri/src/sync/wire.rs` + `wire_tests.rs`: Rahmen `u32 BE Länge ‖ postcard`, Länge vor dem Puffer prüfen (vor `Accept` ≤ 64 KiB, danach ≤ 4 MiB), Nachrichten `Challenge`, `Response`, `Accept`, `Reject`, `DeviceListPush`, `Progress`, `Pull { vector, replace }`, `Page { changes, group_continues, more }`, `Resync` laut contracts/sync-protocol.md; unbekannte Nachricht oder zu großer Rahmen → Verbindung mit Fehlercode schließen
- [x] T034 [US1] (Endpunkt, Router, Anwählen, eine Verbindung je Gerät und `shutdown` stehen als `SyncNode`; das Starten je Vault-Session im `SyncService` mit dem iroh-Relay aus den Einstellungen folgt mit T038/T039, `haex-link/1` mit US5) `src-tauri/src/sync/endpoint.rs`: iroh-Endpunkt je Vault-Session nach dem Entsperren (`presets::Minimal`, gespeicherter iroh-Schlüssel, ALPN `haex-sync/1` und `haex-link/1`, `RelayMode::custom` aus den Einstellungen, `MemoryLookup`, `bind()` mit 15 s Zeitgrenze), `Router` je ALPN, `router.shutdown()` mit 2 s Grenze beim Session-Ende (FR-031)
- [x] T035 [US1] `src-tauri/src/sync/handshake.rs` + `handshake_tests.rs`: gegenseitiger Nachweis laut contracts/sync-protocol.md (Challenge/Response/Accept, Signatur über `haex-device-auth/v1 ‖ lp(nonce_a) ‖ lp(nonce_d) ‖ lp(endpoint_x) ‖ lp(endpoint_y) ‖ lp(vault)`, `endpoint` gleich `remote_id()`, Geräteliste per `DeviceListPush` abgleichen und danach erneut prüfen, `SchemaVersion` vergleichen); Grundfall für US1, Abweisungen vertieft US2
- [x] T036 [US1] `src-tauri/src/sync/session.rs`: Austausch mit einem verbundenen Gerät: beide senden `Progress`; wer mehr sieht, schickt `Pull` (höchstens einer je Verbindung); Sender antwortet mit `Page`s aus `outbound.rs`, Empfänger verarbeitet mit `inbound.rs`; nach jedem lokalen Commit (Notify aus `VaultDb`) und jeder angewendeten Seite neues `Progress`, dicht folgende Anstöße zusammengefasst; Daten nie ungefragt; nach einer angewendeten Seite `sync-data-changed { tables }` senden und, wenn `vault_key_envelopes` oder `device_lists` dabei sind, eigene Umschläge entpacken und die geltende Liste neu wählen
- [x] T037 [US1] `src-tauri/src/sync/presence.rs` + `presence_tests.rs`: Präsenz laut contracts/nostr-events.md (Art 21059, Siegel Art 13 mit Geräteschlüssel, innere Art 24100, `mb_pk` aus `HKDF(inhaltsschlüssel, "haex/presence/v1" ‖ tag_u32 BE)`, `created_at` = jetzt, Abo `#p = [mb_pk(heute), mb_pk(gestern)]` ohne `since`, Takt beim Öffnen, bei Adressänderung, alle 60 s, höchstens eine Meldung je Minute außer bei Adressänderung, Frische `ts` ≤ 150 s alt und ≤ 30 s in der Zukunft, Ereignisse > 16 KiB vor dem Entschlüsseln verwerfen); Ein-Gerät-Vault abonniert nur (FR-007); gefundene Geräte in `MemoryLookup` und `device_presence_no_sync`
- [x] T038 [US1] `src-tauri/src/sync/servers.rs`: voreingestellte Nostr-Relays und iroh-Relays (research R7) aus den Einstellungen lesen, Änderungen zur Laufzeit übernehmen (FR-008); ohne Server läuft lokale Arbeit unverändert (Constitution VII)
- [x] T039 [US1] Wiederaufbau (FR-010): Verbindungen nach Abbruch oder Adressänderung selbständig neu aufbauen in `src-tauri/src/sync/mod.rs`
- [x] T040 [US1] Offene Ansichten laden bei `sync-data-changed` gezielt neu: `src/composables/useSync.ts` (neu, Ereignisse abonnieren) und die Stores für Chat, Einstellungen und Sitzung (`src/stores/`, `src/composables/useChat.ts`, `usePreferences.ts`, `useWmSession.ts`) (FR-032)
- [x] T040a [US1] Nachtrag zu T040: `sync-data-changed` ist durch `vault-data-changed` ersetzt, das für jeden Schreiber gilt (`src-tauri/src/vault_events.rs`, `Database::observe_committed_changes` aus haex-crdt, `src/composables/useVaultData.ts`); Farbschema, Einstellungen, Chat, Anbieter und Modelle laden bei Änderungen still nach

**Checkpoint**: US1 läuft in `sync_devices.rs`; zwei Geräte gleichen sich ab.

---

## Phase 4: User Story 2 - Nur eigene Geräte kommen an die Daten (Priority: P1)

**Goal**: Fremde, nicht gelistete, gefälschte oder entfernte Geräte werden abgewiesen, bevor Inhalt
fließt; der Verkehr enthält keinen Klartext.

**Independent Test**: fremde Vault, Geräteschlüssel nicht auf der Liste, gefälschte Geräteliste →
abgewiesen vor jedem Inhalt; Mitschnitt zwischen zwei eigenen Geräten ohne Klartext (quickstart A3,
A5, A9).

### Tests für User Story 2

- [x] T041 [P] [US2] Integrationstest in `src-tauri/tests/sync_devices.rs` für US2: Szenarien 1–6 aus spec.md und SC-005 (fremde Vault, nicht auf der Liste, fehlender Besitznachweis, fremd signierte Geräteliste, veränderte Daten → Verbindung endet ohne Anwendung, Präsenz für Fremde unlesbar)
- [x] T042 [P] [US2] Klartext-Prüfung (SC-006, A9) in `src-tauri/tests/sync_devices.rs`: Rahmen mitschneiden; kein Tabellen- oder Spaltenname, kein Inhalt, kein Geräteschlüssel, kein privater Schlüssel der Vault-Identität im Klartext
- [x] T043 [P] [US2] Präsenztest `src-tauri/tests/sync_presence.rs`: nur Geräte der eigenen Vault entschlüsseln; Ein-Gerät-Vault veröffentlicht während der ganzen Session nichts (SC-007); unbekannte Geräte führen zu keiner Verbindung

### Implementierung für User Story 2

- [x] T044 [US2] Abweisungen in `src-tauri/src/sync/handshake.rs` vollständig: `RejectCode` `ForeignVault`, `NotOnList`, `Removed`, `BadSignature`, `Incompatible`, `Duplicate`, `Closing`; vor `Accept` fließt nur die Geräteliste (FR-009); fail closed ohne gültige Liste (FR-005)
- [x] T045 [US2] Versionen und Doppelte (FR-029, FR-030, R14) in `src-tauri/src/sync/handshake.rs` und `src-tauri/src/sync/presence.rs`: unverträgliche `SchemaVersion` → kein Sync, `problem = incompatible_version`; gleicher Geräteschlüssel mit verschiedenen `endpoint_id`s oder gleiche `vault_device_uuid` auf zwei Schlüsseln → Sync mit beiden anhalten, `problem = duplicate`, `sync-devices-changed`
- [x] T046 [US2] Private Schlüssel nie in Protokollen (FR-002): `Debug`/`Display` aller Schlüsseltypen in `src-tauri/src/sync/keys.rs` geschwärzt, Test in `keys_tests.rs`, der einen Log-Mitschnitt nach Schlüsselbytes durchsucht

**Checkpoint**: US1 und US2 grün; kein fremdes Gerät bekommt etwas.

---

## Phase 5: User Story 3 - Keine Änderung geht verloren oder doppelt, der Autor stimmt (Priority: P1)

**Goal**: Über indirekte Wege und Abbrüche kommt jede Änderung genau in ihrer Wirkung an, mit ihrem
Ursprungsgerät; große Transaktionen halten nichts an; veraltete Geräte gleichen über `Resync` ab.

**Independent Test**: drei Geräte, A und C nie gleichzeitig mit B verbunden; wechselnd ändern, nur
über B abgleichen → alle gleich, jede Änderung mit Ursprungsgerät (quickstart A4, SC-003, SC-014).

### Tests für User Story 3

- [x] T047 [P] [US3] Integrationstest in `src-tauri/tests/sync_devices.rs` für US3: Szenarien 1–5 aus spec.md (A → B → C, keine Doppelung beim späteren direkten Kontakt, Autor A statt B, Abbruch mitten im Austausch ohne Verlust oder Teilgruppe, Neustart ab dem Fortschrittsstand)
- [x] T048 [P] [US3] Lasttest `src-tauri/tests/sync_three_devices.rs` (`#[ignore]`, in CI separat): 1.000 Änderungen über wechselnde Wege mit zufälligen Abbrüchen; am Ende gleicher Stand, jede Änderung mit Ursprungsgerät, Fortschritt nie über einer fehlenden, noch geltenden Änderung (SC-003, SC-004, SC-014)

### Implementierung für User Story 3

- [x] T049 [US3] Gruppen über mehrere Rahmen in `src-tauri/src/sync/outbound.rs` und `src-tauri/src/sync/inbound.rs`: eine Gruppe > 4 MiB reist als `Page`s mit `group_continues = true` bis auf die letzte; Empfänger puffert höchstens eine Gruppe je Verbindung, misst sie mit `haex_crdt::serialized_parameter_bytes` über die gebildete Parameterliste und bricht den `Pull` ab, wenn sie `Database::max_transaction_bytes()` übersteigt; bei Abbruch Puffer verwerfen, kein Fortschritt; Tests in `outbound_tests.rs`, `inbound_tests.rs`
- [x] T050 [US3] `Resync` (R20) in `src-tauri/src/sync/session.rs`: nennt ein `Pull` für einen Ursprung einen Stand älter als 90 Tage, antwortet der Sender `Resync`; das veraltete Gerät lässt erst seine eigenen Änderungen holen, dann `Pull { replace: true }` mit leerem Stand und ersetzt seine synchronisierten Tabellen in einer Transaktion über einen dedizierten Remote-Apply/Replace-Pfad, der die ursprünglichen HLCs erhält (nicht über `VaultDb::write`, das lokale CRDT-HLCs erzeugt); Test in `src-tauri/tests/sync_devices.rs`

**Checkpoint**: US1–US3 grün; SC-003 und SC-014 bestanden.

---

## Phase 6: User Story 5 - Gerät verknüpfen (Priority: P1)

**Goal**: Eine frische Installation tritt per Code eines Hauptgeräts bei, standardmäßig als
verknüpftes Gerät ohne privaten Schlüssel der Vault-Identität.

**Independent Test**: Code auf dem Hauptgerät, auf frischer Installation eingeben, ohne
Hauptgerät-Rolle bestätigen → neue Vault mit allen Daten, verknüpftes Gerät auf beiden Listen, kein
privater Schlüssel; mit Rolle wiederholen → kann selbst verknüpfen (quickstart A6, M1).

### Tests für User Story 5

- [x] T051 [P] [US5] Integrationstest `src-tauri/tests/sync_link.rs`: Szenarien 1–7 aus spec.md; falscher, abgelaufener, verbrauchter Code scheitert ohne Übertragung; ohne Rolle kein privater Schlüssel auf N (SC-009, SC-012); Abbruch vor vollständigem Transfer hinterlässt keine veröffentlichte neue Liste; ein gespeicherter Transfer bleibt nach verlorenem `LinkDone` verknüpft und wird über den gewöhnlichen Sync abgeschlossen

### Implementierung für User Story 5

- [x] T052 [US5] `src-tauri/src/sync/link.rs` + `link_tests.rs`, Seite Hauptgerät: Code aus 16 Zufallsbytes (Base32 in Vierergruppen, QR-SVG über `qrcode`, einmal verwendbar, 10 min gültig), Treffpunkt-Abo auf `rv_pk = HKDF(code, "haex/link/rendezvous/v1")` (Art 24102), Anwählen über `haex-link/1`, `LinkHello`/`LinkProof` mit `HMAC-SHA256(HKDF(code, "haex/link/proof/v1"), …)`, Rollenfrage abwarten, `pending_links_no_sync` vor `LinkTransfer` anlegen, Momentaufnahme als `Page`s mit Stand „nichts“, alle Generationen als Umschläge an N, neue Geräteliste, `vault_secret` nur bei Rolle Hauptgerät (FR-038); nach `LinkDone` Liste idempotent veröffentlichen und Datensatz löschen; verlorenes `LinkDone` über die gewöhnliche Gerätelisten-Synchronisation bereinigen
- [x] T053 [US5] `src-tauri/src/sync/link.rs`, Seite neue Installation: Vault mit eigener Passphrase anlegen über `src-tauri/src/instances/create.rs` (Passphrase nie übertragen, FR-025), Geräteschlüssel erzeugen, Treffpunkt melden, Nachweis, Momentaufnahme je Gruppe prüfen und anwenden, `awaiting_publication` bis die neue Liste eintrifft; bei unvollständigem Transfer wird der Link nicht abgeschlossen
- [x] T054 [US5] Tauri-Befehle in `src-tauri/src/sync/commands.rs` + `commands_tests.rs`: `link_code_create`, `link_code_cancel`, `link_confirm { asMainDevice }`, `link_reject` (nur Hauptgerät, Backend prüft `NotMainDevice`), `link_join_start { code, vaultName, deviceName, passphrase }`, `link_join_status`, `link_join_cancel` (Startseite ohne offene Vault) laut contracts/tauri-commands.md; `ts-rs`-Typen `LinkJoinState`; getrennte Ereignisse `link-host-state-changed` und `link-join-state-changed`; Registrierung in `src-tauri/src/lib.rs`
- [x] T055 [P] [US5] Oberfläche Hauptgerät: `src/components/settings/LinkDeviceView.vue` (Code als QR und Text, Ablauf, Rollenfrage mit „nein“ vorausgewählt und der Warnung „Wird ein Hauptgerät kompromittiert und ist die Passphrase bekannt, ist die Vault verloren (Totalausfall).“), Ort `/federation/devices/link` in `src/lib/settings/registry.ts`, Aktion `settings.devices.link` (nicht für Agenten) in `src/lib/actions/settingsActions.ts` und `src/stores/settingsActionHandlers.ts`
- [x] T056 [P] [US5] Oberfläche neue Installation: `src/components/onboarding/LinkSheet.vue` („Mit einer Vault verknüpfen“: Code eingeben oder einfügen, Gerätename, Passphrase, Fortschritt, klare Fehlermeldungen) und Knopf in `src/pages/index.vue`
- [x] T057 [US5] Texte de/en für US5 in `src/i18n/locales/de.json` und `en.json` (FR-037)

**Checkpoint**: Alle P1-Stories grün; ein Gerät lässt sich verknüpfen und synchronisiert.

---

## Phase 7: User Story 4 - Föderation, Geräte: Rollen, Identität und wer online ist (Priority: P2)

**Goal**: Die Unteransicht „Geräte“ zeigt Rollen, online/zuletzt online, Probleme und die öffentliche
Vault-Identität, live aktualisiert.

**Independent Test**: A offen, B einschalten, umbenennen, beenden → A zeigt online, neuen Namen,
„zuletzt online“ mit Zeit, ohne Neuladen; beide zeigen dieselbe `npub` (quickstart M3, M4).

### Tests für User Story 4

- [x] T058 [P] [US4] Reine Anzeige-Logik `src/lib/sync/deviceStatus.ts` mit Test unter `scripts/check-settings.ts` (online bis 150 s nach letzter Meldung, „noch nie online gesehen“, Sortierung, Rollenbeschriftung)
- [x] T059 [P] [US4] Test der erweiterten Geräteliste in `src-tauri/src/device/commands_tests.rs` (Rolle, `isCurrent`, `online`, `lastSeen`, `problem`)

### Implementierung für User Story 4

- [x] T060 [US4] `list_vault_devices` in `src-tauri/src/device/commands.rs` und `src-tauri/src/storage/known_devices.rs` um `role`, `devicePubkey`, `online`, `lastSeen`, `problem` erweitern (Typ `VaultDevice` laut contracts/tauri-commands.md); „zuletzt online“ über dritte Geräte aus dem `last_seen`-Austausch im Handshake (data-model.md)
- [x] T061 [US4] Befehle `sync_status`, `vault_public_identity` (`{ npub, hex }`), `sync_servers_get`/`sync_servers_set` in `src-tauri/src/sync/commands.rs`; `sync-devices-changed` bei jeder Änderung von Liste, Online-Stand, Name, Problem, Aufnahmeanfragen (FR-034)
- [x] T062 [P] [US4] `src/stores/syncDevices.ts` (Liste live über `sync-devices-changed`) und `src/composables/useSync.ts` (Befehle)
- [x] T063 [US4] Unteransicht „Geräte“: `src/components/settings/FederationView.vue` aufteilen in `DeviceRow.vue` (Name, Rolle, dieses Gerät, online/zuletzt online, Grund bei Problemen), `VaultIdentityRow.vue` (öffentlicher Schlüssel als `npub`, kopierbar, nicht änderbar, FR-046) und `SyncServersGroup.vue` (Nostr- und iroh-Relays); auf verknüpften Geräten fehlen „Gerät verknüpfen“ und „Gerät entfernen“ mit Hinweis (FR-035); COSMIC/GNOME-Aufbau mit Group/Row/OptionRow; jede Datei ≤ 500 Zeilen
- [x] T064 [US4] Aktionen `settings.devices.list` (erweitert, für Agenten lesbar), `settings.devices.identity` (lesbar), `settings.sync.servers.set` (nicht für Agenten) in `src/lib/actions/settingsActions.ts` und `src/stores/settingsActionHandlers.ts`; keine Aktion gibt Schlüssel, Codes oder Umschläge heraus (FR-036)
- [x] T065 [US4] Texte de/en für US4 in `src/i18n/locales/de.json` und `en.json`; `pnpm check:settings` prüft beide Sprachen (FR-037)

**Checkpoint**: Geräteansicht zeigt den Live-Stand beider Rollen.

---

## Phase 8: User Story 6 - Gerät entfernen (Priority: P2)

**Goal**: Ein Hauptgerät entfernt ein anderes Gerät mit Grenze und neuer Schlüsselgeneration; das
entfernte Gerät bekommt nichts mehr, alles andere läuft weiter.

**Independent Test**: drei Geräte, auf dem Hauptgerät eines entfernen → das entfernte kann sich mit
keinem verbinden, bekommt nichts Neues, kann nichts der neuen Generation entschlüsseln; die anderen
synchronisieren weiter (quickstart M6, M7, SC-010).

### Tests für User Story 6

- [x] T066 [P] [US6] Integrationstest in `src-tauri/tests/sync_link.rs` für US6: Szenarien 1–9 aus spec.md und SC-010; gegenseitiges Entfernen zweier Hauptgeräte offline → auf allen Geräten bleibt dasselbe Hauptgerät (Liste mit kleinerem Hash), das andere ist Solitär

### Implementierung für User Story 6

- [x] T067 [US6] `src-tauri/src/sync/removal.rs` + `removal_tests.rs`: in einer `VaultDb::write`-Transaktion Geräteliste Generation+1 ohne das Gerät, mit `limit_hlc` = eigener Fortschrittsstand für dieses Gerät (FR-028), und neue Inhaltsschlüssel-Generation nur an die Verbleibenden (FR-026); sich selbst entfernen verboten; laufende Verbindungen schließen, `MemoryLookup`-Eintrag und `device_presence_no_sync`-Zeile löschen; Geheimnisse nicht verändern; Erweiterungspunkt für Mitgliederlisten (027/028) leer anlegen
- [x] T068 [US6] Grenze beim Empfang in `src-tauri/src/sync/inbound.rs`: Gruppen eines entfernten Ursprungs jenseits `limit_hlc` ablehnen, Fortschritt darf darüber hinaus (R5); übrige Geräte übernehmen neue Liste und Generation ohne Zutun (FR-027); Test in `inbound_tests.rs`
- [x] T069 [US6] Befehl `device_remove { devicePubkey }` in `src-tauri/src/sync/commands.rs` (nur Hauptgerät, nicht für sich selbst); Aktion `settings.devices.remove` (nicht für Agenten) in `src/lib/actions/settingsActions.ts` und `src/stores/settingsActionHandlers.ts`
- [x] T070 [US6] `src/components/settings/RemoveDeviceView.vue` mit Ort `/federation/devices/:devicePubkey/remove` in `src/lib/settings/registry.ts`: Folgen vor der Bestätigung erklären (FR-026), bei Hauptgeräten zusätzlich „hilft nur gegen ein ehrliches Gerät“; Texte de/en in `src/i18n/locales/de.json` und `en.json`

**Checkpoint**: Entfernen wirkt überall, wo die neue Liste bekannt ist.

---

## Phase 9: User Story 7 - Kopie der Vault-Datei als Nebenweg (Priority: P2)

**Goal**: Eine kopierte Datei erzeugt eigene Schlüssel, löscht nichts, wird als Kopie eines
Hauptgeräts selbst Hauptgerät oder wartet als Kopie eines verknüpften Geräts auf Aufnahme.

**Independent Test**: Datei eines Hauptgeräts kopieren und öffnen → Hauptgerät, synchronisiert;
Datei eines verknüpften Geräts kopieren → „wartet auf Aufnahme“, nach „Aufnehmen“ gehen auch die
Änderungen aus der Wartezeit mit (quickstart M5, SC-018).

### Tests für User Story 7

- [x] T071 [P] [US7] Integrationstest in `src-tauri/tests/sync_copy.rs` (eigene Datei, `sync_link.rs` ist über 500 Zeilen) für US7: Szenarien 1–6 aus spec.md und SC-018; Zeile des Quellgeräts in `device_keys_no_sync` bleibt unverändert; Quellgerät als bisher einziges Gerät findet die Kopie über deren Präsenz (SC-002)

### Implementierung für User Story 7

- [x] T072 [US7] `src-tauri/src/sync/admission.rs` + Einbindung in `src-tauri/src/instances/open.rs`: Kopie erkennen (keine Zeile für die eigene Installations-UUID, aber fremde), neue Schlüssel und neue `vault_device_uuid` über `known_devices`; mit `vault_identity_secret_no_sync` neue Geräteliste mit sich als Hauptgerät plus Hinweis an die Nutzerin; ohne signierte Aufnahmeanfrage (innere Art 24101) im Präsenztakt; bis zur Aufnahme kein Sync
- [x] T073 [US7] Aufnahmeanfragen empfangen und begrenzen in `src-tauri/src/sync/presence.rs` und `src-tauri/src/sync/admission.rs`: gültige Anfrage in `admission_requests` ablegen; nach jeder Zusammenführung deterministisch nur die 20 kleinsten offenen nach `(requested_at, device_pubkey)` offen lassen, übrige idempotent `rejected`; nach Entscheidung löschen, offene nach 30 Tagen löschen (R20)
- [x] T074 [US7] Befehl `admission_decide { devicePubkey, admit }` in `src-tauri/src/sync/commands.rs` (nur Hauptgerät): „Aufnehmen“ trägt als verknüpftes Gerät ein und verpackt alle Generationen, „Ablehnen“ verwirft; Aktion `settings.devices.admit` (nicht für Agenten)
- [x] T075 [US7] `src/components/settings/AdmissionRequests.vue` in der Unteransicht „Geräte“ (Name, „Aufnehmen“, „Ablehnen“), Zustand „wartet auf Aufnahme durch ein Hauptgerät“ auf der Kopie; Texte de/en in `src/i18n/locales/de.json` und `en.json`

**Checkpoint**: Alle Stories unabhängig grün.

---

## Phase 10: Polish & Cross-Cutting

- [x] T076 [P] Aufräumen je Tabelle (R20; ohne `device_lists` und `sync_progress_no_sync`, siehe R20 „Stand der Umsetzung“) im Wartungsablauf `src-tauri/src/storage/maintenance.rs`: `device_lists` nur löschen, wenn keine behaltene Liste per `base_list_hash` auf sie verweist; `pending_links_no_sync` nach 24 h; `sync_progress_no_sync`-Zeilen entfernter Geräte, sobald ihr Stand die Grenze erreicht hat; Tests in `maintenance_tests.rs`
- [x] T077 Release-Gate für `vault_key_*` (R20) festhalten: Aktivierung erst mit einem Release, für das Spec 026 eine begrenzte Aufbewahrung definiert; Vermerk in `plans/README.md` (Zeile 024) und im PR-Text
- [x] T078 [P] E2E-Szenarien mit zwei App-Prozessen: `scripts/e2e/scenarios/sync-two-devices.test.ts` (US1), `sync-own-devices-only.test.ts` (US2), `sync-indirect.test.ts` (US3), `sync-link.test.ts` (US5); Nostr-Test-Relay über `scripts/e2e/lib/nostr-relay.ts` und `src-tauri/src/bin/e2e_nostr_relay.rs` (nur mit Feature `e2e`, `MockRelay` als Prozess) (SC-011)
- [x] T079 [P] Leistung prüfen (R17, SC-001, SC-002): Zeit bis zur Anzeige auf dem anderen Gerät in `src-tauri/tests/sync_devices.rs` messen und gegen 5 s (95 %) und 30 s bis zur Verbindung prüfen
- [x] T080 Constitution-Nachprüfung: Graphify-Warnung aus plan.md (R18) nach dem Bau des Moduls `sync/` mit einer neuen Abfrage auflösen; alle Dateien ≤ 500 Zeilen; keine Tests in Produktionsdateien
- [x] T081 quickstart.md vollständig durchlaufen (A1–A9 automatisch, M1–M9 ursprünglich manuell) und Ergebnis im PR festhalten (A1–A9 am 2026-10-01 durchlaufen und grün; M1–M9 laufen seit Spec 033 als E2E-Szenarien, `pnpm test:e2e --grep sync-`: SC-002 von Spec 033 am 2026-10-04 über die 20 jüngsten CI-Läufe auf `main` erfüllt, am selben Tag lokal zweimal 16 von 16); CI-Satz: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `pnpm lint:rust`, `cargo test --manifest-path src-tauri/Cargo.toml`, `pnpm check:settings`, `pnpm check:templates`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`

---

## Dependencies & Execution Order

### Phasen

- **Phase 1** zuerst: T001 → T002 → T003 → T004 hängen aneinander (Pin, Config-Feld, Fehlerabbildung, Prüfungen); T005–T007 parallel dazu.
- **Phase 2** blockiert alle Stories. In 2a zuerst T008 (zeigt, ob der Transformer haex-vaults SQL versteht) und T009 (`VaultDb`); dann T010–T013 parallel, T014 und T015 nacheinander oder parallel, T016 nach T010 und T015, dann T017–T020. 2b beginnt nach T009: T021 zuerst, dann T022/T023 parallel, T024 und T025 (brauchen T022, T023), T026 (braucht T021–T025), T027 (braucht T009).
- **US1** (Phase 3) nach Phase 2. **US2** und **US3** bauen auf den Modulen von US1 auf (Handshake, Session, Inbound); **US5** braucht Session und Inbound aus US1. **US4** braucht die Befehle der Geräteliste (Phase 2) und Präsenz aus US1. **US6** braucht US1 und Geräteliste; **US7** braucht Präsenz aus US1.
- **Phase 10** nach den gewünschten Stories.

### Innerhalb einer Story

Tests zuerst schreiben und fehlschlagen sehen, dann Module, dann Befehle, dann Oberfläche.

### Parallel

- Phase 1: T005, T006, T007.
- Phase 2a: T010, T011, T012, T013 (getrennte Dateien).
- Phase 2b: T022, T023.
- US1: T028, T029, T030, T031, T033.
- US2: T041, T042, T043.
- US3: T047, T048.
- US5: T051, T055, T056.
- US4: T058, T059, T062.
- Polish: T076, T078, T079.

## Parallel Example: User Story 1

```bash
Task: "Fixture src-tauri/tests/common/sync_fixture.rs"
Task: "Integrationstest US1 in src-tauri/tests/sync_devices.rs"
Task: "Fortschrittsstand in src-tauri/src/sync/progress.rs"
Task: "Liefern in src-tauri/src/sync/outbound.rs"
Task: "Rahmen und Nachrichten in src-tauri/src/sync/wire.rs"
```

## Implementation Strategy

### MVP (User Story 1)

1. Phase 1 (Pin, typisierte Fehler) und Phase 2 (Schreibweg, Schlüssel, Geräteliste, Sync-Dienst).
2. Phase 3 (US1): zwei Geräte gleichen sich ab.
3. Anhalten und prüfen: `sync_devices.rs`, quickstart M2.

### Schrittweise (eigene PRs wie in plan.md)

1. Phase 1 + 2a (Plan-Schritte 0 und 1): haex-vault schreibt nur noch über `Database::write`, ohne sichtbare Änderung.
2. Phase 2b (Plan-Schritt 2): Schlüssel und Geräteliste ohne Netz.
3. US1 (Plan-Schritte 3–5), dann US2, US3.
4. US5 (Plan-Schritt 6), US7 (7), US6 (8), US4 (9), E2E (10).

## Notes

- Vor US1, aus der Umsetzung von 2b (erledigt im Vorbereitungs-PR):
  - **haex-crdt neu gepinnt** auf den Merge von haexmas/haex-crdt#38 (`d35d43f9250a7d9a7d2dca3a0c8ee7ffabc750a7`, enthält #37): BLOB-Werte und BLOB-Schlüssel reisen typtreu, auch `providers.credentials`, und im Bootstrap ohne HLC angelegte Zeilen werden beim Öffnen gestempelt. `HAEX_TRIGGER_VERSION` ist 13, damit bestehende Vaults die neuen Trigger bekommen.
  - **Lokale Provider mit fester ID:** `providers::local` legt die lokalen Provider unter UUIDv5-IDs an (`haex:provider/local/chat`, `haex:provider/local/transcription`), so teilen sich alle Geräte einer Vault eine Zeile. Zeilen mit zufälliger ID aus der Zeit davor zieht die Wartung beim Öffnen auf die feste ID um, samt Verweisen in `models`, `chat_threads` und `chat_messages`.
  - **Verknüpfen (US5):** `open_instance` ruft `ensure_sync_state` mit `allow_genesis = false`, `create_instance` mit `true`. Die neue Installation beim Verknüpfen darf ihre Vault nicht über `create_instance` anlegen, sonst erzeugt sie eine eigene Vault-Identität.
- Offen: `cli_delegate`-Provider sind je Anbieter ein Singleton mit zufälliger ID. Verbindet man denselben Anbieter auf zwei Geräten, entstehen nach dem Abgleich zwei Zeilen; `find_cli_delegate_provider` nimmt die älteste. Das ist wie zweimal Verbinden auf einem Gerät und kein Datenverlust, gehört aber in US1-Tests beobachtet.

- [P] = andere Dateien, keine offene Abhängigkeit.
- Nie „Relay“ allein schreiben: Nostr-Relay, iroh-Relay, Sync-Server.
- Keine Agenten-Attribution in Commits oder PRs; Worktree je Änderung; haex-crdt nur per voller SHA pinnen.
