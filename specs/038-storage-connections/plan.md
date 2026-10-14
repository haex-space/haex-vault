# Implementation Plan: Speicherverbindungen (S3) und ihre Weitergabe an Erweiterungen

**Branch**: `038-storage-connections` | **Date**: 2026-10-05 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/038-storage-connections/spec.md`

## Summary

holzi spricht S3 selbst: Der Nutzer legt in den Einstellungen Speicherverbindungen (Anbieter, Endpunkt,
Region, Zugangsdaten) und darauf Speicher (je ein Bucket) an, testet und entfernt sie; die Zugangsdaten
liegen im Passwortmanager. Erweiterungen nutzen einen Speicher über die neun Funktionen
`extension_remote_storage_*` des vault-sdk, nur mit Berechtigung je Speicher und nur in ihrem eigenen
Präfix-Bereich. Neue Speicher kann eine Erweiterung nur anstoßen: holzi öffnet einen eigenen Dialog, in
dem allein der Nutzer Zugangsdaten eingibt. Das erfüllt Spec 017 T106 (FR-054, FR-055) und liefert die
Speicherverbindung, auf die Spec 029 später aufbaut.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **S3-Client** (R1): `rusty-s3` signiert (SigV4, Sans-IO), das vorhandene `reqwest` mit rustls und ring
  überträgt; Grenzen werden streamend wie in `extensions/web.rs` durchgesetzt.
- **Zugangsdaten** (R2): ein Eintrag im Passwortmanager mit Eigentümer `storage`; neue Regel Z14 in 034
  macht Einträge mit Eigentümer für jeden Aufrufer außer dem Nutzer und der besitzenden holzi-Funktion
  unsichtbar, auch für eine Freigabe für `*` und als Quelle eines Verweises. Gelesen wird als
  `Caller::Internal { feature: "storage" }`.
- **Daten** (R3): Migration `0027_passwords_owner` (Spalte `owner` in `haex_passwords_item_details`,
  mit PR B) und `0028_storage_connections`: `haex_storage_connections` und `haex_storages`
  (synchronisiert), `storage_tests_no_sync` (geräteeigen); Trigger-Version jeweils erhöht.
- **Erweiterungen** (R4–R7): Präfix `holzi-ext/<vault_id>/<extension_id>/` (Entwicklerversionen
  getrennt unter `holzi-ext-dev/`), strenge Schlüsselprüfung vor jedem Aufruf, Liste nur mit Namen,
  Dialog nach dem Muster von `extension_dialog_confirm` (nur Bestätigung; Zugangsdaten nur im Fenster
  von holzi über der ganzen App), Grenzen und Fehlerarten aus Spec 017; Aufrufe mit Zugangsdaten werden
  abgelehnt.
- **Sicherheit des Transports** (R8): `https` immer, `http` nur zu lokalen Adressen und gekennzeichnet;
  jede aufgelöste Adresse geprüft und für die Verbindung festgehalten, Link-Local nie, lokale Adressen
  nur für Endpunkte des Nutzers; keine Weiterleitungen.
- **vault-sdk** (R11): eigener PR, der die Zugangsdaten aus `AddBackendRequest`/`UpdateBackendRequest`
  nimmt und die Liste auf Namen reduziert.

## Technical Context

**Language/Version**: Rust (Tauri 2.12, Edition wie `src-tauri/Cargo.toml`); TypeScript 6 (strict),
Vue 3.5, Nuxt 4.5 (SPA)

**Primary Dependencies**: neu `rusty-s3` 0.10 (R1); vorhanden `reqwest` 0.13 (rustls, ring), `hmac`,
`sha2`, `serde`, `ts-rs`, `uuid`; Frontend: haex-ui-Komponenten (Group/Row/OptionRow, Dialoge), keine neue
Abhängigkeit

**Storage**: Vault-Datenbank über haex-crdt; zwei synchronisierte Tabellen, eine `_no_sync`-Tabelle, eine
neue Spalte in `haex_passwords_item_details` ([data-model.md](./data-model.md))

**Testing**: `cargo test` (Lib-Tests in `*_tests.rs`, `wiremock` als lokaler S3-Mock, Fälschung des
Traits `ObjectStore`), `pnpm typecheck`, `pnpm lint`, `pnpm format:check`; manuell bzw. e2e gegen RustFS
([quickstart.md](./quickstart.md))

**Target Platform**: Linux, macOS, Windows, Android, iOS (FR-016; nichts `cfg(desktop)`)

**Project Type**: Desktop- und Mobil-App (Tauri) mit Erweiterungs-Host

**Performance Goals**: Test einer Verbindung < 10 s bei erreichbarem Anbieter (SC-001); Bridge-Aufrufe
innerhalb `timeout_ms` der Erweiterung

**Constraints**: keine Zugangsdaten an Erweiterungen (FR-012, SC-002); Grenzen von Spec 017 (16 MiB Antwort
Standard); kein aws-lc-rs; keine Netzdienste in `cargo test`

**Scale/Scope**: wenige Verbindungen und Speicher je Vault; Auflisten bis `max_rows` (10 000) Objekte je
Aufruf

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                                          | Stand                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worktree auf Topic-Branch                                      | ✅ `.worktrees/038-storage-connections`, Branch `038-storage-connections`                                                                                                                                                                                                                                                                                                                                                                                                          |
| Speckit-Ablauf                                                 | ✅ specify → Freigabe des Betreibers (2026-10-05) → plan; danach tasks, analyze, implement                                                                                                                                                                                                                                                                                                                                                                                         |
| Tests in eigenen Dateien                                       | ✅ jede neue Rust-Datei mit `*_tests.rs` daneben, `#[cfg(test)] #[path]`                                                                                                                                                                                                                                                                                                                                                                                                           |
| 500 LoC je Datei                                               | ✅ geplante Aufteilung (unten) hält jede Datei klar darunter                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Graphify zuerst / Wiederverwendung                             | ✅ wiederverwendet: `PasswordsService` und `access.rs` (Z14 statt eigenem Geheimnisspeicher), `permissions::store::{candidates, put}` und `evaluate`, `host.open_dialog`-Muster, `sql::exec::Limits`, `bridge::blocking::block_on`, Streaming-Grenzen nach `web.rs`, `extensions::ids::extension_id`, `UsageRegistry` für die Lösch-Warnung. Graphify-Graph: im Worktree wird der Schnappschuss des Abzweigs benutzt; die Kandidaten oben kamen aus einer gezielten Code-Recherche |
| Laziness Ladder / keine unnötige Abhängigkeit                  | ✅ eine neue Abhängigkeit (`rusty-s3`), begründet in R1; Signieren selbst zu schreiben verworfen                                                                                                                                                                                                                                                                                                                                                                                   |
| Kein `unwrap`/`expect` auf Eingaben, Fehler nicht verschlucken | ✅ Fehler des Anbieters werden abgebildet, die Ursache geloggt ohne Geheimnisse (R7)                                                                                                                                                                                                                                                                                                                                                                                               |
| Async nicht blockieren                                         | ✅ Bridge-Handler laufen auf Blocking-Threads, `block_on` wie bei Passwörtern und Web                                                                                                                                                                                                                                                                                                                                                                                              |
| Keine Netzdienste in Tests                                     | ✅ `wiremock` (lokaler Listener) und eine Fälschung des Traits (R10)                                                                                                                                                                                                                                                                                                                                                                                                               |
| Keine Geheimnisse im Repo                                      | ✅ Test-Zugangsdaten sind Platzhalter im Test, RustFS-Schlüssel nur lokal                                                                                                                                                                                                                                                                                                                                                                                                          |
| Phasen-Disziplin                                               | ✅ Voraussetzungen (017 L1–L4, 034) sind gebaut und in Gebrauch; 029 wird nicht vorgezogen, nur ihre Verbindung                                                                                                                                                                                                                                                                                                                                                                    |
| ADR bei Prinzip-relevanten Entscheidungen                      | ✅ keine Kernprinzipien berührt; die Regel Z14 ist eine Vertragsänderung an 034 und wird dort mit Verweis festgehalten                                                                                                                                                                                                                                                                                                                                                             |
| Keine Agent-Attribution                                        | ✅                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |

Nach dem Entwurf erneut geprüft: unverändert, keine Verstöße.

## Project Structure

### Documentation (this feature)

```text
specs/038-storage-connections/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── bridge.md          # die neun Bridge-Methoden
│   ├── tauri-commands.md  # Einstellungen und Dialog
│   └── access-z14.md      # Änderung an 034 (Einträge mit Eigentümer)
├── checklists/requirements.md
└── tasks.md               # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── identity/
│   ├── migrations_passwords_owner.rs   # 0027 (neu, PR B) + _tests
│   ├── migrations_storage.rs           # 0028 (neu, PR C) + _tests
│   └── migrations.rs                   # Registrierung, HOLZI_TRIGGER_VERSION
├── passwords/
│   ├── access.rs                       # ItemState.owner, Z14
│   ├── items.rs                        # item_state/headers_in_scope/agent_headers mit owner
│   └── service/items.rs, trash.rs      # create mit owner (nur Internal), endgültiges Löschen eigener Einträge
├── remote_storage/                     # neu: holzis S3 (unabhängig von Erweiterungen)
│   ├── mod.rs                          # Typen, Trait ObjectStore
│   ├── s3.rs                           # rusty-s3 + reqwest, Grenzen, Fehlerabbildung + _tests (wiremock)
│   ├── address.rs                      # Endpunkt-Prüfung http/https (R8) + _tests
│   ├── store.rs                        # Tabellen lesen/schreiben + _tests
│   ├── credentials.rs                  # Eintrag im Passwortmanager als Internal{storage} + _tests
│   ├── probe.rs                        # Verbindungstest (R9) + _tests
│   └── commands.rs                     # Tauri-Commands (contracts/tauri-commands.md)
└── extensions/
    ├── remote_storage.rs               # Liste und die vier Objekt-Methoden + _tests
    ├── remote_storage_manage.rs        # Hinzufügen, Ändern, Testen, Entfernen (US3) + _tests
    ├── remote_storage_endpoint.rs      # vorgeschlagener Endpunkt: Form, `add`, Bereich (FR-009b, R8)
    ├── remote_storage_keys.rs          # Bereich und Schlüsselprüfung (R4, R5) + _tests
    ├── remote_storage_dialog.rs        # Dialog-Warten (R6), getestet in remote_storage_manage_tests.rs
    └── bridge/dispatch.rs              # Registrierung, LATER ohne remote_storage

src/
├── components/settings/storage/        # Kategorie „Speicher“: Liste, Verbindung/Speicher-Formular, Entfernen-Vorschau
├── components/extensions/StorageDialog.vue   # Dialog für Erweiterungen über dem Tab, nur Bestätigung (R6)
├── components/StorageCredentialsModal.vue    # Zugangsdaten über der ganzen App (R6)
├── lib/settings/registry.ts            # Kategorie „storage“
├── components/wm/appRoutes.ts          # Ansichten
└── i18n/locales/{de,en}.json
```

**Structure Decision**: Das S3-Protokoll liegt in `src-tauri/src/remote_storage/`, nicht unter
`extensions/`, weil holzi es selbst nutzt (029, später Dateien) und Erweiterungen nur ein Verbraucher
sind (Grundsatz des Betreibers). Das Erweiterungs-Modul ist ein dünner Adapter wie
`extensions/passwords.rs`. Der Name `storage/` ist schon für Vault-Einstellungen belegt, daher
`remote_storage/`.

## Lieferungen

1. **PR A (Docs)**: diese Spec, Plan, Tasks; Abgleich in Spec 017 (T106 verweist auf 038, R21 und
   `contracts/bridge.md` Zeile `extension_remote_storage_*` auf 038) und Spec 029 (Speicherverbindung aus
   038).
2. **PR B**: Z14 in 034 (Spalte `owner`, Regel, Tests) — klein, eigenständig prüfbar, Voraussetzung für C.
3. **PR C**: `remote_storage/` mit Migration, Commands und Einstellungen (US1, US4).
4. **PR D**: Bridge-Methoden und Dialog (US2, US3), e2e-Szene gegen RustFS, falls das Rig einen
   Container erlaubt, sonst manuell nach Quickstart §5.
5. **vault-sdk-PR** (R11) parallel zu D.

## Complexity Tracking

Keine Verstöße gegen die Constitution.
