# Implementation Plan: Externe Laufwerke im Dateibrowser

**Branch**: `064-external-drives` | **Date**: 2026-10-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/064-external-drives/spec.md`

## Summary

Die Laufwerke der Seitenleiste (`src-tauri/src/files/local/drives.rs`, Spec 044) werden zu einem
Modul `files/local/drives/`, das über ein `DriveBackend` je Plattform alle Laufwerke mit Zustand
liefert:

- **Linux**: udisks2 über eigene zbus-5-Proxys, blockierende API
- **macOS**: `diskutil` mit Plist
- **Windows**: `windows-sys`
- **Android**: `/storage` wie heute

Ein `DriveMonitor` fragt alle 2 s ab, solange ein Dateibrowser-Fenster zuhört. Er meldet Änderungen
über einen Tauri-`Channel`, wie es `files_watch` tut. `files_drive_open` hängt ein oder entsperrt.
Liegt genau ein Laufwerks-Eintrag (`haex:drive:<key>`) vor, entsperrt Rust selbst, ohne dass das
Passwort das Fenster erreicht. `files_drive_release` gibt erst alles frei, was haex selbst unter dem
Pfad offen hält (Watches, Media-Tokens, Suchen, Erweiterungs-Watches, auf Wunsch Transfers), hängt
dann aus, sperrt und wirft aus.

`drives()` bleibt „nur eingehängte“. Agent und andere Geräte sehen deshalb nichts Neues. Der Tab-Ort
bekommt `drive`/`droot`, damit ein wiederhergestellter Tab auf einem nicht eingehängten Laufwerk
„Einhängen“ anbietet. Der in 044 versprochene, aber fehlende Rücksprung zum nächsten vorhandenen
Ordner kommt dazu.

## Technical Context

**Language/Version**: Rust 2021 (MSRV 1.95, `src-tauri`), TypeScript (Nuxt 4, Vue 3)

**Primary Dependencies**:

- **Vorhanden**:
  - `FilesState`, `MediaServer`, `SearchManager`, `TransferManager`
  - `PasswordsService` (Aufrufer `User`)
  - `tauri::ipc::Channel`
  - `libc::statvfs` (`space`)
  - Muster aus `remote_storage/credentials.rs` und `CredentialsEntryPicker.vue` (Spec 052)
- **Neu als direkte Abhängigkeiten**: alle schon im Lockfile, nur je Plattform, ohne neue Versionen:
  - `zbus = "5"` (Linux ohne Android, keine zusätzlichen Features)
  - `plist = "1"` (macOS)
  - `windows-sys` (Windows, Features in research R6)

**Storage**:

- Keine neue Tabelle.
- Laufwerks-Einträge sind normale Passworteinträge (Spec 034).
- Der Tab-Ort bekommt zwei neue Query-Schlüssel (Sitzung, Spec 022).

**Testing**:

- **Rust**: `*_tests.rs` neben jedem Modul, nie inline. Die reinen Teile sind ohne System testbar:
  - Klassifizierung (udisks und Plist)
  - `diff`
  - Sortierung und Invarianten
  - Eintragssuche und Merken, gegen einen Test-Vault
  - Freigeben nach Präfix
  - Ein Fake-`DriveBackend` deckt die Befehlsabläufe ab.
- **Frontend**: `node --test scripts/check-files-drives.ts` (neu, reine `lib/files/drives.ts`) und eine
  Erweiterung von `check-files-registry.ts`.
- **E2E**: `scripts/e2e/scenarios/files-drives.test.ts` mit Loop-Images über udisks2. Es überspringt
  sich ohne `udisksctl`; ein CI-Spike geht voraus (R12).

**Target Platform**:

- Linux, macOS, Windows vollständig (Windows ohne „nicht eingehängt“ und ohne BitLocker-Passwort)
- Android nur Liste live
- iOS: unverändert (kein Dateibrowser-Laufwerk)

**Project Type**: desktop-app (Tauri) mit Mobil-Zielen

**Performance Goals**:

- SC-001: Änderung in ≤ 3 s sichtbar, bei 2-s-Abfrage.
- Eine Abfrage kostet auf Linux einen D-Bus-Aufruf aus dem Speicher von udisksd.
- Auf macOS kostet sie ein `read_dir` (diskutil nur bei Änderung), auf Windows eine Volume-Aufzählung
  ohne `GetLogicalDrives`.

**Constraints**:

- Blockierende Arbeit nie auf dem async-Executor (Konstitution); der Monitor hat einen eigenen Thread.
- Passwörter:
  - nur in Rust
  - nie in argv, Log oder Fenster-State
  - macOS nur über `-stdinpassphrase`
- Keine Admin-Rechte von haex selbst; polkit oder das System fragen selbst.
- Agents und andere Geräte bekommen keine neuen Fähigkeiten.

**Scale/Scope**:

- Rust: 1 neues Modul mit 7 Dateien plus Tests, 1 neue Befehlsdatei, kleine Erweiterungen an 6
  bestehenden Modulen.
- Frontend: 3 Composables, 1 reine Bibliothek, 2 neue Komponenten, 4 geänderte Dateien.
- Dazu 1 E2E-Szenario und eine CI-Anpassung.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Grundsatz                                                        | Bewertung                                                                                                                                                                           |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git                                      | Berührt nicht. Test-Passwörter für Loop-Images sind Wegwerf-Fixtures im E2E-Code, kein echtes Geheimnis.                                                                            |
| II. Keine lokalen absoluten Pfade in versionierter Konfiguration | Die polkit-Regel im CI-Workflow nennt nur den Runner-Nutzer und Aktionsnamen; Images liegen unter `$RUNNER_TEMP`.                                                                   |
| III. Identität gerätunabhängig                                   | Laufwerks-Einträge hängen an der UUID des Datenträgers, nicht am Gerät; sie synchronisieren wie jeder Eintrag.                                                                      |
| IV/V. Externe Quellen                                            | Keine neue externe Quelle; die neuen direkten Abhängigkeiten sind schon im Lockfile.                                                                                                |
| VI. Selbständernde Anweisungen                                   | Keine Änderung an Skills, Konstitution oder Berechtigungen.                                                                                                                         |
| VII. Relay-Ausfall blockiert nichts                              | Rein lokal. Der Passwortmanager ist lokal; Sync nur für den Eintrag auf anderen Geräten.                                                                                            |
| VIII. Keine Verschleierung                                       | Berührt nicht.                                                                                                                                                                      |
| Rust-Regeln (spaex)                                              | `spawn_blocking`/eigener Thread für D-Bus, Prozesse und Win32; `unsafe` nur in `windows.rs` und `space`, je klein, mit Safety-Kommentar und Test; Fehler mit Kontext statt Strings. |
| Workflow                                                         | PRs je Slice vom Themen-Branch; Tests in `*_tests.rs`; Prettier auf Markdown/JSON. Kein ADR nötig.                                                                                  |
| Spaex (Worktree, graphify-first)                                 | Arbeit im Worktree `.worktrees/external-drives`; Code-Research per graphify, dann gezielte Ausschnitte.                                                                             |

**Ergebnis**: Pass, keine Verstöße. Nach Phase 1 erneut geprüft: unverändert Pass.

## Project Structure

### Documentation (this feature)

```text
specs/064-external-drives/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── commands.md      # Tauri-Befehle, Antworten, Fehlercodes
│   └── ui.md            # Seitenleiste, Dialog, Hinweise, Tab-Hinweis
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/
├── Cargo.toml                         # + zbus (linux), plist (macos), windows-sys (windows)
└── src/
    ├── lib.rs                         # Befehle registrieren; on_page_load beendet Laufwerks-Abos
    ├── files/
    │   ├── local/drives/              # NEU (ersetzt drives.rs)
    │   │   ├── mod.rs                 # Drive & Co., volumes(), drives() = nur eingehängte, display_name, Sortierung, space
    │   │   ├── mountinfo.rs           # bisheriger Parser + is_user_drive (Fallback ohne udisks2, R1)
    │   │   ├── backend.rs             # trait DriveBackend, Wahl je Plattform, Fake für Tests
    │   │   ├── udisks.rs              # Linux: zbus-Proxys, Befehle, Fehlerzuordnung
    │   │   ├── udisks_classify.rs     # Linux, rein: Abbild der Eigenschaften → Drive (R2, R3)
    │   │   ├── macos.rs               # diskutil-Aufrufe, stdin-Passwort
    │   │   ├── macos_plist.rs         # rein: Plist → Drive
    │   │   ├── windows.rs             # Volumes, Art, Auswerfen, BitLocker-Dialog
    │   │   ├── android.rs             # /storage, is_volume_id
    │   │   ├── monitor.rs             # DriveMonitor (Thread, Abos, diff)
    │   │   ├── unlock.rs              # Laufwerks-Einträge: suchen, lesen, merken (PasswordsService, Caller::User)
    │   │   └── *_tests.rs             # je Modul
    │   ├── browser_commands/drives.rs # NEU: files_drives_watch/unwatch, files_drive_open/unlock/unlock_candidates/release
    │   ├── browser_commands/navigate.rs # files_sources nutzt volumes(); files_watch merkt sich root
    │   ├── state.rs                   # Watching::Local { root }, unwatch_under
    │   ├── media/server.rs            # Grant.root, release_under
    │   ├── search_manager.rs          # root je Suche, cancel_under
    │   ├── transfer/mod.rs            # touching(prefix), cancel_touching(prefix)
    │   ├── agent/exec.rs              # display_name statt name
    │   ├── peer/serve/ops.rs          # display_name statt name
    │   └── mod.rs                     # neue FilesErrorCode
    └── extensions/fs/watch.rs         # end_unless für Pfade unter einem Laufwerk (Aufruf aus release)

src/
├── lib/files/
│   ├── drives.ts                      # NEU, rein: driveOf(path), remap(p, droot, root), Anzeigename, current
│   └── registry.ts                    # drive/droot in FilesPlace und KEYS
├── composables/
│   ├── useFilesDrives.ts              # NEU: Singleton-Abo (Channel) + Liste; Start/Stopp über openFrames
│   ├── useFilesDriveActions.ts        # NEU: Öffnen, Entsperren, Auswerfen, Hinweise
│   ├── useFilesDriveRestore.ts        # NEU: Tab-Hinweis (FR-025) und Rücksprung (R10)
│   ├── useFiles.ts                    # neue invoke-Wrapper
│   └── useFilesFolder.ts              # notFound → Restore/Rücksprung statt nur Fehler
├── components/
│   ├── apps/FilesApp.vue              # openFrames-Haken, onSidebarPick/go mit drive/droot, Startordner nur eingehängt
│   └── files/
│       ├── Sidebar.vue                # Zustände, Symbole, Auswerfen/Aushängen, Spinner
│       ├── DriveUnlockDialog.vue      # NEU
│       └── DriveNotice.vue            # NEU
├── stores/filesActionHandlers.ts      # Orte mit drive/droot
└── i18n/locales/{de,en}.json          # files.drives.*

scripts/
├── check-files-drives.ts              # NEU
├── check-files-registry.ts            # erweitert
└── e2e/
    ├── lib/udisks.ts                  # NEU: Loop-Images anlegen/entfernen, Skip ohne udisksctl
    └── scenarios/files-drives.test.ts # NEU

.github/workflows/ci.yml               # e2e-Job: udisks2, cryptsetup, polkit-Regel für runner
```

**Structure Decision**: Wie in Spec 044. Plattformcode steckt hinter einem Trait, die Klassifizierung
in reinen Funktionen (testbar ohne System). Passwörter fasst nur `unlock.rs` an. Das Frontend trennt
reine Bibliothek, Composables und Komponenten. Dialog und Tab-Hinweis sind eigene Komponenten, damit
der Umbau von Spec 062 (`Panel.vue`) sie nur verschiebt (R14).

## Slicing (Pull Requests)

1. **PR A – Kern und Linux-Liste live** (US1 auf Linux, Android live):
   - Modul `drives/` mit Typen, `volumes()`/`drives()`, mountinfo-Fallback, udisks-Liste und
     Einhängen, Monitor
   - die Befehle `files_drives_watch`/`unwatch` und `files_drive_open` (ohne Entsperren)
   - Seitenleiste mit Zuständen, `useFilesDrives`
   - der Rücksprung zum nächsten vorhandenen Ordner für alle Gerätepfade (R10, US1 Szenario 7)
   - CI-Spike R12 und E2E für Einstecken, Einhängen und Abziehen
2. **PR B – Freigeben und Auswerfen** (US2):
   - Präfix-Freigaben in `state.rs`, `media/server.rs`, `search_manager.rs` und bei den
     Erweiterungs-Watches
   - `touching` an den Transfers, `files_drive_release`, Knopf, Rückfrage und Hinweise
   - E2E Auswerfen
3. **PR C – Entsperren und Passwortmanager** (US3, US4):
   - `unlock.rs`, `files_drive_unlock`/`unlock_candidates`, Entsperren in `files_drive_open`
   - `DriveUnlockDialog.vue`, E2E mit dem LUKS-Image
4. **PR D – Sitzung** (FR-025, R10):
   - `drive`/`droot` im Tab-Ort, `useFilesDriveRestore`, `DriveNotice.vue`
   - E2E Neustart
5. **PR E – macOS** (R5): Spike mit echten Plist-Ausgaben als Fixtures, `macos.rs`/`macos_plist.rs`,
   manueller Test nach quickstart.
6. **PR F – Windows** (R6):
   - Spikes: Auswerfen über Datenträger oder Elterngerät, BitLocker-Erkennung, Verb `unlock-bde`
   - `windows.rs`, manueller Test

A–D liefern die Spec auf Linux vollständig. E und F sind voneinander unabhängig.

## Complexity Tracking

Keine Verstöße gegen die Konstitution; nichts zu begründen.
