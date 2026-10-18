# Implementation Plan: Dock

**Branch**: `045-dock` | **Date**: 2026-10-07 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/045-dock/spec.md`

## Summary

Die drei schwebenden Schaltflächen in `Desktop.vue` werden ein konfigurierbares Dock: eine geordnete,
synchronisierte Liste aus Steuer-Einträgen und angehefteten Apps, ergänzt um laufende Apps aus allen
Arbeitsbereichen, dargestellt als Leiste oder Rad an einer von zwölf Positionen pro Gerät.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Fläche vs. Kompaktmodus** (R1): `wm.area` misst die Fensterfläche, `wm.compact` die App-Breite;
  `updateArea` bekommt `viewportWidth`, `hydrate` optional `compact`.
- **Speichern** (R2): Vault-Präferenz `dock.items`, Geräte-Präferenz `dock.placement`, Composable
  `useDock` nach dem Muster `useWorkspaceBackground`.
- **Logik** (R3, R6): rein in `src/lib/wm/dock.ts` (Parsen, Normalisieren, Einträge, Platzierung,
  Aktivierung, Rad-Geometrie).
- **Aktionen** (R4): keine neuen; `wm.app.open`, `wm.tab.activate`, `wm.tab.close` und die drei
  Übersichten.
- **Instanzen** (R5): Tabs, nicht Fenster; Spec beim Planen entsprechend präzisiert.
- **UI** (R7–R9): `WmDock` → `WmDockBar` / `WmDockWheel` über `WmDockItem`; `ShadcnContextMenu`,
  `ShadcnPopover`, HTML5-DnD in der Leiste; Einstellungen `general.dock` (R10).

## Technical Context

**Language/Version**: TypeScript (strict), Vue 3.5, Nuxt 4 (SPA); kein Rust-Code betroffen

**Primary Dependencies**: vorhanden — haex-ui-Layer (`ShadcnContextMenu`, `ShadcnPopover`,
`UiDrawerModal`), `@vueuse/core` (`useElementSize`, `useWindowSize`); keine neue Abhängigkeit, keine neue
Tauri-Berechtigung

**Storage**: zwei neue Schlüssel in `preferences` (`dock.items` Vault-Scope, `dock.placement`
Geräte-Scope); keine Migration ([data-model.md](./data-model.md))

**Testing**: `pnpm check:wm-state` (neu `scripts/check-wm-dock.ts`), `pnpm check:settings`,
`pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`, E2E
`scripts/e2e/scenarios/dock.test.ts` (nur Arch)

**Target Platform**: Linux, macOS, Windows, Android (Kompaktmodus), iOS

**Project Type**: Desktop- und Mobil-App (Tauri)

**Performance Goals**: Dock-Änderungen ohne spürbare Verzögerung; Auffächern des Rads ≤ 200 ms Animation,
ohne Animation bei `prefers-reduced-motion`

**Constraints**: offline-fähig; Dock nie leer (Launcher garantiert); bestehende `open-launcher`-Testkennung
bleibt

**Scale/Scope**: ~10 Dock-Einträge typisch; 1 reines Modul, 1 Composable, 4 Komponenten (+ 1
Einstellungsansicht), Launcher-Kontextmenü, Änderung an `Desktop.vue` und `layoutState.ts`

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

`.specify/memory/constitution.md` (I–VIII) und `.spaex/constitution.md`:

| Prinzip                                                      | Bewertung                                                                                                                                                                                                                                                                                                                    |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I No secrets in git                                          | ✅ keine Geheimnisse berührt                                                                                                                                                                                                                                                                                                 |
| II No local absolute paths                                   | ✅ keine                                                                                                                                                                                                                                                                                                                     |
| III–V Identity / pinned refs / opt-in                        | ✅ nicht berührt                                                                                                                                                                                                                                                                                                             |
| VI Self-modifying instructions                               | ✅ keine Agent-Konfiguration geändert                                                                                                                                                                                                                                                                                        |
| VII Relay unavailability                                     | ✅ rein lokal; Sync nur über bestehende Präferenzen                                                                                                                                                                                                                                                                          |
| VIII No concealment                                          | ✅                                                                                                                                                                                                                                                                                                                           |
| spaex: worktree on topic branch                              | ✅ `.worktrees/045-dock`, Branch `045-dock` von `origin/main`                                                                                                                                                                                                                                                                |
| spaex: speckit-workflow-adherence                            | ✅ specify → (Review) → plan → tasks → implement                                                                                                                                                                                                                                                                             |
| spaex: pr-required-for-main, conventional commits, no squash | ✅ Topic-Branch, PR                                                                                                                                                                                                                                                                                                          |
| spaex: graphify-first-authoring                              | ⚠️ Graph liefert für Frontend nur Fremdtreffer (research „Graphify-Abfrage“); Kandidaten per Code-Suche geprüft: `usePasswordsRowPress` (gegenteilig), `tabDisplayInfo` (wiederverwendet), `useWorkspaceBackground` (Muster), `wm.tab.activate` (wiederverwendet). Nachprüfung mit frischem Graphen vor `/speckit-implement` |
| spaex: ponytail / laziness ladder                            | ✅ keine neue Abhängigkeit, keine neue Action; `ponytail:` für Last-Writer-Wins der Liste und HTML5-DnD ohne Touch                                                                                                                                                                                                           |
| spaex: tests in separate files                               | ✅ `scripts/check-wm-dock.ts`                                                                                                                                                                                                                                                                                                |
| spaex: 500 LoC                                               | ✅ `dock.ts` ~250, Komponenten je < 200 geschätzt                                                                                                                                                                                                                                                                            |
| spaex: phasing-discipline                                    | ✅ Teil der laufenden haex-vault-2.x-Angleichung des Arbeitsbereichs                                                                                                                                                                                                                                                         |
| ADR nötig?                                                   | Nein — keine Core-Principle-Änderung                                                                                                                                                                                                                                                                                         |

**Post-Design Re-Check**: ✅ bestanden. Zwei dokumentierte Abweichungen vom Entwurf: keine neue Action
`wm.dock.activate` (R4) und Instanz = Tab statt Fenster (R5, Spec angepasst).

## Project Structure

### Documentation (this feature)

```text
specs/045-dock/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/dock-contract.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
src/
├── lib/wm/
│   ├── dock.ts                     # neu: Typen, Parsen, Normalisieren, Einträge, Platzierung, Rad
│   ├── layoutState.ts              # updateArea(viewportWidth), hydrate(compact?)
│   ├── sessionSync.ts              # replaceState gibt state.compact mit
│   └── types.ts                    # Kommentar zu compact
├── composables/
│   └── useDock.ts                  # neu: Präferenzen laden/schreiben
├── stores/windowManager.ts         # updateArea(area, viewportWidth)
├── components/wm/
│   ├── Desktop.vue                 # Layout Dock + Fensterfläche, Messung
│   ├── Dock.vue                    # neu: wählt Leiste/Rad, Platzierung, Auto-Hide
│   ├── DockBar.vue                 # neu
│   ├── DockWheel.vue               # neu
│   ├── DockItem.vue                # neu: Symbol, Zähler, läuft, Aufmerksamkeit, Kontextmenü, Auswahlfeld
│   ├── Launcher.vue                # Kontextmenü anheften/lösen
│   └── appRoutes.ts                # SETTINGS_VIEWS + general.dock
├── components/settings/DockView.vue  # neu
├── lib/settings/registry.ts        # subView general.dock
├── pages/workspace/[instance].vue  # useDock load/refresh
└── i18n/locales/{de,en}.json       # wm.dock.*, settings.dock.*, settings.locations.general.dock.*

scripts/
├── check-wm-dock.ts                # neu
├── check-wm-geometry.ts / check-wm-state.ts   # updateArea/hydrate-Fälle
└── e2e/scenarios/dock.test.ts      # neu
package.json                        # check:wm-state um check-wm-dock.ts erweitern
```

**Structure Decision**: Bestehende Aufteilung — reine WM-Logik in `src/lib/wm/`, Präferenz-Zustand als
Composable, WM-Komponenten in `src/components/wm/`, Einstellungsansichten in `src/components/settings/`.

## Complexity Tracking

Keine Verstöße.
