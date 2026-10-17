# Implementation Plan: Passwörter aus haex-vault 2.x (Legacy) übernehmen

**Branch**: `037-haex-vault-import` | **Date**: 2026-10-04 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/037-haex-vault-import/spec.md`

## Summary

Der Import des Passwortmanagers (034) bekommt die Quelle „haex-vault 2.x“. haex-vault 2.x hat keinen
Export; holzi liest deshalb die verschlüsselte Vault-Datei direkt. Weil 034 das Datenmodell von
haex-vault 2.x übernommen hat, ist die Abbildung fast 1:1. Die Arbeit liegt in drei Teilen
(Begründungen in [research.md](./research.md)):

- **Leser** (R1–R3, R5–R9): neues Modul `passwords/import/legacy_haex_vault/`. Es kopiert die Datei samt
  `-wal` in ein temporäres Verzeichnis (die Quelle bleibt unberührt, der WAL-Stand kommt mit),
  öffnet die Kopie mit einer eigenen SQLCipher-Verbindung (`PRAGMA key`, Standardwerte wie
  haex-vault 2.x, dieselbe SQLCipher-Version), prüft den Aufbau und baut das gemeinsame
  `ImportModel`. `read_model` verzweigt für diese Quelle vor dem Lesen der Bytes; danach ist
  alles geteilt (Vorschau, Doppelte, `apply::run`, Bericht, Abbruch).
- **Gemeinsames Importmodell und Schreiber** (R4): Farbe und Autofill-Aliase am Eintrag, Farbe
  und Reihenfolge am Ordner, Tag-Farben, Passkeys ohne Eintrag und Generator-Voreinstellungen
  kommen ins Modell und werden geschrieben. Dazu nimmt `write_groups` vorhandene Ordner mit
  gleichem Namen am gleichen Ort wieder, statt sie zu verdoppeln; das behebt den doppelten
  Ordnerbaum bei wiederholtem Import für alle Quellen.
- **Oberfläche** (Contract): Quelle im Wizard, Passwortfeld, zwei neue Zahlen in der Vorschau,
  Hinweis zum Schließen von haex-vault 2.x, Texte für neue Berichtsarten und Fehlergründe.

Kein neues Tabellenschema, keine Migration, keine neue Abhängigkeit.

## Technical Context

**Language/Version**: Rust (Edition des Workspace, `src-tauri`), TypeScript/Vue 3 (Nuxt) im Frontend

**Primary Dependencies**: vorhanden: haex-crdt @ `928d06a` (re-exportiert rusqlite 0.40.2 mit
`bundled-sqlcipher-vendored-openssl`), `tempfile 3`, `base64 0.23`, `sha2`, `zeroize`, `serde_json`,
`ts-rs`; Frontend `@tauri-apps/plugin-dialog`

**Storage**: Vault von holzi (SQLCipher, unverändertes Schema); Quelle: Vault-Datei von haex-vault 2.x
(SQLCipher 4, nur gelesen, als Kopie in `TempDir`)

**Testing**: `cargo test` (Integrationstests unter `src-tauri/tests/`, Unit-Tests in eigenen
`*_tests.rs`), `pnpm check:passwords` (`node --test`), manueller Quickstart §3

**Target Platform**: Desktop (Linux, macOS, Windows); Android/iOS ohne plattformabhängigen Code,
aber noch ohne Build (R11)

**Project Type**: Desktop-App (Tauri: Rust-Backend + Vue-Frontend)

**Performance Goals**: 1.000 Einträge in unter einer Minute (SC-005); Lesen im `spawn_blocking`

**Constraints**: Quelldatei und Begleitdateien nie verändert (FR-003); Vault-Passwort nie
gespeichert oder protokolliert (FR-004); Anhänge ≤ 25 MiB; Bericht ohne Geheimnisse

**Scale/Scope**: eine Vault des Nutzers, typisch 100–5.000 Einträge, einige hundert MiB Anhänge
möglich

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                              | Bewertung                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git                        | ✅ Fixture wird zur Laufzeit erzeugt; im Repo nur Testpasswort-Konstanten und das Schema-SQL, keine echte Vault.                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| II. Keine lokalen absoluten Pfade                  | ✅ Verweise auf haex-vault 2.x über Repo + SHA + relativen Pfad.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| IV. Fremde Verweise auf feste Revision             | ✅ haex-vault 2.x @ `8dce379d94e18fcd42c3b73686a06f984ca3f574` in Spec, Research, Mapping und im Kommentar der Fixture-SQL.                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Speckit-Ablauf                                     | ✅ specify → (Gate) → plan → (Gate) → tasks → implement.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Worktree / PR / Conventional Commits / kein Squash | ✅ Worktree `.worktrees/037-haex-vault-import`, Branch `037-haex-vault-import`, Merge per PR.                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Keine Agenten-Spuren in Artefakten                 | ✅ keine Co-Authored-By-Zeilen oder Hinweise auf Agenten.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Tests getrennt vom Produktionscode                 | ✅ `tests/passwords_import_legacy_haex_vault.rs`, `tests/common/legacy_haex_vault_fixture.rs`, Unit-Tests in `legacy_haex_vault/*_tests.rs`.                                                                                                                                                                                                                                                                                                                                                                                                             |
| 500 Zeilen je Datei                                | ✅ Leser in `open.rs`, `read.rs`, `icons.rs` getrennt nach Grund der Änderung. `apply.rs` (677 Zeilen) und `model.rs` (662) liegen schon heute über der Grenze. Die neuen Schreibschritte (Tag-Farben, Passkeys ohne Eintrag, Voreinstellungen) kommen deshalb nach `import/apply_extras.rs`. In `apply.rs` und `model.rs` kommen nur wenige Zeilen hinzu. Ihr Aufteilen ist ein eigener Refactor außerhalb dieser Spec.                                                                                                                                 |
| graphify zuerst                                    | ⚠️ Geprüft (`graphify query`, Budget 1000). Der Graph lieferte nur Treffer aus `extensions/sql`, nichts zum Import. Die Kandidaten kamen aus gezielter Suche: `import::parse` und die drei Parser (anderes Eingabeformat, Bytes statt Datei), `presets::save`, `tags::set_color`, `passkeys::insert` und `binaries::ensure_binary` (werden wiederverwendet), `ensure_group_path` (nur innerhalb eines Modells, ersetzt nicht das Wiederverwenden vorhandener Ordner). Vor dem Anlegen neuer benannter Funktionen fragt die Umsetzung den Graphen erneut. |
| Laziness ladder                                    | ✅ Wiederverwendung der ganzen Import-Pipeline; neue Felder nur, wo die Quelle sonst verliert; kein neues Crate.                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Wurzel statt Symptom                               | ✅ Doppelte Ordner werden im geteilten `write_groups` behoben, nicht nur für die neue Quelle.                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `ponytail:`-Markierung                             | ✅ Passwort im SQL-Text von `PRAGMA key` (R10).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Phasen-Disziplin                                   | ✅ 034 ist gemergt. Diese Spec ist die Voraussetzung dafür, dass der Operator 034 täglich nutzt.                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ADR nötig?                                         | Nein. Kein Kernprinzip betroffen.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Umfang ~100/1000 Zeilen                            | ⚠️ Geschätzt 1.000–1.300 Zeilen einschließlich Tests und Fixture. Begründung und Schnitt siehe Complexity Tracking.                                                                                                                                                                                                                                                                                                                                                                                                                                      |

**Nach Phase 1**: unverändert bestanden. Kein Schema, keine Abhängigkeit, kein neuer Command.

## Project Structure

### Documentation (this feature)

```text
specs/037-haex-vault-import/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── haex-vault-mapping.md
│   └── tauri-commands.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/passwords/
├── import/
│   ├── mod.rs                 # ImportSource::LegacyHaexVault, neue Modellfelder
│   ├── apply.rs               # color/aliases am Eintrag, Ordner wiederverwenden, Ledger
│   ├── apply_extras.rs        # neu: Tag-Farben, Passkeys ohne Eintrag, Voreinstellungen (+ Rückbau)
│   ├── report.rs              # neue AttentionKind-Namen
│   └── legacy_haex_vault/            # neu
│       ├── mod.rs             # pub fn read(path, &Credentials) -> Result<ImportModel>
│       ├── open.rs            # Kopie in TempDir, Kopfprüfung, PRAGMA key, Aufbau prüfen
│       ├── read.rs            # Zeilen → ImportModel
│       ├── icons.rs           # Symbol-Abbildung
│       ├── open_tests.rs
│       ├── read_tests.rs
│       └── icons_tests.rs
├── service/import.rs          # read_model verzweigt für LegacyHaexVault
└── model.rs                   # ImportPreview + tags/presets, AttentionKind + 4 Arten

src-tauri/tests/
├── common/legacy_haex_vault_fixture.rs        # neu: SQLCipher-Vault von haex-vault 2.x zur Laufzeit
├── fixtures/passwords/legacy_haex_vault_0000_passwords.sql   # neu: Auszug aus 0000 @ 8dce379
├── passwords_import_legacy_haex_vault.rs      # neu: Leser + Dienst Ende-zu-Ende
└── passwords_import.rs                 # + Ordner-Wiederverwendung für bestehende Quellen

src/
├── components/passwords/ImportWizard.vue   # Quelle, Passwortzeile, Vorschau, Hinweis
├── lib/passwords/importReport.ts           # ggf. Regel „Quelle braucht Passwort“
├── lib/passwords/icons.ts                  # IMPORT_ICONS um fehlende Ziele ergänzt
├── i18n/locales/{de,en}.json               # Quelle, Hinweise, Gründe, Berichtsarten
└── types/bindings/                         # ts-rs: ImportSource, ImportPreview, AttentionKind

scripts/check-passwords-import.ts           # Fälle für neue Hilfslogik
```

**Structure Decision**: Der Leser sitzt als eigenes Untermodul neben den anderen Parsern im
Import von 034. Er hat einen eigenen Grund zur Änderung (Aufbau von haex-vault 2.x) und als einziger
Parser Datei-I/O; seine Teile sind nach Öffnen, Abbilden und Symbolen getrennt.

## Complexity Tracking

| Abweichung                                            | Warum nötig                                                                                                     | Einfachere Alternative verworfen, weil                                                                                                                                                                                                                                                  |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Änderung über ~1.000 Zeilen (inkl. Tests und Fixture) | Verlustfreiheit (FR-006–FR-013) braucht Leser, Modellfelder und Schreiber. Die Fixture muss jedes Feld belegen. | Zwei PRs (erst Modell und Schreiber, dann Leser) hinterlassen nach dem ersten PR Felder ohne Nutzer (YAGNI). Stattdessen ein PR mit getrennten Commits: (1) Ordner wiederverwenden (alle Quellen), (2) Modell und Schreiber erweitern, (3) Leser und Fixture, (4) Oberfläche und Texte. |
| Kopie der Vault in ein temporäres Verzeichnis         | FR-003: Quelle unverändert **und** WAL-Stand eingelesen                                                         | Nur lesend öffnen legt `-shm` an oder scheitert. `immutable=1` verliert die WAL (R1).                                                                                                                                                                                                   |
