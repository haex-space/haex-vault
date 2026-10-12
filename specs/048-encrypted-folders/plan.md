# Implementation Plan: Verschlüsselte Ordner in Speichern

**Branch**: `048-encrypted-folders` | **Date**: 2026-10-10 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/048-encrypted-folders/spec.md`

## Summary

In einem Speicher aus 038 kann der Nutzer im Dateibrowser aus 044 verschlüsselte Ordner anlegen. Alles
darin legt holzi je Datei verschlüsselt ab: zufällige Objektnamen, verschlüsselte Begleitdateien mit
Namen und Struktur, Inhalt in Blöcken, die sich einzeln lesen lassen. Alle Geräte der Vault lesen ohne
Passwort. Agents und Erweiterungen erreichen einen solchen Ordner nur nach einer Frage an den Nutzer.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Schlüssel** (R1): KEK per HKDF aus dem Inhaltsschlüssel der Vault (`sync::content_keys`, auf jedem
  Gerät, mit Generationen); Ordnerschlüssel zufällig, im Kopf verpackt; Dateischlüssel je Fassung.
- **Format `HXEF` v1** (R2 bis R4, [contracts/format.md](./contracts/format.md)): XChaCha20-Poly1305 aus
  vorhandenen Abhängigkeiten, Blöcke zu 64 KiB mit Kopf, Index und „letzter Block“ im AAD;
  Einträge mit Eltern-Kennung (Umbenennen = eine Begleitdatei); Fassungen mit `base` für Konflikte.
  Modell von haex-vault, ohne dessen Lücken.
- **Einbau in `files/`** (R9): neues Modul `files/encrypted/`; Weiche in `StorageFiles` für Pfade durch
  verschlüsselte Ordner; `Side::Encrypted` in Transfers; `EncryptedFileSource` für den Medienserver
  (R5).
- **Ansicht** (R6, R8): verschlüsselte Ordner an der Endung `.hxef/` erkannt, Köpfe und Begleitdateien
  parallel geladen, Zwischenspeicher in zwei `_no_sync`-Tabellen nach ETag.
- **Spuren** (R10): Vorschaubilder nur im Arbeitsspeicher, `Redacted` in Logs, Kopien für System-Apps
  in eigenem Ordner, gelöscht beim Sperren und beim Start.
- **Freigaben** (R12, R13): Agents über `files/access.rs` und `agent_file_permissions` (aus 044) mit
  Reichweite lokal/Cloud; Erweiterungen über eine neue Berechtigungsart `encryptedFolder`, die nie im
  Manifest stehen darf, und neue Funktionen im vault-sdk.

## Technical Context

**Language/Version**: Rust (MSRV 1.95, Edition 2021, Tauri 2); TypeScript (strict), Vue 3.5, Nuxt 4.5
(SPA)

**Primary Dependencies**: keine neuen Crates im Build. Vorhanden: `chacha20poly1305` 0.10, `hkdf` 0.13, `sha2` 0.11,
`zeroize` 1, `getrandom` 0.4 (über `sync::keys::random_bytes`), `base64` 0.23, `rusty-s3`, `tokio`,
`serde_json`. Für Base32 wird `data-encoding` direkte Abhängigkeit; es steht schon transitiv im
`Cargo.lock`, also ohne neuen Code im Build. Im Fenster: haex-ui (Dialoge, Icons).

**Storage**: Bucket als Wahrheit ([contracts/format.md](./contracts/format.md)); Vault-Datenbank:
`encrypted_folder_heads_no_sync`, `encrypted_folder_entries_no_sync`, Erweiterung von
`agent_file_permissions` und der Erweiterungs-Berechtigungen ([data-model.md](./data-model.md))

**Testing**: `cargo test` mit Testvektoren und `FakeStore` (Tests in `*_tests.rs`), `pnpm typecheck`,
`pnpm lint`, `pnpm format:check`, E2E gegen RustFS (`files-encrypted`, `files-encrypted-two-devices`,
`files-encrypted-agent`, `extension-encrypted-folder`), Testvektoren in der Plattform-Probe der CI
(Windows, macOS, Android); von Hand nur SC-004 mit einem 4-GB-Video ([quickstart.md](./quickstart.md))

**Target Platform**: Linux, Windows, macOS, Android (iOS ohne eigenen Aufwand)

**Project Type**: Desktop- und Mobil-App (Tauri) mit eingebautem Agent und Erweiterungen

**Performance Goals**: SC-003 (1 000 Einträge < 5 s beim ersten, < 1 s beim nächsten Öffnen; 10 000
< 30 s; 20 Köpfe < 2 s), SC-004 (Video-Start und Sprung < 5 s, < 100 MB mehr Speicher), SC-005 (≤ 20 %
langsamer, ≤ 1 % größer), SC-006 (Umbenennen von 1 000 Dateien < 30 s, 0 Inhaltsobjekte)

**Constraints**: kein Klartext beim Anbieter und auf dem Gerät außerhalb der verschlüsselten Vault-Daten
(FR-010, FR-026); nie ganze Dateien in den Arbeitsspeicher (FR-015); keine Namen in Logs (FR-027);
kein aws-lc-rs; keine Netzdienste in `cargo test`; 500 Zeilen je Datei

**Scale/Scope**: verschlüsselte Ordner bis 10 000 Einträge in den Zielen, technisch bis 100 000
(`MAX_ENTRIES` aus 044); Dateien bis mehrere GB

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                              | Stand                                                                                                                                                                                             |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git                        | ✅ Testvektoren nutzen feste Testschlüssel ohne Bezug zu echten Daten; sie sind kein Schlüsselmaterial einer Identität                                                                            |
| II. Keine lokalen absoluten Pfade in Konfiguration | ✅ Pfade nur zur Laufzeit                                                                                                                                                                         |
| III. Projektidentität geräteunabhängig             | ✅ nicht berührt                                                                                                                                                                                  |
| IV. Fremde Inhalte mit unveränderlicher Revision   | ✅ haex-vault mit `8dce379d94e18fcd42c3b73686a06f984ca3f574` zitiert                                                                                                                              |
| V. Externe Quellen nur per Allowlist               | ✅ keine neuen Harness-Quellen                                                                                                                                                                    |
| VI. Selbständernde Anweisungen nur mit Review      | ✅ keine Änderung an Constitution oder Skills; die neue Berechtigungsart betrifft Erweiterungen, nicht Agent-Anweisungen                                                                          |
| VII. Relay blockiert lokale Arbeit nie             | ✅ verschlüsselte Ordner brauchen nur ihren Anbieter; ein Gerät ohne Inhaltsschlüssel (vor dem ersten Sync) bekommt `noKey`                                                                       |
| VIII. Keine Verschleierung                         | ✅ Verbergen richtet sich gegen den Anbieter, nicht gegen den Nutzer; Agents sehen ohne Freigabe nur, dass ein Ordner existiert, und können fragen                                                |
| Phasenfolge                                        | ✅ 038 und 044 (Speicher als Quelle) sind gebaut und im Einsatz; die Agent-Lieferung wartet auf 044 PR G, externe Agents auf 021                                                                  |
| Topic-Branch, Speckit-Ablauf                       | ✅ Branch `048-encrypted-folders`; specify → clarify → plan; danach tasks, analyze, implement in einem Worktree je PR                                                                             |
| Tests in eigenen Dateien                           | ✅ jede neue Rust-Datei mit `*_tests.rs` daneben; Testvektoren als Fixture                                                                                                                        |
| graphify vor neuen Artefakten                      | ✅ Recherche über graphify; vorhandene Bausteine (`content_keys`, `random_bytes`, `StreamingSource`, `PermissionRequestDialog`, `files/access.rs`, `FakeStore`) werden erweitert statt dupliziert |
| Eine Umsetzung je Regel                            | ✅ ein Format-Modul für Dateibrowser, Agents und Erweiterungen; Sync-Regeln und Umbau 027/029 nutzen es später (FR-041)                                                                           |
| 500 Zeilen je Datei                                | ⚠️ `browser_commands.rs` hat 817 Zeilen; Teilen ist Lieferung B0, bevor 048 dort ergänzt                                                                                                          |
| ADR bei Prinzip-relevanten Entscheidungen          | ✅ ADR 0013 „Format verschlüsselter Ordner“ im Docs-PR, weil Sync-Regeln und Spaces darauf bauen                                                                                                  |
| Keine Agent-Attribution in Commits                 | ✅                                                                                                                                                                                                |

Nach dem Entwurf erneut geprüft: keine Verstöße; die Überlänge von `browser_commands.rs` ist Altlast aus
044 und wird vor der Umsetzung behoben.

## Project Structure

### Documentation (this feature)

```text
specs/048-encrypted-folders/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── format.md           # Format HXEF v1 (zieht nach docs/formats/ um)
│   ├── tauri-commands.md   # Änderungen an den Commands des Dateibrowsers
│   ├── agent-actions.md    # Änderungen an den files.*-Aktionen
│   └── bridge.md           # Funktionen für Erweiterungen, vault-sdk
├── checklists/requirements.md
└── tasks.md                # /speckit-tasks
docs/adr/0013-encrypted-folder-format.md
docs/formats/encrypted-folder-v1.md        # ab PR B
```

### Source Code (repository root)

```text
src-tauri/src/
├── files/
│   ├── encrypted/                   # neu
│   │   ├── mod.rs                   # EncryptedFolders: geöffnete Ordner, Schlüssel aus content_keys
│   │   ├── format/                  # rein, ohne E/A: keys.rs, head.rs, content.rs, sidecar.rs, base32.rs + _tests
│   │   ├── tree.rs                  # Ansicht aus Begleitdateien, Fassungen, Konflikte, „Wiederhergestellt“ + _tests
│   │   ├── load.rs                  # paralleles Laden, Zwischenspeicher nach ETag + _tests
│   │   ├── ops.rs                   # anlegen, umbenennen, verschieben, löschen, kopieren + _tests
│   │   ├── cleanup.rs               # verwaiste Objekte nach 24 h + _tests
│   │   ├── source.rs                # EncryptedFileSource (StreamingSource) + _tests
│   │   ├── transfer.rs              # Ver-/Entschlüsseln in upload/fetch/fill + _tests
│   │   ├── cache.rs                 # Tabellen *_no_sync
│   │   └── redacted.rs              # Pfade für Logs
│   ├── storage_source.rs            # Weiche: Pfade durch .hxef/ → encrypted
│   ├── transfer/remote_plan.rs      # Side::Encrypted, Frage leavesEncryption
│   ├── transfer/remote.rs           # Kopieren beim Anbieter nur im selben Ordner
│   ├── thumbnails.rs                # render aus Bytes; LRU im Arbeitsspeicher
│   ├── access.rs                    # Ziel EncryptedFolder, Reichweite
│   ├── state.rs                     # LRU, Aufräumen beim Sperren
│   └── commands/                    # aus browser_commands.rs geteilt (B0), neue Commands
├── identity/migrations_encrypted_folders.rs   # 00NN
├── extensions/permissions/{model,target,manifest_map}.rs   # Art encryptedFolder, verboten im Manifest
├── extensions/registry/install.rs, extensions/dev.rs       # BundleRejection::ForbiddenPermission
├── extensions/bridge/permissions.rs, prompts.rs            # Freigabe bis Frame-Ende
├── extensions/encrypted_folder.rs   # neue Brückenfunktionen
└── remote_storage/store.rs          # Zwischenspeicher beim Entfernen eines Speichers löschen

src/
├── components/files/                # Schloss, Zustände, Dialog „Neuer verschlüsselter Ordner“, Hinweise
├── components/extensions/PermissionRequestDialog.vue   # Ordnername, Haken ab Werk aus
├── components/extensions/EncryptedFolderChooser.vue    # Auswahl für extension_encrypted_folder_choose
├── components/settings/…            # Freigaben der Agents und Erweiterungen für Ordner
└── i18n/locales/{de,en}.json

src-tauri/tests/fixtures/encrypted/v1-vectors.json
scripts/e2e/scenarios/files-encrypted*.test.ts, extension-encrypted-folder.test.ts
~/Projekte/vault-sdk                 # eigener PR: extension_encrypted_folder_*
```

**Structure Decision**: Die Verschlüsselung liegt unter `files/encrypted/`, weil der Dateibrowser ihr
erster Nutzer ist und Agents und Erweiterungen über `files` gehen. Das Format ist ein reines Untermodul
ohne Ein- und Ausgabe, damit Sync-Regeln und Spaces es ohne den Dateibrowser nutzen können (FR-041).

## Lieferungen

Jeder PR ist für sich prüfbar und lauffähig.

1. **PR A (Docs)**: Spec, Plan, Research, Data Model, Contracts, Quickstart, Tasks, ADR 0013, Zeile in
   `plans/README.md`.
2. **PR B0 (Refactor)**: `browser_commands.rs` in `files/commands/` teilen, ohne Verhaltensänderung.
3. **PR B (Format)**: `files/encrypted/format/`, Testvektoren, `docs/formats/encrypted-folder-v1.md`,
   Vektoren in der Plattform-Probe der CI.
4. **PR C (US1, US2)**: Migration, Erkennen und Anlegen, Ansicht mit Zwischenspeicher, Lesen und
   Schreiben über Transfers, Vorschaubilder im Arbeitsspeicher, `Redacted`, Fenster (Schloss, Zustände,
   Hinweis); E2E `files-encrypted` Teile 1 bis 3.
5. **PR D (US4)**: `EncryptedFileSource`, Medien und PDF; E2E-Teil Video.
6. **PR E (US5)**: Umbenennen, Verschieben, Kopieren, Löschen, Konflikte, Aufräumen, Frage beim
   Herauskopieren, „Mit System-App öffnen“ mit Hinweis und Aufräumen; E2E-Teil Verwalten.
7. **PR F (US3)**: E2E `files-encrypted-two-devices` (Rig 033), Ordner einer anderen Vault.
8. **PR G (US6, Agents)**: nach 044 PR G; `access.rs`, Spalte `reach`, Frage mit Modell, gehaltene
   Freigaben je Turn, Einstellungen; E2E `files-encrypted-agent`.
9. **PR H (US6, Erweiterungen)**: vault-sdk-PR, Berechtigungsart, Ablehnung im Manifest,
   Brückenfunktionen, Auswahl-Dialog, Freigabe bis Frame-Ende, Einstellungen; E2E
   `extension-encrypted-folder`.

**Nachtrag 2026-10-12, User Story 7 (verschlüsselte Ordner auf dem Gerät; research R15 bis R19):**

10. **PR I (Lokale Anbindung)**: `LocalStore` mit `Protocol::Local` hinter `ProtocolStore`, Regel für
    fremde Dateien im Laden, Aufräumen und Löschen (auch im Speicher), Formatdokument. Nach Absprache mit
    der Sitzung, die Spec 050 baut, weil beide `remote_storage/protocol.rs` und `mod.rs` ändern.
11. **PR J (US7)**: Weiche im Dateibrowser des Geräts (R17): Erkennen beim Auflisten, Anlegen, Öffnen,
    Medien, Vorschaubilder, Transfers, endgültiges Löschen mit Rückfrage, Suche, Beobachten,
    „Mit System-App öffnen“, Hinweis auf fremde Dateien; E2E `files-encrypted-device`.
12. **PR K (US7, Agents und Erweiterungen)**: rohe Dateien gesperrt, Freigaben je Ordner auf dem Gerät
    (R18); nach PR G (#389).
13. **PR L (Umbenennen)**: `RemoteStore` → `ObjectStore` in einem rein mechanischen PR, nach dem Merge
    von Spec 050.

## Complexity Tracking

| Abweichung                                               | Warum nötig                                                       | Einfachere Alternative verworfen, weil                                         |
| -------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Eigenes Format statt Übernahme von haex-vault            | Kürzen, Umbiegen und Wiederverwenden der Schlüssel unerkannt (R3) | Nachbessern ändert das Format ohnehin; Kompatibilität bringt keinen Nutzer mit |
| Zwei Zwischenspeicher-Tabellen neben dem Bucket          | SC-003 beim zweiten Öffnen < 1 s                                  | Jedes Mal alle Begleitdateien laden dauert bei 10 000 Einträgen ~16 s          |
| Neue Berechtigungsart, die nicht im Manifest stehen darf | Betreiber: Freigabe nur durch den Nutzer (FR-037)                 | Ziel unter `remoteStorage` wäre über das Manifest erteilbar                    |
