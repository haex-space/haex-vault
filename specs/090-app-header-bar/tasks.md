# Tasks: The header bar belongs to the app

**Input**: Design documents from `specs/090-app-header-bar/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: requested by the constitution (tests in `*_tests.rs` next to each Rust module, a check script for each
piece of TypeScript logic) and by the spec's independent tests. Test tasks come before the code they test.

**Order**: the phases follow the plan's four pull-request steps, not the spec's priorities alone. The shared header
(phases 1–2) comes first because every story needs it; Settings (US1) proves it, narrowing (US2) and the tab model
(US5) finish step 1; the extension channel (US3 and US6 together, they are one code path) is step 2; the file browser
(US4) is step 3; polish is step 4.

| Step (= PR) | Phases                                       |
| ----------- | -------------------------------------------- |
| 1           | 1 Setup, 2 Foundational, 3 US1, 4 US2, 5 US5 |
| 2           | 6 Extension channel (US3 + US6)              |
| 3           | 7 US4                                        |
| 4           | 8 Polish                                     |

**Rules for every task**:

- Run `graphify query "<what the function does>"` before writing a new named function, component or module, and
  reuse what exists (spaex). Plan time already found `notifications.rs::icon` as a match (T026).
- Keep each file at most 500 lines. `src/stores/windowManager.ts` is at 501 lines: **do not add to it**; put new store
  code in a sibling module as `wmLayoutHandlers.ts` does (T005).
- Rust builds only as in [quickstart.md](quickstart.md) (10 GB scope, `-j 1`, `env -u CC -u CXX -u AR`). After a
  `cargo test` that touches ts-rs types, run `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.
- Before every push: `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`, and **all** `pnpm check:*` after a change
  to a composable or a module under `src/lib/wm/` (they run in Node sandboxes: no DOM globals, no `instanceof Element`).
- UI texts that holzi adds go in `src/i18n/locales/de.json` and `en.json` in lockstep; no save or apply buttons.
- Commits follow Conventional Commits, English, short, **no agent trailer**; a PR per step from a topic branch in a repository-local worktree
  (`git worktree add .worktrees/090-app-header-bar -b 090-app-header-bar origin/main`, never `git switch` in the
  primary checkout); E2E does not run on pull requests, so before merging a step run
  `gh workflow run ci.yml --ref <branch>`.
- Comments in code: say what and why, cite spec 090 FR numbers where a rule comes from; mark known ceilings
  `ponytail:` with the ceiling and the upgrade path.

## Format: `- [ ] [ID] [P?] [Story] Description with file path`

---

## Phase 1: Setup

- [ ] T001 Measure the title row's height in the running app at the default window size (`getBoundingClientRect()` of
      the block that holds `WmTabBar` in `src/components/wm/Window.vue`) and write the number into `research.md` R8
      and `quickstart.md`; if it is not 37 px (36 + 1 border) within 1 px, correct R8's number (not the spec)
- [ ] T002 Create `src-tauri/src/extensions/bridge/header.rs` with only the types of
      [data-model.md](data-model.md) (`WmHeaderDescription`, `WmHeaderControl`, `WmHeaderCollapse`,
      `WmHeaderMenuItem`, `WmHeaderEvent`, `icons` table) as `serde` + `ts_rs::TS` types exported with
      `#[ts(export, export_to = "../../src/types/bindings/")]`; add `pub mod header;` to
      `src-tauri/src/extensions/bridge/mod.rs`; run the Rust tests once to generate the bindings, strip trailing
      spaces, then import one binding with `import type` from a file under `src/lib/wm/` and run
      `pnpm typecheck:scripts`. If it cannot resolve the nested binding imports, record the fallback of research R16
      (a structural subset type declared in `src/lib/wm/headerLayout.ts`) in `research.md`

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: the header row, its layout function and the system-app API. No app uses it yet; Window.vue switches in
T011.

- [ ] T003 [P] Write `scripts/check-wm-header.ts` for `layoutHeader` with every case of the table in
      [contracts/collapse.md](contracts/collapse.md) (fits; order 1 before 2; order across areas; `icon` then `overflow`;
      `shrink` never below the minimum; `keep` last; all `keep`; `icon` without icon; equal orders independent of
      array order; menu button counted once; window narrower than the window buttons; compact budget; restore order by
      widening); also the search rules as a pure module `src/lib/wm/headerSearch.ts` (research R7): the `input` merge
      (several changes within 100 ms give one event with the latest text; Enter, blur and a new description flush at
      once, FR-012) and the typed-text rule (a new `value` is applied only when it differs from the previous
      description's `value`, an echo of the typed text changes nothing, FR-013), tested with an injected clock; add
      the file to `check:wm-state` in `package.json`; it fails until T004
- [ ] T004 Implement `layoutHeader` in `src/lib/wm/headerLayout.ts` exactly as
      [contracts/collapse.md](contracts/collapse.md) and `src/lib/wm/headerSearch.ts` (pure, relative `.ts` imports so Node
      loads them, no Vue); make T003 pass
- [ ] T005 [P] Add `header: WmHeaderDescription | null` and `headerHandler` to `TabRuntime` in
      `src/lib/wm/types.ts`, initialised to `null` where `TabRuntime` is created; add the store functions
      `setTabHeader(tabId, header)`, `registerTabHeaderHandler(tabId, handler)` and `headerOf(tabId)` in a **new**
      `src/stores/wmTabHeader.ts` (taking the `tabRuntime` map, as `wmLayoutHandlers.ts` does) and expose them from
      `src/stores/windowManager.ts`, **after first moving at least 10 lines of existing code out of it** into a sibling
      module so the file ends at or below 500 lines (it is at 501 today; the rule above forbids it growing);
      extend `scripts/check-wm-state.ts`: a tab closed or a runtime pruned by `syncTabRuntime` loses its header; a
      description stays on a background tab
- [ ] T006 Add `setHeader(header)` and `registerHeaderHandler(handler)` to `WmTabApi` and its inert default in
      `src/composables/useWmTab.ts`; provide them in `src/components/wm/TabPanel.vue` next to `setTitle` and
      `registerActionHandler`, calling T005's store functions
- [ ] T007 Create `src/composables/useWmHeader.ts`: `useWmHeader(description: () => WmHeaderDescription | null,
onEvent: (event: WmHeaderEvent) => void)`; a `watchEffect` calls `tab.setHeader(description())`, the handler is
      registered once, both are cleared in `onScopeDispose`; a development-only `console.warn` for an obviously
      malformed description (research R6, contracts/header-description.md §System apps)
- [ ] T008 Create `src/components/wm/HeaderControl.vue`: renders one control by `type` in its **row form** and its
      **menu form** (research R10): `button`, `toggle`, `search` (field with search icon and clear button, Escape
      clears first), `menu` (`ShadcnDropdownMenu` in the row), `choice` (segmented buttons), `breadcrumb` (crumbs;
      `shrink` form = the open segment's name with the menu of the segments above, then Edit and Reload when
      `editable`, as `src/components/files/PathBar.vue` does on a small screen; `editable` turns it into a text field
      with the whole path and emits `submit`), `title`, `separator`; text only (`{{ }}`, never `v-html`, FR-023);
      a `lucide:` icon through Nuxt Icon, an extension picture from the description's `icons` table as `<img>`
      (`mask-image` in the text colour when `monochrome`, R9); every interactive element stops `pointerdown`
      propagation; emits `WmHeaderEvent`s; `data-testid` is the description's `testId` (system apps only) or
      `wm-header-<id>`; at most 500 lines (split the breadcrumb into
      `HeaderBreadcrumb.vue` if needed)
- [ ] T009 Create `src/components/wm/HeaderOverflow.vue`: the `⋯` button and its `ShadcnPopover` holding the moved
      controls in their menu form (research R10); closes after a button, toggle, choice or menu entry, stays open
      for a search; returns focus to the menu button when the focused control moved into it; `aria-label`, `data-testid`
      `wm-header-overflow`
- [ ] T010 Create `src/components/wm/Header.vue`: the row of start, middle and end areas plus the window buttons slot;
      36 px + border (T001), `bg-muted/40`, `role="toolbar"` labelled with the window title; drag and double-click
      behaviour of the title row (props/emit from `Window.vue`); a hidden, inert, `aria-hidden` ghost row measures
      each control's full and icon width, a `ResizeObserver` on the header re-runs `layoutHeader` from T004; at most one
      description applied per animation frame; the first frame stays `visibility: hidden` until the first layout;
      the title of the active tab in the middle when `center` is empty and as `aria-label`/tooltip otherwise (FR-004);
      keeps what the user typed in a `search` and merges `input` events, both through `headerSearch.ts` from T004
      (FR-012, FR-013), and flushes on Enter, blur and a new description; keys controls by `id` so focus stays; a focused control that moves into the menu hands
      focus to the menu button
- [ ] T011 Replace the title-row block of `src/components/wm/Window.vue` by `<WmHeader>`: no `WmTabBar`, no
      `WmTabListMenu`; keep `WmWindowControls` as the end slot and the `@pointerdown`/`@dblclick` move and maximize
      behaviour; add the header strings (overflow button label, empty-title fallback) to `src/i18n/locales/de.json`
      and `en.json`; run `pnpm check:templates`
- [ ] T012 After T021 (it adjusts the checks that still name the tab strip), run `pnpm check:wm-state && pnpm
check:wm-navigation && pnpm typecheck`, and open a window in the dev app:
      title and window buttons show for every app that gives no description (FR-027), dragging and double-click work

**Checkpoint**: windows have the new header; apps without a description show only the title and the window buttons.

---

## Phase 3: User Story 1 - Settings has its search in the header (Priority: P1)

**Goal**: Settings shows the sidebar button, the search and the window buttons in one row; no row of its own below.

**Independent Test**: open Settings: exactly one bar above the content; search, open a hit, Escape; hide and show the
sidebar; drag by an empty part of the bar.

- [ ] T013 [US1] In `src/components/apps/SettingsApp.vue` call `useWmHeader`: `start: [toggle "sidebar" pressed =
sidebarVisible, collapse keep]`, `center: [search "q" value = query, collapse shrink order 10]`; the handler:
      `toggle` → `toggleSidebar`, `input` → `query`, `submit` → `selectFirstHit`; remove `<SettingsToolbar>` and the
      `searchOpen` state (the search is always an open field, research R12); Escape clears first and then returns
      focus as spec 023 FR-023 says; keep the sidebar's `@container` rules untouched
- [ ] T014 [US1] Delete `src/components/settings/Toolbar.vue`; remove its i18n keys that nothing uses any more in
      `de.json` and `en.json` (`settings.sidebar.*` stay if the toggle's label still uses them); keep the test ids
      `settings-sidebar-toggle` on the header toggle (set the control's `testId`)
- [ ] T015 [P] [US1] Update `scripts/e2e/scenarios/settings-categories.test.ts`, `settings-search.test.ts` and
      `settings-header-back.test.ts` to the header controls (`wm-header-q` for the search, `settings-sidebar-toggle`
      for the toggle); no use of `settings-search-open`
- [ ] T016 [US1] Create `scripts/e2e/scenarios/window-header.test.ts`: Settings window has one bar and no row under it
      (the content's top edge is at the header's bottom edge) and the header is at most 37 px high, the value T001
      measured for the old title row (FR-005, SC-001); dragging an empty part moves the window and a double
      click maximizes; the search works as in spec 023 FR-023 (hits, Enter, Escape, clear button)

**Checkpoint**: US1 works alone; compare Settings to the COSMIC screenshot by eye.

---

## Phase 4: User Story 2 - A narrow window keeps everything reachable (Priority: P1)

**Goal**: controls collapse as their app ordered; nothing is clipped; the menu holds what moved.

**Independent Test**: resize a window from its minimum to the full screen and back; at every width nothing is clipped,
the window buttons stay, every control is reachable in the row or the menu.

- [ ] T017 [US2] Rewrite `scripts/e2e/scenarios/settings-narrow-window.test.ts` for the header: below 672 px the sidebar
      is gone and the toggle opens it over the content (spec 023 SC-004 unchanged); the always-open search shrinks and
      then moves into `wm-header-overflow`
- [ ] T018 [US2] In `scripts/e2e/scenarios/window-header.test.ts` add the width sweep of SC-002: at no fewer than ten
      widths from the minimum to full screen, assert no control's box leaves the header, no two overlap, the window
      buttons are visible, and every control id is either in the row or listed in the overflow popover; widening
      returns the controls in reverse; a control that stays in the row keeps its focus while others collapse
      (FR-016); repeat three of the widths with `dir="rtl"` on the document and check the areas mirror (spec Edge
      Cases, layout direction)
- [ ] T019 [US2] Keyboard and assistive checks in the same scenario: Tab walks start → middle → end → window buttons;
      the overflow popover opens and closes by keyboard and returns focus; `role="toolbar"` and the labels exist
      (FR-006, SC-005)
- [ ] T020 [P] [US2] Check the compact window: in `scripts/e2e/scenarios/android-phone-layout.test.ts` the header
      shows only the close button on the end, collapses by the same rules, and the content is not covered; if that
      scenario cannot show the header (it is Android only), do this check by hand in T043

**Checkpoint**: US1 and US2 together are the COSMIC behaviour.

---

## Phase 5: User Story 5 - Tabs are out of the header but not lost (Priority: P2)

**Goal**: no tab strip; the tab model, actions, history, restore and guards behave as before.

**Independent Test**: a window with three tabs restarts with all three; actions switch and close tabs.

- [ ] T021 [P] [US5] Adjust the checks that referred to the strip to the model: `scripts/check-wm-navigation.ts`,
      `scripts/check-wm-state.ts`, `scripts/check-wm-keys.ts` (search for `tabBar`, `active tab emphasis`, spec 040
      names); the active-tab emphasis and the inactive-tab attention dot are suspended (FR-029), the model's
      `attention` flag is still checked
- [ ] T022 [US5] Extend the existing session-restore scenario (`scripts/e2e/scenarios/passwords-session-restore.test.ts`
      shows the pattern; add `scripts/e2e/scenarios/window-tabs-restore.test.ts` if no generic one exists): a window with
      three tabs (create two with `wm.tab.new`), restart, all three come back with location, history and the active
      tab; `wm.tab.activate`, `wm.tab.back`/`forward` (also a mouse side button where the platform delivers it) and
      `wm.tab.close` act as before, a close guard still asks (SC-007, FR-028)
- [ ] T023 [US5] Find every other E2E use of the strip (`grep -rn "tab-close\|wm-new-tab" scripts/e2e`) and move it to the
      actions or remove it; then run knip (`mcp__knip__knip-run` or `pnpm knip`) and delete `src/components/wm/TabBar.vue`,
      `TabListMenu.vue` and `NewTabMenu.vue` if nothing uses them, with their i18n keys (research R11)
- [ ] T024 [P] [US5] Add a one-line note to `specs/015-workspace-shell/spec.md` (FR-007, FR-031–036), `specs/020-tab-navigation/spec.md`
      and `specs/040-active-tab-emphasis/spec.md`: "The tab strip is suspended by spec 090 until spec 091 (switching tabs) is
      done" (FR-029); no other change to those specs

**Checkpoint (end of step 1)**: open the PR for phases 1–5; `gh workflow run ci.yml --ref <branch>`.

---

## Phase 6: Extension channel — User Story 3 (Priority: P1) and User Story 6 (Priority: P2)

**Goal**: an extension puts controls in its window's header and gets events; a hostile one cannot do anything else.

**Independent Test (US3)**: the `probe` fixture sends a description with every control; they appear, events arrive,
a new description replaces the old without resetting a typed search. **(US6)**: forged, oversized, wrongly shaped,
duplicated and cross-tab input changes nothing.

- [ ] T025 [P] [US6] Write `src-tauri/src/extensions/bridge/header_tests.rs` (and `#[cfg(test)] #[path = ...] mod` in
      `mod.rs` as the other bridge tests do) for every check of
      [contracts/header-description.md](contracts/header-description.md) (#1–#11, 9a, 9b) at the boundary and one over:
      32/33 controls, 64/65 menu entries, 8/9 options, 64/65 segments, 120/121 characters, a 64/65 character id, order
      1000/1001, 32 KiB/over without inline icons; an unknown control type is left out and the rest stays; a missing
      `label`, a duplicate id (across controls, entries, options and segments), a `lucide:` icon, a URL, a path with
      `..`, a non-picture file, an icon over 256 KiB (and 100 icons of 100 KiB in one description are accepted), a missing bundle file, an inline `data:` icon of an installed extension (refused) and of a development version (accepted up to 256 KiB decoded, refused above), a `testId`, a control or bidi-format character in a text, an `icons`
      field sent by the caller (ignored), `choice.selected` not an option; the happy path with a bundle icon (build the
      test bundle as `notifications_tests.rs` does) returns the `icons` table with each file once; a second `set` of the same frame sends only the icons not sent before, and a new frame session sends them again; it fails until T027
- [ ] T026 [US3] Extract `notifications.rs::icon` (path-or-`data:` icon reading, `ICON_TYPES`, `MAX_ICON_BYTES`) into a
      shared `src-tauri/src/extensions/icon_file.rs` with the allowed types and the byte limit as parameters
      (notifications keep raster types only, the header adds SVG); register the module in
      `src-tauri/src/extensions/mod.rs`; notifications' behaviour and tests stay unchanged and green; the header path maps the helper's limit error
      (`LimitExceeded`, an oversized icon) to `3001`, as the contract says
- [ ] T027 [US3] Implement `validate`, `resolve_icons` and `set` in `src-tauri/src/extensions/bridge/header.rs`
      ([contracts/extension-header.md](contracts/extension-header.md)): the checks in the contract's order, each control
      parsed on its own so an unknown type is only left out, errors as `3001` with `header: control "<id>": …`,
      icons through T026's helper (a blocking read via `ctx.db.read_blocking`, never on the async executor), the `icons`
      table added by Rust only, a per-frame-session cache of resolved icons so that an event carries only the icons not yet sent (research R9), then `ctx.emitter.emit("extension-header-set", {frame, header})`; register
      `extension_header_set` next to `extension_tab_attention` in `src-tauri/src/extensions/bridge/allowlist.rs`;
      keep `header.rs` under 500 lines (move icon handling to `header_icons.rs` if needed); make T025 pass
- [ ] T028 [P] [US3] Add the method row to `specs/017-extension-host/contracts/bridge.md` (§Methoden) and the
      "no permission, own header only" note to `specs/017-extension-host/contracts/permissions.md`
- [ ] T029 [P] [US3] Write `scripts/check-extensions-header.ts` and the small pure module
      `src/lib/extensions/header.ts` it tests: `acceptHeaderSet(ownFrame, payload)` takes only the frame's own payload
      (US6: another frame id changes nothing), `headerEventMessage(event)` builds
      `{type: "haextension:header:event", data, timestamp}`; add the script to `check:extensions` in `package.json`
- [ ] T030 [US3] Create `src/composables/useExtensionHeader.ts`: listens to `extension-header-set`, accepts it through
      T029's function, calls `tab.setHeader(header)`, registers the handler that posts events to the SDK port, and
      calls `tab.setHeader(null)` when the frame reloads, closes, fails or unmounts (FR-019, FR-020); it merges each
      event's `icons` into an icon table kept for the frame session (dropped in `closeAsync`, not when only the
      description is cleared) and hands the description with the full table to `tab.setHeader`; add one call in
      `src/composables/useExtensionFrame.ts` (it is at 460 lines: only the call and a port-post callback)
- [ ] T031 [US3] Create `scripts/e2e/scenarios/extension-header.test.ts` with the `probe` fixture
      (`window.probe.request('extension_header_set', …)` as `extension-dialog.test.ts` does): every control shows in
      the right area; a click and a search `input`/`submit` arrive as events in the frame; a second description
      replaces the first and a typed search keeps its text and caret when the value is echoed; a description over the
      limit is refused with `3001` and the old header stays; an extension whose frame reloads loses its controls; two
      tabs of a window show their own headers when active; in a development-version frame (opened as `extension-dev-mode.test.ts` does) an inline `data:` SVG icon with a
      script and an external reference is drawn and nothing runs or loads (US6 scenario 7), and the installed probe's
      inline `data:` icon is refused with `3001` (a bundle icon file is covered by the Rust tests); a label with markup shows as text (US6 scenario 1); a click on a control that a new description removed sends
      no event; two windows of the same extension each keep their own header
- [ ] T032 [P] [US3] In the `haex-space/vault-sdk` repository (a separate pull request, as for `tab.ts`): add
      `src/api/header.ts` (`HeaderAPI` with `set` and `onEvent`), `EXTENSION_COMMANDS.headerSet =
"extension_header_set"` in `src/commands/extension.ts`, `HAEXTENSION_EVENTS.HEADER_EVENT =
'haextension:header:event'` in `src/events.ts`, the types, `client.header` in `src/client.ts`, and a test as in
      `src/api/__tests__/tab.test.ts`; `header.set` resolves `true`, and `false` when the host answers unknown method (holzi's code 8000 or haex-vault 2.x's
      `Error("Unknown method: …")` text), every other error still rejects; test both replies and a `3001` (research R14). holzi does not
      wait for the release; when merged, cite it in `specs/090-app-header-bar/` as `haex-space/vault-sdk@<sha>:<path>`
- [ ] T033 [US3] Run the Rust gates: the new tests and `notifications` tests in the 10 GB scope; `cargo clippy` for the
      host and for `aarch64-linux-android` as in the feedback notes; strip trailing spaces in `src/types/bindings`;
      `git status` shows only the expected bindings

**Checkpoint (end of step 2)**: open the PR; `gh workflow run ci.yml --ref <branch>`; the SDK PR is linked.

---

## Phase 7: User Story 4 - The file browser has its bar in the header (Priority: P2)

**Goal**: with one panel the whole first row of spec 044 is in the header; with two panels the header keeps only the
sidebar button and the split switch.

**Independent Test**: use every control of the first row from the header, with one and with two panels; narrow the
window until the path collapses.

- [ ] T034 [P] [US4] Write `scripts/check-files-header.ts` for the builder of T035: one panel → sidebar toggle, back,
      forward, breadcrumb (with reload and edit), view choice, sort menu, hidden toggle, create menu (disabled by
      `canCreate`/`canCreateEncrypted`), split switch with the collapse order and forms; split on → only the sidebar
      toggle and the split switch; the split switch disabled when the window is too narrow (spec 062 FR-022); add it to
      `check:files`
- [ ] T035 [US4] Create `src/lib/files/header.ts`: a pure builder from the one-panel state
      (`src/components/files/PanelBar.vue` props: path, `canGoUp`, `canGoBack`, `canGoForward`, view, sort,
      `showHidden`, `canCreate`, `canCreateEncrypted`, `rootLabel`) to a `WmHeaderDescription`; make T034 pass
- [ ] T036 [US4] In `src/components/apps/FilesApp.vue` call `useWmHeader` with the builder; the handler runs the same
      catalog actions or functions the removed controls ran (FR-025; find them in `src/components/files/Toolbar.vue`
      and `PanelBar.vue`); with the split on the description is the reduced one; delete `src/components/files/Toolbar.vue`;
      `PanelBar.vue` stays for two panels; mark the twin with `ponytail:` (research R13: "controls exist as description
      and as PanelBar; upgrade: render PanelBar from the description")
- [ ] T037 [US4] The path bar in the header collapses through the breadcrumb control's `shrink` form (T008), decided by
      the window's width; remove the `max-md` swap from the one-panel use and keep it where `PanelBar` uses `PathBar`
      for a panel (`src/components/files/PathBar.vue`)
- [ ] T038 [P] [US4] Update the E2E scenarios that use the first row's test ids so they find them on the header controls
      (keep `files-sidebar-toggle`, `files-reload`, `files-path-bar` ids on the header controls through their `testId`
      where the control survives): `files-basic`, `files-agent`, `files-split`, `files-split-large`, `files-split-restore`,
      `files-split-storage`, `files-sync-rule-menu`, `files-restore`, `files-drives`, `files-media`, `files-storage*`,
      `files-encrypted*` (find the full list with `grep -rlE "files-(sidebar-toggle|split|path|reload|view|sort|back|forward|crumb)" scripts/e2e`)
- [ ] T039 [US4] In `scripts/e2e/scenarios/files-split.test.ts` add: turning the switch on moves the panel's bar into the
      panels and leaves the sidebar button and the switch in the header; turning it off puts the one panel's bar back
      (US4 scenario 2); and the path collapses to the open folder's name with its menu when the window is narrow

**Checkpoint (end of step 3)**: open the PR; `gh workflow run ci.yml --ref <branch>`.

---

## Phase 8: Polish & Cross-Cutting

- [ ] T040 [P] Run `mcp__knip__knip-run` and fix what this spec left unused (components, i18n keys, exports); do not touch
      what other code left
- [ ] T041 [P] Run `pnpm lint`, `pnpm format:check`, `pnpm typecheck`, `pnpm typecheck:scripts` and **all** `pnpm check:*`
- [ ] T042 [P] Add the term to `CONTEXT.md` (section "Erweiterungen"): **Header-Beschreibung** (what an app hands holzi
      for the window's header), **Header-Control**, **Überlauf-Menü**; German, in the file's style; no other change
- [ ] T043 Hand check with the quickstart's "See it in the running app": Settings against the COSMIC screenshot (SC-008,
      the operator), the effect time of a control (SC-006, about 100 ms), and a 400 px wide window (a phone)
- [ ] T044 `gh workflow run ci.yml --ref <branch>` for the whole E2E set after the last step (locally: `pnpm test:e2e --grep <text>`); fix or report each failure;
      note in the PR which scenarios were moved
- [ ] T045 Set the spec's status to `Implemented` and list the merged PRs (spec, header, extension channel, SDK,
      files) in a short note at the end of `spec.md`

---

## Dependencies & Execution Order

```text
Phase 1 Setup ─▶ Phase 2 Foundational ─▶ Phase 3 US1 ─▶ Phase 4 US2 ─┐
                                              │                      ├─▶ Phase 5 US5 ─▶ [step 1 PR]
                                              └──────────────────────┘
Phase 2 ─▶ Phase 6 (US3 + US6)  [step 2 PR; needs T002 types and T008–T010 renderer]
Phase 2 ─▶ Phase 7 (US4)        [step 3 PR; needs T008 breadcrumb; independent of Phase 6]
Phases 5, 6, 7 ─▶ Phase 8 Polish
```

- T004 needs T003 (test first); T010 needs T004, T007–T009; T011 needs T010; T012 needs T011 and T021 (T021, a Phase 5 task, is done right after T011).
- US1 needs the whole Phase 2. US2's E2E needs US1's scenario file (T016). US5's deletion (T023) needs every E2E that
  used the strip moved first.
- Phase 6: T025 before T027; T026 before T027; T030 needs T027 and T029; T031 needs T027 and T030; T032 is independent
  (another repository).
- Phase 7 is independent of Phase 6 and may be built first if the extension work waits for the SDK review; the order in
  the plan puts the extension channel first because it carries the security risk.

## Parallel Opportunities

- Phase 2: T003 (check) and T005 (store) are in different files; T008 and T009 can start once T002 exists.
- Phase 3: T015 (E2E updates) next to T013–T014.
- Phase 5: T021 and T024 next to T022.
- Phase 6: T025, T028, T029 and T032 at the same time; later T026.
- Phase 7: T034 and T038 next to each other.
- Phase 8: T040, T041, T042.

## Implementation Strategy

- **MVP = step 1** (phases 1–5): the header, the layout, Settings and the tab model. It delivers what the operator sees
  first and leaves extensions with a title-only header, which is harmless.
- Then the extension channel, then the file browser, each its own PR; every step ends with the fast checks green and
  the E2E workflow dispatched on the branch.
- A step that loses a scenario is not merged: scenarios are moved to the header, never deleted without a replacement.

## Task count

45 tasks: Setup 2, Foundational 10, US1 4, US2 4, US5 4, extension channel (US3 + US6) 9, US4 6, Polish 6.
