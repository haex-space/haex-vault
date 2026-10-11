# Implementation Plan: WebDAV- und SFTP-Speicher neben S3

**Branch**: `050-webdav-sftp-storage` | **Date**: 2026-10-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/050-webdav-sftp-storage/spec.md`

## Summary

holzi spricht neben S3 auch WebDAV und SFTP. Eine Speicherverbindung bekommt ein Protokoll, ein Speicher
ist bei WebDAV und SFTP ein Ordner. Alles, was S3-Speicher heute können, gilt für die neuen Protokolle:
Einstellungen mit Test vor dem Speichern, Zugangsdaten im Passwortmanager, Sync auf alle Geräte,
Adressprüfung, Dateibrowser, Agents, Erweiterungen und später verschlüsselte Ordner. Geteilt wird nichts.
Erst WebDAV, dann SFTP (Betreiber 2026-10-11).

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Protokoll je Verbindung** (R1): `ProtocolStore` ersetzt `s3_store()` und reicht nach
  `access.location.protocol` an `S3Store`, `WebDavStore` oder `SftpStore` weiter. Aufrufer und Fälschungen
  in den Tests bleiben, wie sie sind.
- **WebDAV** (R2): eigener Client auf `reqwest` und `quick-xml` 0.42; nur `Depth: 1`, Ordner-Adressen mit
  `/`, keine Weiterleitungen, `href` geprüft. Der HTTP-Client mit festen Adressen zieht aus `s3.rs` nach
  `http.rs`.
- **SFTP** (R3–R5): `russh` 0.63.3 (Backend ring, ohne aws-lc-rs) und `russh-sftp` 3.0.0; eigene TCP-
  Verbindung zur geprüften Adresse; Hostschlüssel beim ersten Verbinden bestätigt, synchronisiert,
  bei Abweichung Abbruch vor der Anmeldung; Schlüssel erzeugen und einlesen über `russh::keys`.
- **Ohne halbe Dateien** (R7): Zwischendatei `.holzi-part-<UUID>`, dann umbenennen; die Multipart-Methoden
  des Traits werden zu Upload-Sitzungen (S3 Multipart, WebDAV ein gestreamter `PUT`, SFTP Schreiben an
  Stellen).
- **Ordner** (R8): WebDAV und SFTP bilden die Markierungs-Form von S3 nach (`ordner/` in Listen), dazu
  `rename_tree`, `remove_tree` und `copies_on_provider`; `ObjectInfo.etag` für Spec 048.
- **Erweiterungen** (R9, contracts/bridge.md): dieselben Ergebnisse auf jedem Protokoll; Liste mit
  `type` und `folder`; Vorschläge für `webdav` und `sftp` ohne Zugangsdaten und ohne Hostschlüssel;
  eigener PR im vault-sdk.
- **Daten** (R13, data-model.md): Spalten `protocol`, `host_key`, `host_key_confirmed_at`; `bucket` trägt
  den Ordner; Trigger-Version +1.

## Technical Context

**Language/Version**: Rust (Tauri 2.12, Edition wie `src-tauri/Cargo.toml`, MSRV ≥ 1.89 wegen russh);
TypeScript 6 (strict), Vue 3.5, Nuxt 4.5 (SPA)

**Primary Dependencies**: neu `russh` 0.63.3 (`default-features = false`, `ring`, `rsa`, `flate2`),
`russh-sftp` 3.0.0, `quick-xml` 0.42 (schon transitiv vorhanden, jetzt direkt); vorhanden `reqwest` 0.13
(rustls, ring), `rusty-s3`, `tokio`, `zeroize`, `uuid`, `ts-rs`; Dev: `wiremock` (vorhanden), ein
WebDAV-Server im Prozess (Kandidat `dav-server`, beim Setup geprüft, R14); Frontend: haex-ui
(Group/Row/OptionRow, Dialoge), keine neue Abhängigkeit

**Storage**: Vault-Datenbank über haex-crdt; drei neue Spalten in `haex_storage_connections`
([data-model.md](./data-model.md)); Geheimnisse nur im Passwortmanager

**Testing**: `cargo test` (Lib-Tests in `*_tests.rs`; WebDAV gegen `wiremock` und einen Server im
Prozess, SFTP gegen einen russh-Server im Prozess, Gleichstand-Katalog gegen alle drei Stores),
`pnpm typecheck`, `pnpm lint`, `pnpm format:check`; E2E gegen SFTPGo; von Hand Hetzner und Nextcloud
([quickstart.md](./quickstart.md))

**Target Platform**: Linux, macOS, Windows, Android, iOS (FR-023; nichts `cfg(desktop)`)

**Project Type**: Desktop- und Mobil-App (Tauri) mit Erweiterungs-Host

**Performance Goals**: Test einer Verbindung < 10 s bei erreichbarem Server (SC-001); Video startet und
springt in < 3 s (SC-006); SSH-Sitzungen wiederverwendet (R3)

**Constraints**: keine Zugangsdaten, Schlüssel oder Hostschlüssel an Erweiterungen oder Agents (FR-020);
keine halbe Datei unter einem Zielnamen (FR-016, SC-003); keine Anmeldung bei unbestätigtem Hostschlüssel
(SC-004); höchstens 4 WebDAV-Anfragen bzw. 2 SSH-Sitzungen je Verbindung (R12); kein aws-lc-rs; keine
Netzdienste in `cargo test`

**Scale/Scope**: wenige Verbindungen je Vault; Auflisten mit denselben Grenzen wie S3 (`MAX_ENTRIES`
100 000 im Dateibrowser, `max_rows` bei Erweiterungen)

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                                          | Stand                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worktree auf Topic-Branch, PR nach `main`                      | ✅ `.worktrees/050-webdav-sftp-storage`, Branch `050-webdav-sftp-storage`, PR #372; Rebase- oder Merge-Commit, kein Squash                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Speckit-Ablauf                                                 | ✅ specify → Freigabe des Betreibers (2026-10-11, „mach das“) → plan; danach tasks, analyze, implement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Tests in eigenen Dateien                                       | ✅ jede neue Rust-Datei mit `*_tests.rs` daneben, `#[cfg(test)] #[path]`; Testserver in eigenen Test-Support-Dateien                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 500 LoC je Datei                                               | ✅ Aufteilung unten (WebDAV und SFTP je in mehrere Dateien)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Graphify zuerst / Wiederverwendung                             | ✅ Graph des Abzweigs (Schnappschuss, ohne Auffrischung) abgefragt nach XML-Parser, HTTP-Client mit festen Adressen, SSH/Schlüsseln, Baum-Durchlauf. Wiederverwendet: `S3Store::client_for` (wird `http::pinned_client`), `address::pin`/`scope_of`, `credentials.rs`, `probe.rs`, `RemoteStore` und seine Fälschung. Geprüft und nicht passend: `extensions/web.rs` `client()` (ohne feste Adressen, für Erweiterungen), `extensions/mail/parsing.rs` (MIME, kein XML), `passwords/import/keepass.rs` `walk` (lokaler KeePass-Baum), `sync/keys.rs` `DeviceKeys` (secp256k1-Geräteschlüssel, kein SSH), `files/local/watch.rs` (lokales Dateisystem). Kein XML- oder SSH-Code vorhanden |
| Laziness Ladder / keine unnötige Abhängigkeit                  | ✅ zwei neue Abhängigkeiten (`russh`, `russh-sftp`), begründet in R3; WebDAV ohne Bibliothek (R2); `quick-xml` war schon im Build                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Eine Implementierung je Regel                                  | ✅ Adressprüfung, Schlüsselprüfung der Erweiterungen, Test und Zugangsdaten bleiben je eine Stelle und werden um Protokolle erweitert, nicht dupliziert                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Kein `unwrap`/`expect` auf Eingaben, Fehler nicht verschlucken | ✅ Fehler der Server werden abgebildet und ohne Geheimnisse geloggt (R10)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| I. No Secrets in Git                                           | ✅ Testschlüssel und Hostschlüssel der Testserver werden zur Laufzeit erzeugt; die festen Hostschlüssel für SFTPGo im E2E sind reine Testschlüssel ohne Zugang zu etwas und werden beim Lauf erzeugt statt eingecheckt (Aufgabe)                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| II./III. Keine lokalen Pfade, Identität geräteunabhängig       | ✅ keine Pfade in Konfiguration; Verbindungen gehören der Vault                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| IV. Fremde Inhalte mit fester Revision                         | ✅ keine fremden Harness-Inhalte                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| VII. Ohne Relay weiterarbeiten                                 | ✅ betrifft diese Spec nicht (kein Relay); Speicher sind ohne Sync-Server nutzbar                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Keine Hinweise auf Agents in Commits und PRs                   | ✅ Commits und PR ohne Trailer                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Phasen-Reihenfolge                                             | ✅ baut auf 038 und 044 auf, beide in Benutzung; US7 wartet auf Spec 048                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |

Nach Phase 1 erneut geprüft: keine Verletzung, Complexity Tracking leer.

## Project Structure

### Documentation (this feature)

```text
specs/050-webdav-sftp-storage/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── remote-store.md      # Verhalten des Traits je Protokoll
│   ├── tauri-commands.md    # geänderte und neue Commands
│   └── bridge.md            # Erweiterungen: Liste, Vorschläge, Fehler
├── checklists/requirements.md
└── tasks.md                 # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/
├── Cargo.toml                               # russh, russh-sftp, quick-xml
└── src/
    ├── identity/
    │   ├── migrations.rs                    # Registrierung, HOLZI_TRIGGER_VERSION +1
    │   └── migrations_storage.rs            # + STORAGE_PROTOCOLS (ALTER TABLE)
    ├── remote_storage/
    │   ├── mod.rs                           # Protocol, Location.protocol/host_key, Credentials enum,
    │   │                                    #   ObjectInfo.etag, StorageError/TestOutcome neu, Trait-Methoden
    │   ├── protocol.rs (+ _tests)           # ProtocolStore (R1)
    │   ├── http.rs (+ _tests)               # pinned_client aus s3.rs (R2)
    │   ├── tree.rs (+ _tests)               # Markierungs-Form, Zwischendateien, Präfix-Durchlauf (R7–R9)
    │   ├── webdav/
    │   │   ├── mod.rs (+ _tests)            # WebDavStore: RemoteStore
    │   │   ├── request.rs (+ _tests)        # Methoden, Adressen, Statusabbildung
    │   │   ├── multistatus.rs (+ _tests)    # PROPFIND-Antwort mit quick-xml, href-Prüfung
    │   │   └── upload.rs (+ _tests)         # gestreamter PUT, Sitzungen
    │   ├── sftp/
    │   │   ├── mod.rs (+ _tests)            # SftpStore: RemoteStore
    │   │   ├── connect.rs (+ _tests)        # TCP zur geprüften Adresse, Handler, Hostschlüssel, Anmeldung
    │   │   ├── pool.rs (+ _tests)           # SSH-Sitzungen je Verbindung, Leerlauf
    │   │   ├── rename.rs (+ _tests)         # posix-rename oder löschen+umbenennen
    │   │   ├── upload.rs (+ _tests)         # Zwischendatei, Teile an Stellen
    │   │   └── keys.rs (+ _tests)           # erzeugen, einlesen, Fingerabdruck
    │   ├── s3.rs, s3_ext.rs, s3_multipart.rs # Methoden umbenannt, client_for → http.rs
    │   ├── address.rs                       # Schema sftp (R6)
    │   ├── credentials.rs                   # Felder je Protokoll (data-model.md)
    │   ├── store.rs                         # Prüfregeln je Protokoll
    │   ├── service.rs                       # Test mit Hostschlüssel, Ordner anlegen, annehmen
    │   ├── probe.rs                         # neue Ergebnisse, holzi-test/ aufräumen
    │   ├── model.rs                         # ConnectionInput/View, CredentialsInput getaggt
    │   ├── commands.rs                      # stores(), neue Commands
    │   └── test_support.rs                  # Fälschung um neue Methoden ergänzt
    ├── extensions/
    │   ├── remote_storage.rs                # Liste type/folder, list-Semantik, leere Ordner
    │   ├── remote_storage_keys.rs           # Schlüsselprüfung je Protokoll
    │   ├── remote_storage_manage.rs         # Vorschläge webdav/sftp, abgelehnte Felder
    │   ├── remote_storage_endpoint.rs       # Ziel der Aktion add für webdav/sftp
    │   └── remote_storage_dialog.rs         # Confirm.hostKey, StorageState::store → stores()
    └── files/
        ├── browser_commands.rs              # stores() statt s3_store()
        ├── storage_source.rs                # is_tree(): rename_tree/remove_tree
        └── transfer/remote.rs               # Upload-Sitzungen, copies_on_provider
src/
├── components/settings/storage/
│   ├── ConnectionForm.vue                   # Protokoll, Voreinstellungen, SFTP-Felder, Schlüssel erzeugen
│   ├── HostKeyConfirm.vue                   # neu: Fingerabdruck bestätigen / neuen annehmen
│   ├── StorageForm.vue                      # „Ordner“, Ordner anlegen
│   └── StorageListView.vue                  # Protokoll, Warnung Hostschlüssel
├── components/extensions/StorageDialog.vue  # Vorschläge webdav/sftp
├── components/StorageCredentialsModal.vue   # Zugangsdaten je Protokoll, Fingerabdruck
├── composables/useStorage.ts
└── i18n/locales/{de,en}.json                # settings.storage.* um Protokolle
scripts/e2e/
├── lib/sftpgo.ts                            # neu: Container, Benutzer, Hostschlüssel zur Laufzeit
└── scenarios/                               # WebDAV- und SFTP-Szenarien
```

**Structure Decision**: Die neuen Protokolle liegen als Unterordner in `remote_storage/` neben der
S3-Umsetzung; gemeinsame Teile (HTTP-Client, Baum-Logik) in eigenen Dateien. Dateibrowser, Transfer und
Erweiterungen ändern sich nur an den Stellen, die R7–R9 nennen.

## Lieferungen

| PR  | Inhalt                                                                                                                                                                                      | Stories            |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ |
| A   | Doku: Spec, Plan, Tasks (#372)                                                                                                                                                              | —                  |
| B   | Umbau ohne neues Verhalten: `Protocol`, `ProtocolStore` (nur S3), Upload-Sitzungen, `Credentials` als `enum`, `http.rs`, Migration, `ObjectInfo.etag`, neue Trait-Methoden mit S3-Umsetzung | Grundlage (FR-018) |
| C   | WebDAV-Store, Einstellungen mit Voreinstellungen, Test, Ordner anlegen                                                                                                                      | US1                |
| D   | WebDAV im Dateibrowser, Transfers und für Agents (Baum-Wege, Zwischendateien)                                                                                                               | US2                |
| E   | Erweiterungen: Gleichstand, Liste, Vorschläge `webdav`; PR im vault-sdk                                                                                                                     | US3, US6 (WebDAV)  |
| F   | SFTP-Store, Schlüssel, Hostschlüssel, Einstellungen                                                                                                                                         | US4                |
| G   | SFTP im Dateibrowser, für Agents und Erweiterungen; Vorschläge `sftp`                                                                                                                       | US5, US6 (SFTP)    |
| H   | Verschlüsselte Ordner auf WebDAV und SFTP, sobald Spec 048 gebaut ist                                                                                                                       | US7                |

E2E-Szenarien kommen mit dem PR ihrer Story; SFTPGo mit PR C.

## Abstimmung mit Spec 048

Spec 048 wird parallel gebaut (Worktrees `048-b0`, `048-format`) und berührt `files/browser_commands.rs`
(Aufteilung in PR B0) und `RemoteStore` (ETag). PR B dieser Spec ändert in `browser_commands.rs` nur den
Aufruf `s3_store()` und führt `ObjectInfo.etag` ein; wer zuerst landet, gibt die Form vor, die andere
Spec zieht nach. Die Migration nimmt die nächste freie Nummer bei der Umsetzung.

## Complexity Tracking

Keine Verletzung der Constitution.
