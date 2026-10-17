# Implementation Plan: Umbenennung von holzi in haex-vault

**Branch**: `054-haex-vault-rename` (Spec) | **Date**: 2026-10-17 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/054-haex-vault-rename/spec.md`

## Summary

Der Name holzi verschwindet aus dem ganzen Repo; haex-vault 3.0.0 tritt an seine Stelle, und das
Repo zieht nach `haex-space/haex-vault`. Die Änderung ist fast ausschließlich mechanisch (rund 4500
Treffer außerhalb von `specs/`), aber an drei Stellen nicht folgenlos: an den festen Texten der
Kryptografie und des Netzes, an Namen, die in Vault-Daten stehen (Tabelle, Kontaktarten, Herkunft,
UUIDs), und am Identifier (Datenordner, Android-Paket).

Der Ansatz: **eine Regel pro Schreibweise** ([contracts/naming-map.md](contracts/naming-map.md)),
angewendet in **elf einzeln grünen Stufen** (S1–S11, unten, je ein PR oder Handgriff), mit einer **Prüfung des
alten Namens in der CI** ([contracts/old-name-gate.md](contracts/old-name-gate.md)) als Schutz gegen
Rückfälle. Weil nicht alle Änderungen folgenlos sind, entsteht **vor** der ersten Umbenennung ein
Testvault aus dem Stand `holzi-final` (Stufe S1), an dem jede spätere Stufe zeigt, was in einer
alten Vault-Datei bricht ([research.md](research.md), R3). Der Umzug des Repos folgt zuletzt nach
einem Ablaufplan ([contracts/cutover.md](contracts/cutover.md)).

## Technical Context

**Language/Version**: Rust (Edition des Crates, Tauri 2), TypeScript (Nuxt, Vue), Kotlin (Android),
Nix (devShell, Flake), Python (Dokumentenprüfung `scripts/ci/check-docs.py`), YAML (GitHub Actions)

**Primary Dependencies**: Tauri 2 und seine Plugins, `haex-crdt` (SQLite und CRDT), SQLCipher,
iroh, vault-sdk (Erweiterungen), ts-rs (Typen für das Frontend), Gradle für Android

**Storage**: SQLCipher-Vault-Dateien unter `<AppLocalData>/instances/`; der Name des Datenordners
folgt dem Identifier

**Testing**: `cargo fmt/check/clippy/test` (alle Pakete und Ziele, auch `--no-default-features`),
`node --test scripts/check-*.ts` (Frontend-Logik), `pnpm lint`, ts-rs-Drift-Prüfung, Desktop-
End-to-End-Suite (tauri-driver, Playwright), Android-End-to-End (3 Shards), Plattform-Probe
(Windows, macOS, Android), `scripts/ci/check-docs.py` mit Test

**Target Platform**: Linux, macOS, Windows, Android, iOS

**Project Type**: Desktop- und Mobile-App (Tauri) mit Nuxt-Frontend und eigenem Android-Plugin

**Performance Goals**: keine; die Laufzeit ändert sich nicht

**Constraints**: kein Alias für alte Namen (FR-002), keine Datenmigration im Code (FR-013), jeder PR
einzeln grün und mergebar; Rust-Builds sind teuer (kalter Build, Speicher), deshalb nur gezielte
Testziele lokal (`-j 4`); Android- und iOS-Builds sind Pflicht

**Scale/Scope**: etwa 1140 Dateien mit „holzi“ außerhalb von `specs/`, davon 3233 Treffer in
`src-tauri` (2431 in `src-tauri/src`, 72 Integrationstests), 107 Dateien in `src`, 111 in
`scripts`; 264 Dateien in `specs/`; 5 Geschwister-Repos

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

Geprüft gegen `.specify/memory/constitution.md` (Principles I–VIII) und `.spaex/constitution.md`:

| Regel                                                                                                                    | Befund                                                                                                                                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Keine Geheimnisse im Git (I)                                                                                             | Erfüllt. Der Testvault (S1) nutzt eine eigens dafür gewählte, wertlose Passphrase und keine Daten aus `test.db`. Die echte Datei und ihre Sicherung bleiben außerhalb.                                 |
| Keine lokalen absoluten Pfade (II)                                                                                       | Erfüllt. `quickstart.md` und die Verträge nennen Pfade nur relativ oder als `<AppLocalData>`.                                                                                                          |
| Cross-Repo-Verweise pinnen unveränderliche Revisionen (IV)                                                               | Gilt für S8–S9: Jeder Verweis auf vault-sdk, atoms und haex-crdt trägt Repo, vollständigen Commit-SHA und Pfad. Der Plan nennt keine Branches.                                                         |
| Änderungen nur über PR nach `main`, in einem eigenen Worktree auf einem Topic-Branch                                     | Erfüllt: jede Stufe ein Worktree und ein PR.                                                                                                                                                           |
| Keine Agenten-Hinweise in Commits und Dateien                                                                            | Erfüllt: Commit-Nachrichten und PR-Texte ohne Trailer.                                                                                                                                                 |
| Tests getrennt vom Produktionscode                                                                                       | Erfüllt: die Prüfung des alten Namens bekommt eine eigene Testdatei (wie `test_check_docs.py`), der Testvault liegt unter `src-tauri/tests/fixtures/`.                                                 |
| Größe: ~100 Zeilen Warnschwelle, ~1000 Zeilen Anlass zum Teilen; mechanische Umbenennungen sind eine anerkannte Ausnahme | Die Stufen S2–S6 sind mechanisch und größer. Sie werden nach Gebiet geschnitten und sind je durch die Regel der Namenstabelle prüfbar; das Review prüft die Regel und die Ausnahmen, nicht jede Zeile. |
| graphify zuerst (nur bei neuen benannten Einheiten; gilt nicht für Umbenennungen)                                        | Die einzige neue Einheit ist das Prüfskript (S6); davor wird per `graphify query` geprüft, ob es ein passendes gibt (`check-docs.py`).                                                                 |
| Relay-Ausfall blockiert keine lokale Arbeit (VII)                                                                        | Nicht berührt.                                                                                                                                                                                         |

**Ergebnis**: keine Verstöße, keine Einträge unter „Complexity Tracking“. Nach Phase 1 erneut
geprüft: unverändert.

## Project Structure

### Documentation (this feature)

```text
specs/054-haex-vault-rename/
├── plan.md                    # Diese Datei
├── research.md                # Phase 0: Entscheidungen R1–R9
├── data-model.md              # Phase 1: Namenstabelle, Inventar persistierter Namen, Ausnahmeliste
├── quickstart.md              # Phase 1: Prüfschritte von der Prüfung des alten Namens bis zum Umzug
├── contracts/
│   ├── naming-map.md          # Regeln und Sonderfälle pro Schreibweise
│   ├── old-name-gate.md       # Vertrag der CI-Prüfung (Umfang, Ausnahmeliste, Ausgabe)
│   └── cutover.md             # Ablauf des Umzugs, Einstellungen, Rückfall
├── checklists/requirements.md
└── tasks.md                   # Phase 2 (`/speckit-tasks`), nicht Teil dieses Befehls
```

### Source Code (repository root)

Betroffene Gebiete, in der Reihenfolge der Stufen:

```text
branding/, public/                         # (Logo erledigt, #474)
src-tauri/
├── Cargo.toml, Cargo.lock                 # Crate, Lib, Bin, Plugin-Pfad, Repository-URL
├── tauri.conf.json                        # productName, identifier, Titel, CSP-Schema
├── src/                                   # Typen, Konstanten, Labels, SQL-Namen (2431 Treffer)
│   ├── identity/migrations*.rs            # Tabelle holzi_maintenance_no_sync, Trigger-Version
│   ├── sync/{keys,signing,content_keys*,presence,wire,link/code}.rs   # feste Texte
│   ├── extensions/protocol/               # Schema holzi-ext
│   ├── contacts/{kinds,card/format}.rs    # Kontaktarten, Visitenkarten-Präfix
│   └── passwords/import/haex_vault/       # wird legacy_haex_vault (S1)
├── tests/                                 # 72 Integrationstests importieren holzi_lib
├── tests/fixtures/pre_rename_vault/       # neu (S1): Testvault aus holzi-final
├── plugins/holzi-android/                 # wird haex-vault-android (Crate, Kotlin-Paket)
├── gen/android/                           # MainActivity, buildSrc: Paket com.haex.holzi
├── capabilities/, icon-sets/              # Berechtigungen, Bilder holzi-*.svg, x-holzi
src/                                       # 107 Dateien: Titel, CSS-Variablen, i18n, Kontaktart
src/types/bindings/                        # von ts-rs erzeugt, nach dem Rust-Umbau neu erzeugen
scripts/                                   # 111 Dateien: E2E-Rig, Umgebungsvariablen, Binärpfade
scripts/ci/check-old-name.py + Test        # neu (S6): die Prüfung des alten Namens
.github/workflows/                         # CI, Android-Release: Namen, Binärpfad, Artefakte
.devshell/                                 # packages.nix (Befehl `holzi-emulator`, `holzi-api`,
                                           #   `holzi-android-env`), rust-toolchain.toml
flake.nix, package.json, nuxt.config.ts, README.md, CONTEXT.md
docs/, plans/, specs/                      # Text (S5); Verzeichnisname 032-model-operates-holzi bleibt
.spaex/, .specify/memory/constitution.md   # S9: mit dem neuen Stand der atoms neu gebaut
```

**Structure Decision**: Kein neuer Code außer der Prüfung des alten Namens und dem Testvault. Die
Struktur des Repos bleibt; es ändern sich Namen und Pfade (`plugins/holzi-android` →
`plugins/haex-vault-android`, `gen/android/.../com/haex/holzi` → `.../space/haex/vault`, die
Bild-Dateien `holzi-*.svg`).

## Umsetzungsstufen

Jede Stufe ist ein PR mit eigener Branch und eigenem Worktree, **grün bevor die nächste beginnt**.
S2 bis S5 lassen sich nicht beliebig tauschen, weil sie sich im Bau gegenseitig brauchen; die
Reihenfolge steht in der Spalte „Braucht“.

| Stufe | Inhalt                                                                                                                                                                                                         | Braucht | Spec                   |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ---------------------- |
| S0    | Sicherung der alten Datei, Tag `holzi-final`, Logo (#474), Spec (#476)                                                                                                                                         | –       | FR-026 (erledigt)      |
| S1    | **Legacy-Markierung** der alten App in Specs, Doku, Code, UI; Modul `legacy_haex_vault`; **Testvault** aus `holzi-final` mit einem Test, der ihn öffnet                                                        | S0      | FR-017, FR-014         |
| S2    | **Rust: Namen** in der Bibliothek: Typen, Konstanten, Fehler, MCP-Namen, User-Agent, Tabellen- und Kennungsnamen, Kontaktarten und Herkunft, Manifest-Schlüssel, ts-rs neu erzeugt, Frontend-Verwendungen dazu | S1      | FR-001, FR-011         |
| S3    | **Feste Texte** der Kryptografie und des Netzes (HKDF, Signatur-Domänen, ALPN, UUID-Namensräume, Präfixe) mit Tests, die die neuen Werte festschreiben                                                         | S2      | FR-010, FR-012         |
| S4    | **Build- und Testwerkzeuge**: Crate, Lib, Binärname, Umgebungsvariablen, E2E-Rig, Skripte, CI, Flake und `.devshell/` (Befehle `holzi-emulator` u. a.), `package.json`, Artefaktnamen                          | S3      | FR-008, FR-003         |
| S5    | **Android und Identifier**: Plugin-Crate und Kotlin-Paket, `gen/android`, Identifier `space.haex.vault`, Schema `haex-ext://`, CSS-Variablen samt Schreibern                                                   | S4      | FR-006, FR-007, FR-009 |
| S6    | **Texte und Oberfläche**: Fenstertitel, `app.vue`, i18n, README, `CONTEXT.md`, Kommentare im Code; Version 3.0.0                                                                                               | S5      | FR-001, FR-003         |
| S7    | **Specs und Doku** (Text; Verzeichnisname 032 bleibt)                                                                                                                                                          | S6      | FR-001, FR-018         |
| S8    | **Prüfung des alten Namens** in der CI, mit Ausnahmeliste, Test und Pflichtprüfung                                                                                                                             | S7      | FR-004, FR-005         |
| S9    | **Prüfung der alten Datei** (Kopie von `test.db`); bei Bedarf der Passwort-Export                                                                                                                              | S8      | FR-014 bis FR-016      |
| S10   | **Geschwister-Repos**: atoms und spaex (Constitution), vault-sdk (Major), haextension, haex-crdt                                                                                                               | S9      | FR-023 bis FR-025      |
| S11   | **Umzug**: Legacy-Repo, Transfer, Einstellungen, URLs, lokale Ordner                                                                                                                                           | S10     | FR-019 bis FR-022      |

Zwischen den Stufen gilt: Während eine Stufe offen ist, werden keine fremden PRs gemergt, die
dieselben Dateien berühren; neue Funktionen anderer Sessions kommen davor oder danach. Konflikte
löst man **durch erneutes Anwenden der Regeln** (Skript in `contracts/naming-map.md`), nicht per
Hand.

## Complexity Tracking

Keine Verstöße gegen die Constitution; nichts einzutragen.
