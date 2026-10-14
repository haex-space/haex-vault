# Implementation Plan: Dateibrowser und Viewer

**Branch**: `044-file-browser` | **Date**: 2026-10-07 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/044-file-browser/spec.md`

## Summary

holzi bekommt einen eingebauten Dateibrowser für das ganze Dateisystem des Geräts und die S3-Speicher
aus Spec 038: ansehen, abspielen, verwalten, suchen. Agents lesen das Gerät mit der Berechtigung
„Dateien des Geräts“, Speicher nur mit Berechtigung je Speicher; Schreiben läuft zusätzlich über die
Freigabestufe; die eigenen Daten von holzi bleiben für Agents gesperrt. Grundlage ist das Design
[`docs/plans/2026-10-07-file-browser-design.md`](../../docs/plans/2026-10-07-file-browser-design.md).

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Kern in Rust** (R3): neues Modul `files/` mit `FilesService` (Aufrufer aus dem Eingang wie
  ADR 0007), Quellen `Device` und `Storage` hinter einem Trait. Pfadauflösung, Sperrliste der eigenen
  Orte und Beobachtung ziehen aus `extensions/fs/` in `files/local/`; Erweiterungen nutzen sie weiter.
- **Medien** (R4): lokaler HTTP-Server auf 127.0.0.1 mit Tokens nach haex-vault, Range, flacher
  Speicher, Tokens je Tab; CSP und Android-Cleartext nur für 127.0.0.1.
- **S3** (R5): `ObjectStore` lernt `head`, Range, gestreamtes Lesen, Ordner-Listen, Multipart und
  Kopieren.
- **Suche** (R6): `walkdir` mit `same_file_system`, ohne Links, `frizbee` für Tippfehler, Treffer über
  einen `Channel`, abbrechbar.
- **Vorschaubilder** (R7): `image` + `fast_image_resize` in Rust, Cache im Cache-Verzeichnis von holzi.
- **Agents** (R1, R2, R8, R14): Definitionen im TS-Katalog mit `runner: 'native'`, Ausführung in Rust
  über `NativeActionTool` (ADR 0011); Text aus PDF (`pdf-extract`), xlsx/ods (`calamine`) und docx/odt
  (ZIP + XML); Bilder als Bildblöcke im `tool_result`, nur für Modelle, die Bilder annehmen; neue Tabelle
  `agent_file_permissions`.
- **Android** (R11): `MANAGE_EXTERNAL_STORAGE` mit zwei Befehlen im Plugin `holzi-android`; Beobachten wie
  auf dem Desktop (`notify` über inotify), dazu Neuladen beim Zurückkehren.

## Technical Context

**Language/Version**: Rust (MSRV 1.95, Edition 2021, Tauri 2); TypeScript (strict), Vue 3.5, Nuxt 4.5
(SPA); Kotlin im Plugin `holzi-android`

**Primary Dependencies**: neu in Rust: `walkdir` 2.5, `frizbee` 0.13, `image` 0.25 (nur jpeg/png/webp/gif),
`fast_image_resize` 6.1, `pdf-extract` 0.12, `calamine` 0.36, `zip` 8, `quick-xml` 0.41, `trash` 5.2
(nicht Android, nicht iOS). Vorhanden: `tokio` (net, fs, io-util), `rusty-s3`, `reqwest`, `notify-debouncer-full`
(bisher nur Desktop; für Android mit eingebunden, R11), `uuid`, `sha2`, `tempfile`. Neu im Fenster: `pdfjs-dist` 6.4. Vorhanden: `photoswipe`,
`@vueuse/core` (`useVirtualList`), haex-ui (Shadcn-Kontextmenü, Dialoge).

**Storage**: Vault-Datenbank: neue synchronisierte Tabelle `agent_file_permissions` (Migration 0029);
Gerätepräferenzen `files.*`; Vorschaubilder unter `<AppCache>/files-thumbnails/`
([data-model.md](./data-model.md))

**Testing**: `cargo test` (Tests in `*_tests.rs`; Temp-Verzeichnisse, `FakeStore`, `wiremock` für S3),
`pnpm check:files` (neu), `pnpm check:agent-actions`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`,
E2E-Szene `files-basic` (Spec 016) unter Linux; Plattform-Probe in der CI unter Windows, macOS
und Android (research R4); manuell nach [quickstart.md](./quickstart.md) nur noch SC-002/SC-003 mit
einem 4-GB-Video

**Target Platform**: Linux, Windows, macOS, Android (iOS ohne eigenen Aufwand; „Zugriff auf alle
Dateien“ gibt es dort nicht)

**Project Type**: Desktop- und Mobil-App (Tauri) mit eingebautem Agent

**Performance Goals**: SC-001 bis SC-004 (1 000 Einträge < 1 s, Video-Start < 2 s, Speicherzuwachs
< 100 MB bei 4 GB, erste Suchtreffer < 1 s)

**Constraints**: nie eine ganze Datei zum Anzeigen im Speicher (FR-013); keine Zugangsdaten außerhalb von
`remote_storage` (FR-038); kein aws-lc-rs; keine Netzdienste in `cargo test`; 500 Zeilen je Datei

**Scale/Scope**: Ordner bis 50 000 Einträge, Dateien bis mehrere GB, Suche über 10 000+ Dateien

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                              | Stand                                                                                                                                                                                                        |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| I. Keine Geheimnisse in Git                        | ✅ Zugangsdaten der Speicher bleiben im Passwortmanager (038); Test-Zugangsdaten sind Platzhalter                                                                                                            |
| II. Keine lokalen absoluten Pfade in Konfiguration | ✅ Pfade nur zur Laufzeit; Quickstart nutzt `/tmp`-Beispiele, keine Konfiguration                                                                                                                            |
| III. Projektidentität geräteunabhängig             | ✅ nicht berührt                                                                                                                                                                                             |
| IV. Fremde Inhalte mit unveränderlicher Revision   | ✅ haex-vault überall mit `fc4e84b61a050576ba42e0dc832d04064a8605a3` zitiert                                                                                                                                 |
| V. Externe Quellen nur per Allowlist               | ✅ keine neuen Harness-Quellen                                                                                                                                                                               |
| VI. Selbständernde Anweisungen nur mit Review      | ✅ keine Änderung an Constitution, Skills oder Berechtigungen; ADR 0011 kommt im PR                                                                                                                          |
| VII. Relay blockiert lokale Arbeit nie             | ✅ Dateibrowser arbeitet ohne Netz; nur Speicher brauchen ihren Anbieter                                                                                                                                     |
| VIII. Keine Verschleierung                         | ✅ nicht berührt                                                                                                                                                                                             |
| Topic-Branch, Speckit-Ablauf                       | ✅ Branch `044-file-browser`; specify → clarify → plan; danach tasks, analyze, implement in einem Worktree je PR                                                                                             |
| Tests in eigenen Dateien                           | ✅ jede neue Rust-Datei mit `*_tests.rs` daneben                                                                                                                                                             |
| Wiederverwendung                                   | ✅ `extensions/fs` (Auflösen, Sperrliste, Beobachten), `remote_storage` (Zugriff, `FakeStore`), `thumbnails.ts`, `downscale.ts`, photoswipe, `onDragDropEvent`-Muster, haex-vault Medienserver und PdfViewer |
| Abhängigkeiten begründet                           | ✅ jede neue Abhängigkeit in research.md mit verworfenen Alternativen; Probe für Android/Windows                                                                                                             |
| ADR bei Prinzip-relevanten Entscheidungen          | ⚠️ Abweichung von ADR 0006 (Ausführung in Rust) → ADR 0011 im Docs-PR                                                                                                                                        |
| Keine Agent-Attribution in Commits                 | ✅                                                                                                                                                                                                           |

Nach dem Entwurf erneut geprüft: keine Verstöße; die eine Abweichung ist durch ADR 0011 gedeckt.

## Project Structure

### Documentation (this feature)

```text
specs/044-file-browser/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── tauri-commands.md   # Commands des Fensters
│   ├── agent-actions.md    # files.*-Aktionen der Agents
│   └── media-server.md     # lokaler HTTP-Server
├── checklists/requirements.md
└── tasks.md                # /speckit-tasks
docs/adr/0011-native-catalog-actions.md
```

### Source Code (repository root)

```text
src-tauri/src/
├── files/                          # besteht seit Spec 043 (gewählte Dateien); 044 ergänzt
│   ├── mod.rs                      # FilesService, Caller, SourceRef, Entry
│   ├── access.rs                   # rein: Sperre, Berechtigungen der Agents auswerten + _tests
│   ├── local/                      # aus extensions/fs gezogen + neu
│   │   ├── resolve.rs, places.rs   # Auflösen, Sperrliste, bekannte Orte, Laufwerke
│   │   ├── watch.rs                # notify (Desktop und Android)
│   │   ├── ops.rs                  # list, stat
│   │   ├── edit.rs                 # Ordner anlegen, umbenennen
│   │   └── text.rs                 # Text bis 5 MB, binär erkennen
│   ├── storage_source.rs           # S3 als Quelle (Präfixe als Ordner)
│   ├── media_server.rs             # Server, Tokens, Range + _tests
│   ├── streaming.rs                # StreamingSource: Datei, S3
│   ├── thumbnails.rs               # image + fast_image_resize, Cache
│   ├── search.rs                   # walkdir + frizbee, S3-Suche
│   ├── transfer/                   # TransferManager; local.rs (inkl. Papierkorb), s3.rs (Multipart)
│   ├── kind.rs                     # Art des Viewers aus MIME/Endung
│   ├── extract/                    # pdf.rs, office.rs, image.rs (für Agents)
│   ├── agent/                      # NativeActionTool-Ausführer, Rückfrage-Brücke
│   ├── permissions.rs              # Tabelle agent_file_permissions
│   └── browser_commands.rs         # Tauri-Commands (contracts/tauri-commands.md); `commands.rs` und `picked.rs` gehören Spec 043
├── extensions/fs/                  # nutzt files::local
├── remote_storage/{mod,s3}.rs      # head, Range, Stream, list_dir, Multipart, copy
├── chat/tools/{mod,native_action}.rs, chat/action_commands.rs   # ToolResult.images, NativeActionTool
├── adapters/request.rs             # Bildblöcke im tool_result
├── identity/migrations_agent_files.rs   # 0029
└── plugins/holzi-android/          # all_files_access_status, request_all_files_access

src/
├── components/apps/FilesApp.vue
├── components/files/               # Toolbar, Sidebar, List, Grid, Viewer*, TransferBar, Dialoge
├── lib/files/                      # registry.ts, viewerKind.ts, state (testbar ohne DOM)
├── lib/actions/filesActions.ts, scopes.ts
├── stores/filesActionHandlers.ts   # nur files.show
├── composables/useFiles*.ts
└── i18n/locales/{de,en}.json
scripts/check-files-*.ts
src-tauri/tauri.conf.json           # CSP
src-tauri/gen/android/...           # Manifest, network_security_config.xml
```

**Structure Decision**: Der Dateizugriff liegt in `src-tauri/src/files/`, nicht unter `extensions/`,
weil holzi ihn selbst nutzt und Erweiterungen und Agents nur Verbraucher sind (wie `remote_storage` in
038).

## Lieferungen

Jeder PR ist für sich prüfbar und lauffähig; die Reihenfolge folgt den Prioritäten der Spec.

1. **PR A (Docs)**: Spec, Plan, Research, Contracts, Tasks, ADR 0011.
2. **PR B (Kern, Medienserver, US1)**: zuerst die Plattform-Probe in der CI (Windows, macOS, Android),
   dann `files/local` aus `extensions/fs` gezogen, Medienserver, CSP, Android
   `network_security_config`, `FilesService` mit Auflisten, Angaben, Beobachten; App `system.files`
   mit Liste, Raster, Pfadleiste, Seitenleiste, Sitzung, Präferenzen; Vorschaubilder; Viewer für Text
   und Bild; Info-Ansicht.
3. **PR C (US2)**: Viewer für Video, Audio und PDF; Probe auf alle Plattformen erweitert.
4. **PR D (Verwalten, US3)**: Transfers, Konflikte, Papierkorb, Drag & Drop, eigene Daten nur lesen.
5. **PR E (Suche, US4)**.
6. **PR F (Speicher, US5)**: `ObjectStore`-Erweiterung, Speicher als Quelle, S3-Streaming und
   Multipart.
7. **PR G (Agents, US6)**: ADR 0011 umgesetzt (`NativeActionTool`), Tabelle 0029, Rückfrage-Brücke,
   Textauszug, Bilder im `tool_result`, `files.show`.
8. **PR H (Android, US7)**: Plugin-Befehle, Manifest, Erklärung im Dateibrowser; Probe mit erteilter
   und fehlender Berechtigung.

**Plattformen in der CI**: Linux mit den E2E-Szenarien; Windows, macOS und Android mit einer
selbstprüfenden Plattform-Probe (Feature `platform-probe`, Jobs `platform-probe` und
`platform-probe-android`), weil volles E2E dort noch nicht gebaut ist (`scripts/e2e/PLATFORMS.md`). Den
Ausbau zu vollem E2E nach dem Vorbild von haex-vault führt `plans/README.md` als eigene Idee.

## Complexity Tracking

| Abweichung                                         | Warum nötig                              | Einfachere Alternative verworfen, weil                                     |
| -------------------------------------------------- | ---------------------------------------- | -------------------------------------------------------------------------- |
| Katalog-Aktionen mit Ausführung in Rust (ADR 0006) | FR-034: Dateiaktionen ohne Fenster       | eigene Rust-Werkzeuge sprengen das Angebot (≤ 10) und doppeln den Katalog  |
| Zweiter Weg für Inhalte (Medienserver) neben IPC   | WebKitGTK spielt Medien nur über http(s) | `asset://` und eigene Schemata ohne Range bzw. ohne Wiedergabe unter Linux |
