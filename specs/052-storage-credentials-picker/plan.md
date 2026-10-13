# Implementation Plan: Zugangsdaten der Cloud-Speicher aus dem Passwortmanager wählen

**Branch**: `052-storage-credentials-picker` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/052-storage-credentials-picker/spec.md`

## Summary

Eine Speicherverbindung verweist schon heute über `credentials_item_id` auf einen Eintrag des
Passwortmanagers; neu ist, dass dieser Eintrag auch ein vorhandener des Nutzers sein kann, und dass die
von holzi angelegten Einträge ein Tag tragen. Es gibt **keine Schemaänderung** und keine neue
Abhängigkeit.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Wahl als Eingabeform** (R1): `CredentialsInput` bekommt die Form `entry { itemId }`. Sie gilt im
  Verbindungsformular und im Fenster für Speichervorschläge von Erweiterungen (`StorageAnswer.credentials`)
  gleichermaßen; eine Erweiterung kann sie nicht senden, weil nur holzis Fenster `storage_dialog_resolve`
  aufruft.
- **Eine Zuordnungsregel** (R2): Aus dem Eintrag werden die Zugangsdaten an einer einzigen Stelle
  gebildet, `credentials::from_item`, herausgelöst aus `credentials::read`. Lesen, „passt der Eintrag?“
  und die Auswahlliste nutzen sie.
- **Auswahlliste in Rust** (R3): neuer Command `storage_credentials_candidates(protocol)` liefert nur
  Metadaten (Titel, Benutzername, Adresse, Tag, Eigentümer) der passenden Einträge; die Suche läuft im
  Fenster über diese Liste.
- **Tag** (R4): `haex-cloud-storage`. Neue Einträge tragen es von Anfang an; beim Öffnen der Vault holt
  holzi es für vorhandene eigene Einträge nach; ein gewählter Eintrag bekommt es beim Verknüpfen, geschrieben
  als `Caller::User`, weil der Nutzer ihn im Fenster von holzi gewählt hat (beantwortet die offene Frage
  der Checkliste: `Internal` bekommt keine Schreibrechte an fremden Einträgen).
- **Zustand prüft die Passung** (R5): „neue Zugangsdaten nötig“ gilt auch, wenn der Eintrag da ist, aber
  Werte fehlen (FR-010); `state_in` bekommt dafür das Protokoll.
- **Nichts Neues beim Löschen** (R6): `credentials::delete` löscht nur Einträge mit Eigentümer `storage`;
  ein gewählter Eintrag bleibt dadurch beim Entfernen und beim Austauschen immer erhalten (FR-012). Die
  Warnung vor dem Löschen (034 FR-034) meldet `StorageUsage` für jeden Eintrag, auf den eine Verbindung
  zeigt.
- **Oberfläche** (R7): neue `CredentialsEntryPicker.vue`, eingebunden im Formular und im Fenster; das
  Formular nennt den Eintrag und öffnet ihn über `openApp('system.passwords', '/entry/<id>')`.

## Technical Context

**Language/Version**: Rust (Tauri 2.12, Edition wie `src-tauri/Cargo.toml`); TypeScript 6 (strict),
Vue 3.5, Nuxt 4.5 (SPA)

**Primary Dependencies**: keine neuen; vorhanden `haex-crdt`, `zeroize`, `ts-rs`, haex-ui
(Group/Row/OptionRow), `vue-i18n`

**Storage**: Vault-Datenbank über haex-crdt; keine neue Spalte und keine neue Tabelle; Verknüpfung =
`haex_storage_connections.credentials_item_id`, Tag = `haex_passwords_tags` und `haex_passwords_item_tags`
([data-model.md](./data-model.md))

**Testing**: `cargo test` gezielt (nur die berührten Ziele, `-j 4`; Speicher-Tests, `passwords_owner`,
`passwords_access`), Skripte `scripts/check-*.ts` für reine TypeScript-Helfer, neues
E2E-Szenario `storage-credentials-entry` (tauri-driver, Docker für den WebDAV-Server), vorhandene
Speicher-Szenarien laufen unverändert weiter

**Target Platform**: Desktop (Linux, macOS, Windows) und Android/iOS; dieselbe Oberfläche (FR-015)

**Project Type**: Desktop-/Mobile-App (Tauri + Nuxt SPA)

**Performance Goals**: Auswahlliste in unter 300 ms bei 5 000 Einträgen im Passwortmanager (Vorfilter
über Kopfdaten, Geheimnisse nur für Einträge, die den Vorfilter bestehen)

**Constraints**: kein Geheimnis in Antworten, Logs oder Fehlern (SC-005); `Zeroizing` für alle Werte in
Rust; neue Logik nicht in `service.rs` (624 Zeilen, über der 500-Zeilen-Grenze), sondern in
`service_link.rs`; `ConnectionForm.vue` (476 Zeilen) wächst nicht

**Scale/Scope**: einige Verbindungen je Vault, bis einige tausend Einträge im Passwortmanager

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Prinzip / MUST                                          | Stand                                                                                                                                                             |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Änderungen im eigenen Worktree auf Themen-Branch        | erfüllt (`.worktrees/052-storage-credentials-picker`)                                                                                                             |
| Graphify vor dem Anlegen benannter Artefakte            | erfüllt, siehe unten „Geprüfte Kandidaten“                                                                                                                        |
| Tests in eigenen Dateien                                | geplant: `credentials_tests.rs`, `service_link_tests.rs`, `scripts/check-*.ts`, E2E-Szenario                                                                      |
| 500-Zeilen-Grenze                                       | `service.rs` bekommt nur Aufrufe; neue Logik in `service_link.rs`; Formular-Helfer ausgelagert                                                                    |
| Eine Umsetzung je Regel                                 | Feldzuordnung nur in `credentials::from_item`; Tagname nur in `credentials::TAG`; Frontend nutzt die Rust-Liste                                                   |
| Keine neue Abhängigkeit, keine spekulative Abstraktion  | erfüllt; Wahl ist eine Variante von `CredentialsInput`                                                                                                            |
| Fehler nicht verschlucken, kein `unwrap` auf Eingaben   | Tag-Fehler beim Verknüpfen wird mit Namen protokolliert (ohne Wert), die Verknüpfung gilt trotzdem                                                                |
| ADR bei Entscheidungen, die ein Kernprinzip berühren    | nicht nötig: kein Kernprinzip der Verfassung wird berührt; Z14 (034) bleibt unverändert. Die Abweichung von 038 FR-005 für gewählte Einträge steht in 038 und 050 |
| Phasen                                                  | Passwortmanager (034) und Speicher (038, 050) sind im Einsatz                                                                                                     |
| Conventional Commits, kein Squash, keine Agent-Spuren   | gilt für die Umsetzung                                                                                                                                            |
| Keine Geheimnisse und keine lokalen Pfade im Repository | erfüllt; Testwerte sind Wegwerfwerte                                                                                                                              |

Nach dem Entwurf (Phase 1) erneut geprüft: keine Verletzung, kein Eintrag unter „Complexity Tracking“.

### Geprüfte Kandidaten (graphify)

| Kandidat                                                          | Ergebnis                                                                                                                                                  |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `credentials::read` (Zuordnung Feld → Wert)                       | wird erweitert: Zuordnung als `from_item` herausgelöst, `read` ruft sie                                                                                   |
| `tags::bulk_set` / `PasswordsService::set_tags`                   | wiederverwendet für gewählte Einträge (`Caller::User`) und für das Nachtragen                                                                             |
| `ItemInput.tags` in `create_item`                                 | wiederverwendet für neue Einträge                                                                                                                         |
| `items::headers_in_scope`                                         | wiederverwendet für den Vorfilter der Liste                                                                                                               |
| `StorageUsage` / `store::credentials_in_use`                      | unverändert; warnt schon für jeden verknüpften Eintrag                                                                                                    |
| `delete_credentials_if_unused`, `credentials::delete`             | unverändert; löschen nur eigene Einträge                                                                                                                  |
| `StorageCredentialsModal.typed()` und Formular-Aufbau der Eingabe | gleiche Aufgabe an zwei Stellen: ein Helfer `credentialsInput.ts`, falls die Felder sich decken, sonst nur der Zweig `entry` (Entscheidung beim Umsetzen) |
| `useWmTab().openApp`                                              | wiederverwendet für „Eintrag öffnen“                                                                                                                      |

## Project Structure

### Documentation (this feature)

```text
specs/052-storage-credentials-picker/
├── plan.md              # Dieser Plan
├── research.md          # Phase 0
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   └── tauri-commands.md
└── tasks.md             # Phase 2 ($speckit-tasks, nicht Teil dieses Befehls)
```

### Source Code (repository root)

```text
src-tauri/src/
├── remote_storage/
│   ├── credentials.rs           # TAG, from_item, state_in(protocol), candidates, tag, start_after_open
│   ├── credentials_tests.rs     # Zuordnung, Passung, Tag, Löschen lässt gewählte Einträge
│   ├── model.rs                 # CredentialsInput::Entry, CredentialsCandidate, CredentialsEntry, ConnectionView.credentials_entry
│   ├── service_link.rs          # NEU: Eingabe auflösen (Entry), verknüpfen, taggen
│   ├── service_link_tests.rs    # NEU: Anlegen/Tauschen/Entfernen mit gewähltem Eintrag
│   ├── service.rs               # ruft service_link; view() trägt den Eintrag
│   ├── service_host_key.rs      # state-Aufruf mit Protokoll
│   └── commands.rs              # storage_credentials_candidates
├── passwords/                   # unverändert (Regeln, Tags)
├── instances/open.rs, import.rs # remote_storage::credentials::start_after_open
└── lib.rs                       # Command registriert
src/
├── components/settings/storage/
│   ├── CredentialsEntryPicker.vue   # NEU: Liste, Suche, Auswahl
│   ├── credentialsInput.ts          # NEU: Eingabe aus dem Entwurf (rein)
│   ├── ConnectionForm.vue           # Quelle wählen, Eintrag nennen, „Eintrag öffnen“
│   └── CredentialsFields.vue        # unverändert bis auf den Entwurf (source, entryId)
├── components/StorageCredentialsModal.vue   # gleiche Auswahl (US4)
├── i18n/locales/{de,en}.json
└── types/bindings/              # ts-rs: CredentialsInput, CredentialsCandidate, CredentialsEntry, ConnectionView
scripts/
├── check-storage-credentials.ts     # NEU: credentialsInput.ts
└── e2e/scenarios/storage-credentials-entry.test.ts   # NEU
specs/038-storage-connections/spec.md, specs/050-webdav-sftp-storage/spec.md   # Verweis auf diese Spec
```

**Structure Decision**: bestehende Aufteilung beibehalten (Rust-Modul `remote_storage`, Einstellungs-
Komponenten unter `settings/storage`); zwei neue Rust-Dateien nach dem Muster von
`service_credentials.rs` und `service_host_key.rs`.

## Complexity Tracking

Keine Verletzungen.
