# Implementation Plan: Austauschbare Icon-Sets für den Dateibrowser

**Branch**: `053-file-icon-sets` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/053-file-icon-sets/spec.md`

## Summary

Das Bild eines Eintrags im Dateibrowser kommt heute aus einer festen Zuordnung in `EntryIcon.vue` und `encrypted.ts` (Lucide über Iconify). Neu wählt der Nutzer ein Icon-Set; drei Sets sind mitgeliefert (Lucide, Phosphor und das Material Icon Theme), eigene lassen sich importieren und gelten auf allen Geräten der Vault.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Regel für Mitgeliefertes** (R1, FR-017): nur permissive Lizenz, Drittquellen benannt, kein Copyleft. Danach bleiben **Lucide** (Standard und Rückfall, kein Paket: die Iconify-Symbole der App), **Phosphor** (MIT, kuratiertes Paket mit rund 35 Dateitypen, beim Bauen für hell und dunkel gefärbt) und das **Material Icon Theme** (MIT, Apache-2.0-Teile, vollständig; das Restrisiko der darin enthaltenen Logos hat der Betreiber angenommen, die Lizenzseite sagt, dass Marken ihren Inhabern gehören). Adwaita und vscode-icons sind vom Nutzer importierbar, aber nicht mitgeliefert.
- **Ein Format für alles** (R6, R7): ein Paket ist ein ZIP mit dem Manifest im Format der Icon-Themes von VS Code, den Bildern und einer `LICENSE`, ergänzt um den Block `x-holzi` (Rollen, Herkunft). Die mitgelieferten Pakete entstehen aus eingecheckten Quellverzeichnissen (jede Änderung im Diff sichtbar), werden vom selben Rust-Code wie der Import gepackt, liegen in `src-tauri/icon-sets/` und sind mit `include_bytes!` eingebunden (kein Resource-Resolver, derselbe Weg auf Android, nichts im JS-Bundle).
- **Auflösung nur in Rust** (R5, R7, R8): Befehl `files_icons_resolve` liefert je Eintrag ein fertiges Bild (`id`, `mime`) oder ein Rückfall-Symbol; `files_icon_image` liefert die Bytes. Rolle und Reihenfolge (Zustand, Name, Endung, Rolle, Vorgabe des Themes, Standard-Set) stehen in einem Modul. Das Frontend sammelt die Anfragen der sichtbaren Zeilen und hält Antworten und `blob:`-URLs in einem begrenzten Speicher.
- **Eine Bereinigung** (R9, R10): `icons/svg.rs` auf `quick-xml` mit Erlaubnisliste; Pixelbilder über `image` mit Limits. Sie läuft beim Import und, idempotent, beim Ausliefern; ein per Sync angekommenes Paket wird beim ersten Öffnen auf Aufbau, Limits und SHA-256 geprüft und gilt sonst als beschädigt (Rückfall Lucide).
- **Atomar und synchronisiert** (R3, R4): ein eigenes Set ist eine Zeile in `haex_file_icon_sets` mit dem Paket als BLOB (Migration `0035`); die Wahl ist die Vault-Preference `files.iconSet`.
- **Import in zwei Schritten** (R11): Vorschau ohne Schreiben, dann Import, wie bei den Erweiterungen. Quellen: Ordner (nur Desktop), Archiv oder VSIX. Pfadregeln über `haex_bundle::format`.
- **Oberfläche** (R12): Gruppe „Icon-Set für Dateien“ in der Gestaltung, speichert beim Anklicken; Unterseite „Lizenzen“ aus den Paketen.

Aus Planung und Vorgabe des Betreibers sind in der Spec nachgetragen: Auf Android gibt es keinen Ordner-Import (FR-009, US3); mitgeliefert werden Lucide, Phosphor und das vollständige Material Icon Theme nach der Lizenzregel mit der vom Betreiber angenommenen Ausnahme für Logos (FR-007, FR-017, SC-002).

## Technical Context

**Language/Version**: Rust (Tauri 2.12, Edition wie `src-tauri/Cargo.toml`); TypeScript 6 (strict), Vue 3.5, Nuxt 4.5 (SPA)

**Primary Dependencies**: keine neuen. Vorhanden: `quick-xml` 0.41, `image` 0.25 (`png`, `webp`), `zip` 8 (`deflate`), `haex-bundle` (Pfadregeln), `sha2`, `serde_json`, `ts-rs`, `haex-crdt`; im Frontend haex-ui (Group/Row/OptionRow), `vue-i18n`, Iconify

**Storage**: Vault-Datenbank über haex-crdt: eine neue synchronisierte Tabelle `haex_file_icon_sets` und die Preference `files.iconSet` ([data-model.md](./data-model.md)); mitgelieferte Pakete als Dateien im Repository und in der Binärdatei

**Testing**: `cargo test` gezielt (`files::icons`, `identity::migrations_icon_sets`, `-j 4`), Skript `scripts/check-file-icons.ts` für die reinen TypeScript-Helfer, E2E `files-icon-sets` und `files-icon-sets-two-devices` (tauri-driver; Zwei-Geräte-Rahmen Spec 033), Fixtures unter `src-tauri/tests/fixtures/icon_sets/`

**Target Platform**: Desktop (Linux, macOS, Windows) und Android/iOS; auf dem Telefon nur der Import eines Archivs (kein Ordner, Spec 043 FR-016)

**Project Type**: Desktop-/Mobile-App (Tauri + Nuxt SPA)

**Performance Goals**: 200 sichtbare Einträge in unter 20 ms auflösen; ein Set in unter 100 ms öffnen; ein Ordner mit 1000 Einträgen öffnet mit einem großen Set höchstens 200 ms langsamer als mit Lucide (SC-004). Zielwerte, beim Umsetzen zu messen

**Constraints**: Kein Bild wird je inline im DOM eingebettet; SVG nur als `<img>`-Quelle (`blob:`, CSP erlaubt `blob:` und `data:`); keine neue Abhängigkeit; die neue Rust-Logik in einem Verzeichnis `files/icons/` mit Dateien unter 500 Zeilen (größte vorhandene Datei in `files/` hat 475); `migrations.rs` ist mit 649 Zeilen eine dokumentierte Ausnahme und bekommt nur die Eintragung der Migration

**Scale/Scope**: drei mitgelieferte und bis zu zehn eigene Sets; bis 2000 Bilder und 5 MiB je Set; Ordner mit einigen tausend Einträgen

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Prinzip / MUST                                          | Stand                                                                                                                                                                                                          |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Änderungen im eigenen Worktree auf Themen-Branch        | erfüllt (`.worktrees/053-file-icon-sets`)                                                                                                                                                                      |
| Graphify vor dem Anlegen benannter Artefakte            | erfüllt, siehe „Geprüfte Kandidaten“                                                                                                                                                                           |
| Tests in eigenen Dateien                                | geplant: `*_tests.rs` neben jedem Modul von `files/icons/`, `migrations_icon_sets_tests.rs`, `scripts/check-file-icons.ts`, E2E-Szenarien                                                                      |
| 500-Zeilen-Grenze                                       | Modul `files/icons/` in neun Dateien mit je einer Aufgabe (siehe Struktur); Frontend-Dateien klein; `migrations.rs` (Ausnahme, dokumentiert) bekommt nur die Eintragung                                        |
| Eine Umsetzung je Regel                                 | Auflösung, Rolle und Rückfall nur in Rust (die Symbol-Wahl in `EntryIcon.vue` und `encrypted.ts` entfällt); Bereinigung nur in `svg.rs`; Pfadregeln über `haex_bundle::format`; Lizenztexte nur in den Paketen |
| Keine neue Abhängigkeit, keine spekulative Abstraktion  | erfüllt; ein Paketformat für mitgelieferte und eigene Sets, kein Lucide-Paket, kein viertes Set                                                                                                                |
| Fehler nicht verschlucken, kein `unwrap` auf Eingaben   | Import und Lader geben typisierte Fehler mit Grund je Datei zurück; ein Set, das nicht lädt, ergibt Rückfall plus sichtbaren Zustand                                                                           |
| Eingaben an Vertrauensgrenzen prüfen                    | Import (Ordner, Archiv, Manifest, Bilder) und Lader (Sync) laufen durch dieselbe Prüfung; SVG über Erlaubnisliste; Pfade, Größe, Verhältnis begrenzt                                                           |
| ADR bei Entscheidungen, die ein Kernprinzip berühren    | nicht nötig: kein Prinzip der Verfassung wird berührt (keine Geheimnisse, keine lokalen Pfade, keine Konfiguration von außen)                                                                                  |
| Phasen                                                  | Dateibrowser (044), Einstellungen (023), Sync (024) sind im Einsatz                                                                                                                                            |
| Conventional Commits, kein Squash, keine Agent-Spuren   | gilt für die Umsetzung                                                                                                                                                                                         |
| Keine Geheimnisse und keine lokalen Pfade im Repository | erfüllt; `SOURCES.md` nennt Quellen als Adresse und SHA, keine lokalen Pfade                                                                                                                                   |

Nach dem Entwurf (Phase 1) erneut geprüft: keine Verletzung, kein Eintrag unter „Complexity Tracking“.

### Geprüfte Kandidaten (graphify)

| Kandidat                                                                                               | Ergebnis                                                                                                                                                                  |
| ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `files_thumbnail` und `useFilesThumbnails`, `thumbnailCache.ts`                                        | Muster übernommen (rohe Bytes, `blob:`-URL, begrenzter Speicher); der Speicher wird wiederverwendet, wenn die Schnittstelle passt, sonst ein zweiter mit denselben Regeln |
| `passwordsIcons.ts` (`passwords_icon_preview`, `sniffImageType`)                                       | Muster für „Bild lazy holen, `null` bei Fehler“; kein gemeinsamer Code, weil Quelle und Schlüssel verschieden sind                                                        |
| `haex_bundle::format` (`path_rule_violation`, `PathChecker`, `limits`)                                 | wiederverwendet für Pfade im Archiv und Ordner; `read_archive` ist `.xt`-spezifisch und wird nicht benutzt                                                                |
| `extension_install_preview` / `extension_install`, `InstallDialog.vue`, `usePickedFile`, `PickedFile`  | Muster und Dateiauswahl übernommen (zwei Schritte, `content://` auf Android)                                                                                              |
| `preferences` (`set_pref`, `validate_value`, `onVaultTablesChanged`)                                   | wiederverwendet für die Wahl und das Neuladen; `vault_events.rs` bekommt den Tabellennamen                                                                                |
| `migrations_storage.rs` / `migrations_contacts.rs` (Familie mit `SYNCED_TABLES`)                       | Muster für `migrations_icon_sets.rs` und seine Tests                                                                                                                      |
| `ColorSchemeSetting.vue`, `AppearanceView.vue`, `Group`/`Row`/`OptionRow`, `AppearanceResetButton.vue` | wiederverwendet (Speichern beim Anklicken, Zurücksetzen mit Bestätigung)                                                                                                  |
| `kind.rs` (`viewer_kind`), `Entry`, `EncryptedInfo`, `encryptedLook`                                   | `viewer_kind` und `Entry` werden von `role.rs` gelesen; `encryptedLook` verliert die Symbol-Wahl, behält den Zustandstext                                                 |
| `onColorSchemeApplied` (`useColorScheme.ts`)                                                           | wiederverwendet für das neue `useAppliedScheme()`                                                                                                                         |
| `extension_blobs` (Zeile je Datei, Kopfzeile zuletzt)                                                  | verworfen, siehe research R3                                                                                                                                              |

## Project Structure

### Documentation (this feature)

```text
specs/053-file-icon-sets/
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
src-tauri/
├── icon-sets/
│   ├── phosphor/, material/      # NEU: Quellverzeichnisse (Manifest, Bilder, Lizenztexte)
│   ├── phosphor.icons.txt, phosphor.mapping.json   # NEU: Auswahl und Endungen für Phosphor
│   ├── phosphor.zip, material.zip  # NEU: gepackte Pakete (reproduzierbar, mit include_bytes! eingebunden)
│   └── SOURCES.md                # NEU: gepinnte Quellen (Adresse, Version, Commit-SHA oder Prüfsumme)
├── src/files/icons/
│   ├── mod.rs                    # NEU: Typen (IconQuery, IconAnswer), Rückfall-Tabelle
│   ├── role.rs                   # NEU: Eintrag -> Rolle (viewer_kind, no_access, EncryptedInfo)
│   ├── manifest.rs               # NEU: VS-Code-Manifest lesen, x-holzi, Tabellen bauen
│   ├── resolve.rs                # NEU: Reihenfolge Zustand/Name/Endung/Rolle/Vorgabe/Standard
│   ├── svg.rs                    # NEU: Bereinigung (quick-xml, Erlaubnisliste)
│   ├── raster.rs                 # NEU: PNG/WebP prüfen (image, Limits)
│   ├── pack.rs                   # NEU: ZIP öffnen und prüfen (Pfade, Limits, SHA-256), Speicher
│   ├── import.rs                 # NEU: Ordner/Archiv/VSIX lesen, Bericht, Paket schreiben
│   ├── store.rs                  # NEU: Tabelle, Zustand der Wahl, mitgelieferte Pakete
│   ├── commands.rs               # NEU: Befehle (files_icons_resolve, files_icon_image, ...)
│   └── *_tests.rs                # NEU: je Modul
├── src/identity/
│   ├── migrations_icon_sets.rs   # NEU: SQL, SYNCED_TABLES
│   ├── migrations_icon_sets_tests.rs
│   └── migrations.rs             # Eintragung 0035, HOLZI_TRIGGER_VERSION erhöhen
├── src/storage/preferences_commands.rs   # validate_value: files.iconSet
├── src/vault_events.rs           # Tabellenname haex_file_icon_sets
├── src/files/mod.rs              # pub mod icons
├── src/lib.rs                    # Befehle registriert
└── tests/fixtures/icon_sets/     # NEU: Theme, bösartige SVG, Archive, Bilder
scripts/
├── icon-sets/prepare.ts          # NEU: gepinnte Quelle -> Quellverzeichnis (filtern, färben, Manifest)
├── check-file-icons.ts           # NEU: reine Helfer (Gruppierung, Speicher)
└── e2e/scenarios/files-icon-sets.test.ts, files-icon-sets-two-devices.test.ts   # NEU
src/
├── composables/useFileIcons.ts, useAppliedScheme.ts      # NEU
├── lib/files/fileIcons.ts                                # NEU: Gruppierung, Schlüssel, Speicher (rein)
├── components/files/EntryIcon.vue                        # fragt useFileIcons statt fester Zuordnung
├── lib/files/encrypted.ts                                # encryptedLook: nur Zustandstext
├── components/settings/
│   ├── AppearanceView.vue                                # Gruppe „Icon-Set für Dateien“
│   ├── FileIconSetSetting.vue, FileIconSetPreview.vue    # NEU
│   ├── FileIconSetImport.vue                             # NEU: Auswahl, Bericht, Bestätigung
│   └── LicensesView.vue                                  # NEU: Seite „Lizenzen“
├── lib/settings/registry.ts, components/wm/appRoutes.ts  # Eintrag „Lizenzen“
├── i18n/locales/{de,en}.json
└── types/bindings/                                       # ts-rs
specs/…                                                   # diese Spec
```

**Structure Decision**: Die Rust-Seite wird ein Verzeichnis `files/icons/` nach dem Muster von `files/media/` und `files/encrypted/` (Untermodule mit `mod.rs` und Testdateien daneben); die Migration folgt der Familie `migrations_<name>.rs`; die Oberfläche bleibt in den Einstellungs-Komponenten unter `settings/`.

### Vorschlag für den Zuschnitt in Pull Requests

Die Spec nennt keine Reihenfolge; ein Vorschlag für `/speckit-tasks`, jeweils einzeln prüfbar:

1. **A – Kern**: Migration `0035`, `files/icons/` ohne Import-Befehle (Typen, Rolle, Manifest, Auflösung, SVG-Bereinigung, Bilder prüfen, Paket öffnen und prüfen, Packen eines Verzeichnisses, Speicher, Befehle `files_icons_resolve`, `files_icon_image`, `file_icon_sets_list`, `file_icon_set_licenses`), die Pakete `phosphor.zip` und `material.zip` (`SOURCES.md`, `prepare.ts`, Quellverzeichnisse), Tests.
2. **B – Anzeige und Auswahl**: `useFileIcons`, `EntryIcon.vue`, Gruppe in der Gestaltung, Wahl, Vorschau, Lizenzseite, E2E `files-icon-sets` (ohne Import).
3. **C – Import**: `import.rs` (Ordner, Archiv, VSIX), Befehle, Dialog, Fixtures, Android-Archiv, E2E-Erweiterung.
4. **D – Sync**: E2E `files-icon-sets-two-devices`, Zustände `syncing` und `damaged`, Handprüfung Android.

## Complexity Tracking

Keine Verletzungen.
