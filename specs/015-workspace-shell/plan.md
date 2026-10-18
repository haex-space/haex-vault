# Implementation Plan: Workspace-Shell (Arbeitsbereiche, Apps und Fenster)

**Branch**: `015-workspace-shell` | **Date**: 2026-09-21 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/015-workspace-shell/spec.md`

## Summary

holzi bekommt eine Shell nach dem Vorbild von haex-vault 2.x (Legacy): Arbeitsbereiche
enthalten Fenster, jedes Fenster enthält einen oder mehrere Tabs, jeder Tab ist
eine App-Instanz. Chat, Einstellungen und Föderation (Platzhalter) wandern von
Vollseiten zu Apps; die alten Routen bleiben als Redirects. Die Titelleiste
folgt der Firefox-Bedienung: „+“ unmittelbar hinter dem letzten Tab, ein Chevron
mit der Tab-Liste vor Minimieren/Maximieren/Schließen.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Persistenz im Rust-Vault-Storage** (R1–R5): drei gerätebezogene CRDT-Tabellen
  (`workspaces`, `shell_windows`, `shell_window_tabs`, Migration `0019`) nach
  ADR-0001 plus ein Preference-Schlüssel für den aktiven Arbeitsbereich; sechs
  Tauri-Commands, das Backend löst das Gerät selbst auf und erzwingt die
  Invarianten in Transaktionen.
- **Reine Frontend-Logik** (R6, R7, R9): Fenster-/Tab-/Geometrie-Reducer als reine
  TypeScript-Module unter `src/lib/shell/`, ein dünner Pinia-Store, Persistenz
  über eine serielle Schreib-Queue mit Debounce für Geometrie.
- **Inhalt bleibt erhalten** (R8): Tabs werden lazy gemountet und danach nur
  per `v-show` verborgen.
- **Chat zuerst zerlegen, dann Seiten → Apps** (R10, R11): Die 1 320-Zeilen-Chat-Seite
  wird verhaltensgleich in Composables und Kindkomponenten geteilt (`check:chat-state`
  als Netz); danach ziehen Chat, Einstellungen und Föderation per `git mv` in
  `components/apps/`, und der Modell-Preload-Status des Stubs wandert in die
  Shell-Statusleiste.
- **Keine neuen Abhängigkeiten.** Oberfläche mit haex-ui-Layer
  (`UiDrawerModal`, `ShadcnAlertDialog`, `ShadcnDropdownMenu`), `@vueuse/core`
  (`useWindowSize`), `@lucide/vue`.

## Technical Context

**Language/Version**: Rust (Tauri 2, Toolchain des Repos), TypeScript 6 (strict),
Vue 3.5, Nuxt 4.5.2 (SPA, `ssr: false`), Node 22.19

**Primary Dependencies**: nur Vorhandenes — haex-crdt (gepinnter Rev `b8c9c6c…`),
ts-rs, Tauri 2, Pinia 4, `@vueuse/core` 15, haex-ui-Layer (shadcn-vue/reka-ui),
Tailwind 4, `@lucide/vue`, `@nuxtjs/i18n`

**Storage**: SQLite über haex-crdt; Migration `0019_shell_layout` (drei
CRDT-getrackte, gerätebezogene Tabellen) und ein Schlüssel in `preferences`
(`shell.active_workspace_id`, Device-Scope); `HOLZI_TRIGGER_VERSION` 10 → 11

**Testing**: `cargo test` (getrennte `*_tests.rs`), neues `pnpm check:shell-state`
(Node-Type-Stripping, Vorbild `check-chat-state.ts`), Regression `check:chat-state`,
`check:templates`, `typecheck`, `typecheck:scripts`, `lint`, `format:check`;
manuell nach [quickstart.md](./quickstart.md)

**Target Platform**: Tauri-Desktop (Linux, macOS, Windows); die
Kompaktdarstellung (< 768 px) bereitet mobil vor, mobil selbst ist nicht Teil

**Project Type**: desktop-app (Nuxt-SPA-Frontend + Rust-Backend in einem Tauri-Projekt)

**Performance Goals**: SC-002 — bei zehn Fenstern ≤ 50 ms sichtbare Verzögerung
beim Ziehen und Größenändern, ≤ 100 ms für Fokus/Minimieren; SC-009 —
Tab-Aktionen ≤ 100 ms, jeder Tab eines Fensters mit zehn Tabs in ≤ 2 Klicks

**Constraints**: Dateien ≤ 500 Zeilen (mögliche Ausnahme nur das Prüfskript, unten); Backend liefert keine
lokalisierten Texte; besuchte Tabs bleiben im Speicher (Grenze: Dutzende, R8);
Module in `src/lib/shell/` importieren relativ mit `.ts`-Endung, ohne `~/`-Alias
(Node-Harness, R6)

**Scale/Scope**: realistisch ≤ 10 Arbeitsbereiche, ≤ 30 Fenster, ≤ 100 Tabs je Gerät;
harte Grenzen im Command-Vertrag (≤ 500 Fenster, ≤ 100 Tabs je Fenster)

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die
spaex-Constitution `.spaex/constitution.md`.

| Prinzip / Vorgabe                                                                               | Status | Begründung                                                                                                                                                                                                                                                                    |
| ----------------------------------------------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                                                                      | ✅     | Keine Geheimnisse, keine Schlüsselmaterialien                                                                                                                                                                                                                                 |
| II Keine lokalen absoluten Pfade in versionierter Konfiguration                                 | ✅     | Spec/Plan/Verträge nutzen repo-relative Pfade                                                                                                                                                                                                                                 |
| III Projektidentität geräteunabhängig                                                           | ✅     | Berührt nicht                                                                                                                                                                                                                                                                 |
| IV Cross-Repo-Referenzen an unveränderliche SHAs gepinnt                                        | ✅     | haex-vault 2.x `8dce379d94e18fcd42c3b73686a06f984ca3f574` in Spec und Research; haex-ui-Layer bleibt im bestehenden Pin                                                                                                                                                       |
| V Externe Quellen nur per Opt-in                                                                | ✅     | Keine neue Quelle                                                                                                                                                                                                                                                             |
| VI Selbstverändernde Anweisungen review-pflichtig                                               | ✅     | Keine Änderung an Constitution, Skills oder Anweisungen; `CONTEXT.md` (Glossar) läuft durch den PR-Review                                                                                                                                                                     |
| VII Relay-Ausfall blockiert lokale Arbeit nicht                                                 | ✅     | Rein lokal                                                                                                                                                                                                                                                                    |
| VIII Keine Verheimlichung in Agent-Ausgaben                                                     | ✅     | –                                                                                                                                                                                                                                                                             |
| Workflow: speckit-Stufen, PR auf `main`, Conventional Commits                                   | ✅     | specify → plan → tasks → implement mit Review-Gates; Topic-Branch im Worktree; keine Freihand-Änderungen am Hauptauftrag                                                                                                                                                      |
| ADR bei prinzipienrelevanter Entscheidung                                                       | ✅     | Keine: ADR-0001 wird befolgt, kein Prinzip berührt; ADR-0004 (Extension-Protokoll) betrifft 017/018, nicht diese Spec                                                                                                                                                         |
| Test-Code in separaten Dateien                                                                  | ✅     | Rust `*_tests.rs`, Frontend-Prüfung als Skript unter `scripts/`                                                                                                                                                                                                               |
| Worktree je Änderung, Primär-Checkout nur Inspektion                                            | ✅     | `.worktrees/015-workspace-shell`                                                                                                                                                                                                                                              |
| 500-LoC-Grenze                                                                                  | ✅     | Die Chat-Seite (1 320 Zeilen) wird als erster Schritt zerlegt (R10); nur das Prüfskript kann die Grenze berühren (Complexity Tracking)                                                                                                                                        |
| Graphify vor neuen benannten Artefakten                                                         | ✅     | Abfragen und Ergebnisse in research.md; `preferences`, `usePreferences`, `useDevice`, `known_devices` werden wiederverwendet, Preload-Status wird verschoben                                                                                                                  |
| `ponytail:`-Kommentar bei bewusster Vereinfachung                                               | ✅     | Geplant an R8 (Tabs bleiben gemountet), R13 (Flush best-effort) und an R2, falls der Fremdschlüssel-Spike scheitert                                                                                                                                                           |
| Nicht-triviale Logik hinterlässt einen ausführbaren Check                                       | ✅     | `cargo test`-Invarianten, `check:shell-state`                                                                                                                                                                                                                                 |
| Keine Rust-Anti-Muster (`unwrap` auf Eingaben, blockierender Executor, neue Crates ohne Bedarf) | ✅     | `spawn_blocking`, keine neuen Crates, Fehler als `HolziError` mit Ursache                                                                                                                                                                                                     |
| Keine Selbstreferenzen von Agenten in Artefakten/Commits                                        | ✅     | Wird bei Commits eingehalten                                                                                                                                                                                                                                                  |
| **Phasen-Disziplin** (spätere Phase nicht vor ihren Voraussetzungen)                            | ⚠️     | `plans/README.md` führt keine Shell-Phase; die Spec wurde vom Betreiber am 2026-09-21 angefordert. Planen ist erlaubt; **vor `/speckit-implement` bestätigt der Betreiber die Roadmap-Einordnung**, `plans/README.md` bekommt einen Eintrag (Aufgabe in tasks) — research R17 |

**Ergebnis vor Phase 0**: kein unbegründeter Verstoß; zwei ⚠️ sind dokumentiert.

**Ergebnis nach Phase 1**: unverändert. Das Design fügt keine Abhängigkeit, keinen
Secrets-Pfad und keine Abweichung von ADR-0001 hinzu; die Tab-Erweiterung vom
selben Tag ändert Datenmodell und Verträge, aber keinen Punkt der Tabelle.

## Project Structure

### Documentation (this feature)

```text
specs/015-workspace-shell/
├── plan.md                        # Dieses Dokument
├── research.md                    # Phase 0 (R1–R18)
├── data-model.md                  # Phase 1: Tabellen, Invarianten, Zustandsübergänge
├── quickstart.md                  # Phase 1: automatische und manuelle Validierung
├── contracts/
│   ├── tauri-commands.md          # Sechs Shell-Commands, DTOs, Fehler, Aufrufreihenfolge
│   └── shell-app-contract.md      # App-Definition, useShellTab(), Routen, Shell-Aktionen
├── checklists/requirements.md     # Spec-Qualitätscheckliste
└── tasks.md                       # Phase 2 — NICHT von /speckit-plan erzeugt
```

### Source Code (repository root)

```text
src-tauri/src/
├── identity/migrations.rs         # + 0019_shell_layout, HOLZI_TRIGGER_VERSION 10 → 11
├── storage/
│   ├── mod.rs                     # + Modulzeilen
│   ├── shell_workspaces.rs        # Arbeitsbereiche: laden, anlegen, löschen (Positionen verdichten)
│   ├── shell_workspaces_tests.rs
│   ├── shell_windows.rs           # Fenster + Tabs: Batch-Speichern, Schließen, Laden
│   ├── shell_windows_tests.rs
│   ├── shell_commands.rs          # Sechs #[tauri::command], DTOs (ts-rs), Gerät auflösen
│   └── shell_commands_tests.rs
└── lib.rs                         # + Registrierung der Commands

src/
├── lib/shell/                     # Reine Module, keine Nuxt-Auto-Imports (relativ, mit .ts)
│   ├── types.ts                   # ShellWindow, ShellTab, Workspace, ShellState
│   ├── apps.ts                    # ShellAppDefinition + Registry (system.chat/settings/federation)
│   ├── geometry.ts                # Klemmen, Kaskade, Mindestgröße, Maximieren, Restore-Korrektur
│   ├── layoutState.ts             # openApp, Fenster-/Arbeitsbereich-Reducer, hydrate
│   └── tabs.ts                    # addTab, switchTab, closeTab, Singleton-Suche
├── stores/shell.ts                # Pinia-Store: Zustand + Aktionen + flushAsync
├── composables/
│   ├── useComposer.ts             # aus der Chat-Seite: Senden, Abbrechen, neue Unterhaltung
│   ├── useComposerAttachments.ts  # aus der Chat-Seite: Anhänge
│   ├── useShellLayout.ts          # invoke-Wrapper + serielle Schreib-Queue + Debounce
│   ├── useShellTab.ts             # provide/inject-Vertrag für Apps
│   ├── useWindowPointerGesture.ts # Verschieben + 8-Wege-Größenändern (Pointer Events)
│   └── useModelPreloadStatus.ts   # aus workspace/[instance].vue verschoben
├── components/
│   ├── chat/                      # + ThreadSidebar.vue, MessageList.vue, Composer.vue (aus der Chat-Seite)
│   ├── shell/
│   │   ├── ShellDesktop.vue       # Arbeitsbereich: Fenster, Statusleiste, Launcher-Leiste
│   │   ├── ShellWindow.vue        # Rahmen, Griffe, Titelleisten-Grundriss
│   │   ├── ShellTabBar.vue        # Leiste, Scroll-Pfeile, „+“
│   │   ├── ShellNewTabMenu.vue    # „+“-Liste der Apps
│   │   ├── ShellTabListMenu.vue   # Chevron-Dropdown mit allen Tabs
│   │   ├── ShellWindowControls.vue# Minimieren, Maximieren/Wiederherstellen, Schließen
│   │   ├── ShellLauncher.vue      # Launcher (UiDrawerModal)
│   │   ├── ShellWindowOverview.vue
│   │   ├── ShellWorkspaceOverview.vue
│   │   ├── ShellCloseConfirm.vue  # AlertDialog für Guards und Löschen
│   │   ├── ShellStatusBar.vue     # Modell-Preload-Status
│   │   └── appComponents.ts       # appId → defineAsyncComponent
│   └── apps/
│       ├── ChatApp.vue            # git mv von pages/chat/[instance].vue
│       ├── SettingsApp.vue        # git mv von pages/settings/[instance].vue
│       └── FederationApp.vue      # git mv von pages/federation/[instance].vue
├── pages/
│   ├── workspace/[instance].vue   # Shell-Host (onboarded), verbraucht ?open=
│   ├── chat/[instance].vue        # nur noch Redirect
│   ├── settings/[instance].vue    # nur noch Redirect
│   └── federation/[instance].vue  # nur noch Redirect
└── i18n/locales/{de,en}.json      # + shell.*, − obsolete workspace.*-Schlüssel

scripts/
├── check-shell-state.ts           # Neue Prüfung (reine Module + Store mit gemocktem invoke)
└── check-chat-state.ts            # Pfad der Chat-Datei aktualisiert

.github/workflows/ci.yml           # + Schritt check:shell-state
package.json                       # + Skript check:shell-state
CONTEXT.md                         # + Begriffe Shell, App, Fenster, Tab, Launcher
plans/README.md                    # + Roadmap-Eintrag (Phasen-Disziplin)
src/components/workspace/ChatFab.vue   # entfällt
```

**Structure Decision**: Ein Tauri-Projekt mit Nuxt-SPA und Rust-Backend, wie bisher.
Die Shell-Logik trennt sich in (1) reine, ohne Nuxt testbare Module unter
`src/lib/shell/`, (2) einen dünnen Store und Composables und (3) Vue-Komponenten
unter `components/shell/`. Im Backend bleibt es bei flachen Dateien in
`storage/` (Vorbild `preferences*.rs`), keine Unterordner nur aus Symmetrie.

## Anforderungsabdeckung

| Anforderungen                                      | Umsetzung                                                                                                               |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| FR-001–005 (Shell, Launcher, Apps, Routen, Status) | `pages/workspace/[instance].vue`, `ShellDesktop`, `ShellLauncher`, `apps.ts`, Redirect-Seiten, `useModelPreloadStatus`  |
| FR-006–012, FR-039 (Fenster, Maximieren)           | `layoutState.ts`, `geometry.ts`, `ShellWindow`, `ShellWindowControls`, `useWindowPointerGesture`, `ShellWindowOverview` |
| FR-013–015 (Inhalt, Guards, Aufmerksamkeit)        | R8 (`v-show`, lazy Mount), `useShellTab`, `ShellCloseConfirm`, `ChatApp` (Guard + Attention)                            |
| FR-016–017 (Instanzen)                             | `apps.ts` (`multiInstance`), `tabs.ts` (Singleton über Fenster), Identität je Fenster und Tab                           |
| FR-018–022 (Arbeitsbereiche)                       | `shell_workspaces.rs`, Store-Aktionen, `ShellWorkspaceOverview`                                                         |
| FR-023–027 (Persistenz)                            | Migration `0019`, `shell_windows.rs`, `shell_commands.rs`, `hydrate`, `flushAsync`                                      |
| FR-028–030 (Kompakt, Bedienung, i18n)              | `useWindowSize` + `COMPACT_MAX_WIDTH`, ARIA-Tab-Muster (R14), `shell.*` in `de.json`/`en.json`                          |
| FR-031–038 (Tabs, Firefox-Bedienung)               | `tabs.ts`, `ShellTabBar`, `ShellNewTabMenu`, `ShellTabListMenu`, Layout R18                                             |

## Reihenfolge der Umsetzung (Grobplan für `/speckit-tasks`)

0. **Chat-Seite verhaltensgleich zerlegen** (R10), jeder Schritt ein Commit,
   `check:chat-state` nach jedem Schritt mit unveränderter Testzahl.
1. **Rust**: Fremdschlüssel-Spike (R2) → Migration `0019` und Trigger-Version →
   Speicherschicht → Commands, jeweils mit `*_tests.rs`.
2. **Reine Module** unter `src/lib/shell/` samt `check:shell-state`.
3. **Store** und Schreib-Queue (`useShellLayout`, `flushAsync`).
4. **Komponenten**, Titelleiste mit Tabs zuerst, dann Launcher und Übersichten.
5. **Seiten → Apps**: Umzug, Redirect-Seiten, Statusleiste, Host-Seite.
6. **i18n, `CONTEXT.md`, `plans/README.md`** und CI-Schritt.

## Complexity Tracking

Nur Abweichungen von der Constitution, die begründet werden müssen. Die frühere Ausnahme für die Chat-Seite entfällt: sie wird zerlegt (R10).

| Abweichung                                                          | Warum nötig                                                           | Einfachere Alternative verworfen, weil                                                                                                                                    |
| ------------------------------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `scripts/check-shell-state.ts` kann die 500-Zeilen-Grenze erreichen | Es prüft Reducer, Geometrie, Tabs, Store und Speicher-Queue gemeinsam | Aufteilen nach Belang (`check-shell-state.ts` für reine Module, `check-shell-store.ts` für Store und Queue), sobald die Grenze erreicht ist; kein Vorab-Split ohne Bedarf |
