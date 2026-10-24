# Implementation Plan: Dateizugriff zwischen eigenen Geräten und Dateifreigaben an Kontakte

**Branch**: `056-file-access-sharing` | **Date**: 2026-10-21 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/056-file-access-sharing/spec.md`

## Summary

Eigene Geräte erreichen gegenseitig ihr ganzes Dateisystem, Kontakte die Ordner und Dateien, die
ihnen freigegeben wurden; beides live über die direkte iroh-Verbindung, ohne Inhaltsschlüssel und ohne
Ablage bei Dritten (Design 2026-10-21, ADR 0015).

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Ein Protokoll, zwei Modi** (R4, R5): neue ALPN `haex-vault-files/1` über `SyncNode::bind_with`; Handshake
  über `sync::foreign_auth` aus Spec 057 (Geräteliste mit Vault-Unterschrift, Geräteschlüssel an die
  QUIC-Gegenstelle gebunden), danach `Welcome` mit dem Modus.
  Ergebnis `OwnDevice` (ganzes Dateisystem, Schalter je Gerät), `AccessOff` (authentifiziert, aber
  Geräte-Dateizugriff lokal ausgeschaltet) oder `Contact` (nur ihre Wurzeln).
  `holzi-sync/1` bleibt nur für eigene Geräte.
- **Bedienende Seite in Fensterpfaden** (R1, R3): jede Anfrage läuft über `StorageFiles` mit Scope für
  verschlüsselte Ordner, das Gerät als `LocalStore`-Speicher wie in 048; Einschluss über
  `local_fs::resolve`/`resolve_below`, Sperre der eigenen Daten über `OwnPlaces`. Entschlüsseln und
  Proxy zu S3/WebDAV/SFTP geschehen beim Eigentümer.
- **Anfragende Seite als Speicher** (R2): `PeerStore: ObjectStore` mit `Protocol::Peer`; Kennungen
  `peer:<gerät>` und `share:<freigabe>` laufen durch `storage_files`. Viewer, Transfers, Suche,
  Vorschaubilder und Agent-Berechtigungen je Speicher funktionieren damit ohne neue Verzweigungen.
- **Schreiben** (R7, R8): Upload-Sitzungen mit Versionsstand und Prüfung bei `finish` → `Conflict`,
  auf die bestehende Konfliktfrage der Transfers abgebildet; Löschen auf dem Gerät in den Papierkorb.
- **Kontakt-Geräte erreichen** (R9, geändert 2026-10-23): direkt von der gespeicherten Visitenkarte
  mit Endpoint-ID und iroh-Relay wie Spec 057; kein Nostr.
- **Live** (R16): Anfrage `Watch` auf eigenem Stream, das bedienende Gerät nutzt den vorhandenen
  Ordner-Watcher; `files_watch` nimmt `peer:`/`share:` an.
- **Agents und Freigaben** (R17): Agent-Berechtigung „Dateifreigaben verwalten“ (Standard aus); Erteilen
  immer mit Bestätigungsdialog, Entziehen ohne.
- **Eigene Daten von holzi** (R1): eigene Geräte sehen sie wie lokal (nur lesend, für Agents gesperrt),
  Kontakte nie.
- **Daten** (R10, data-model.md): `haex_file_shares`, `haex_file_share_grants`,
  `haex_file_shares_received` (synchronisiert), `haex_file_device_access_no_sync` (lokal); Quittung `served_at` für „wartet auf Gerät“.

## Technical Context

**Language/Version**: Rust (Tauri 2.12, Edition und MSRV wie `src-tauri/Cargo.toml`); TypeScript 6
(strict), Vue 3.5, Nuxt 4.5 (SPA)

**Primary Dependencies**: vorhanden `iroh` 1.3.0, `postcard`, `tokio`, `tokio-util`, `trash`, `uuid`,
`ts-rs`, `async-trait`; Dev: `tempfile`. **Keine neue Abhängigkeit.** Frontend: haex-ui
(SettingsGroup/Row, Dialoge, Switch), keine neue Abhängigkeit

**Storage**: Vault-Datenbank über haex-crdt, Migration `0039_file_shares`, Trigger-Version 26 → 27
([data-model.md](./data-model.md)); keine Dateiinhalte in der Datenbank

**Testing**: `cargo test` (Lib-Tests in `*_tests.rs`; zwei Vaults im Prozess mit Loopback-Endpoints
und MockRelay; Gleichstand-Katalog `ObjectStore` auch gegen `PeerStore`), `pnpm typecheck`,
`pnpm lint`, `pnpm format:check`, `pnpm typecheck:scripts`; E2E nach Spec 033 mit zweiter Vault;
Handprüfung [quickstart.md](./quickstart.md)

**Target Platform**: Linux, macOS, Windows, Android (nichts `cfg(desktop)`; Papierkorb nur wo
vorhanden, R8)

**Project Type**: Desktop- und Mobil-App (Tauri)

**Performance Goals**: Ordner mit 1 000 Einträgen < 2 s im selben Netz (SC-001); Video startet < 3 s
(SC-002); Entzug wirkt < 2 s (SC-004); bedienendes Gerät bleibt bedienbar bei 5 lesenden Kontakten
(SC-006)

**Constraints**: Inhalte nur über die iroh-Verbindung (FR-001); keine Zugangsdaten und Schlüssel nach
außen (FR-043); Prüfung bei jeder Anfrage (FR-005); keine halbe Datei (SC-008); Dateisystemarbeit nie
auf dem Executor; höchstens 16 Streams je Verbindung, 64 je Gerät (R6); keine Netzdienste und kein
`sleep` in `cargo test`; ts-rs-Typen mit Präfix `FileShare`/`FilesPeer`

**Scale/Scope**: wenige eigene Geräte, Dutzende Freigaben und Kontakte je Vault; Listen bis
`MAX_ENTRIES` (100 000)

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                                          | Stand                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worktree auf Topic-Branch, PR nach `main`                      | ✅ `.worktrees/056-file-access-sharing`, Branch `056-file-access-sharing`; Rebase- oder Merge-Commit, kein Squash                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Speckit-Ablauf                                                 | ✅ Brainstorming und Design 2026-10-21 → specify → Freigabe des Betreibers („erst plan“) → plan; danach tasks, analyze, implement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Tests in eigenen Dateien                                       | ✅ jede neue Rust-Datei mit `*_tests.rs` daneben; Testaufbau zweier Vaults in eigener Test-Support-Datei                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 500 LoC je Datei                                               | ✅ Aufteilung unten (Handshake, Server, Anfragen, Upload, Verbindungen je eigene Datei)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Graphify zuerst / Wiederverwendung                             | ✅ Graph des Abzweigs (Schnappschuss `7d813aa`) abgefragt nach Bedienen über iroh, atomarem Schreiben, Pfad-Einschluss, Nachricht per Gift-Wrap; dazu Code-Recherche. Wiederverwendet aus Spec 057: `sync::foreign_auth`, `SyncNode::bind_with`/`dial_alpn`, `ContactsService::card_of_vault`. Wiederverwendet: `StorageFiles`, `LocalStore` + `local_fs::resolve*`, `DeviceRoot`, `OwnPlaces`, Transfers mit `Hooks`, `StorageFileSource`, `presence::wrap_payload`/`unwrap_rumor`, `LinkMeeting`-Form, `device_list::check_pushed`/`valid_lists`, Transkript-Muster aus `handshake.rs`, `AgentFileKind::Storage`, `vault_changes()`, `local::watch::watch_folder` mit dem `files_watch`-Channel, `FilesAgentPermissionRequest` als Vorbild für den Bestätigungsdialog. Geprüft und nicht passend: `holzi-sync/1`-Handshake (lehnt fremde Vaults ab), `picked::write` (nicht atomar), `preferences` mit `PrefScope::Device` (synchronisiert), Präsenz-Client (beendet alle Abos) |
| Laziness Ladder / keine unnötige Abhängigkeit                  | ✅ keine neue Abhängigkeit; anfragende Seite als `ObjectStore` statt neuer Quellenart (R2)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Eine Implementierung je Regel                                  | ✅ Pfad-Einschluss, Sperre eigener Daten, Listenprüfung, Gift-Wrap und Transfers bleiben je eine Stelle; neue Prüfung je Anfrage an genau einer Stelle (`files/peer/serve/check.rs`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Kein `unwrap`/`expect` auf Eingaben, Fehler nicht verschlucken | ✅ Draht-Eingaben werden einmal geparst und begrenzt (64 KiB Kopf); Fehler nach außen ohne Text (FR-042), intern mit Ursache geloggt                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| I. No Secrets in Git                                           | ✅ Testschlüssel zur Laufzeit erzeugt                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| II./III. Keine lokalen Pfade, Identität geräteunabhängig       | ✅ Pfade nur als Daten in der Vault, keine in Konfiguration                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| IV. Fremde Inhalte mit fester Revision                         | ✅ keine fremden Harness-Inhalte                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| VII. Ohne Relay weiterarbeiten                                 | ✅ Ohne Nostr-Relay bleibt alles Lokale nutzbar; eigene Geräte im selben Netz finden sich weiter, Kontakte über ihr iroh-Relay aus der Karte                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| VIII. Keine Verschleierung                                     | ✅ nicht berührt                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ADR für prinzipielle Entscheidungen                            | ✅ ADR 0015 (Zugriff zwischen eigenen Geräten, fremde Vaults am Endpoint)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Keine Hinweise auf Agents in Commits und PRs                   | ✅ Commits und PR ohne Trailer                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Phasen-Reihenfolge                                             | ✅ baut auf 024, 044, 048, 050 und 051 auf, alle in Benutzung (051 US1–US6 gemergt, PR B–G; US7 von 051 wird nicht gebraucht)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

Nach Phase 1 erneut geprüft: keine Verletzung, Complexity Tracking leer. Hinweis: `CLAUDE.md` mit
Speckit-Markern gibt es im Repository nicht; der Schritt „agent context update“ entfällt.

## Project Structure

### Documentation (this feature)

```text
specs/056-file-access-sharing/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── files-protocol.md    # haex-vault-files/1: Handshake, Anfragen, Fehler, Grenzen
│   └── tauri-commands.md    # Commands und Katalog-Aktionen
├── checklists/requirements.md
└── tasks.md                 # /speckit-tasks
docs/adr/0015-file-access-between-devices-and-contacts.md
```

### Source Code (repository root)

```text
src-tauri/src/
├── identity/
│   ├── migrations.rs                     # 0039 registrieren, HOLZI_TRIGGER_VERSION 27
│   └── migrations_file_shares.rs (+ _tests)
├── sync/
│   ├── wire.rs                           # FILES_ALPN
│   ├── signing.rs                        # Domain::FilesAuth
│   ├── foreign_auth.rs                   # aus Spec 057, unverändert genutzt (Kommentar zur Tabelle)
│   └── service.rs                        # Server-Zustand starten
├── files/
│   ├── peer/
│   │   ├── mod.rs                        # Modus, Fehler, gemeinsame Typen
│   │   ├── frame.rs (+ _tests)           # Kopf mit Länge, Rohstrom, Grenzen
│   │   ├── handshake.rs (+ _tests)       # foreign_auth + Welcome, Trust → Mode
│   │   ├── serve/
│   │   │   ├── mod.rs (+ _tests)         # FilesProtocol: ProtocolHandler, Streams, Grenzen
│   │   │   ├── check.rs (+ _tests)       # Prüfung je Anfrage: Modus, Recht, Stufe, Wurzel, eigene Daten
│   │   │   ├── roots.rs (+ _tests)       # Fensterpfad → StorageFiles (Gerät je Laufwerk, Speicher, Freigabe)
│   │   │   ├── ops.rs (+ _tests)         # ListDir/Stat/Read/CreateFolder/Rename/Move/Delete
│   │   │   └── upload.rs (+ _tests)      # Upload-Sitzungen, Versionsstand, Aufräumen
│   │   ├── store.rs (+ _tests)           # PeerStore: ObjectStore
│   │   ├── connect.rs (+ _tests)         # Verbindungen je Ziel, Adressen aus Präsenz/Karte
│   │   └── test_support.rs               # zwei Vaults, Loopback, MockRelay
│   ├── share/
│   │   ├── mod.rs                        # Typen FileShare*, Stufen
│   │   ├── store.rs (+ _tests)           # Tabellen lesen/schreiben, abgeleitete IDs, changed_at
│   │   ├── served.rs (+ _tests)          # Abonnent: served_at, source_state, Entzug → Streams abbrechen
│   │   ├── received.rs (+ _tests)        # empfangene Freigaben aus Shares-Antworten
│   │   ├── device_access.rs (+ _tests)   # Schalter (no_sync)
│   │   └── commands.rs                   # file_share_*, files_contact_query, files_device_access_*
│   ├── browser_commands/
│   │   ├── mod.rs                        # storage_files: peer:/share: → PeerStore
│   │   └── navigate.rs                   # Sources.devices, Sources.contacts
│   ├── transfer/
│   │   ├── entries.rs                    # remove → pub(crate)
│   │   └── remote*.rs                    # Conflict → Konfliktfrage
│   └── mod.rs                            # FilesErrorCode::Conflict, ::Unreachable
├── remote_storage/
│   ├── mod.rs                            # Protocol::Peer (serde/ts skip), StorageError::Conflict
│   └── protocol.rs                       # ProtocolStore: Peer → PeerStore
├── contacts/service/                     # Lookup per Vault-Identität (Caller::Internal), delete_impact
└── lib.rs                                # Commands registrieren

src/
├── components/files/
│   ├── Sidebar.vue                       # Gruppen „Meine Geräte“, „Von Kontakten“
│   ├── ShareDialog.vue                   # „Freigeben …“
│   └── SharesOverview.vue                # „Meine Dateifreigaben“
├── components/contacts/EntryPage.vue     # Abschnitt „Dateifreigaben“ auf der Kontaktseite
├── components/contacts/DeleteDialog.vue  # betroffene Dateifreigaben nennen (FR-023)
├── components/settings/FileDeviceAccessSetting.vue  # Schalter in FederationView
├── composables/useFileShares.ts
├── lib/actions/filesActions.ts           # files.shares.* (Scope files.shares)
├── lib/actions/settingsActions.ts        # settings.files.deviceAccess.*
├── stores/…ActionHandlers.ts
├── lib/files/registry.ts                 # Routen für peer:/share: (OPAQUE_ID prüfen)
└── i18n/locales/{de,en}.json             # files.shares.*, files.sidebar.devices/contacts, settings.federation.deviceAccess
```

**Structure Decision**: Netz und Bedienen unter `files/peer/`, Daten und Commands der Dateifreigaben
unter `files/share/`; `sync/` bekommt nur ALPN, Domäne und die Registrierung im Endpoint, damit die
Grenze von 024 bleibt. Frontend in den bestehenden Ordnern des Dateibrowsers.

## Lieferungen (PRs)

Jede Lieferung ist für sich prüfbar und wird vor der nächsten gemergt (Aufgaben in [tasks.md](./tasks.md)).

| PR  | Inhalt                                                                                                                                                       | Stories / FRs                                                      |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| A   | Migration und Tabellen, Fehlercodes, `Protocol::Peer`, Rahmen, Handshake, Server-Gerüst mit Prüfung je Anfrage, `PeerStore` lesend; keine sichtbare Änderung | Grundlage für alle; FR-001–FR-007, FR-042                          |
| B   | Eigene Geräte: Lesen und Schreiben, Papierkorb, „Meine Geräte“, Live                                                                                         | US1; FR-011–FR-013, FR-032, FR-035, FR-038, FR-044, FR-046, SC-009 |
| C   | Dateifreigaben anlegen (Dialog, Aktionen `files.shares`), Wählen von der Karte, Modus `Contact`, „Von Kontakten“                                             | US2; FR-008–FR-010, FR-015–FR-019, FR-028–FR-034                   |
| D   | Schreiben für Kontakte, Schreibkonflikt in Transfers, Löschen ohne Papierkorb                                                                                | US3; FR-036–FR-039                                                 |
| E   | Übersicht, Entzug mit Stream-Abbruch, „wartet auf Gerät“, Quelle fehlt, Kontaktseite, Löschdialog; Schalter                                                  | US4, US5; FR-014, FR-020–FR-027                                    |
| F   | Agents auf fremden Quellen, Agents verwalten Freigaben mit Bestätigung, E2E, Handprüfung                                                                     | US6; FR-040, FR-041, FR-045, SC-001–SC-008, SC-010                 |

## Complexity Tracking

Keine Verletzungen.
