# Implementation Plan: Navigation im Tab (Vor/Zurück je Tab)

**Branch**: `020-tab-navigation` | **Date**: 2026-09-25 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/020-tab-navigation/spec.md`

## Summary

Jeder Tab der Workspace-Shell (Spec 015) bekommt einen Ort (Pfad plus Query) und
eine eigene lineare Vor/Zurück-Historie. Apps melden Routen an, auch
verschachtelt; die Titelleiste bekommt Zurück/Vor mit Verlaufsliste; Maustasten,
Alt+Pfeil und System-Zurück wirken nach festen Zielregeln. Jede Bedienung der
Shell, des Chats und der Einstellungen läuft über einen beschriebenen
Aktionskatalog (JSON-Schema-Eingaben, Bereiche, Wirkungsart, Aufrufer), der
Spec 021 den Zugang für Agenten ohne Umbau erlaubt; Leitplanken sind für Agenten
gesperrt. Inhalte in Tabs (haextensions) können holzis Navigation nicht
verändern.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Eigener Tab-Router** (R1, R3–R5): reine Historien-Reducer und ein
  Pfad-Matcher unter `src/lib/wm/`, `useTabRouter()`, `WmRouterView`,
  `WmLink`. Kein zweiter vue-router, keine Closures in der Historie.
- **Historie im Store** (R2, R12): Feld `history` in der bestehenden, nie
  persistierten `TabRuntime`; keine Migration, keine neuen Tauri-Commands.
- **Eingaben mit eindeutigem Ziel** (R6, R7): Titelleisten-Knöpfe, ein globaler
  `keydown`-Listener, Maustasten am Fenster-Element; System-Zurück nur über den
  nativen Plattform-Hook. Flache Webview-Historie; Router-Abwehr bricht ab, ohne
  zu deuten.
- **Agentenfähige Aktionen** (R8, R18–R20): Katalog unter `src/lib/actions/`,
  Runner mit Eingabeprüfung, Aufrufer, Ziel-Pflicht für Agenten und
  Leitplanken-Sperre; globale und tab-gebundene Handler; Name „Aktion“ statt
  „Command“.
- **Isolation eingebetteter Dokumente** (R17): Webview-Historie wird nie
  gelesen; Browser-eigene Kürzel und Maus-Navigation des Webviews aus, wo die
  Plattform es erlaubt.
- **Chat** (R10): Routen `/` und `/thread/:id`, Anbindung in einem neuen
  `useChatNavigation.ts`, weil `ChatApp.vue` an der 500-Zeilen-Grenze steht.
- **Keine neuen Abhängigkeiten** (R8, R14): eigener Validator für eine kleine
  JSON-Schema-Teilmenge.

## Technical Context

**Language/Version**: TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA,
`ssr: false`), Node 22.19; Rust unverändert: Tauri 2.11 reicht die
Webview-Einstellungen nicht durch (R17); der Android-Zurück-Hook nutzt
`onBackButtonPress` aus `@tauri-apps/api` im Plugin (R7)

**Primary Dependencies**: nur Vorhandenes — Pinia 4, `@vueuse/core` 15,
haex-ui-Layer (shadcn-vue/reka-ui: `ShadcnDropdownMenu`, `ShadcnTooltip`,
Toast), Tailwind 4, `@lucide/vue`, `@nuxtjs/i18n`

**Storage**: keine Änderung (Historie nur im Speicher, FR-011)

**Testing**: neues `pnpm check:wm-navigation` (Node-Type-Stripping, Vorbild
`check-wm-state.ts`); Regression `check:wm-state`, `check:chat-state`,
`check:templates`, `typecheck`, `typecheck:scripts`, `lint`, `format:check`;
manuell nach [quickstart.md](./quickstart.md)

**Target Platform**: Tauri-Desktop (Linux, macOS, Windows). Android-Geste über
denselben Pfad vorbereitet, end-to-end erst mit Android-Target prüfbar (R7)

**Project Type**: desktop-app (Nuxt-SPA-Frontend + Rust-Backend in einem Tauri-Projekt)

**Performance Goals**: SC-003 — ≤ 100 ms von Zurück/Vor bis zur sichtbaren
Ansicht bei geladenen Daten (reine Zustandsänderung, kein I/O außer dem
Nachladen von Chat-Nachrichten, das bereits heute gecacht wird)

**Constraints**: Dateien ≤ 500 Zeilen (`ChatApp.vue` steht bei 499 →
`useChatNavigation.ts`; `check-wm-state.ts` steht bei 829 → neue Prüfungen in
eigene Datei, R13); Module in `src/lib/wm/` relativ mit `.ts`-Endung, ohne
`~/`-Alias (Node-Harness); Historie ≤ 50 Einträge je Tab

**Scale/Scope**: ≤ 100 Tabs × ≤ 50 Einträge je Gerät — vernachlässigbarer
Speicher; rund 45 Aktionen in 9 Bereichen (endgültig nach Bestandsaufnahme),
2 Standardbelegungen

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die
spaex-Constitution `.spaex/constitution.md`.

| Prinzip / Vorgabe                                                          | Status | Begründung                                                                                                                                                                                                                           |
| -------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| I Keine Geheimnisse in Git                                                 | ✅     | Keine Geheimnisse berührt; `settings.get` gibt nie Zugangsdaten von Anbietern zurück                                                                                                                                                 |
| II Keine lokalen absoluten Pfade in versionierter Konfiguration            | ✅     | Nur repo-relative Pfade                                                                                                                                                                                                              |
| III Projektidentität geräteunabhängig                                      | ✅     | Berührt nicht                                                                                                                                                                                                                        |
| IV Cross-Repo-Referenzen an unveränderliche SHAs gepinnt                   | ✅     | haex-vault 2.x (Legacy) `8dce379d94e18fcd42c3b73686a06f984ca3f574` in Spec und Research                                                                                                                                              |
| V Externe Quellen nur per Opt-in                                           | ✅     | Keine neue Quelle                                                                                                                                                                                                                    |
| VI Selbstverändernde Anweisungen review-pflichtig                          | ✅     | Keine Änderung an Constitution/Skills; `CONTEXT.md` läuft durch den PR                                                                                                                                                               |
| VII Relay-Ausfall blockiert lokale Arbeit nicht                            | ✅     | Rein lokal                                                                                                                                                                                                                           |
| VIII Keine Verheimlichung in Agent-Ausgaben                                | ✅     | –                                                                                                                                                                                                                                    |
| Workflow: speckit-Stufen, PR auf `main`, Conventional Commits, kein Squash | ✅     | specify → plan → tasks → implement; Topic-Branch im Worktree                                                                                                                                                                         |
| ADR bei prinzipienrelevanter Entscheidung                                  | ✅     | Keine für 020; die Richtung „externer Agent → holzi“ bekommt ADR-0005 mit Spec 021 (R16)                                                                                                                                             |
| Test-Code in separaten Dateien                                             | ✅     | `scripts/check-wm-navigation.ts`                                                                                                                                                                                                     |
| Worktree je Änderung                                                       | ✅     | `.worktrees/020-tab-navigation`                                                                                                                                                                                                      |
| 500-LoC-Grenze                                                             | ⚠️     | Neue Dateien bleiben unter 500 Zeilen (`ChatApp.vue` 484, `stores/windowManager.ts` 485). `HuggingFaceModelManagement.vue` lag schon vorher bei 582 Zeilen; 020 tauscht dort nur Aufrufe aus (jetzt 580) — siehe Complexity Tracking |
| Graphify vor neuen benannten Artefakten                                    | ✅     | Abfrage in R8: keine Router/Keybinding-Artefakte; „Command“ belegt → Name „Aktion“                                                                                                                                                   |
| `ponytail:`-Kommentar bei bewusster Vereinfachung                          | ✅     | Geplant an R7 (Android erst mit Target geprüft) und R6-Fallback (Zeigerposition)                                                                                                                                                     |
| Nicht-triviale Logik hinterlässt einen ausführbaren Check                  | ✅     | `check:wm-navigation`                                                                                                                                                                                                                |
| Keine Selbstreferenzen von Agenten in Artefakten/Commits                   | ✅     | Wird bei Commits eingehalten                                                                                                                                                                                                         |
| **Phasen-Disziplin**                                                       | ⚠️     | Setzt 015 im Einsatz voraus. Planen ist erlaubt; **`/speckit-implement` erst nach Merge von 015**, dann Rebase auf `main`; `plans/README.md` bekommt einen Eintrag (Aufgabe in tasks) — R15                                          |

**Ergebnis vor Phase 0**: kein unbegründeter Verstoß; ein ⚠️ dokumentiert.

**Ergebnis nach Phase 1**: unverändert. Das Design fügt keine Abhängigkeit, keine
Persistenz und keinen Tauri-Command hinzu. Die Erweiterung vom selben Tag
(agentenfähige Aktionen, Isolation eingebetteter Dokumente) ändert
Datenmodell, Verträge und Umfang, aber keinen Punkt der Tabelle; ohne
Rust-Anteil. Nach der Umsetzung kam das ⚠️ zur 500-LoC-Grenze hinzu
(Complexity Tracking).

## Project Structure

### Documentation (this feature)

```text
specs/020-tab-navigation/
├── plan.md                              # Dieses Dokument
├── research.md                          # Phase 0 (R1–R16)
├── data-model.md                        # Phase 1: Ort, Historie, Routen, Aktionen, Übergänge
├── quickstart.md                        # Phase 1: automatische und manuelle Validierung
├── contracts/
│   ├── tab-navigation-contract.md       # Routen, useTabRouter, WmRouterView/WmLink, openApp(at)
│   └── wm-actions.md                 # Aktionsliste, runAction, Tastatur, Maus, System-Zurück
├── checklists/requirements.md           # Spec-Qualitätscheckliste
└── tasks.md                             # Phase 2 — NICHT von /speckit-plan erzeugt
```

### Source Code (repository root)

Stand nach der Umsetzung (auf `main` nach dem Merge von 015).

```text
src/
├── lib/wm/                           # reine Module (relativ, mit .ts)
│   ├── navigation.ts                    # NEU: TabLocation, TabHistory, push/replace/go/removeEntry, withQuery, isLocationActive
│   ├── routeMatch.ts                    # NEU: Pfadmuster, verschachteltes Matching, locationTitle
│   ├── keybindings.ts                   # NEU: KeyboardEvent → Chord, Plattform, Auflösung, Textfeld-Vorrang
│   ├── tabNavigation.ts                 # NEU: Historien mit dem Layout abgleichen, openAppAt/addTabAt, skipTabEntry, resolveSystemBack
│   └── apps.ts                          # nur Kommentar (appRoutes.ts)
├── lib/actions/                         # reine Module (relativ, mit .ts)
│   ├── types.ts, scopes.ts, schema.ts   # NEU: Aktion, Bereiche, Aufrufer, Ergebnis; Validator der JSON-Schema-Teilmenge
│   ├── runner.ts, handlers.ts           # NEU: runAction-Ablauf; Registries für globale und tab-gebundene Handler
│   ├── catalog.ts                       # NEU: ALL_ACTIONS
│   ├── wmActions.ts                  # NEU: Navigation, App/Tab an Ort öffnen, System-Zurück
│   ├── wmLayoutActions.ts            # NEU: Anordnung und lesende Aktionen
│   ├── chatActions.ts                   # NEU: Chat (tab-gebunden) und Modell/Effort/Diktat (global)
│   └── settingsActions.ts               # NEU: Einstellungen
├── plugins/actions.client.ts            # NEU: globale Handler registrieren; Android-Hook (onBackButtonPress, ponytail)
├── stores/
│   ├── windowManager.ts                         # + openApp/addTab mit Ort, Durchreichen von Navigation und Aktionen
│   ├── wmNavigation.ts               # NEU: Historien, Laufrichtung, Overlays, systemBack, Runner-Verdrahtung
│   ├── wmGuards.ts                   # 015-Close-Guard-Abfragen, unverändert ausgelagert (500-Zeilen-Grenze)
│   ├── wmActionHandlers.ts           # NEU: Handler Navigation und Öffnen
│   ├── wmLayoutHandlers.ts           # NEU: Handler Anordnung und Lesen
│   ├── chatActionHandlers.ts            # NEU: Handler Modell, Effort, Download, Integrität, Diktat
│   └── settingsActionHandlers.ts        # NEU: Handler Einstellungen (Schreiblogik aus den Komponenten)
├── composables/
│   ├── useTabRouter.ts                  # NEU: App-Schnittstelle (inert außerhalb der Shell), skipCurrent
│   ├── useAction.ts, useActionOrThrow.ts# NEU: Aktionen aus Komponenten; OrThrow wirft den Rohfehler weiter
│   ├── useWmKeyboard.ts              # NEU: globaler keydown → runAction
│   ├── useChatNavigation.ts             # NEU: Chat-Orte ↔ selectThread/newChat, Titel, Überspringen
│   ├── useChatTab.ts                  # NEU: Chat↔Shell (Navigation, tab-gebundene Handler, Freigabe, Close-Guard)
│   ├── useWmTab.ts                   # + appId, openApp(appId, at?), registerActionHandler, requestDeleteWorkspace
│   └── useComposer.ts                   # + setReasoningExpanded (aus ChatApp.vue verschoben)
├── components/wm/
│   ├── wm/NavButtons.vue, wm/HistoryMenu.vue, wm/RouterView.vue, wm/Link.vue  # NEU
│   ├── appRoutes.ts                     # ersetzt appComponents.ts (Routentabellen, titleForLocation)
│   └── übrige Shell-Komponenten         # Bedienung über useAction; Overlays im Store
├── components/apps/ChatApp.vue          # useChatTab statt eigener Shell-Verdrahtung (484 Zeilen)
├── components/chat/**, settings/**, models/**  # Bedienung über useAction/useActionOrThrow
├── pages/index.vue, onboarding/[instance].vue   # Einstieg in den Arbeitsbereich per replace
├── pages/workspace/[instance].vue       # + onBeforeRouteLeave (Abwehr), ?at=, useWmKeyboard, Models-Store im Setup
└── i18n/locales/{de,en}.json            # + shell.nav.*, shell.chat.thread, actions.*, actions.scopes.*

scripts/
├── check-wm-navigation.ts            # NEU: Historie, Matcher, Hilfen, Titel
├── check-wm-actions.ts               # NEU: Validator, Runner, Katalog- und Agenten-Regeln
├── check-wm-nav-store.ts             # NEU: Historien im Layout, Registries, System-Zurück, Überspringen
├── check-wm-keys.ts                  # NEU: Tastenkürzel
├── check-chat-navigation.ts             # NEU: Chat-Navigations-Replays (Teil von check:chat-state)
├── lib/chat-state-harness.ts            # + aufzeichnender Tab-Router, Aktions-Doubles
├── check-vue-templates.ts               # + Sperrliste direkter Schreibaufrufe (R20)
└── e2e/scenarios/tab-content-isolation.test.ts  # NEU: M13/M21, SC-009
package.json                             # + check:wm-navigation, check:chat-state über zwei Dateien
.github/workflows/ci.yml                 # + Schritt check:wm-navigation
specs/016-e2e-testing/contracts/test-hooks.md  # + nav-back/nav-forward
CONTEXT.md                               # + Ort, Tab-Historie, Aktion, Berechtigungsbereich, Aufrufer
plans/README.md                          # + Roadmap-Eintrag 020
```

**Structure Decision**: Wie in 015: reine, ohne Nuxt testbare Module unter
`src/lib/wm/`, dünne Store-Erweiterung und Composables, Vue-Bausteine unter
`components/wm/`. Kein Backend-Anteil.

## Anforderungsabdeckung

| Anforderungen                                              | Umsetzung                                                                                          |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| FR-001–009 (Ort, Historie, Push/Replace, Grenzen)          | `navigation.ts`, `routeMatch.ts`, `useTabRouter`, `WmRouterView`, `WmLink`                         |
| FR-010–011 (Lebensdauer)                                   | `TabRuntime.history`, `syncTabRuntime`, `hydrate`                                                  |
| FR-012–014 (Öffnen an einem Ort, Legacy, unbekannter Ort)  | `wm.app.open`, `useWmTab().openApp`, `?at=`, Wurzel-`WmRouterView`                                 |
| FR-015–016 (Knöpfe, Verlaufsliste)                         | `WmNavButtons`, `WmHistoryMenu`, `WmWindow`                                                        |
| FR-017 (Tastatur)                                          | `keybindings.ts`, `useWmKeyboard`                                                                  |
| FR-018 (Maus)                                              | `WmWindow` (`mouseup`/`auxclick`), Spike R6                                                        |
| FR-019–020 (System-Zurück, Webview-Historie)               | nativer Hook → `wm.system.back`, Abwehr in `pages/workspace/[instance].vue`                        |
| FR-021 (Titel)                                             | Store: Titel-Kette R9, Eintrags-Titel beim Verlassen                                               |
| FR-022–023 (Zugänglichkeit, i18n)                          | ARIA an Knöpfen und Menü, `de.json`/`en.json`                                                      |
| FR-024–026 (Aktionen, Beschreibung, lokale Tasten)         | `lib/actions/*`, `useAction`, Umstellung aller Bedienelemente, Template-Regel                      |
| FR-027 (Chat-Navigation)                                   | `useChatNavigation.ts`, `appRoutes.ts`                                                             |
| FR-028 (lesende Aktionen)                                  | `wm.state.get`, `wm.tab.history`, `wm.apps.list`, `wm.actions.list`, `chat.*.list`, `settings.get` |
| FR-029–033 (Aufrufer, Ziel, Bereiche, Leitplanken, Fehler) | `runner.ts`, `scopes.ts`, `schema.ts`                                                              |
| FR-034–035 (Schutz der Navigation)                         | flache Historie + Abwehr (R7), Webview-Einstellungen (R17), keine Deutung von Webview-Historie     |

## Reihenfolge der Umsetzung (Grobplan für `/speckit-tasks`)

0. **Gate**: 015 gemerged, Branch auf `main` rebased, `plans/README.md`-Eintrag.
1. **Reine Module** `navigation.ts`, `routeMatch.ts`, `keybindings.ts`,
   `lib/actions/{types,schema,runner,scopes}.ts` samt `check:wm-navigation`
   (Test zuerst).
2. **Store**: `TabRuntime.history`, `openApp`/`addTab` mit Ort, Navigation je
   Tab, Runner-Anbindung, tab-gebundene Handler, `systemBack`.
3. **Vue-Bausteine**: `useTabRouter`, `WmRouterView`, `WmLink`,
   `appRoutes.ts` (Apps ohne Routen verhalten sich wie bisher).
4. **Shell-Katalog** und Umstellung der 015-Komponenten auf `useAction`.
5. **Titelleiste**: `WmNavButtons`, `WmHistoryMenu`; Spike Maustasten und
   Webview-Einstellungen (quickstart §3, R17), dann Maus-Anbindung.
6. **Tastatur, Abwehr, System-Zurück**: `useWmKeyboard`, Host-Seite, Hook.
7. **Chat**: Routen, `useChatNavigation.ts`, `useChatActions.ts`; Bestandsaufnahme
   der Chat-Bedienelemente; `check:chat-state` unverändert grün.
8. **Einstellungen**: Bestandsaufnahme, `settingsActions.ts`, globale Handler,
   Umstellung der Einstellungs- und Modell-Komponenten.
9. **Template-Regel** (R20), i18n, `CONTEXT.md`, CI-Schritt, quickstart-Durchlauf.

## Complexity Tracking

| Abweichung                                                                                        | Warum sie bleibt                                                                                                                                                                                                   | Plan für die Aufteilung                                                                                                                                                       |
| ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/components/models/HuggingFaceModelManagement.vue` hat 580 Zeilen (vorher 582, Stand vor 020) | 020 tauscht dort nur Aufrufe gegen Aktionen aus und verkleinert die Datei; eine Aufteilung gehört thematisch zur Folge-Spec „Einstellungs-App“, die diese Ansicht ohnehin in Kategorien und Unteransichten zerlegt | Mit der Einstellungs-Spec: Katalog-, Such- und Installiert-Bereich als eigene Unteransichten (je unter 250 Zeilen), die Integritäts-Verdrahtung in ein gemeinsames Composable |
