# Implementation Plan: Marketplace für Erweiterungen

**Branch**: `069-marketplace` | **Date**: 2026-10-28 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/069-marketplace/spec.md`

## Summary

HaexVault bekommt einen Katalog für Erweiterungen. Die ganze Netzwerk- und Prüflogik sitzt im Host, in
einem neuen Modul `src-tauri/src/marketplace/`; das Frontend zeigt nur an und ruft Tauri-Commands.

- **Katalog**: Der Host fragt alle eingeschalteten Marketplaces parallel (`GET /extensions`,
  `/categories`, `/extensions/{slug}`), mit fester Frist je Quelle, und führt die Antworten zusammen
  (gleichzeitig, jede Kachel mit ihrer Quelle, **keine** Zusammenlegung über Quellen, Fehler je Quelle). Den Installationsstand
  rechnet der Host aus der Erweiterungs-Registry (Spec 017) dazu, damit es eine Stelle gibt.
- **Download**: `marketplace_download` holt die signierte Download-Adresse, lädt die `.xt` gestreamt
  (≤ 64 MiB, genau `bundleSize` Bytes), prüft SHA-256 gegen `bundleHash` und legt die Bytes in **einen
  Speicherplatz im Host** (nichts auf der Platte, nichts im Webview).
- **Installieren**: `extension_install_preview` und `extension_install` nehmen statt `file: PickedFile`
  eine `BundleSource` (`file` oder `marketplace`). Der Marketplace-Fall liest die Bytes aus dem
  Speicherplatz und ruft denselben `install_preview` und `install` wie die Datei. `InstallDialog.vue`
  bekommt eine `source`-Eigenschaft; es gibt keine zweite Vorschau.
- **Marketplace-Liste (generisch)**: neue CRDT-Tabelle `haex_marketplaces`; jede Zeile ist gleichrangig
  (Name, Adresse mit beliebigem Pfadanteil, Ein/Aus, Rang, Auth-Art, Zugangsdaten-Verweis). Es gibt **keinen
  Sonderfall im Code**: der öffentliche Marketplace des Projekts ist nur die **Voreinstellung**, eine
  gewöhnliche Zeile mit fester Kennung `preset-public`, die ein neuer Vault mitbringt und die der Nutzer
  ändern oder löschen kann. Ein Vault ohne Marketplace bietet sie im Leerzustand per Tipp an.
- **Offenes Protokoll**: `contracts/marketplace-api.md` ist der normative Vertrag „Marketplace-Protokoll
  v1“ für jeden, der einen Marketplace betreiben will; der Host setzt über einen Marketplace nichts
  voraus außer diesem Vertrag (FR-034).
- **Zugangsdaten**: Verweis `credentials_item_id` auf einen Passwort-Eintrag (Spec 034/052), Tag
  `haex-marketplace`; ein eigenes Modul `marketplace/credentials.rs` liest ihn (Bearer: Passwortfeld ist
  das Token; Basic: Benutzername und Passwort) und meldet `present`/`missing`/`syncing`. Die Auswahl
  nutzt `CredentialsEntryPicker.vue`, das dafür eine Quelle als Eigenschaft bekommt statt `protocol`.
- **Oberfläche**: neue wm-App `system.marketplace` („Erweiterungen entdecken“), neue Unteransicht
  `extensions/marketplaces` in den Einstellungen mit Formular `extensions/marketplaces/:id`, ein
  Update-Hinweis in der Liste der installierten Erweiterungen.

Keine neue Abhängigkeit: `reqwest`, `sha2`, `semver`, `url`, `futures`, `tokio-util` sind da; Testserver
über `hyper` (Dev-Abhängigkeit, schon vorhanden).

## Technical Context

**Language/Version**: Rust 2021 (MSRV 1.95, `src-tauri`), TypeScript (Nuxt 4, Vue 3)

**Primary Dependencies**:

- **Vorhanden**:
  - `reqwest` 0.13 (rustls, stream, json, query), `sha2` 0.11, `semver` 1, `url`, `futures`, `tokio-util`
  - `PasswordsService`, `passwords::usage::EntryUsage` (Löschwarnung, Spec 034 FR-034)
  - `extensions::registry::{install_preview, install, list}`, `limits::ARCHIVE_BYTES` (64 MiB)
  - `tauri::ipc::Channel` für den Fortschritt (wie `files_watch`, `TransferManager`)
  - Muster: `extensions/web.rs` (reqwest-Client, Weiterleitungen), `remote_storage/credentials.rs`
    (Zugangsdaten-Eintrag), `vault-data-changed` und `onVaultTablesChanged` (Spec 028-Live-Daten)
- **Neu**: keine.

**Storage**:

- Neue CRDT-Tabelle `haex_marketplaces` (Migration `0047_marketplaces`, nächste freie Nummer beim Bauen;
  `HOLZI_TRIGGER_VERSION` + 1). Kein Geheimnis in der Tabelle.
- Der Speicherplatz für den geladenen Bundle ist nur Arbeitsspeicher im `AppState`, höchstens ein Bundle.
- Bilder-Zwischenspeicher (Symbole) nur im Arbeitsspeicher, je Sitzung.

**Testing**:

- **Rust**: `*_tests.rs` neben jedem Modul, nie inline. Ein Test-Marketplace aus `hyper` (wie bei den
  WebDAV-Tests) liefert Katalog, Download und Fehlerfälle: Prüfsumme falsch/fehlt, Größe falsch, zu
  groß, Weiterleitung auf anderen Host, `http` mit Zugangsdaten, Frist, kaputte Antworten, 401.
- **Reine Teile ohne Netz**: Zusammenführen und Blättern, Fassungsvergleich (`semver`), Kompatibilität
  (`minAppVersion`/`maxAppVersion`), Voreinstellung als gewöhnliche Zeile.
- **Vault-Test**: Tabelle, Voreinstellung (anlegen, löschen, nicht wieder anlegen), Zugangsdaten-Zustand gegen einen Test-Vault.
- **Frontend**: keine eigene Logik, die Einheitstests brauchte (sie liegt im Host); Typprüfung, Lint,
  `check:templates` und e2e.
- **E2E** (tauri-driver-Rig, Spec 016): ein lokaler Test-Marketplace (Node, im Rig) mit zwei Erweiterungen,
  eine davon signiert und installierbar; Desktop und Android-Emulator (SC-008); Zwei-Geräte-Fall für
  SC-007 (`marketplace-two-devices.test.ts`, Muster `contacts-two-devices.test.ts`).

**Target Platform**: Linux, Windows, macOS, Android, iOS (kein plattformabhängiger Code außer dem
Fassungs-Kopf `X-Platform`)

**Project Type**: Desktop- und Mobil-App (Tauri 2 mit Nuxt-Frontend)

**Performance Goals**: erste Katalogseite sichtbar in < 3 s bei erreichbarem Marketplace; ein
ausgefallener Marketplace verlängert das nicht über die Frist (R5); Symbole laden nachgelagert.

**Constraints**:

- Keine Zugangsdaten über `http://` oder an einen anderen Host (FR-027).
- Prüfsumme Pflicht, 64 MiB, genau `bundleSize` (FR-013, FR-014).
- Kein Agent, keine Erweiterung erreicht den Katalog (FR-030).
- Dateien ≤ 500 Zeilen, Tests getrennt, Clippy nach Repo-Konfiguration.

**Scale/Scope**: eine bis wenige Quellen, einige Hundert Erweiterungen je Quelle, bis zu 20 Einträge je
Quelle und Seite (eine gemeinsame Seite enthält entsprechend der Quellenzahl mehr Einträge).

## Constitution Check

_GATE: vor Phase 0 geprüft, nach Phase 1 erneut._

| Prinzip                                  | Stand                                                                                                                                                                                                                        |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git              | Erfüllt. Spec, Plan und Testserver enthalten nur Platzhalter; Test-Token sind Zufallswerte zur Laufzeit. Die Beispiel-Adressen in den Verträgen sind öffentlich und tragen kein Token (die signierte Adresse ist abgekürzt). |
| II. Keine lokalen absoluten Pfade        | Erfüllt. Verweise sind repo-relativ.                                                                                                                                                                                         |
| III. Projekt-Identität gerätelos         | Nicht berührt.                                                                                                                                                                                                               |
| IV. Cross-Repo-Referenzen mit festem SHA | Erfüllt. haex-vault 2.x ist mit `8dce379d…` angegeben, das `marketplace-sdk` mit Fassung 0.1.6; der Dienst selbst ist ein Live-Endpunkt (siehe R1, Risiko).                                                                  |
| V. Externe Quellen nur per Allowlist     | Nicht berührt (keine neue Harness-Quelle).                                                                                                                                                                                   |
| VI. Selbständernde Anweisungen           | Nicht berührt.                                                                                                                                                                                                               |
| VII. Relay-Ausfall blockiert nichts      | Entsprechend: Marketplace-Ausfall blockiert nichts Lokales; Installation aus Datei bleibt (FR-031, Edge Cases).                                                                                                              |
| VIII. Keine Verschleierung               | Nicht berührt.                                                                                                                                                                                                               |

Weitere MUSS-Regeln der spaex-Constitution, die hier greifen:

- **Worktree und PR**: Arbeit liegt in `.worktrees/069-marketplace`; Docs-PR, danach gestufte PRs, kein
  Squash-Merge, Conventional Commits.
- **Graphify zuerst**: vor jedem neuen Namen eine `graphify query`. Geprüft für den Plan: Zugangsdaten
  (`remote_storage/credentials.rs`, `CredentialsEntryPicker.vue`) werden **erweitert**, nicht kopiert;
  die Weiterleitungsregeln aus `extensions/web.rs` sind an Berechtigungen gebunden und werden nicht
  wiederverwendet (R6).
- **Eine Implementierung pro Regel**: Installations-Vorschau und -Installation bleiben eine
  Implementierung (`BundleSource`); Fassungsvergleich nutzt `semver` wie die Registry.
- **Tests getrennt, deterministisch, ohne Netz**: Testserver lokal, Uhr und Frist einspeisbar.
- **Nicht über 500 Zeilen**: das Modul ist vorab in sieben Dateien geschnitten (siehe Struktur).
- **ADR**: nicht nötig, keine Kernprinzip-Entscheidung. Die Entscheidungen stehen in `research.md`.

Nach Phase 1: erneut geprüft, keine Verletzung; Complexity Tracking bleibt leer.

## Project Structure

### Documentation (this feature)

```text
specs/069-marketplace/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── marketplace-api.md   # Was HaexVault vom Dienst erwartet (Vertrag nach außen)
│   ├── commands.md          # Tauri-Commands, Typen, Fortschritt
│   └── ui.md                # App, Einstellungen, Test-IDs
├── checklists/requirements.md
└── tasks.md                 # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/marketplace/
├── mod.rs              # Zustand (AppState-Teil), Frist- und Größenkonstanten
├── model.rs            # Katalog- und Listentypen (ts-rs), Eingaben
├── store.rs            # Tabelle haex_marketplaces, Voreinstellung (preset-public), Lesen und Schreiben
├── client.rs           # reqwest: Anfragen, Kopfzeilen, Weiterleitungsregel, Fehlerarten
├── catalog.rs          # parallele Abfrage, Zusammenführen, Blättern, Installationsstand, Kompatibilität
├── download.rs         # Stream, Prüfsumme, Größe, Speicherplatz, Fortschritt, Abbruch
├── credentials.rs      # Zugangsdaten-Eintrag (Zustand, Lesen, Tag), Auth-Kopfzeile
├── updates.rs          # bekannte Updates im Arbeitsspeicher für die Liste der Installierten
├── commands.rs         # Tauri-Commands
└── *_tests.rs          # je Datei
src-tauri/src/identity/migrations_marketplaces.rs   # Migration 0047
src-tauri/src/extensions/commands/install.rs        # BundleSource (ein Ort)
src-tauri/src/extensions/registry/install.rs        # read_bundle: Datei oder Speicherplatz

src/lib/marketplace/        # registry.ts (Routen), compat/Anzeige-Helfer
src/composables/useMarketplaceCatalog.ts, useMarketplaces.ts, useMarketplaceInstall.ts
src/components/marketplace/ # Katalog, Kachel, Details, Filter, Quellen-Fehler
src/components/apps/MarketplaceApp.vue
src/components/settings/extensions/ (Marketplaces-Liste, Formular)
src/components/extensions/InstallDialog.vue         # source-Eigenschaft
src/lib/wm/apps.ts, src/components/wm/appRoutes.ts  # App registrieren
src/i18n/locales/{de,en}                            # Texte
scripts/e2e/                                        # Test-Marketplace und Szenarien
```

**Structure Decision**: ein Rust-Modul je Verantwortung (Netz, Zusammenführen, Download,
Zugangsdaten, Speicher), gemeinsame Installationsroutine bleibt in `extensions/`, Frontend folgt dem
Muster der bestehenden Apps (`lib/<app>/registry.ts`, `components/apps/<App>.vue`).

### Stufen (PR-Zuschnitt)

Die Zuordnung der Aufgaben steht in `tasks.md` (Shipping-Tabelle):

1. **A – Kern** (T004–T024): Migration, `store` samt Voreinstellung und Anlegen bei einem neuen Vault,
   `client`, `catalog`, Typen, Commands `marketplace_list`, `marketplace_catalog`, `marketplace_categories`,
   `marketplace_details`, `marketplace_icon`; Rust-Tests mit Testserver. Keine Oberfläche.
2. **B – Download und Installation** (T032–T039): `download`, Speicherplatz, `BundleSource`, alle Aufrufer,
   `InstallDialog`-Quelle, Fortschritt, Abbruch. Rust-Tests.
3. **C – Katalog-App** (T025–T031, T040–T041): `MarketplaceApp`, Kacheln, Details, Suche, Kategorien,
   Quelle, Blättern, Symbole, Installationsablauf. e2e Desktop.
4. **D – Updates** (T042–T046): bekannte Updates je Eintrag, Update-Prüfung für alle Installierten,
   Kompatibilitätssperre, Entfernen, Hinweis in der Liste der Installierten.
5. **E – Verwaltung** (T047–T055): Einstellungsansicht, Formular mit Test, Reihenfolge, Ein/Aus, mehrere
   Quellen gleichzeitig, Leerzustand mit Voreinstellung.
6. **F – Private Marketplaces** (T056–T063): Zugangsdaten-Eintrag, Picker-Erweiterung, Löschwarnung.
7. **G – Abschluss** (T064–T067): Zwei-Geräte-e2e, Android, Glossar, Handprüfung (Quickstart).

## Complexity Tracking

Keine Verletzungen; nichts zu begründen.
