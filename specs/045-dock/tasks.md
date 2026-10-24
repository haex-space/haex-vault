# Tasks: Dock

**Input**: Design documents from `/specs/045-dock/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/dock-contract.md, quickstart.md

**Tests**: Per ponytail rule (non-trivial logic leaves one runnable check) and research R11: pure-logic
checks in `scripts/check-wm-dock.ts` and `scripts/check-wm-geometry.ts` (`node --test`, relative `.ts`
imports, no `~/` alias), the unchanged registry check `scripts/check-settings.ts`, and one E2E scenario
`scripts/e2e/scenarios/dock.test.ts`. No new test framework, no new dependency.

**Conventions**: Pure modules under `src/lib/wm/` import only relative `.ts` paths (rule in
`src/lib/wm/types.ts:1-5`). Components use Nuxt auto-imports like their neighbours (`WmDock` →
`src/components/wm/Dock.vue`). Keep `data-testid="open-launcher"` on the Launcher entry (≈31 E2E scenarios
use it). i18n keys go into both `src/i18n/locales/de.json` and `en.json`.

## Phase 1: Setup

**Purpose**: Satisfy the graphify-first rule before new named artifacts exist.

- [x] T001 Before creating `src/lib/wm/dock.ts`, `src/composables/useDock.ts` and the `src/components/wm/Dock*.vue` components, run one bounded `graphify query "<intent>" --graph ../../graphify-out/graph.json --budget 1000` per artifact (worktree uses the main checkout's snapshot as-is, no refresh); record evaluated candidates and why each did not match in the "Graphify-Abfrage" section of `specs/045-dock/research.md`. If a near-identical candidate appears, stop and ask the operator before authoring

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Decouple area from compact mode (R1), add the pure dock module and the preference state, and
put an empty dock slot into the desktop. Every story builds on these.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T002 In `src/lib/wm/layoutState.ts`: give `updateArea(state, area, apps)` a fourth parameter `viewportWidth: number` and set `state.compact = viewportWidth <= COMPACT_MAX_WIDTH` instead of using `area.width`; give `hydrate(layout, apps, area)` an optional fourth parameter `compact?: boolean` (default `area.width <= COMPACT_MAX_WIDTH`, as today); update both doc comments. In `src/lib/wm/types.ts` change the `compact` field comment to "derived from the app window's width (`COMPACT_MAX_WIDTH`), not from `area` (spec 045 research R1)"
- [x] T003 In `src/lib/wm/sessionSync.ts` `replaceState`: call `hydrate(layout, deps.apps(), state.area, state.compact)` so a restored session keeps the viewport-derived compact flag
- [x] T004 In `src/stores/windowManager.ts`: change the store's `updateArea(area: Size)` to `updateArea(area: Size, viewportWidth: number)` and pass it to the reducer; update its doc comment. Search all `updateArea` callers, including `scripts/e2e/scenarios/compact-tab-close.test.ts`, and pass `viewportWidth` through to every call. Keep existing `hydrate` callers unchanged unless they need to provide an explicit `compact` value
- [x] T005 Add cases to `scripts/check-wm-geometry.ts`: (a) `updateArea` with `area.width` 700 and `viewportWidth` 1200 leaves `compact` false; (b) area 1200 with viewport 700 sets it true; (c) `hydrate(layout, APPS, { width: 700, height: 600 }, false)` yields `compact: false`; (d) without the fourth argument `hydrate` behaves as before. Run `pnpm check:wm-state`
- [x] T006 Create `src/lib/wm/dock.ts` with the types from `specs/045-dock/data-model.md` (`DockControlId`, `DockItem`, `DockEdge`, `DockAlign`, `DockStyle`, `DockMode`, `DockPlacement`, `DockInstance`, `DockEntry`), the constants `DOCK_ITEMS_KEY = 'dock.items'`, `DOCK_PLACEMENT_KEY = 'dock.placement'`, `DEFAULT_DOCK_ITEMS` (`launcher`, `system.files`, `system.chat`, `system.passwords`, `system.settings`) and `DEFAULT_DOCK_PLACEMENT` (`bar`, `bottom`, `center`, `reserved`), plus: `parseDockItems(raw: string | null): DockItem[] | null` (null for missing/non-JSON/non-array; drops malformed elements), `parseDockPlacement(raw: string | null): DockPlacement` (each invalid field falls back to its default), `normalizeDockItems(items: readonly DockItem[], apps: readonly AppDefinition[]): DockItemState[]` where `DockItemState = DockItem & { available: boolean }` (alias via `resolveAppAlias` from `./apps.ts`, first occurrence wins, missing `launcher` inserted at index 0, unknown `appId` kept with `available: false`), and `serializeDockItems(items)` that writes back without the `available` flag. Import only relative `.ts` paths
- [x] T007 Create `scripts/check-wm-dock.ts` (`node:test`, `node:assert/strict`, fixtures from `scripts/lib/wm-fixtures.ts`): every row of the `normalizeDockItems` table in `data-model.md`, `parseDockItems` on `null`, `'x'`, `'{}'`, mixed valid/invalid elements, and `parseDockPlacement` field fallbacks; append `scripts/check-wm-dock.ts` to the `check:wm-state` script in `package.json`; run `pnpm check:wm-state` and `pnpm typecheck:scripts`
- [x] T008 Create `src/composables/useDock.ts` modeled on `src/composables/useWorkspaceBackground.ts` (module-level refs, one vault per process): `items` (normalized `DockItemState[]`, recomputed when `useExtensionsStore().apps` changes), `placement`; `loadAsync()` resets to defaults then reads `DOCK_ITEMS_KEY` (vault scope) and `DOCK_PLACEMENT_KEY` (device scope, uuid from `useDevice().currentDeviceInfoAsync()` like `src/stores/settingsActionHandlers.ts:53-56`); `refreshAsync()` re-reads and assigns only on change; writers per `contracts/dock-contract.md` (`pinAsync`, `unpinAsync`, `addControlAsync`, `removeAsync` — throws for the launcher, `moveAsync`, `setPlacementAsync`) serialized through one promise chain; never write on load (FR-038). Add a `ponytail:` comment on the whole-list last-writer-wins write naming the ceiling (concurrent reorder on two devices) and the upgrade path (one CRDT row per entry)
- [x] T009 In `src/pages/workspace/[instance].vue`: `const dock = useDock()`, add `onVaultTablesChanged(['preferences'], dock.refreshAsync)` next to the background line, and in `onMounted` start `void dock.loadAsync().catch((error: unknown) => { console.error('[dock] reading the dock failed', error) })` next to `background.loadAsync()`
- [x] T010 In `src/components/wm/Desktop.vue`: delete the three-button stack (`:60-86`) and its `useAction` constants; wrap the windows layer in a `ref="windowArea"` element; measure it with `useElementSize` and the viewport with `useWindowSize`; watch both and call `wm.updateArea({ width, height }, viewportWidth)`; render `<WmDock />` as a sibling (component created in T014); update the doc comment (spec 045, FR-001, FR-033)

**Checkpoint**: `pnpm check:wm-state` and `pnpm typecheck` green; the desktop renders without the old buttons.

---

## Phase 3: User Story 1 - Häufige Apps aus dem Dock starten (Priority: P1) 🎯 MVP

**Goal**: A bottom bar with the launcher, frequently used system apps and pinned apps; the other
control entries can be added from the dock settings. Apps can be pinned and unpinned from the dock
and launcher.

**Independent Test**: Spec US1 acceptance scenarios 1–6 (quickstart steps 1–2).

- [x] T011 [US1] In `src/lib/wm/dock.ts` add `resolveDockEntries(items: readonly DockItemState[], windows: readonly WmWindow[]): DockEntry[]` — for now only the available items in order, each app entry with `pinned: true` and its `instances` (every tab with that `appId`: `{ tabId, windowId, workspaceId }`); add cases to `scripts/check-wm-dock.ts` (unavailable app hidden, order kept, instances collected across workspaces)
- [x] T012 [P] [US1] Create `src/components/wm/DockItem.vue`: props `entry: DockEntry`, `orientation: 'horizontal' | 'vertical'`; renders a 48 px round button with the app icon (`wm.apps()` lookup, like `Launcher.vue`) or the control icon (`lucide:layout-grid` launcher, `lucide:monitor` workspaces, `lucide:copy` windows), `aria-label` and tooltip with the translated name (FR-009); `data-testid` = `open-launcher` for the launcher, `dock-control-<id>` for other controls, `dock-item-<appId>` for apps; click: controls call `useAction('wm.launcher.open' | 'wm.workspaces.overview' | 'wm.windows.overview')`, apps call `useAction('wm.app.open')({ appId })` (FR-010, FR-014)
- [x] T013 [P] [US1] Create `src/components/wm/DockBar.vue`: props `entries: DockEntry[]`, `orientation`; `role="toolbar"`, roving tabindex with arrow keys along the orientation and Enter/Space activating (FR-041); `overflow-auto` along its axis (FR-026); background/shadow like the old buttons (`bg-background shadow-lg ring-1 ring-border`)
- [x] T014 [US1] Create `src/components/wm/Dock.vue`: reads `useDock()` and `useWindowManagerStore()`, computes `resolveDockEntries(dock.items, wm.windows)`, renders `DockBar` at bottom-center for now with `data-testid="dock"` and `data-style`/`data-edge`/`data-align` attributes (contract); in `Desktop.vue` the dock sits below the window area as a flex sibling (reserved mode)
- [x] T015 [US1] Wrap each app entry in `DockItem.vue` with `ShadcnContextMenu` (pattern `src/components/passwords/EntryMenu.vue:30-53`): "Lösen" for pinned apps → `useDock().unpinAsync(appId)` (FR-015, right-click and touch long press via reka, FR-043)
- [x] T016 [US1] In `src/components/wm/Launcher.vue`: wrap each app tile in `ShadcnContextMenu` with "An Dock anheften" / "Vom Dock lösen" depending on whether `useDock().items` contains the app (FR-017); keep the existing click and `data-testid`s; the lock tile gets no menu
- [x] T017 [P] [US1] i18n in `src/i18n/locales/de.json` / `en.json` under `wm.dock`: `label` ("Dock"), `pin` ("An Dock anheften" / "Pin to dock"), `unpin` ("Vom Dock lösen" / "Unpin from dock"), `controls.launcher`, `controls.workspaces`, `controls.windows`
- [x] T018 [US1] Create `scripts/e2e/scenarios/dock.test.ts` (format of `settings-categories.test.ts`): after `createAndUnlock`, assert `dock` exists with launcher, files, chat, passwords and settings while the old workspace/window controls are absent; open launcher, right-click the contacts tile, choose pin, assert `dock-item-system.contacts`; unpin via its context menu, assert it is gone; add the windows control from settings and remove it again

**Checkpoint**: US1 complete and shippable alone (fixed bottom bar, pinning, launching).

---

## Phase 4: User Story 2 - Laufende Apps sehen und zu ihnen springen (Priority: P1)

**Goal**: Running indicators, unpinned running apps, focus across workspaces, instance chooser, middle
click, close all.

**Independent Test**: Spec US2 acceptance scenarios 1–8 (quickstart steps 3–5).

- [x] T019 [US2] In `src/lib/wm/dock.ts`: extend `resolveDockEntries` to append, after the items, every app with instances that is not pinned (`pinned: false`), ordered by the position of its first tab in `windows`; an app both pinned and running appears once at its pinned slot; add `dockActivation(entry): { kind: 'open' } | { kind: 'focus'; tabId: string } | { kind: 'choose' }` (0/1/≥2 instances); add cases to `scripts/check-wm-dock.ts`
- [x] T020 [US2] In `src/components/wm/DockItem.vue`: running dot when `instances.length > 0`, count badge when ≥ 2 (FR-007), attention highlight from `wm.appHasAttention(appId)` styled like the launcher's attention dot (FR-008); expose `data-running` and `data-count`
- [x] T021 [US2] In `src/components/wm/DockItem.vue`: click uses `dockActivation` — `open` → `wm.app.open`, `focus` → `useAction('wm.tab.activate')` with target `{ tabId }` (FR-011), `choose` → open a `ShadcnPopover` (`data-testid="dock-instances"`) listing instances grouped by workspace (heading `t('wm.workspaces.numbered', …)` as in `WorkspaceOverview.vue`), each row titled from `wm.tabDisplayInfo(tab)` translated like `TabBar.vue:50`, plus "Neues Fenster" → `wm.app.open` (FR-012); the popover closes on selection and when its instance list becomes empty
- [x] T022 [US2] In `src/components/wm/DockItem.vue`: `@auxclick` with `button === 1` → `wm.app.open` for `multiInstance` apps, else the normal click path (FR-013); context menu gains "Anheften" for unpinned apps, "Neues Fenster" for multi-instance apps and "Alle schließen" for running apps, which calls `useAction('wm.tab.close')` with target `{ tabId }` sequentially per instance and stops when one is declined (FR-015, FR-016)
- [x] T023 [US2] In `src/components/wm/DockBar.vue`: render a separator between pinned entries and the first `pinned: false` entry (FR-005)
- [x] T024 [P] [US2] i18n under `wm.dock`: `newWindow`, `closeAll`, `instances` (popover title), `running` (screen-reader text), `count` (badge aria, with `{count}`)
- [x] T025 [US2] Extend `scripts/e2e/scenarios/dock.test.ts`: open chat twice → `dock-item-system.chat` has `data-count="2"`; click → `dock-instances` visible; pin passwords, open it, create and switch to a second workspace, click `dock-item-system.passwords` → back in the first workspace with no second passwords window

**Checkpoint**: US1 + US2 = full taskbar at the default position.

---

## Phase 5: User Story 3 - Dock platzieren (Priority: P2)

**Goal**: Twelve positions and three bar modes, per device, set in settings and from the dock.

**Independent Test**: Spec US3 acceptance scenarios 1–6 (quickstart step 6).

- [x] T026 [US3] In `src/lib/wm/dock.ts` add `effectivePlacement(placement: DockPlacement, compact: boolean): DockPlacement` per the table in `data-model.md`; add cases for all style × compact combinations to `scripts/check-wm-dock.ts`
- [x] T027 [US3] In `src/components/wm/Desktop.vue` and `src/components/wm/Dock.vue`: lay out by `effectivePlacement(dock.placement, wm.compact)` — `reserved`: flex sibling of the window area (`flex-col` / `flex-col-reverse` / `flex-row` / `flex-row-reverse` by edge), so T010's measurement yields the remaining area (FR-024); `floating`: absolutely positioned over the window area; alignment via `justify-start/center/end` along the edge; vertical orientation for `left`/`right` (FR-026)
- [x] T028 [US3] In `src/components/wm/Dock.vue`: `autohide` mode — a 4 px transparent strip at the edge (`pointerenter` shows), `pointerleave` of the bar hides after 400 ms; stays visible while `:focus-within`, its popover or a context menu is open (FR-025); slide transition disabled under `prefers-reduced-motion`
- [x] T029 [US3] In `src/components/wm/Dock.vue`: `ShadcnContextMenu` on the dock's free area with radio groups for edge, alignment, style and mode (mode only for `bar`) → `useDock().setPlacementAsync` (FR-018)
- [x] T030 [US3] Settings: in `src/lib/settings/registry.ts` add `subView('general.dock', 'general/dock', 'general', { icon: 'lucide:panel-bottom', overviewRow: true, settingKeys: ['settings.dock.style', 'settings.dock.edge', 'settings.dock.align', 'settings.dock.mode', 'settings.dock.items'] })` after `general.appearance`; create `src/components/settings/DockView.vue` with a placement group (`SettingsGroup`, `SettingsRow`, `SettingsSelect` like `ColorSchemeSetting.vue`; mode select hidden for `wheel`); map `'general.dock'` in `SETTINGS_VIEWS` of `src/components/wm/appRoutes.ts`
- [x] T031 [P] [US3] i18n: `settings.locations.general.dock.{title,description,keywords}` ("Dock", "Position, Stil, Einträge" / "Position, style, entries"), `settings.dock.{style,edge,align,mode,items}` labels and option texts (`bar`/`wheel`, `top`/`bottom`/`left`/`right`, `start`/`center`/`end`, `reserved`/`floating`/`autohide`); run `pnpm check:settings`
- [x] T032 [US3] Extend `scripts/e2e/scenarios/dock.test.ts`: in settings set edge `left` → `dock` has `data-edge="left"`

**Checkpoint**: Position and mode configurable per device.

---

## Phase 6: User Story 4 - Einträge ordnen und Steuer-Einträge ausblenden (Priority: P2)

**Goal**: Reorder by drag in the bar, full list management in settings, launcher not removable.

**Independent Test**: Spec US4 acceptance scenarios 1–5 (quickstart step 7).

- [x] T033 [US4] In `src/components/wm/DockBar.vue`: native HTML5 drag-and-drop between pinned entries and controls (pattern `src/components/passwords/List.vue` / `TreeItem.vue`), drop → `useDock().moveAsync(from, to)` using indexes into `dock.items` (not into the rendered entries, which skip unavailable apps); unpinned running entries are not draggable (FR-019). Add a `ponytail:` comment: mouse only, touch sorts in settings (research R8)
- [x] T034 [US4] In `src/components/settings/DockView.vue`: entries group listing every `DockItemState` with icon and name, "Nach oben"/"Nach unten" buttons, "Entfernen" for all but the launcher, unavailable apps greyed with "nicht verfügbar" (FR-020, FR-021, FR-006); an "Hinzufügen" select listing apps from `wm.apps()` not yet pinned plus removed controls
- [x] T035 [US4] In `src/components/wm/DockItem.vue` context menu: "Aus Dock entfernen" for the workspaces and windows controls → `useDock().removeAsync(index)`; nothing for the launcher (US4 scenario 5)
- [x] T036 [P] [US4] i18n: `settings.dock.{moveUp,moveDown,remove,add,unavailable}`, `wm.dock.remove`
- [x] T037 [US4] Extend `scripts/e2e/scenarios/dock.test.ts`: remove the windows control in settings → `dock-control-windows` gone; assert the launcher row has no remove button

**Checkpoint**: Dock content fully user-defined.

---

## Phase 7: User Story 6 - Dock im Kompaktmodus (Priority: P2)

**Goal**: Bottom bar or bottom-corner wheel below the compact threshold, no oscillation.

**Independent Test**: Spec US6 acceptance scenarios 1–5 (quickstart step 9).

**Depends on**: T026 (effectivePlacement), T027 (layout).

- [x] T038 [US6] In `src/components/wm/Dock.vue`: watch `wm.compact`; on change close an open instance popover and a fanned-out wheel (FR-034); the bar in compact scrolls horizontally with touch (`overflow-x-auto`, `touch-pan-x`)
- [x] T039 [US6] Extend `scripts/e2e/scenarios/dock.test.ts`: with the bar at edge `left`, the window area is narrower than the app window (in the rig's 800 px window below the compact threshold) and `wm.compact` stays false (FR-033), checked without driving `updateArea` by hand. The rig cannot resize the app window (no window-rect call in `scripts/e2e/lib/webdriver.ts`), so the compact placement itself (FR-031/FR-032) is covered by `check-wm-dock.ts` (`effectivePlacement`) and quickstart step 9

**Checkpoint**: Dock usable on phones; no position flip-flop near the threshold.

---

## Phase 8: User Story 5 - Dock als Rad (Priority: P3)

**Goal**: A round button that fans entries into a quarter or half circle, multiple rings when needed.

**Independent Test**: Spec US5 acceptance scenarios 1–6 (quickstart step 8).

- [x] T040 [US5] In `src/lib/wm/dock.ts` add `wheelLayout(count: number, edge: DockEdge, align: DockAlign): { x: number; y: number }[]` per research R6 (corner = `start`/`end` → 90°, `center` → 180°, opening toward the screen centre; ring radius `r0 + k·gap`, capacity `max(1, floor(arcLength / 56) + 1)`, fill inner to outer); add cases to `scripts/check-wm-dock.ts`: every pairwise distance ≥ 48 for counts 1–20 at all 12 positions, all offsets point into the screen, ring count grows when capacity is exceeded
- [x] T041 [US5] Create `src/components/wm/DockWheel.vue`: a 56 px round toggle (`data-testid="dock-wheel-toggle"`, `aria-haspopup="menu"`, `aria-expanded`) with an attention dot when any entry's app has attention (US5 scenario 5); when open renders the same `DockItem`s absolutely at `wheelLayout` offsets with a `transform` transition (none under `prefers-reduced-motion`); closes on activation, Escape and outside click (`onClickOutside`) (FR-027–FR-029); keyboard: Enter/Space opens, arrow keys move along the entry order, Enter activates (FR-042); `role="menu"` / `menuitem`
- [x] T042 [US5] In `src/components/wm/Dock.vue`: render `DockWheel` when the effective style is `wheel`, always absolutely positioned at the effective corner/edge over the window area, never reserving space (FR-030); ignore `mode`
- [x] T043 [P] [US5] i18n: `wm.dock.wheel.open` / `close` (toggle aria labels)
- [x] T044 [US5] Extend `scripts/e2e/scenarios/dock.test.ts`: set style `wheel` → `dock-wheel-toggle` visible, click → entries visible, Escape → hidden

**Checkpoint**: All six stories done.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [x] T045 [P] Add a **Dock** entry to the window-manager glossary in `CONTEXT.md` (Dock, Leiste, Rad, Steuer-Eintrag, angeheftete/laufende App, Instanz = Tab; `dock.*` preference keys; avoid "Taskleiste" in UI text)
- [x] T046 Check every new or changed file stays under 500 lines (`wc -l src/lib/wm/dock.ts src/components/wm/Dock*.vue src/components/settings/DockView.vue src/composables/useDock.ts`); split `wheelLayout` into `src/lib/wm/dockWheel.ts` if `dock.ts` exceeds it
- [x] T047 Run `pnpm check:wm-state`, `pnpm check:wm-navigation`, `pnpm check:settings`, `pnpm check:agent-actions` (must stay unchanged — no new actions), `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`; fix until green
- [x] T048 Walk through `specs/045-dock/quickstart.md` steps 1–11 in `pnpm tauri:dev` (steps 9 on a real Android device if available, step 10 with two devices); record skipped steps in the PR description. **Done 2026-10-08** by the operator in `pnpm tauri:dev`. That pass found the clipped instance count, several context menus open at once, and the missing mode for the wheel, all fixed in #335. Not checked: step 10 (sync with a second device) and step 11 (disabled extension)
- [x] T049 Run `pnpm test:e2e --grep dock` plus the scenarios using `open-launcher` (`--grep settings`, `--grep chat-multi-instance`) on Arch, or confirm via `gh pr checks` (E2E job) on Pop!\_OS. **Done**: the desktop E2E suite with `dock.test.ts` passed in CI for #330, #331 and #335

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none
- **Foundational (Phase 2)**: after Setup; blocks all stories. T002 → T003, T004 → T005; T006 → T007, T008 → T009; T010 needs T004
- **US1 (Phase 3)**: after Foundational
- **US2 (Phase 4)**: after US1 (extends `resolveDockEntries`, `DockItem`, `DockBar`)
- **US3 (Phase 5)**: after US1; independent of US2
- **US4 (Phase 6)**: after US3 (needs `DockView.vue` from T030)
- **US6 (Phase 7)**: after US3 (T026, T027)
- **US5 (Phase 8)**: after US3 (placement) — US6's wheel scenario (FR-032) is covered by T026's tests
- **Polish (Phase 9)**: after all stories

### Within Each Story

Pure logic + check first, then components, then i18n, then the E2E extension. Commit per story
(`feat(wm): …`).

### Parallel Opportunities

- T012 and T013 (different new files) once T011 is done; T017 any time in US1
- i18n tasks T024, T031, T036, T043 alongside their story's component work
- After US1: US2 and US3 can proceed in parallel (different files except `DockItem.vue` — T035 and
  T020–T022 touch it, so sequence those)

## Parallel Example: User Story 1

```text
T012 Create src/components/wm/DockItem.vue
T013 Create src/components/wm/DockBar.vue
T017 i18n wm.dock.* in de.json / en.json
```

## Implementation Strategy

### MVP First

1. Phase 1 + Phase 2 (area/compact split, dock module, `useDock`, desktop slot)
2. Phase 3 (US1): bottom bar, pin/unpin, launch → validate quickstart 1–2 → shippable
3. Phase 4 (US2): full taskbar behaviour

### Incremental Delivery

US1 → US2 → US3 → US4 → US6 → US5, each a separate commit and verifiable on its own. One PR for the
whole feature unless review size (spaex ≈ 1000 lines) suggests splitting after US2.
