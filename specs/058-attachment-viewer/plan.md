# Implementation Plan: Dateianhänge im Passwortmanager anzeigen und öffnen

**Branch**: `058-attachment-viewer` | **Date**: 2026-10-23 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/058-attachment-viewer/spec.md`

## Summary

Text- und PDF-Anhänge öffnen beim Tippen in einer Anhang-Ansicht über dem Eintrag. Jeder Anhang
lässt sich auf dem Desktop mit der System-App öffnen, nach dem Hinweis aus 048. Es gibt **keine
Schemaänderung und keine neue Abhängigkeit**. Was der Datei-Viewer aus 044 schon kann, wird
wiederverwendet, nicht nachgebaut.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Eine Regel für die Art** (R1): `fileKind` nimmt Text und PDF aus `viewerKind`; die eigene Liste
  `TEXT_EXTENSIONS` entfällt. Bilder bleiben bei `imageMime`, weil nur diese Formate eine Vorschau
  bekommen.
- **Text** (R2): `passwords_attachment_text` liest die Bytes aus der Vault und gibt sie an
  `files::local::text::text_from` (5 MB, Binärerkennung, Ersatzzeichen wie in 044).
- **PDF** (R3): `passwords_attachment_open` meldet die Bytes als neue `BytesSource` beim Media-Server
  an; das Fenster zeigt die URL mit dem unveränderten `LazyFilesPdfViewer` und gibt sie beim Schließen
  frei.
- **System-App** (R4): `passwords_attachment_open_system` mit `needsNotice`, Bestätigung als
  Geräte-Einstellung `passwords.attachmentOpenAck`, Kopie im Ordner `files-opened-encrypted` aus 048
  (gleiche Aufräumregeln), sofort gelöscht, wenn das Öffnen scheitert. Dateiname der Kopie auf jedem
  System gültig (`copy_file_name`).
- **Oberfläche** (R5): `FilesTextContent.vue` aus `Viewer.vue` herausgelöst und in beiden Viewern
  genutzt; neue `PasswordsAttachmentViewer.vue`; `OpenSystemNotice.vue` unverändert wiederverwendet;
  Karte und Anhangliste bekommen Ansicht und „Mit System-App öffnen“.

## Technical Context

**Language/Version**: Rust (Tauri 2, Edition wie `src-tauri/Cargo.toml`); TypeScript (strict),
Vue 3, Nuxt 4 (SPA)

**Primary Dependencies**: keine neuen; vorhanden `tauri-plugin-opener`, `pdfjs-dist` (über
`lib/files/pdf.ts`), `ts-rs`, haex-ui, `vue-i18n`

**Storage**: Vault-Datenbank über haex-crdt; keine neue Spalte oder Tabelle. Bestätigung in
`preferences` (Geräte-Scope); Kopien als Dateien unter `<AppCache>/files-opened-encrypted/`
([data-model.md](./data-model.md))

**Testing**: gezielte `cargo test -j 4` über `scripts/with-nix-host-bridge.sh`
(`passwords::attachment_view`, `files::streaming`); `scripts/check-passwords-format.ts`,
`scripts/check-files-viewer.ts`; E2E-Szenario `passwords-attachments` erweitert (tauri-driver)

**Target Platform**: Desktop (Linux, macOS, Windows) und Android/iOS. Die Anhang-Ansicht gibt es
überall; „Mit System-App öffnen“ nur auf dem Desktop (043, 044 R11)

**Project Type**: Desktop-/Mobile-App (Tauri + Nuxt SPA)

**Performance Goals**: SC-002 – Text bis 1 MB und erste PDF-Seite bis 10 MB in unter 1 s; die Bytes
kommen aus der lokalen Datenbank, ein Lesen von 25 MiB liegt deutlich darunter

**Constraints**: nichts auf dem Gerät außer der Kopie für die System-App (FR-006); kein Inhalt und
kein Pfad in Logs oder Fehlern (FR-013); nur `Caller::User` (FR-014); `binaries.rs` (382 Zeilen)
wächst nicht über 500, deshalb neue Logik im eigenen Modul `passwords/attachment_view.rs`;
`Viewer.vue` (271) wird kleiner; `AttachmentCard.vue` (221) und `Attachments.vue` (198) bleiben
unter 300

**Scale/Scope**: einige Anhänge je Eintrag, höchstens 25 MiB je Anhang

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Prinzip / MUST                                        | Stand                                                                                                                                       |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Änderungen im eigenen Worktree auf Themen-Branch      | erfüllt (`.worktrees/058-attachment-viewer`, Branch `058-attachment-viewer`)                                                                |
| speckit-Ablauf mit Review-Gates                       | Spec vom Betreiber freigegeben (2026-10-23); dieser Plan wartet auf das Review                                                              |
| Graphify vor dem Anlegen benannter Artefakte          | erfüllt, siehe „Geprüfte Kandidaten“; ein Vorschlag zur Erweiterung wartet auf Zustimmung (`Fixture`)                                       |
| Kleinste korrekte Umsetzung, keine neue Abhängigkeit  | erfüllt; vorhandener Text-Leser, Media-Server, PDF-Viewer, Hinweis und Aufräumregeln                                                        |
| Eine Umsetzung je Regel                               | Textendungen nur in `viewer_kind`/`viewerKind`; 5-MB-Grenze und Binärerkennung nur in `text_from`; Aufräumen der Kopien nur in `FilesState` |
| Async nicht blockieren                                | Datei schreiben und Ordner löschen über `tokio::fs` bzw. `spawn_blocking`, wie in 048                                                       |
| `Result` mit Kontext, Fehler nicht verschlucken       | Fehler von `open_path` wird ohne Pfad gemeldet und die Kopie entfernt; das Ergebnis des Entfernens wird protokolliert (ohne Namen)          |
| Tests in eigenen Dateien, Randfälle und Fehlerwege    | `attachment_view_tests.rs`, `streaming_tests.rs`, `check-passwords-format.ts`, E2E                                                          |
| Jedes Gerät und Betriebssystem                        | Dateiname der Kopie mit den Regeln von Windows (`copy_file_name`); Android ohne System-App wie in 044                                       |
| Keine Geheimnisse, keine lokalen Pfade im Repository  | erfüllt; Testdateien entstehen im Test                                                                                                      |
| Conventional Commits, kein Squash, keine Agent-Spuren | gilt für die Umsetzung                                                                                                                      |

Nach dem Entwurf (Phase 1) erneut geprüft: keine Verletzung, kein Eintrag unter „Complexity Tracking“.

### Geprüfte Kandidaten (graphify)

| Kandidat                                                            | Ergebnis                                                                                                                                                                    |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `files::local::text::text_from` (text.rs:41)                        | wiederverwendet für Text                                                                                                                                                    |
| `files::kind::viewer_kind` / `viewerKind.ts`                        | wiederverwendet für die Art; ersetzt `TEXT_EXTENSIONS`                                                                                                                      |
| `binaries::preview_bytes` (binaries.rs:253)                         | unverändert; bleibt nur für Bilder (Lightbox, Vorschaubilder)                                                                                                               |
| `binaries::attachment_data`                                         | wiederverwendet; einziger Lesezugriff                                                                                                                                       |
| `MediaServer::register` / `files_release`                           | wiederverwendet für PDFs                                                                                                                                                    |
| `platform_probe::Fixture` (platform_probe.rs:170)                   | **nahezu gleich** wie `BytesSource` (statische statt geteilte Bytes). Vorschlag unten                                                                                       |
| `LocalFileSource`, `StorageFileSource`, `EncryptedFileSource`       | passen nicht: lesen aus Datei, S3 oder verschlüsseltem Objekt                                                                                                               |
| `encrypted::opened::copy_for_app`                                   | passt nicht: liest aus einem verschlüsselten Ordner und prüft dessen Bestätigung; der Ort (`opened_encrypted`) und `sweep` werden wiederverwendet                           |
| `FilesState::forget_on_close`, `FilesState::from_app`               | wiederverwendet; räumen die Kopien der Anhänge mit auf                                                                                                                      |
| `binaries::sanitize_file_name` (binaries.rs:42)                     | wiederverwendet als erster Schritt von `copy_file_name`; selbst unverändert                                                                                                 |
| `files::local::edit::FORBIDDEN` (edit.rs:17), `check_name`          | `FORBIDDEN` wiederverwendet (`pub(crate)`); `check_name` lehnt nur ab, passt nicht                                                                                          |
| `storage::preferences::get`/`insert_or_update`, `PrefScope::Device` | wiederverwendet für die Bestätigung                                                                                                                                         |
| `Viewer.vue` Textblock (Zeilen 192–203)                             | herausgelöst als `FilesTextContent.vue`, in beiden Viewern genutzt                                                                                                          |
| `PdfViewer.vue`                                                     | unverändert wiederverwendet                                                                                                                                                 |
| `OpenSystemNotice.vue`, `useFilesOpenSystem.ts`                     | Hinweis unverändert wiederverwendet; der Composable hängt an `SourceRef`/`useFiles` und passt nicht, Anhänge bekommen den gleichen Ablauf in `Attachments.vue` (~15 Zeilen) |

**Vorschlag zur Erweiterung (wartet auf Zustimmung)**: `platform_probe::Fixture` hält
`&'static [u8]` und setzt `StreamingSource` selbst um (~25 Zeilen). Nach dem Plan baut
`platform_probe` seine Quellen mit `BytesSource`, und `struct Fixture` samt
`impl StreamingSource for Fixture` entfällt. Der Aufruf in `serve_fixtures` (platform_probe.rs:201)
wird zu:

```rust
let url = media.register(
    "platform-probe",
    Arc::new(BytesSource::new(Arc::from(*bytes), *content_type)),
);
```

Kosten: einmal je Probe-Start eine Kopie der Fixture-Bytes (wenige KiB) statt der statischen
Referenz.

Ohne Zustimmung bleibt `Fixture` unverändert; `BytesSource` entsteht in jedem Fall.

## Project Structure

### Documentation (this feature)

```text
specs/058-attachment-viewer/
├── plan.md              # Dieser Plan
├── research.md          # Phase 0
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   └── tauri-commands.md
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 ($speckit-tasks)
```

### Source Code (repository root)

```text
src-tauri/src/
├── passwords/
│   ├── attachment_view.rs          # neu: Text, PDF-Quelle, Kopie, copy_file_name
│   ├── attachment_view_tests.rs    # neu
│   ├── mod.rs                      # Modul eintragen
│   ├── service/attachments.rs      # drei Service-Methoden (require_user)
│   └── commands/attachments.rs     # drei Commands, Args, OpenSystemOutcome
├── files/
│   ├── streaming.rs                # BytesSource
│   ├── streaming_tests.rs          # neu (oder vorhandene Testdatei)
│   └── local/edit.rs               # FORBIDDEN pub(crate)
├── platform_probe.rs               # nur mit Zustimmung: Fixture über BytesSource
└── lib.rs                          # Commands registrieren

src/
├── components/
│   ├── files/
│   │   ├── TextContent.vue         # neu (FilesTextContent), aus Viewer.vue
│   │   └── Viewer.vue              # nutzt FilesTextContent
│   └── passwords/
│       ├── AttachmentViewer.vue    # neu (PasswordsAttachmentViewer)
│       ├── AttachmentCard.vue      # Tipp öffnet Ansicht; Aktion System-App
│       └── Attachments.vue         # Zustand Ansicht und Hinweis
├── composables/
│   ├── usePasswords.ts             # drei Aufrufe
│   └── useErrorString.ts           # Gründe binary, not_text, not_pdf, no_app
├── lib/passwords/format.ts         # fileKind über viewerKind
├── i18n/locales/{de,en}.json       # neue Texte
└── types/bindings/                 # OpenSystemOutcome, AttachmentOpenSystemArgs (ts-rs)

scripts/
├── check-passwords-format.ts       # fileKind-Fälle
└── e2e/scenarios/passwords-attachments.test.ts   # Text und PDF öffnen
```

**Structure Decision**: Die bestehende Aufteilung von holzi (Rust in `src-tauri/src/<Bereich>`,
Fenster in `src/components/<Bereich>`). Neue Rust-Logik liegt in einem eigenen Modul des
Passwortmanagers, weil `binaries.rs` sonst an die 500-Zeilen-Grenze kommt. Die Teile, die die
Dateien-App schon hat, bleiben in `files/` und werden von dort genutzt.

## Umsetzung in Schritten (Vorschlag für tasks.md)

1. **PR A – Text (US1, MVP)**: R1, R2, `FilesTextContent`, `PasswordsAttachmentViewer` mit Text,
   Karte öffnet Text, Tests, E2E-Teil Text.
2. **PR B – PDF (US2)**: `BytesSource`, `passwords_attachment_open`, PDF in der Ansicht, Tests,
   E2E-Teil PDF.
3. **PR C – System-App (US3)**: `copy_file_name`, `passwords_attachment_open_system`, Aktion auf
   Karte und in der Ansicht, Hinweis, Tests; Quickstart M9–M13 von Hand.

Jeder Schritt ist für sich nutzbar; B und C hängen nur von A ab.

## Complexity Tracking

Keine Verletzung der Konstitution; nichts einzutragen.
