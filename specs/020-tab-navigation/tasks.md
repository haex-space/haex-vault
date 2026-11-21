---
description: 'Task list for spec 020-tab-navigation'
---

# Tasks: Navigation im Tab (Vor/Zurück je Tab)

**Input**: Design documents from `/specs/020-tab-navigation/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires an executable check for non-trivial logic; the plan names `scripts/check-wm-navigation.ts` (`pnpm check:wm-navigation`, Node type-stripping, same style as `scripts/check-wm-state.ts`). Write each test task before its implementation task and see it fail first.

**Organization**: Tasks are grouped by user story. File paths assume spec 015 is merged (they come from branch `015-workspace-shell`). Modules under `src/lib/**` import siblings relatively with a `.ts` suffix and never use `~/` or Nuxt auto-imports (015 research R6). Every file stays ≤ 500 lines. Commits follow Conventional Commits and carry no agent attribution.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US8)

---

## Phase 1: Setup

**Purpose**: Gate, environment, script wiring

- [x] T001 Confirm the phase gate (research R15): spec 015 (PR #141) is merged into `main`; rebase `020-tab-navigation` onto `main` and resolve conflicts in `specs/` only
  - Done 2026-09-25: 015 merged as ccf932a (PR #141); rebased 020-tab-navigation onto main, the only conflict was .specify/feature.json (kept specs/020-tab-navigation); force-pushed with lease (no PR yet).
- [x] T002 Run a real `pnpm install` in `.worktrees/020-tab-navigation` (never symlink `node_modules`), then confirm `pnpm check:wm-state` and `pnpm check:chat-state` are green as the baseline; record both test counts in the T002 note
  - Done 2026-09-25: real pnpm install in the worktree; baseline check:wm-state 44/44 (one test added by the 015 review fixes), check:chat-state 45/45.
- [x] T003 [P] Add the 020 roadmap row to `plans/README.md` (after the 015 row): priority P1, effort M, status "Spezifiziert 2026-09-25, Umsetzung nach Merge von 015", gate "setzt Spec 015 voraus"
  - Done 2026-09-25: row added after 015 in plans/README.md.
- [x] T004 [P] Create `scripts/check-wm-navigation.ts` as an empty `node:test` harness with a header comment in the style of `scripts/check-wm-state.ts`; add `"check:wm-navigation": "node scripts/check-wm-navigation.ts"` to `package.json` and a matching step next to `check:wm-state` in `.github/workflows/ci.yml`
  - Done 2026-09-25: harness + package script + CI step. Deviation: check:wm-navigation runs several files via node --test (check-wm-navigation.ts, check-wm-actions.ts, check-wm-nav-store.ts as they appear) so each stays below 500 lines; later tasks name the file they extend.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Pure navigation and action modules, the store extension and the Vue building blocks every story uses

**⚠️ CRITICAL**: No user-story work starts before this phase is complete

### Tests first

- [x] T005 [P] Add history-reducer tests to `scripts/check-wm-navigation.ts`: `createHistory` defaults to `/`; `push` appends, drops forward entries and stores the leaving title; `push` of an equal location is a no-op (equality = normalized path and equal query regardless of key order); `replace` changes only the current entry; `go` within bounds moves `index`, outside bounds changes nothing; max 50 entries (`MAX_HISTORY_ENTRIES`), oldest dropped and `index` adjusted; `removeEntry` before, at and after `index` (at `index`: neighbor in travel direction); locations survive `JSON.parse(JSON.stringify(x))`
  - Done 2026-09-25: 21 tests in scripts/check-wm-navigation.ts (history + matcher sections), failing first on the missing modules.
- [x] T006 [P] Add route-matcher tests to `scripts/check-wm-navigation.ts`: literal and `:param` segments, `''` index child, nested chains outer → inner with merged params, URL-decoding of params, trailing-slash normalization, no match → `null`, an app without routes matches only `/`
  - Done 2026-09-25: matcher tests in the same file (index child, nested params with URL-decoding, trailing slash, unknown path, implicit '/' route).
- [x] T007 [P] Add action-core tests to `scripts/check-wm-navigation.ts`: schema validator accepts only the subset (`object`, `properties`, `required`, `string`, `number`, `integer`, `boolean`, `array`, `enum`, `description`) and reports the failing `field`; runner order per data-model.md "Ablauf runAction" with every error code (`unknown_action`, `invalid_input`, `target_required`, `target_not_found`, `forbidden_for_agents`, `app_unavailable`, `failed`); a `guardrails` action is refused for `builtinAgent` and `externalAgent` before its handler runs and executed for `user`; agents without explicit target get `target_required`, `user` falls back to focus
  - Done 2026-09-25: in scripts/check-wm-actions.ts (added to check:wm-navigation): schema subset incl. strict objects and array items, every runner error code, guardrail lock for both agent kinds, target rules.
- [x] T008 [P] Add catalog-invariant tests to `scripts/check-wm-navigation.ts` (data-model.md invariants 6–7): unique ids, every schema inside the subset, every `scope` exists in `scopes.ts`, `scope = 'guardrails'` ⇒ `agentCallable = false`, `binding = 'tab'` ⇒ `appId` set
  - Done 2026-09-25: catalog invariants iterate src/lib/actions/catalog.ts (ALL_ACTIONS aggregator, empty until the per-area catalogs land in US1/US4/US7), so they tighten automatically.

### Implementation

- [x] T009 [P] Implement `src/lib/wm/navigation.ts`: `TabLocation = { path: string; query: Record<string, string> }` (path app-relative, starts with `/`, no trailing `/` except `/`, segments URL-encoded), `HistoryEntry = { location; title: string | null }`, `TabHistory = { entries; index }` with `1 ≤ entries.length ≤ 50`, `parseLocation(string)`, `locationsEqual`, `createHistory`, `push`, `replace`, `go`, `removeEntry`, `canGoBack`, `canGoForward`, `backList`/`forwardList` (max 15 each, nearest first), per data-model.md transitions
  - Done 2026-09-25: immutable reducers (no-op returns the same object); also formatLocation/normalizePath helpers.
- [x] T010 [P] Implement `src/lib/wm/routeMatch.ts`: pure pattern type `RoutePattern = { path; titleKey?; children? }` and `matchRoute(patterns, path) → { chain, params } | null` per data-model.md "RouteRecord"
  - Done 2026-09-25: matchRoute(patterns, path) → { chain, params } | null.
- [x] T011 [P] Implement `src/lib/actions/types.ts` (`ActionDefinition`, `ActionScope`, `ActionCaller`, `ActionOutcome`, error codes) and `src/lib/actions/scopes.ts` (`wm.layout`, `wm.navigation`, `wm.read`, `chat.read`, `chat.write`, `settings.read`, `settings.device`, `settings.models`, `guardrails`, each with `titleKey` `actions.scopes.<name>`) exactly as in data-model.md
  - Done 2026-09-25: types.ts + scopes.ts; JsonSchema subset adds 'items' for arrays (needed by array inputs).
- [x] T012 [P] Implement `src/lib/actions/schema.ts`: validator for the JSON-schema subset, returning `{ ok: true } | { ok: false; field; message }` (no new dependency, research R8)
  - Done 2026-09-25: schema.ts validate + isSchemaInSubset, no dependency.
- [x] T013 Implement `src/lib/actions/runner.ts`: `createActionRunner({ catalog, resolveFocus, handlers })` with `runAction(id, input, caller)` following data-model.md steps 1–6; tab-bound handlers resolved through an injected `awaitTabHandler(appId, id, timeoutMs = 5000)`
  - Done 2026-09-25: runner.ts createActionRunner(deps) with globalHandler/resolveFocus/targetExists/awaitTabHandler injection; TAB_HANDLER_TIMEOUT_MS = 5000. 38/38 tests green.
- [x] T014 Extend `src/lib/wm/types.ts` with `TabRuntime.history: TabHistory`; make `layoutState.ts` `hydrate`, `openApp` and `tabs.ts` `addTab` accept an optional start location and create each new tab's history (`hydrate` always `[{ path: '/' }]`, FR-011); keep all 015 tests green
  - Done 2026-09-25: Deviation: histories live in a separate reactive Map next to TabRuntime (not a TabRuntime field) and the layout reducers stay unchanged — src/lib/wm/tabNavigation.ts runs openApp/addTab and then tells a new tab (fresh history at the location) from a re-activated singleton (push). hydrateFromBackendAsync clears all histories, so every restored tab starts at '/'.
- [x] T015 Extend `src/stores/windowManager.ts`: per-tab `navigate(tabId, to, { replace })`, `go(tabId, delta)`, `historyOf(tabId)`, runner instance wired to the store (focus resolution from `state.activeWindowId`), tab-handler registry (`registerTabActionHandler(tabId, actionId, handler)`, removed on tab close), and `runAction(id, input?, caller = { kind: 'user' })`; keep the file ≤ 500 lines (extract a `src/stores/wmNavigation.ts` helper if needed)
  - Done 2026-09-25: store wiring in src/stores/wmNavigation.ts (navigate, goTab, historyOf, runAction, registerGlobalActionHandler, registerTabActionHandler) with the pure handler registry src/lib/actions/handlers.ts; openApp/addTab take an optional location and return the tab id. The 015 close-guard queries moved unchanged to src/stores/wmGuards.ts to keep stores/windowManager.ts at 474 lines.
- [x] T016 Add store-integration tests to `scripts/check-wm-navigation.ts` for T014/T015 with the mocked `invokeFn` pattern from `scripts/check-wm-persistence.ts`: new tab has one `/` entry; `openApp(appId, at)` starts at `at`; `hydrate` gives every tab exactly `[/]`; closing a tab drops its history
  - Done 2026-09-25: scripts/check-wm-nav-store.ts (15 tests: histories, open/add at a location, singleton push/no-op, isolation, FR-010/FR-011, handler registry with timeout). check:wm-navigation 53/53, check:wm-state 44/44, check:chat-state 45/45, typecheck and lint clean.
- [x] T017 [P] Implement `src/composables/useAction.ts` (`useAction(id)` → `(input?) => shell.runAction(id, input, { kind: 'user' })`)
  - Done 2026-09-25: src/composables/useAction.ts; logs failed outcomes in dev.
- [x] T018 [P] Implement `src/composables/useTabRouter.ts` per contracts/tab-navigation-contract.md §2 (`route` with `path`, `query`, `params`, `matched`; `canGoBack`, `canGoForward`, `push`, `replace`, `setQuery` defaulting to replace, `back`, `forward`); inert router at `/` outside a Shell, like `useWmTab()`
  - Done 2026-09-25: src/composables/useTabRouter.ts (reactive route/canGoBack/canGoForward, push/replace/setQuery/back/forward) + ROUTER_DEPTH_KEY; inert at '/' outside a Shell tab.
- [x] T019 [P] Create `src/components/wm/RouterView.vue` (renders the matched record at its own depth via provide/inject) and `src/components/wm/Link.vue` (`<a href="#">`, `to`, `replace`, `prefix`, `aria-current="page"` when active) per contract §3
  - Done 2026-09-25: WmRouterView (renders chain[depth], root falls back to '/' for an unknown location — toast follows in T043) and WmLink (triggers shell.tab.navigate with its own tabId; the action itself lands in T022).
- [x] T020 Replace `src/components/wm/appComponents.ts` with `src/components/wm/appRoutes.ts` (per-app route tables with components; apps without routes map `/` to their root component) and make `src/components/wm/TabPanel.vue` render the root `WmRouterView` for its tab; all three apps behave exactly as before
  - Done 2026-09-25: git mv appComponents.ts → appRoutes.ts; child records may omit component so a root app stays mounted across child routes. WmTabPanel renders the root WmRouterView.
- [x] T021 Extend `src/composables/useWmTab.ts` with `openApp(appId, at?)` and `registerActionHandler(actionId, handler)` (contract §4), wired in `wm/TabPanel.vue`
  - Done 2026-09-25: useWmTab gains appId, openApp(appId, at?) and registerActionHandler; provided by WmTabPanel. Phase 2 checkpoint: check:wm-navigation 53/53, check:wm-state 44/44, check:chat-state 45/45, check:templates 54/54, typecheck and lint clean.

**Checkpoint**: `pnpm check:wm-navigation`, `check:wm-state`, `check:chat-state`, `typecheck` green; the app looks and behaves like 015

---

## Phase 3: User Story 1 - Zurück und Vor innerhalb eines Tabs (Priority: P1) 🎯 MVP

**Goal**: Back/forward buttons in every window's title bar navigate the active tab's history

**Independent Test**: quickstart M1–M3: open Chat, open three conversations in turn, back twice, forward once; buttons enable/disable correctly

- [x] T022 [P] [US1] Add `wm.tab.back`, `wm.tab.forward`, `wm.tab.go` (`steps`) and `wm.tab.navigate` (`to`, `replace?`) with global handlers to `src/lib/actions/wmActions.ts` (target `tab`, scope `wm.navigation`, effect `write`, `agentCallable: true`; back/forward `defaultKeys` `Alt+ArrowLeft`/`Alt+ArrowRight`, mac additionally `Meta+BracketLeft`/`Meta+BracketRight`, `yieldToTextInput: { mac: true }`) and register them in a new `src/plugins/actions.client.ts`
  - Done 2026-09-25: src/lib/actions/wmActions.ts (WM_NAVIGATION_ACTIONS, in catalog.ts), handlers in src/stores/wmActionHandlers.ts, registered by src/plugins/actions.client.ts; catalog invariants now cover them.
- [x] T023 [US1] Create `src/components/wm/NavButtons.vue`: two icon buttons (`@lucide/vue` `arrow-left`/`arrow-right`), `aria-label` `wm.nav.back`/`wm.nav.forward`, `disabled` without entries in that direction, tooltip with the shortcut, `pointerdown.stop` so no window drag starts, clicks via `useAction`
  - Done 2026-09-25: native title attribute shows the shortcut (the project has no tooltip component in use); touch size p-2 in compact mode.
- [x] T024 [US1] Place `WmNavButtons` left of `WmTabBar` in `src/components/wm/Window.vue`, bound to the window's active tab; keep the 015 title-bar order otherwise (FR-015)
  - Done 2026-09-25: placed left of WmTabBar.
- [x] T025 [US1] Give the chat its routes in `src/components/wm/appRoutes.ts` (`/` start = new conversation, `thread/:id` with `titleKey` `wm.chat.thread`) and create `src/composables/useChatNavigation.ts`: opening a conversation from the sidebar calls `push('/thread/<id>')`; a watcher on the route calls the existing `selectThread(id)` (or the new-conversation path for `/`); wire it into `src/components/apps/ChatApp.vue` with a single call (file stays ≤ 500 lines)
  - Done 2026-09-25: useChatNavigation takes the router as a parameter (the chat replay harness injects a recording router, scripts/lib/chat-state-harness.ts) and syncs both ways: location → selectThread/newChat, chat → location via replace. This already covers T055's push on 'new conversation' and replace on thread creation/deletion. chatTitle moved into it and setReasoningExpanded into useComposer to keep ChatApp.vue at 497 lines. check:chat-state 45/45, check:vault-lifecycle 12/12.
- [x] T026 [US1] Verify replace semantics end to end with a test in `scripts/check-wm-navigation.ts`: a `setQuery` on the current location keeps the history length and back returns to the previous view (US1 AS5); nested chain `/models/hf/x` highlights `/models` via prefix match (US1 AS7)
  - Done 2026-09-25: withQuery/isLocationActive extracted into navigation.ts (used by useTabRouter/WmLink) and tested; check:wm-navigation 56/56. Manual M1–M3 run with the quickstart pass (T060).

**Checkpoint**: MVP — back/forward works for the chat in any window

---

## Phase 4: User Story 2 - Jeder Tab hat seine eigene Historie (Priority: P1)

**Goal**: Navigation in one tab never touches another; history survives window operations

**Independent Test**: quickstart M9–M12

- [x] T027 [P] [US2] Add isolation tests to `scripts/check-wm-navigation.ts` (data-model.md invariants 1–3): navigation in tab A leaves tab B unchanged; `switchTab`, `minimizeWindow`, `toggleMaximizeWindow`, `moveWindowToWorkspace`, `switchWorkspace` never change any history; back without entries changes nothing and closes nothing (SC-005)
  - Done 2026-09-25: isolation and lifecycle cases were already in check-wm-nav-store.ts from T016 (FR-008, FR-010, FR-011); added SC-005 (back/forward without entries change and close nothing) and the active-tab switch exposing each tab's own history. 58/58.
- [x] T028 [US2] Make `WmNavButtons` in `src/components/wm/Window.vue` react to the active-tab switch (US2 AS2) and fix any store gap the T027 tests expose in `src/stores/windowManager.ts`
  - Done 2026-09-25: no code change needed: WmNavButtons derives its state from its tabId prop, bound to window.activeTabId, so it follows every tab switch; the T027 tests exposed no store gap.

**Checkpoint**: Two windows with independent histories (M9–M12)

---

## Phase 5: User Story 3 - Vor/Zurück per Maus, Tastatur und Geste (Priority: P1)

**Goal**: Keyboard, mouse side buttons, history list and system back reach the intended tab

**Independent Test**: quickstart M5–M8, M13; `wm.system.back` cases in `check:wm-navigation`

- [x] T029 [P] [US3] Add keybinding tests to `scripts/check-wm-navigation.ts`: chord normalization `Ctrl+Alt+Shift+Meta+<code>`; platform detection; `Alt+ArrowLeft` resolves to `wm.tab.back` on all platforms and `Meta+BracketLeft` on mac; on mac an editable target with `Alt+ArrowLeft` is not intercepted, on Linux/Windows it is
  - Done 2026-09-25: keybinding tests in check-wm-actions.ts. Deviation: yieldToTextInput is a per-platform list of chords (not a boolean) because on macOS only Alt+Arrow yields to text fields, Cmd+[ / Cmd+] do not.
- [x] T030 [P] [US3] Implement `src/lib/wm/keybindings.ts` (`chordFromEvent`, `isMac`, `resolveChord(catalog, chord, platform)`, `shouldYield(action, platform, target)`)
  - Done 2026-09-25: src/lib/wm/keybindings.ts; shouldYield(action, platform, chord, target).
- [x] T031 [US3] Implement `src/composables/useWmKeyboard.ts` (one capture-phase `keydown` listener in the top document, `preventDefault` + `runAction` on a hit) and call it from `src/pages/workspace/[instance].vue`
  - Done 2026-09-25: src/composables/useWmKeyboard.ts (vueuse useEventListener, capture phase), called from pages/workspace/[instance].vue.
- [ ] T032 [US3] Spike (quickstart §3): on Linux (WebKitGTK), and where available macOS and Windows, record whether mouse side buttons reach the DOM as `mouseup` with `button` 3/4 and whether the webview navigates its own history on them; write the results into a `Spike T032` note in this task and into research.md R6
  - Status 2026-09-25: open — needs a physical mouse with side buttons on each platform (not available to the implementing agent). Research so far: tauri-apps/tauri#5677 reports that mouse back/forward buttons cannot be captured with DOM listeners in Tauri webviews; WebKitGTK exposes the triggering mouse button on navigation actions (decide-policy), which a Rust-side policy could reject. The code handles both outcomes (T033). Record the operator's findings here and in research.md R6.
- [x] T033 [US3] Handle mouse side buttons in `src/components/wm/Window.vue`: `data-wm-window-id` on the root; `mouseup` `button === 3/4` → `wm.tab.back`/`forward` with the window's active `tabId`, no `focusWindow`; `preventDefault` on `mousedown`/`auxclick` for those buttons (contracts/wm-actions.md §4)
  - Done 2026-09-25: root handles mousedown/auxclick/mouseup for buttons 3/4 (preventDefault; mouseup runs back/forward for the window's active tab), and the root pointerdown no longer focuses the window for side buttons (FR-018). Delivery to the DOM per platform is T032's open spike.
- [x] T034 [US3] Create `src/components/wm/HistoryMenu.vue` (haex-ui `ShadcnDropdownMenu`, max 15 entries nearest first, entry titles, keyboard operable) and open it from `WmNavButtons` on long press (≥ 500 ms, pointer stays on the button) or `contextmenu`; selection runs `wm.tab.go` with `steps` (FR-016)
  - Done 2026-09-25: wm/HistoryMenu.vue: controlled haex-ui dropdown anchored by an invisible trigger, so a plain click stays 'back'; entry title = stored leaving title, else the location's route title (titleKeyForLocation in appRoutes.ts).
- [x] T035 [US3] Add `wm.system.back` (scope `wm.navigation`, `agentCallable: false`) to `src/lib/actions/wmActions.ts` with the three-step handler from contracts/wm-actions.md §5 (close open shell overlay → back in top visible window's active tab → compact mode without entries opens the window overview); expose the overlay open-state it needs from the shell components/store; add its three cases to `scripts/check-wm-navigation.ts`
  - Done 2026-09-25: resolveSystemBack in tabNavigation.ts (3 tests); overlay open states moved from wm/Desktop.vue into the store (shell.overlays) so system back can close them and open the window overview; reka dropdown menus close themselves and are not covered.
- [x] T036 [US3] Add the Android back hook behind a platform check in the Tauri setup (research R7): verify the Tauri 2 back-button API first, call `wm.system.back`; mark with a `ponytail:` comment that it is untested until an Android target exists
  - Done 2026-09-25: @tauri-apps/api 2.11 onBackButtonPress in plugins/actions.client.ts behind a user-agent check, ponytail-marked; the Rust crate is tauri 2.6.2, so whether the event fires there is unverified until an Android target exists.

**Checkpoint**: M5–M8 pass; system-back logic covered by tests

---

## Phase 6: User Story 8 - Inhalte in Tabs verändern die Navigation nicht (Priority: P1)

**Goal**: Embedded documents (haextensions) cannot change haex-vault's navigation

**Independent Test**: quickstart M13, M21, M22; SC-009

- [x] T037 [US8] Keep the top document's webview history flat: enter the workspace page with `router.replace` from `src/pages/index.vue` and the onboarding completion path in `src/pages/onboarding/[instance].vue`; add `onBeforeRouteLeave` to `src/pages/workspace/[instance].vue` that cancels every router navigation away and ignores it (never interpreted as back, research R7)
  - Done 2026-09-25: index.vue and the onboarding completion navigate to the workspace with replace: true; pages/workspace/[instance].vue cancels every route leave (onBeforeRouteLeave(() => false)) without interpreting it. The ?open= cleanup is a same-route replace and unaffected. check:vault-lifecycle 12/12.
- [x] T038 [US8] Disable webview-level history navigation from input devices where the platform allows it, in the Tauri window setup (`src-tauri/src/lib.rs` or the window builder): WebView2 browser accelerator keys off; mouse back/forward navigation off per the T032 findings; document platforms without such a setting as limitations in research.md R17
  - Done 2026-09-25: no Rust change possible: Tauri 2.11.5 does not expose WebView2 browser_accelerator_keys (only wry does) nor a mouse-navigation switch; documented in research.md R17 with the resulting Windows limitation (Alt+← inside a focused iframe may move that iframe's own history). FR-035 still holds via the flat history + non-interpreting leave guard.
- [x] T039 [US8] Run quickstart M13, M21 and M22 manually and record the results (including per-platform limitations) in a note on this task
  - Done 2026-09-25: M13 and M21 automated as e2e scenario scripts/e2e/scenarios/tab-content-isolation.test.ts and passing on Linux/WebKitGTK (webview history.back/go(-5) and an embedded document's 20 pushState + go(-30) + back leave the workspace page, the open chat and the disabled back button untouched; SC-009). M22 (physical mouse over the iframe) stays with T032.

**Checkpoint**: 20 history writes and backs inside an iframe change no tab history (SC-009)

---

## Phase 7: User Story 4 - Apps direkt an einem Ort öffnen (Priority: P2)

**Goal**: Open an app at a location; a running singleton navigates instead

**Independent Test**: quickstart M14–M15; singleton-push tests

- [x] T040 [P] [US4] Add tests to `scripts/check-wm-navigation.ts`: `wm.app.open` with `at` on a closed app starts a single-entry history at `at`; on an open singleton it activates the tab (015 FR-016) and pushes `at`; the same location adds no entry (US4 AS3)
  - Done 2026-09-25: covered by the T016 tests in check-wm-nav-store.ts (closed app → single entry at 'at'; open singleton → activated + pushed; same location → no entry).
- [x] T041 [US4] Add `wm.app.open` (`appId`, `at?`; target `none`, scope `wm.navigation`) and `wm.tab.new` (`appId`, `at?`; target `window`, scope `wm.layout`) to `src/lib/actions/wmActions.ts`, delegating to the store's `openApp`/`addTab`
  - Done 2026-09-25: WM_OPEN_ACTIONS in wmActions.ts; handlers reject unknown app ids with 'unknown app <id>' (openApp itself ignores them silently) and report { tabId, created }.
- [x] T042 [US4] Accept `&at=<path>` next to `?open=` in `src/pages/workspace/[instance].vue` (call `wm.app.open`, then strip both via `router.replace`) and keep the legacy redirects in `src/pages/{chat,settings,federation}/[instance].vue` working (FR-013)
  - Done 2026-09-25: ?open= now runs shell.app.open with the optional &at=; both parameters are stripped with router.replace. Legacy redirect pages unchanged.
- [x] T043 [US4] Make the root `WmRouterView` handle an unknown location: `replace('/')` and show the `wm.nav.unknownLocation` toast (haex-ui toast) (FR-014)
  - Done 2026-09-25: WmRouterView root: replace('/') + vue-sonner toast shell.nav.unknownLocation (the app already mounts ShadcnSonnerToaster).
- [x] T044 [US4] Route the chat header's settings button (`src/components/chat/ChatHeader.vue` / `ChatApp.vue`) through `wm.app.open` with `appId: 'system.settings'`
  - Done 2026-09-25: both open-settings bindings in ChatApp.vue call useAction('wm.app.open'); the chat harness injects a recording useAction (pageGlobals.actionLog). ChatApp.vue 498 lines; check:chat-state 45/45, check:vault-lifecycle 12/12.

---

## Phase 8: User Story 5 - Tab-Titel folgt der Ansicht (Priority: P2)

**Goal**: Tab, tab list, window overview and history list show the view's title

**Independent Test**: quickstart M5 titles; title tests

- [x] T045 [P] [US5] Add title tests to `scripts/check-wm-navigation.ts`: display title = `setTitle` override → deepest matched `titleKey` (with `{param}` interpolation) → app name; the override resets on navigation; a left entry keeps the title shown when it was left (research R9)
  - Done 2026-09-25: locationTitle (routeMatch.ts) tested in check-wm-navigation.ts; leaving-title storage is covered by the history-reducer tests (push/go store the leaving title), the override reset lives in stores/wmNavigation.ts navigate/go.
- [x] T046 [US5] Implement the title chain in `src/stores/windowManager.ts` `tabDisplayInfo` and store the leaving title on `push`/`go`; show it in `wm/TabBar.vue`, `wm/TabListMenu.vue`, `wm/WindowOverview.vue` and `wm/HistoryMenu.vue`; let the chat set the conversation title via `setTitle` in `useChatNavigation.ts`
  - Done 2026-09-25: tabDisplayInfo returns the routed titleKey plus titleParams (titleForLocation in appRoutes.ts); tab bar, tab list, window title, window overview, workspace overview and history menu translate with the params. useChatNavigation sets the conversation title as the tab title (new conversation keeps the route/app title). ChatApp.vue 499 lines, stores/windowManager.ts 484.

---

## Phase 9: User Story 7 - Jede Aktion ist beschrieben und aufrufbar (Priority: P2)

**Goal**: A complete, described action catalog for shell, chat and settings; guardrails locked for agents

**Independent Test**: quickstart M23; runner and catalog tests with agent callers (SC-007, SC-008)

- [x] T047 [US7] Complete `src/lib/actions/wmActions.ts` with every shell action of contracts/wm-actions.md §1 (layout, workspace, overview, launcher, read actions `wm.state.get`, `wm.tab.history`, `wm.apps.list`, `wm.actions.list`), each with English `description`, input/result schema, target, scope, effect; destructive window/tab closes run the 015 guards and return `failed` "declined by user" on decline
  - Done 2026-09-25: src/lib/actions/wmLayoutActions.ts (layout + read actions; also shell.window.setGeometry and shell.workspaces.overview for agents) with handlers in src/stores/wmLayoutHandlers.ts; the plugin passes nuxtApp.$i18n.t so read results carry human titles. requestCloseWindow/requestCloseTab now resolve a boolean and the workspace delete flow moved into requestDeleteWorkspace (useWmTab.ts), so a declined confirmation fails the action with 'declined by user'.
- [x] T048 [US7] Switch every state-changing control in `src/components/wm/*.vue` (tab bar, "+" menu, Chevron, window controls, overview, workspace overview, launcher, close confirm triggers) to `useAction`; no direct store mutation from templates remains
  - Done 2026-09-25: WmWindow (tab select/close, minimize, maximize incl. double click, close), WmWindowOverview, WmWorkspaceOverview, WmLauncher, WmNewTabMenu and the WmDesktop buttons call useAction. Exempt: implicit focus on pointer down and drag/resize gestures (continuous gestures; agents use shell.window.setGeometry).
- [x] T049 [US7] Inventory every state-changing control in `src/components/apps/ChatApp.vue`, `src/components/chat/**` and write the list into this task's note; create `src/lib/actions/chatActions.ts` with at least the chat rows of contracts/wm-actions.md §1 (`chat.approval.decide` and `chat.permissionMode.set` in scope `guardrails`, `agentCallable: false`) plus every inventoried control
  - Done 2026-09-25: Inventory (ChatApp + components/chat): sidebar new chat / select / save title / confirm delete, header new chat, composer send / abort / permission mode / approval allow+deny / cancel, model + effort selection (ComposerSettingsPopover via Composer), status-banner retry send / retry model load, empty-state catalog download (ModelSelection), integrity dialog load untrusted / repair / choose other, voice auto-send toggle → all actions (src/lib/actions/chatActions.ts: 11 tab-bound + 6 global; approvals, permission mode and integrity overrides in scope guardrails). Exempt (draft/view state): typing, attachments add/remove, voice recording and transcript, reasoning accordion, editing start/cancel, delete dialog open/close, error dismissal, autonomy preference re-read, integrity dialog open state. Also exempt: the lock button — it ends the process (spec 013), is guardrail-level anyway, and check:vault-lifecycle tests ChatApp's lock() directly; a shell.vault.lock action is a follow-up for the shortcuts spec.
- [x] T050 [US7] Implement tab-bound chat handlers in `src/composables/useChatActions.ts` (registered through `useWmTab().registerActionHandler`, one call from `ChatApp.vue`) and switch the chat controls to `useAction`; `pnpm check:chat-state` keeps its T002 count
  - Done 2026-09-25: src/composables/useChatTab.ts (navigation + tab-bound handlers reusing the chat's own functions + approval response + close guard; returns ui callbacks that run the actions) and src/stores/chatActionHandlers.ts (global model/reasoning/download/integrity/auto-send handlers). Composer, ModelSelection, VoiceInputControl and ChatApp bind actions. The runner now only opens the app when no mounted tab has the handler yet. Chat harness stubs extended (runAction, registerActionHandler). ChatApp.vue 484 lines; check:chat-state 45/45, check:vault-lifecycle 12/12.
- [x] T051 [US7] Inventory every state-changing control in `src/components/apps/SettingsApp.vue`, `src/components/settings/**`, `src/components/models/**` and write it into this task's note; create `src/lib/actions/settingsActions.ts` with at least the settings rows of contracts/wm-actions.md §1 (`settings.delegate.connectProvider`, `settings.autonomy.setMode`, `settings.delegate.setDenyRules` in scope `guardrails`, `agentCallable: false`) plus every inventoried control
  - Done 2026-09-25: Inventory (SettingsApp, components/settings, components/models): device alias save; default model save/clear per scope; STT model switch; HF management update check / install update / catalog download / delete / load here / integrity decisions; HF file picker install; delegate provider connect / submit code / refresh models; autonomy mode save; deny rules save → src/lib/actions/settingsActions.ts (16 actions; connect, submit code, autonomy mode and deny rules in guardrails; load-here and integrity reuse chat.model.select / chat.modelIntegrity.decide). Exempt: HF search, details, previews and file selection (browsing), tab switches, the delete confirm() prompt. Finding: HuggingFaceModelManagement.vue was already 582 lines (> 500) before this spec — now 580; splitting it is a follow-up.
- [x] T052 [US7] Implement the global settings handlers in `src/plugins/actions.client.ts` (via `usePreferences`, `useDevice`, provider and model composables) and switch the settings/model controls to `useAction`; `settings.get` never returns provider credentials or secrets
  - Done 2026-09-25: src/stores/settingsActionHandlers.ts holds the write logic; components call new useActionOrThrow, which rethrows the handler's raw error (ActionOutcome.error, runner now also extracts a backend error's reason as message), so existing try/catch and HF error-key parsing stay unchanged. settings.get returns preference values only, never provider credentials.
- [x] T053 [US7] Add the template rule to `scripts/check-vue-templates.ts` (research R20): in `src/components/wm/**` and `src/components/apps/**`, `@click`/`@select` handlers that change state call only `useAction`-derived functions; exemptions carry an `action-exempt:` comment; extend its tests
  - Done 2026-09-25: Implemented as a denylist of write APIs (shell store mutations, preference/device/model/HF/provider writes, the models store's write actions) that .vue files under src/ may not call directly unless an 'action-exempt: <reason>' comment is on the line or up to three lines above (a static 'changes state' check is not feasible; this catches exactly the regressions that bypass the catalog). Exempted with reasons: the Shell↔App programmatic API in WmTabPanel, the drag/resize gesture, the onboarding wizard (runs before the Shell). The script has no own test file; the rule runs in check:templates and is green (56 templates).
- [x] T054 [US7] Extend `scripts/check-wm-navigation.ts` with catalog-wide agent tests: every `guardrails` action refused for both agent kinds and executed for `user` (SC-008); `wm.state.get` returns workspaces, windows, tabs with app, location, title, attention
  - Done 2026-09-25: check-wm-actions.ts: expected guardrail ids present; every guardrails action refused for builtinAgent and externalAgent before its handler and executed for user (SC-008); every other action (except the platform-only shell.system.back) callable by an agent naming its target; the FR-028 read actions exist. The shell.state.get handler itself depends on the Pinia store and i18n and is covered by the quickstart pass (T060). Keybinding tests moved to scripts/check-wm-keys.ts for the line limit. 73/73.

---

## Phase 10: User Story 6 - Navigation im Chat (Priority: P3)

**Goal**: Conversation switches and new conversations are history entries, consistent with specs 003/004/006

**Independent Test**: quickstart M2–M4, M16

- [x] T055 [US6] In `src/composables/useChatNavigation.ts`: "new conversation" pushes `/`; the first message creating a thread (`useComposer.ts` sets `activeThreadId`) replaces `/` with `/thread/<id>`; deleting the active thread (spec 006 FR-015) replaces with `/`
  - Done 2026-09-25: implemented with T025 in useChatNavigation (new conversation pushes '/'; the first message's thread creation and the active thread's deletion replace via the chat → location sync); covered by the T057 replay.
- [x] T056 [US6] Skip entries to deleted conversations: on reaching `/thread/<id>` for a missing thread, `removeEntry` and continue in the same direction, else `replace('/')` (FR-027); navigation reuses `selectThread`, so running replies and pending approvals follow specs 003/006 unchanged
  - Done 2026-09-25: skipTabEntry (tabNavigation.ts, 2 tests) + per-tab last travel direction in stores/wmNavigation.ts, exposed as useTabRouter().skipCurrent(); useChatNavigation skips entries of conversations no longer in the list, else replaces with '/'. Navigation still goes through selectThread, so specs 003/006 rules apply unchanged.
- [x] T057 [US6] Add chat-navigation cases to the replay harness via `scripts/lib/chat-state-harness.ts` / `scripts/check-chat-state.ts` where feasible (open → back → forward, create-replace, delete-skip); existing replay count unchanged plus the new cases
  - Done 2026-09-25: new scripts/check-chat-navigation.ts (4 replays: open/back/forward, create-replace, delete-skip, unknown id → '/') on the shared harness with its recording router; check:chat-state now runs both files: 49/49 (45 + 4). Finding: check-chat-state.ts itself is 1199 lines (> 500) since before this spec.

---

## Phase 11: Polish & Cross-Cutting Concerns

- [x] T058 [P] Add all new keys (`wm.nav.*`, `wm.chat.thread`, `actions.*`, `actions.scopes.*`) to `src/i18n/locales/de.json` and `en.json` in lockstep; `jq` key comparison shows no difference (FR-023)
  - Done 2026-09-25: titles for all 58 actions and 9 scopes in de.json/en.json; key sets identical (verified), every catalog titleKey resolves.
- [x] T059 [P] Add Ort, Tab-Historie, Aktion, Berechtigungsbereich and Aufrufer to `CONTEXT.md` (note: code says "action", never "command")
  - Done 2026-09-25: Ort/Tab-Historie and Aktion/Berechtigungsbereich/Aufrufer entries after the Shell entry, with the 'not command' note.
- [ ] T060 Run quickstart §1 (all automated checks) and §2 (M1–M23); record results and platform limitations for the PR
  - Status 2026-09-25: §1 all green — check:wm-navigation 75/75, check:wm-state 44/44, check:chat-state 49/49, check:vault-lifecycle 12/12, check:vault-passphrase-lifetime 7/7, check:e2e-lib 160/160, check:templates 56 (no direct writes), typecheck, typecheck:scripts, lint, format:check. Full e2e suite (debug build, Xvfb): 7 passed, 1 skipped (relaunch needs a release build) — the first run exposed a real bug (models store created in the startup plugin → vue-i18n error 26 on the workspace page), fixed in 9a6a494. Open: the manual §2 scenarios M1–M20 and M22–M24 need an operator in the running app (M13/M21 automated, T039; M24 measures SC-003).
- [x] T061 Run `/speckit-analyze` on spec/plan/tasks; resolve all CRITICAL/HIGH findings before opening the PR
  - Done 2026-09-25: /speckit-analyze: 1 CRITICAL (HuggingFaceModelManagement.vue > 500 lines touched without Complexity Tracking) and the documentation findings I1/I2/U1/U2/G1 fixed in the plan, spec, research and quickstart; C2 (change size) accepted by the operator as one large PR, noted in the PR description. Open by nature: T032 (physical mouse) and T060's manual scenarios.

---

## Dependencies & Execution Order

- **Setup (T001–T004)** → **Foundational (T005–T021)** → user stories.
- **US1 (T022–T026)** is the MVP and needs Foundational only.
- **US2 (T027–T028)** and **US3 (T029–T036)** need US1 (buttons, back/forward actions).
- **US8 (T037–T039)** needs US3's T032 spike for T038; T037 can start after Foundational.
- **US4 (T040–T044)** needs Foundational; **US5 (T045–T046)** needs US3's T034 for the history-list titles.
- **US7 (T047–T054)** needs Foundational; T048 touches the same shell components as US1/US3 — do it after them. T050 and T055–T057 both touch chat composables — run US6 after T050.
- **US6 (T055–T057)** needs T025.
- **Polish (T058–T061)** last; T061 is the PR gate.

## Parallel Examples

- Foundational tests: T005, T006, T007, T008 together; then T009, T010, T011, T012 together.
- After Foundational: T017, T018, T019 together.
- US3: T029 and T030 together while T032 (spike) runs.
- US7: T049 and T051 inventories in parallel; their handler tasks T050/T052 afterwards.
- Polish: T058 and T059 together.

## Implementation Strategy

1. **MVP**: Setup + Foundational + US1 → back/forward for the chat, all 015 checks green. Stop and validate M1–M3.
2. **P1 completion**: US2, US3, US8 → correct targets, inputs, isolation.
3. **P2**: US4, US5, US7 → deep links, titles, full agent-ready catalog (prerequisite for spec 021).
4. **P3**: US6 → chat history details.
5. Each story ends with its checkpoint; commit per task, push after each phase.
