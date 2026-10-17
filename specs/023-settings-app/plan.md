# Implementation Plan: Einstellungs-App mit Kategorien

**Branch**: `023-settings-app-plan` | **Date**: 2026-09-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/023-settings-app/spec.md`

## Summary

Die Einstellungen bekommen den Aufbau aus haex-vault 2.x (Legacy): links eine Seitenleiste
mit den Kategorien Allgemein, Darstellung, Modelle, Agenten und Föderation,
rechts ein Kopf mit dem Titel und darunter der Inhalt als zentrierte Liste. Modelle und Agenten
zeigen eine Übersicht mit Unteransichten. Jede Ansicht ist ein Ort im Tab (Spec
020). Neu ist ein Farbschema (Hell, Dunkel, System). Die eigene
Föderations-App entfällt; alte Aufrufe führen in die Kategorie „Föderation“, die
die Geräte der Vault zeigt. Keine Einstellung hat mehr einen Knopf zum
Speichern.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Orte statt interner Zustände** (R1): eine flache Routentabelle für
  `system.settings`; `/` ist „Allgemein“; der HuggingFace-Suchbegriff steht in
  der Query.
- **Ein Register** (R2) in `src/lib/settings/registry.ts` liefert Routenmuster,
  Seitenleiste, Übersichtszeilen, Kopf und Tab-Titel; rein und unter Node
  testbar.
- **Zurück im Kopf** (R3): wie Zurück im Tab, wenn die übergeordnete Ansicht die
  vorige Station ist, sonst Navigation dorthin.
- **Schmale Fenster** (R4): Container-Abfragen, unter 672 px nur Symbole.
- **Sofort speichern** (R5): Auswahlen beim Wählen, Textfelder beim Verlassen;
  „nicht festgelegt“ als Option statt Zurücksetzen-Knopf.
- **Farbschema** (R8, R14): Präferenz `appearance.color_scheme` für die Vault,
  Klasse `dark` an `<html>`, eine neue Aktion; alte Gerätewerte werden beim
  Öffnen übernommen.
- **Theme-Farben** (R9): rund 170 feste Farben auf Theme-Farben umstellen, damit
  das dunkle Schema überall lesbar ist; ein Check verhindert neue.
- **Föderation** (R7, R12): App raus, Alias `system.federation` → Einstellungen
  `/federation`; dort die Geräte der Vault über einen neuen Lesebefehl
  `list_vault_devices` und die Lese-Aktion `settings.devices.list`.
- **Modellverwaltung aufteilen** (R6, R10): Download-Fortschritt in den
  Modell-Store; die 580-Zeilen-Komponente zerfällt entlang der Orte.

## Technical Context

**Language/Version**: TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA), Node 22;
Rust (Tauri 2.11) nur für den Lesebefehl der Geräteliste

**Primary Dependencies**: nur Vorhandenes — haex-ui-Layer (Pin
`db48f9a948522c18a00331aac232718825cc9317`, seit T069 `b8549be89880bc26046c994cc7e2d7c082db2a63`, `.dark`-Theme, `UiButton` mit
Tooltip), Tailwind v4 (Container-Abfragen), Pinia, vue-i18n, reka-ui

**Storage**: Präferenz `appearance.color_scheme` in der vorhandenen Tabelle
`preferences` über `get_pref`/`set_pref`/`clear_pref`; die Geräteliste aus
`known_devices`; Migration alter Scope-Werte beim Öffnen

**Testing**: neu `known_devices_tests.rs` (`cargo test --manifest-path
src-tauri/Cargo.toml known_devices`); neu
`check:settings` (Register, Orte, `headerBack`, Farbschema, Alias,
de/en-Schlüssel); `check:templates` mit Sperrliste für Palettenfarben;
Regression `check:wm-state`, `check:wm-navigation`, `check:chat-state`,
`check:vault-lifecycle`, `typecheck`, `typecheck:scripts`, `lint`,
`format:check`, e2e; manuell nach [quickstart.md](./quickstart.md)

**Target Platform**: Tauri-Desktop (Linux, macOS, Windows); Android ohne
Besonderheiten

**Project Type**: desktop-app (Nuxt-SPA + Rust-Backend in einem Tauri-Projekt)

**Performance Goals**: SC-003 — Kategoriewechsel und Unteransicht ≤ 100 ms bei
geladenen Daten (nur Komponentenwechsel, kein IPC nötig); SC-005 — Farbschema
< 1 s in allen Fenstern (eine Klasse in einer Webview)

**Constraints**: Dateien ≤ 500 Zeilen (die 580-Zeilen-Komponente wird
aufgeteilt); Einstellungsänderungen nur über Katalog-Aktionen (Spec 020 FR-024,
`check:templates`); Aktions-Schemas ohne `null` (Spec 022); Route-Komponenten
ohne Props

**Scale/Scope**: 5 Kategorien, 14 Orte, 2 neue Aktionen, 1 neuer Tauri-Befehl,
rund 28 Dateien mit Farbumstellung

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die
spaex-Constitution `.spaex/constitution.md`.

| Prinzip / Vorgabe                                                          | Status | Begründung                                                                                                                                       |
| -------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| I Keine Geheimnisse in Git                                                 | ✅     | Keine berührt; `settings.get` liefert weiter keine Zugangsdaten                                                                                  |
| II Keine lokalen absoluten Pfade in versionierter Konfiguration            | ✅     | Nur repo-relative Pfade                                                                                                                          |
| III Projektidentität geräteunabhängig                                      | ✅     | Berührt nicht                                                                                                                                    |
| IV Cross-Repo-Referenzen an unveränderliche SHAs gepinnt                   | ✅     | haex-vault 2.x `8dce379d94e18fcd42c3b73686a06f984ca3f574`, haex-ui `db48f9a948522c18a00331aac232718825cc9317`                                    |
| V Externe Quellen nur per Opt-in                                           | ✅     | Keine neue Quelle; haex-vault 2.x nur als Vorbild gelesen                                                                                        |
| VI Selbstverändernde Anweisungen review-pflichtig                          | ✅     | Keine Änderung an Constitution oder Skills                                                                                                       |
| VII Relay-Ausfall blockiert lokale Arbeit nicht                            | ✅     | Rein lokal                                                                                                                                       |
| VIII Keine Verheimlichung in Agent-Ausgaben                                | ✅     | –                                                                                                                                                |
| Workflow: speckit-Stufen, PR auf `main`, Conventional Commits, kein Squash | ✅     | Spec über #144/#147; Plan im Topic-Branch `023-settings-app-plan`                                                                                |
| ADR bei prinzipienrelevanter Entscheidung                                  | ✅     | Keine; die Betreiberentscheidung in R14 legt den Vault als Scope der Einstellungen fest                                                          |
| Test-Code in separaten Dateien                                             | ✅     | `known_devices_tests.rs`, `device/commands_tests.rs`, `scripts/check-settings.ts`, Erweiterung von `check-vue-templates.ts`                      |
| Worktree je Änderung                                                       | ✅     | `.worktrees/023-settings-app`                                                                                                                    |
| 500-LoC-Grenze                                                             | ✅     | Löst die Ausnahme von `HuggingFaceModelManagement.vue` auf; neue Dateien klein                                                                   |
| Graphify vor neuen benannten Artefakten                                    | ⚠️     | Abfragen in research.md; der Graph (21.09.) ist älter als 015/020/022, Kandidaten zusätzlich im Code geprüft; zur manuellen Nachprüfung vermerkt |
| `ponytail:`-Kommentar bei bewusster Vereinfachung                          | ✅     | Geplant an `watchDownloads` (Abo lebt bis Prozessende, R6)                                                                                       |
| Nicht-triviale Logik hinterlässt einen ausführbaren Check                  | ✅     | `check:settings`, Farb-Sperrliste                                                                                                                |
| Keine Selbstreferenzen von Agenten in Artefakten/Commits                   | ✅     | Wird bei Commits eingehalten                                                                                                                     |
| **Phasen-Disziplin**                                                       | ✅     | Setzt 015, 020 und 022 voraus, alle auf `main` und im Einsatz                                                                                    |

**Ergebnis vor Phase 0**: kein Verstoß; die Graphify-Einschränkung ist eine
Warnung nach der Regel für fehlgeschlagene oder unbrauchbare Abfragen.

**Ergebnis nach Phase 1**: unverändert. Keine neue Abhängigkeit; die
Migration übernimmt alte Scope-Werte beim Öffnen. Ein Lesebefehl über eine
vorhandene Tabelle und zwei neue Aktionen (eine davon nur lesend).

## Project Structure

### Documentation (this feature)

```text
specs/023-settings-app/
├── plan.md                    # Dieses Dokument
├── research.md                # Phase 0 (R1–R12)
├── data-model.md              # Phase 1: Kategorie, Ort, Farbschema, Download-Fortschritt, Gerät, Alias
├── quickstart.md              # Phase 1: automatische und manuelle Validierung (S1–S18)
├── contracts/
│   └── settings-app.md        # Orte, Gerüst, Aktionen, Farbschema, Alias, Store, i18n
├── checklists/requirements.md # Spec-Qualitätscheckliste
└── tasks.md                   # Phase 2 — NICHT von /speckit-plan erzeugt
```

### Source Code (repository root)

```text
src-tauri/src/
├── storage/
│   ├── known_devices.rs           # list_devices (ohne Vault-Bereichszeile)
│   ├── known_devices_tests.rs     # NEU
│   └── mod.rs                     # Testmodul anmelden
├── device/commands.rs             # list_vault_devices, VaultDevicePayload, Sortierung
├── device/commands_tests.rs       # NEU: Sortierung der Geräte
└── lib.rs                         # Befehl registrieren

src/
├── composables/useDevice.ts       # listVaultDevicesAsync, VaultDevice
├── lib/settings/
│   ├── registry.ts                # NEU: Kategorien, Orte, settingsRoutePatterns, locationFor, overviewRows, headerBack
│   └── colorScheme.ts             # NEU: parseColorScheme, isDark, Ergebnis-Projektion
├── lib/wm/apps.ts                 # system.federation raus; LEGACY_APP_ALIASES, resolveAppAlias; tabTitle, tabTitleFor
├── lib/actions/settingsActions.ts # settings.appearance.setColorScheme, settings.devices.list
├── composables/useColorScheme.ts  # NEU: Zustand, Klasse `dark`, Medienabfrage
├── plugins/colorScheme.client.ts  # NEU: „System“ beim Start
├── stores/
│   ├── modelDownloads.ts          # NEU: downloads, watchDownloads, clearDownload
│   ├── windowManager.ts           # tabDisplayInfo über tabTitleFor
│   ├── settingsActionHandlers.ts  # Farbschema-Handler, settings.devices.list, settings.get + colorScheme
│   └── wmActionHandlers.ts        # Alias vor knownApp
├── components/
│   ├── apps/SettingsApp.vue       # NEU aufgebaut: Gerüst (Seitenleiste, Kopf, WmRouterView), provide Gerät
│   ├── apps/FederationApp.vue     # ENTFÄLLT
│   ├── wm/appRoutes.ts            # Routen der Einstellungen aus dem Register; App-Föderation raus
│   ├── settings/
│   │   ├── Sidebar.vue            # NEU
│   │   ├── deviceContext.ts       # NEU: SETTINGS_DEVICE_KEY (provide/inject des Geräts)
│   │   ├── FederationView.vue     # NEU: Geräte der Vault
│   │   ├── OverviewView.vue       # NEU: Übersichtszeilen einer Kategorie
│   │   ├── GeneralView.vue        # NEU: Gerätename + Sitzung wiederherstellen
│   │   ├── ColorSchemeSetting.vue # NEU
│   │   ├── InstalledModels.vue    # NEU (aus HuggingFaceModelManagement)
│   │   ├── DownloadModels.vue     # NEU (aus HuggingFaceModelManagement)
│   │   ├── AliasSetting.vue, DefaultModelSetting.vue, SttModelSetting.vue,
│   │   │   AutonomyModeSetting.vue, DelegateDenyRulesSetting.vue   # speichern ohne Knopf (R5), Gerät per inject
│   │   └── ConnectDelegateProvider.vue, SessionRestoreSetting.vue  # nur Farben/inject
│   └── models/
│       ├── HuggingFaceModelManagement.vue  # ENTFÄLLT (aufgeteilt)
│       ├── HuggingFaceSearch.vue  # q über den Tab-Router, Ergebnis → Repo-Ort
│       └── HuggingFaceFilePicker.vue # owner/name aus der Route, Fortschritt aus dem Store
├── pages/
│   ├── workspace/[instance].vue   # nach dem Öffnen: useColorScheme().loadAsync, useModelDownloadsStore().watchDownloads
│   └── federation/[instance].vue  # Umleitung auf ?open=system.settings&at=/federation
├── **/*.vue (28 Dateien)          # feste Farben → Theme-Farben (R9, eigener Commit)
└── i18n/locales/{de,en}.json      # Kategorien, Orte, Farbschema, Geräte, Aktionen; Speichern-Texte raus

scripts/
├── check-settings.ts              # NEU (package.json `check:settings`, CI)
├── check-vue-templates.ts         # Sperrliste für Palettenfarben
└── check-vault-lifecycle.ts       # Föderations-Tests raus
.github/workflows/ci.yml           # Schritt check:settings
specs/015-workspace-shell/spec.md  # Vermerk: Föderations-App entfällt (023)
CONTEXT.md                         # Begriffe „Einstellungskategorie“, „Farbschema“
plans/README.md                    # Zeile 023
```

**Structure Decision**: Bestehende Tauri-Struktur. Im Backend nur ein
Lesebefehl neben `current_device_info`. Reine Logik
unter `src/lib/settings/` (unter Node testbar wie `src/lib/wm/`), Komponenten
unter `src/components/settings/` (Auto-Import `Settings*`).

## Anforderungen → Umsetzung

| Anforderungen                                            | Umsetzung                                                                                                                |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| FR-001, FR-002 (Aufbau, Kopf)                            | `SettingsApp.vue`, `Sidebar.vue`, `locationFor`                                                                          |
| FR-003 (Übersicht, Einzelbereich direkt)                 | `OverviewView.vue`, `overviewRows`; Allgemein/Darstellung/Föderation ohne Übersicht                                      |
| FR-004 (schmale Fenster)                                 | `@container`, Schwelle `@2xl`; ausblendbar, schmal als Vollbild-Menü (R4)                                                |
| FR-005–007 (Kategorien, Zuordnung)                       | `registry.ts`, Routen in `appRoutes.ts`; S3                                                                              |
| FR-008, FR-010–012 (Orte, Start, Deep-Link, Unbekanntes) | Routentabelle (R1), `wm.app.open` mit `at`, `RouterView`-Rückfall; Tab-Titel „Einstellungen“ über `tabTitle: 'app'` (R2) |
| FR-009 (Zurück im Kopf)                                  | `headerBack` (R3)                                                                                                        |
| FR-013, FR-014 (Farbschema)                              | `colorScheme.ts`, `useColorScheme`, Plugin, zwei Aktionen (R8)                                                           |
| FR-015 (kein Hintergrund)                                | nichts zu tun                                                                                                            |
| FR-016–018 (Föderation-App)                              | `apps.ts` Alias auf `/federation`, `FederationApp.vue` und Tests raus, Umleitung (R7)                                    |
| FR-022 (Geräte der Vault)                                | `known_devices::list_devices`, `list_vault_devices`, `settings.devices.list`, `FederationView.vue` (R12)                 |
| FR-019 (Verhalten unverändert)                           | bestehende Aktionen; Regressionschecks                                                                                   |
| FR-020 (de, en)                                          | i18n; `check:settings` prüft Schlüssel in beiden Sprachen                                                                |
| FR-021 (ohne Speichern-Knopf)                            | Umstellung der Einstellungskomponenten (R5)                                                                              |
| FR-023 (Suche)                                           | `lib/settings/search.ts`, `keywordsKey`/`settingKeys` in der Registry, Suchfeld in `Sidebar.vue` (R13)                   |
| Edge Case Download läuft weiter                          | `stores/modelDownloads.ts`, `watchDownloads` (R6)                                                                        |
| Complexity Tracking aus Spec 020                         | Aufteilung der Modellverwaltung (R10)                                                                                    |

## Complexity Tracking

Keine Einträge. Die Ausnahme von `HuggingFaceModelManagement.vue` (580 Zeilen)
aus Spec 020 wird mit dieser Spec aufgelöst.
