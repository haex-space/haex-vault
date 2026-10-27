# Implementation Plan: Dateibrowser mit zwei Panels (Split-Ansicht)

**Branch**: `062-files-split-view` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/062-files-split-view/spec.md`

## Summary

Der Dateibrowser (`components/apps/FilesApp.vue`, Spec 044) wird in einen **Rahmen** und ein **Panel**
geteilt. Der Rahmen behält Werkzeugleiste, Seitenleiste, Transferleiste und den Viewer-Platz; das
Panel ist die heutige Ordneransicht samt eigener Auswahl, Tastatur, Suche, Ordnerliste und Dialogen und
wird einmal oder zweimal gezeigt. Der Split-Zustand liegt als zusätzliche Query-Schlüssel (`split`,
`side`, `o.*`) im Ort des Tabs, also in Sitzung und Verlauf (Spec 020, 022). Jedes Panel hat seinen
eigenen Verlauf: der Verlauf des Tabs gehört dem aktiven Panel, der Rahmen tauscht ihn beim Wechsel
gegen die Spur des anderen (ein kleiner neuer Anschluss `setHistory`/`swapHistory` im Window Manager;
Alt+←/→ und die Schaltflächen bleiben unverändert). F5, F6, Tab und Ziehen laufen über die vorhandenen
Transfers (`stores/filesTransfers.ts`, Backend `files_transfer`), die nur ein Ziel mit eigener Quelle
brauchen. **Kein Rust-Code ändert sich.**

## Technical Context

**Language/Version**: TypeScript (Nuxt 4.6, Vue 3.5), keine Änderung an Rust

**Primary Dependencies**: vorhanden: Tab-Router (`useTabRouter`), `useFilesFolder`, `useFilesManage`,
`useFilesKeyboard`, `stores/filesTransfers.ts`, Tailwind Container-Queries (`@container`)

**Storage**: Ort des Tabs (`TabLocation`: Pfad und Query aus Strings); Ansicht, Sortierung und
versteckte Dateien weiter als Geräteeinstellung (`useFilesPrefs`). Neue Speicher: keine.

**Testing**: `node --test scripts/check-files-*.ts` (reine Teile: neu `check-files-split.ts`; erweitert
`check-files-shortcuts.ts`, `check-files-registry.ts`), E2E im vorhandenen Rig (neu
`scripts/e2e/scenarios/files-split.test.ts`; `files-keyboard.test.ts` prüft den F5-Wechsel)

**Target Platform**: Linux, Windows, macOS (Desktop); Android und iOS zeigen den Split nicht

**Project Type**: desktop-app (Tauri) mit Mobil-Zielen

**Performance Goals**: SC-006: Auswahl und Navigation im einen Panel unter 200 ms, auch wenn das
andere 50 000 Einträge zeigt; heißt: zwei unabhängige Ordner-Zustände, keine gemeinsame
Neuberechnung

**Constraints**: Nur ein Viewer zugleich (Medien-URLs gehören dem Tab); der Verlauf des Tabs gehört dem
aktiven Panel (Spur des anderen nur im Arbeitsspeicher); keine Änderung der Transfer- und
Berechtigungsregeln (FR-010)

**Scale/Scope**: 3 neue Composables, 1 neue reine Bibliothek, 1 extrahiertes Panel, 1 kleiner Anschluss
im Window Manager, 5 geänderte Composables/Bibliotheken, 2 neue Menübefehle, E2E-Szenario

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Grundsatz                                                        | Bewertung                                                                                                      |
| ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git                                      | Berührt nicht; keine Zugangsdaten im Ort des Tabs (Quelle nur als Id, wie bisher).                             |
| II. Keine lokalen absoluten Pfade in versionierter Konfiguration | Berührt nicht; die Pfade im Tab-Ort sind Laufzeitdaten der Sitzung, nicht versionierte Konfiguration.          |
| III. Identität gerätunabhängig                                   | Berührt nicht.                                                                                                 |
| IV/V. Externe Quellen                                            | Keine neue externe Quelle.                                                                                     |
| VI. Selbständernde Anweisungen                                   | Keine Änderung an Skills, Konstitution oder Berechtigungen.                                                    |
| VII. Relay-Ausfall blockiert nichts                              | Der Split arbeitet auf lokalen Daten; Quellen hinter einem Relay fallen einzeln aus (FR-020).                  |
| VIII. Keine Verschleierung                                       | Berührt nicht.                                                                                                 |
| Workflow                                                         | PR vom Themen-Branch, Rebase- oder Merge-Commit, Conventional Commits. Kein ADR nötig: kein Grundsatz berührt. |
| Spaex (Worktree, graphify-first)                                 | Arbeit im Worktree `.worktrees/062-files-split-view`; Suche per graphify.                                      |

**Ergebnis**: Pass, keine Verstöße. Nach Phase 1 erneut geprüft: unverändert Pass.

## Project Structure

### Documentation (this feature)

```text
specs/062-files-split-view/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── split-location.md       # Query-Schlüssel des Tab-Orts
│   ├── panel-context.md        # Schnittstelle Rahmen ↔ Panel (intern)
│   └── shortcuts-and-menu.md   # Tasten, Menübefehle, Beschriftungen
├── checklists/requirements.md
└── tasks.md                    # /speckit-tasks
```

### Source Code (repository root)

```text
src/
├── lib/files/
│   ├── split.ts                 # NEU, rein: Split-Zustand ↔ Tab-Ort, Wechsel, Mindestbreite, Plattform
│   ├── registry.ts              # unverändert im Verhalten (ignoriert die Schlüssel des Splits, Test ergänzt)
│   ├── shortcuts.ts             # geändert: Eingabe `split`, Befehle switchPanel, copyToOther, moveToOther
│   ├── menus.ts                 # geändert: zwei Befehle nur in der Split-Ansicht
│   └── clipboard.ts             # geändert: `pasteRefusal` kennt `sameSource` (Pfade verschiedener Quellen nie vergleichen)
├── composables/
│   ├── useFilesSplit.ts         # NEU: Rahmen-Zustand aus dem Tab-Router (aktiv, Orte, Umschalten, Verlaufstausch)
│   ├── useTabRouter.ts          # geändert: `swapHistory(next)` (Anschluss, contracts/split-location.md)
│   ├── useFilesManage.ts        # geändert (klein): Zielquelle und `onDone` in `placeAsync`/`startAsync`, Menü-Optionen
│   ├── useFilesOtherPanel.ts    # NEU: F5/F6 (`copyToOther`, `moveToOther`) auf dem Ziel des Gegenpanels
│   ├── useFilesKeyboard.ts      # geändert: reicht `split` und die neuen Befehle durch
│   └── useSplitFit.ts           # NEU: misst die Breite, sagt ob zwei Panels passen
├── components/
│   ├── apps/FilesApp.vue        # geändert: nur noch Rahmen (Werkzeugleiste, Seitenleiste, Panels, Transferleiste)
│   └── files/
│       ├── Panel.vue            # NEU: bisheriger Inhalt von FilesApp (Pfadleiste, Suche, Liste, Dialoge, Viewer)
│       ├── PanelDialogs.vue     # NEU, nur falls Panel.vue über 500 Zeilen käme: die Dialoge des Panels
│       ├── Toolbar.vue          # geändert: Split-Schalter; Pfadleiste wird je Panel (PathBar)
│       ├── PathBar.vue          # NEU: aus Toolbar.vue gelöst (Hinauf, Pfad, Neuladen)
│       ├── FolderView.vue       # geändert: `droppable` (FR-011), `takesFocus` nur im aktiven Panel
│       ├── ActionBar.vue        # geändert: zwei Schaltflächen mit Kürzel (nur im Split)
│       └── Sidebar.vue          # unverändert (wirkt auf das aktive Panel über den Rahmen)
├── stores/
│   ├── wmNavigation.ts          # geändert: `setHistory(tabId, history)`
│   ├── windowManager.ts         # geändert: reicht `setHistory` durch und speichert die Sitzung (Zeilenzahl bleibt)
│   └── filesActionHandlers.ts   # geprüft (`files.show`, FR-023), nur bei Befund geändert
├── lib/wm/tabNavigation.ts      # geändert: reine Hilfsfunktion `setTabHistory` (Prüfung der Invarianten)
└── i18n/locales/{de,en}.json    # geändert: Beschriftungen

scripts/
├── check-files-split.ts         # NEU
├── check-files-shortcuts.ts     # erweitert
├── check-files-registry.ts      # erweitert
├── check-files-clipboard.ts     # erweitert: `pasteRefusal` über Quellen hinweg, Menü
├── check-wm-navigation.ts       # erweitert: setTabHistory
├── check-wm-nav-store.ts        # erweitert: setHistory
├── check-wm-session.ts          # erweitert: Split-Schlüssel überleben die Sitzung
└── e2e/
    ├── lib/files-split.ts       # NEU: Selektoren je Panel (`[data-files-panel]`)
    └── scenarios/files-split.test.ts   # NEU (files-keyboard.test.ts: F5-Fall; files-agent.test.ts: `files.show` im Split)
```

**Structure Decision**: Wie in der Spec 044: reine Teile in `src/lib/files/` (laufen ohne Vue unter
`node --test`), Vue-Anschluss in Composables, eine Komponente je Zuständigkeit. Es entsteht keine
neue Schicht und kein neuer Store; der Zustand lebt im Tab-Ort.

## Slicing (Pull Requests)

Entspricht dem Shipping-Hinweis in [tasks.md](tasks.md).

1. **PR A – Reiner Kern und Anschluss** (Phase 1–2): `lib/files/split.ts`, Schlüssel-Vertrag,
   `check-files-split.ts`, Erweiterung von `registry.ts`/`shortcuts.ts`/`menus.ts` mit Tests; im Window
   Manager `setTabHistory`, `setHistory`, `swapHistory` mit Prüfung in `check-wm-navigation.ts` und
   `check-wm-nav-store.ts` und der Zusatz im Spec-020-Vertrag §2. Keine sichtbare Änderung.
2. **PR B – Panel herauslösen** (Phase 3): `Panel.vue`, `PathBar.vue`, Rahmen mit einem Panel; Verhalten
   und alle bestehenden E2E-Szenarien unverändert (gleiche `data-testid`).
3. **PR C – US1** (Phase 4): `useFilesSplit` mit Verlaufstausch, `useSplitFit`, Schalter, zwei Panels,
   aktives Panel, Seitenleiste, F5/F6/Tab, Menü, Mobil und Mindestbreite, E2E für US1.
4. **PR D – US2 und US3** (Phase 5–6): Ziehen mit Zielprüfung (`droppable`), Panels mit verschiedenen
   Quellen und Ablehnungen, E2E.
5. **PR E – US4, US5, Politur** (Phase 7–9): Merken und Sitzung, Zwischenablage/Viewer/Suche je Panel,
   Messung SC-006, Verweise in Spec 044, Status.

## Complexity Tracking

Keine Verstöße gegen die Konstitution; nichts zu begründen.
