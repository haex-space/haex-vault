---
description: 'Task list for spec 023-settings-app'
---

# Tasks: Einstellungs-App mit Kategorien

**Input**: Design documents from `/specs/023-settings-app/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/settings-app.md](./contracts/settings-app.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires an executable check for non-trivial logic: the new `scripts/check-settings.ts` (`pnpm check:settings`, in CI) for the registry, header back, color scheme and app alias; a palette-color denylist in `scripts/check-vue-templates.ts`; Rust tests in `src-tauri/src/storage/known_devices_tests.rs` and `src-tauri/src/device/commands_tests.rs`. Write each test task before its implementation and see it fail first.

**Organization**: Grouped by user story. The branch `023-settings-app-plan` starts from `main` (specs 015, 020, 022 and the haex-ui pin `db48f9a948522c18a00331aac232718825cc9317` merged). Modules under `src/lib/**` import siblings relatively with a `.ts` suffix so the Node check scripts can load them. Every file stays ≤ 500 lines. Commits follow Conventional Commits and carry no agent attribution. Rust commands run as `nix develop --command scripts/with-nix-host-bridge.sh cargo …`; frontend commands as `nix develop --command bash -c '…'`.

**Shipping note**: Everything lands in one PR. US1 registers all five categories; the views of "Darstellung" (US4) and "Föderation" (US5) are added in their phases, so those two locations render an empty content area until then. Do not merge before all phases are done.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US6)

---

## Phase 1: Setup

- [x] T001 Prepare `.worktrees/023-settings-app`: rerun the real `pnpm install` (main moved to the haex-ui pin `db48f9a`); reflink the Rust build cache from the primary checkout if its `src-tauri/Cargo.lock` matches (`cp -a --reflink=always ../../src-tauri/target src-tauri/target`); record baseline counts of `pnpm check:wm-state`, `check:wm-navigation`, `check:chat-state`, `check:vault-lifecycle`, `check:templates` and `cargo test` in this task's note
  - Done 2026-09-26: pnpm install done; Rust target already present in the worktree (Cargo.lock matches main). Baseline: check:wm-state 64, check:wm-navigation 77, check:chat-state 50, check:vault-lifecycle 12, check:templates 56 templates, cargo test 573 — all green.
- [x] T002 [P] Add the 023 row to `plans/README.md` after the 022 row: priority P1, effort M, status "Spezifiziert 2026-09-26, Plan 2026-09-26", gate "setzt Spec 015, 020, 022 voraus"
  - Done 2026-09-26.
- [x] T003 [P] Add `"check:settings": "node --test scripts/check-settings.ts"` to `package.json` and a step "Check settings app" running `corepack pnpm check:settings` after "Check window manager navigation" in `.github/workflows/ci.yml`; create `scripts/check-settings.ts` with the file header only
  - Done 2026-09-26.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The pure settings registry and its routes. Every user story builds on it.

**⚠️ CRITICAL**: No user-story work starts before this phase is complete

### Tests first

- [x] T004 Write registry tests in `scripts/check-settings.ts` (fail until T007): exactly five categories in the order `general`, `appearance`, `models`, `agents`, `federation` with paths `/`, `/appearance`, `/models`, `/agents`, `/federation`; every location id and path unique; every location has a category, `titleKey`, `descriptionKey`; every `parent` exists and belongs to the same category; `overviewRows('models')` = `models.default`, `models.installed`, `models.download`, `models.speech` and `overviewRows('agents')` = `agents.providers`, `agents.autonomy`, `agents.denyRules`; `general`, `appearance`, `federation` have no overview rows; `matchRoute(settingsRoutePatterns(), path)` resolves every location from contracts §1, including `/models/download/repo/Qwen/Qwen2.5-GGUF` with `params = { owner: 'Qwen', name: 'Qwen2.5-GGUF' }`; `locationFor('/nope')` is `undefined`; every registry i18n key exists in both `src/i18n/locales/de.json` and `en.json` (FR-020)
  - Done 2026-09-26: failed first (module missing), green after T007/T009.
- [x] T005 Write `headerBack` tests in `scripts/check-settings.ts` (fail until T008), using `createHistory`/`push` from `src/lib/wm/navigation.ts`: previous entry is the parent path → `{ kind: 'back' }`, also when the previous entry carries a query (`/models/download/search?q=qwen` as parent of a repo); previous entry is some other location → `{ kind: 'push', path: parent }`; a history with only the current entry (deep link) → `{ kind: 'push', path: parent }`
  - Done 2026-09-26.
- [x] T006 Write `tabTitleFor` tests in `scripts/check-settings.ts` (fail until T011): an app with `tabTitle: 'app'` gets its own `titleKey` and no params even when the location has a routed title; an app without `tabTitle` (`system.chat`) gets the routed title with its params; `getAppDefinition('system.settings')?.tabTitle === 'app'`
  - Done 2026-09-26.

### Implementation

- [x] T007 Implement `src/lib/settings/registry.ts` per data-model.md and contracts §1: `SETTINGS_CATEGORIES`, `SETTINGS_LOCATIONS`, `settingsRoutePatterns()`, `locationFor(path)`, `categoryOf(path)`, `overviewRows(categoryId)`; pure, relative imports only (`../wm/routeMatch.ts`); make T004 pass except the i18n part
  - Done 2026-09-26: plus `locationPath` and `parentPathOf`; `SettingsRoutePattern` carries `locationId` for appRoutes.
- [x] T008 Implement `headerBack(history, parentPath)` in `src/lib/settings/registry.ts` (research R3); make T005 pass
  - Done 2026-09-26.
- [x] T009 [P] Add `settings.categories.<id>.{title,description}` for the five categories, `settings.locations.<id>.{title,description}` for every location with a parent, and `settings.back` ("Zurück zu {title}" / "Back to {title}") to `src/i18n/locales/de.json` and `en.json`; the repo location title interpolates `{owner}/{name}`; make the i18n part of T004 pass
  - Done 2026-09-26: texts reuse the established terms ("Abo verbinden", "Autonomie für Delegaten", "Verbotsregeln für Delegaten", "Sprach-zu-Text-Modell").
- [x] T010 Replace the `system.settings` entry in `src/components/wm/appRoutes.ts` with a root `/` record (component `SettingsApp`) whose children come from `settingsRoutePatterns()`, each with its async component from contracts §1 keyed by location id and its `titleKey` for tab titles and the history list (spec 020 R9)
  - Done 2026-09-26, together with US1: the async components need the view files to exist, so the wiring moved after T012–T025. `appearance` and `federation` have no view until T039/T048.
- [x] T011 Add `tabTitle?: 'location' | 'app'` to `AppDefinition` and `tabTitleFor(app, routed)` to `src/lib/wm/apps.ts`, set `tabTitle: 'app'` on `system.settings`, and use `tabTitleFor` in `tabDisplayInfo` in `src/stores/windowManager.ts`; `wm/HistoryMenu.vue` keeps `titleForLocation` (research R2); make T006 pass
  - Done 2026-09-26.

**Checkpoint**: `pnpm check:settings` green; the settings app still renders its old page at `/`.

---

## Phase 3: User Story 1 - Einstellungen nach Kategorien finden (P1) 🎯 MVP

**Goal**: Sidebar, header and content; every existing setting lives in exactly one category and saves without a button (FR-021).

**Independent Test**: quickstart S1–S3, S17.

- [x] T012 [US1] Rebuild `src/components/apps/SettingsApp.vue` as the frame (contracts §2): sidebar left, header with title and one-line description from `locationFor` (params interpolated through `t`), content `<WmRouterView />` at depth 1 as the only scrolling area; load `currentDeviceInfoAsync()` once and `provide` it with a reload function under `SETTINGS_DEVICE_KEY` (exported from a small `src/components/settings/deviceContext.ts`)
  - Done 2026-09-26: includes the header back arrow (T028) and the `@container` frame (T053).
- [x] T013 [P] [US1] Create `src/components/settings/Sidebar.vue`: one button per `SETTINGS_CATEGORIES` entry with icon and name, `aria-current="page"` on `categoryOf(route.path)`, click → `router.push(category.path)` even when that category is already active (FR-010)
  - Done 2026-09-26: the tooltip content is portalled out of the container, so it is enabled by the measured sidebar width (`useElementSize`), not a container query. Reka tooltips open on hover and focus, not on a touch long press (see T055).
- [x] T014 [P] [US1] Create `src/components/settings/OverviewView.vue`: rows from `overviewRows(categoryOf(route.path))` with icon (`text-primary`), title, one-line description and chevron; click → `router.push(row.path)` (FR-003)
  - Done 2026-09-26: plus `settings/OverviewRow.vue`, shared with the search row in DownloadModels.
- [x] T015 [P] [US1] Create `src/components/settings/GeneralView.vue` with `SettingsAliasSetting` (device from `SETTINGS_DEVICE_KEY`, reload on save) and `SettingsSessionRestoreSetting`
  - Done 2026-09-26.
- [x] T016 [P] [US1] Convert `src/components/settings/AliasSetting.vue` (research R5): save on blur and on Enter through `settings.device.setAlias`; an empty name is not saved and the field explains why; drop the save button and its i18n text
  - Done 2026-09-26.
- [x] T017 [P] [US1] Convert `src/components/settings/DefaultModelSetting.vue`: two selects "Dieses Gerät" (Wie alle Geräte / models) and "Alle Geräte" (Keins / models), saved on selection via `settings.models.setDefault`; "Wie alle Geräte" and "Keins" call `settings.models.clearDefault` (contracts §3); device uuid from `SETTINGS_DEVICE_KEY`; drop the scope radios and both buttons
  - Done 2026-09-26: an unknown stored model id stays visible as "(nicht mehr vorhanden)".
- [x] T018 [P] [US1] Convert `src/components/settings/SttModelSetting.vue`: the select lists only installed models and saves on selection via `settings.models.setStt`; models that are not installed are listed below with a button "Herunterladen und verwenden" (same action, with visible progress); drop the switch button (clarification 2026-09-26)
  - Done 2026-09-26: the select shows the default as "nicht installiert" when it is not installed yet.
- [x] T019 [P] [US1] Convert `src/components/settings/AutonomyModeSetting.vue`: the options save on selection via `settings.autonomy.setMode`; drop the save button
  - Done 2026-09-26.
- [x] T020 [P] [US1] Convert `src/components/settings/DelegateDenyRulesSetting.vue`: save on blur via `settings.delegate.setDenyRules`; invalid rules are not saved and the field explains why; drop the save button
  - Done 2026-09-26: correction — the deny rules are checkboxes, not a text field; each checkbox saves on change and a failure restores the stored selection. There is no invalid input.
- [x] T021 [US1] Add `downloads: Record<string, DownloadProgressEvent>` and an idempotent `watchDownloads()` to `src/stores/models.ts` (research R6), with a `ponytail:` comment that the subscription lives until the process ends (one vault per process, spec 013); call it in `src/pages/workspace/[instance].vue` after the session restore
  - Done 2026-09-26: deviation — a separate store `src/stores/modelDownloads.ts` (`useModelDownloadsStore`) instead of `stores/models.ts`, which would have exceeded 500 lines; shared `lib/models/format.ts` (`humanBytes`, `progressPercent`) and `models/DownloadBar.vue`/`DownloadStatus.vue`.
- [x] T022 [US1] Create `src/components/settings/InstalledModels.vue` from the installed-models tab of `src/components/models/HuggingFaceModelManagement.vue`: list, load, delete, check and install updates, integrity dialog; reads `useModelsStore()`
  - Done 2026-09-26.
- [x] T023 [P] [US1] Create `src/components/settings/DownloadModels.vue` from the catalog tab: recommended models with progress from `models.downloads`, and a row "Auf HuggingFace suchen" → `router.push('/models/download/search')`
  - Done 2026-09-26: plus a "Laufende Downloads" section for downloads that are no catalog row (HuggingFace files), so they stay visible after leaving the repository view.
- [x] T024 [P] [US1] Rework `src/components/models/HuggingFaceSearch.vue` as the location `/models/download/search`: search term in `route.query.q` via `router.setQuery({ q })` (replace), a result → `router.push('/models/download/repo/<owner>/<name>')` instead of emitting `select`
  - Done 2026-09-26: filters also live in the query (`quant`, `size`, `fit`); a result passes the matching files as `?files=`, keeping the filter behavior from 8b4d9d5; the last result list is cached in `useHuggingFace` (ponytail comment) so back does not refetch.
- [x] T025 [P] [US1] Rework `src/components/models/HuggingFaceFilePicker.vue` as the location `/models/download/repo/:owner/:name`: repo from `route.params`, progress from `models.downloads`; remove its own download listeners
  - Done 2026-09-26: after installing it opens `/models/installed`, like the old tab switch.
- [x] T026 [US1] Delete `src/components/models/HuggingFaceModelManagement.vue` (its size exception goes with it) and the unused i18n keys (`settings.header.forDevice`, the save texts of T016–T020, the old tab labels); `pnpm typecheck`, `lint`, `check:templates` green
  - Done 2026-09-26: typecheck, lint, check:templates green.
- [x] T027 [US1] Operator: run quickstart S1–S3 and S17
  - Done 2026-09-27: operator confirmed S1–S3 and S17.

**Checkpoint**: every existing setting is reachable by category and saves without a button.

---

## Phase 4: User Story 2 - Unteransichten mit Übersicht und Zurück (P1)

**Goal**: The header back arrow and robust sub-views.

**Independent Test**: quickstart S4–S7, S9.

- [x] T028 [US2] Add the back arrow to the header in `src/components/apps/SettingsApp.vue` for locations with a `parent`: label `settings.back` with the parent's title; click runs `headerBack(wm.historyOf(tabId), parentPath)` → `router.back()` or `router.push(path)` (FR-009)
  - Done 2026-09-26 with T012.
- [x] T029 [P] [US2] Show a hint with the way back to the overview instead of an error when a sub-view's data is gone: unknown repo in `HuggingFaceFilePicker.vue`, no provider connected in `ConnectDelegateProvider.vue`, deleted model in `InstalledModels.vue` (edge case)
  - Done 2026-09-26: the hint is a row of the boxed list that leads to the place with the data — a missing repo or one without GGUF files to the HuggingFace search, no installed model to "Modelle herunterladen", no model at all in "Standard-Modell" to downloads and "Abo verbinden". "Abo verbinden" needs no hint: a disconnected provider is its normal state with the connect button.
- [x] T030 [US2] Operator: run quickstart S4–S7 and S9
  - 2026-09-27: S4–S7 confirmed by the operator; S9 (deep link to a repo) still open — `location.href` with the `<vault>` placeholder failed, retry with `location.search = '?open=…'`.
  - Done 2026-09-27: the dev webview's reload fails in Vite, so S9 runs as the e2e scenario `settings-deep-links` against the built app (T070).

---

## Phase 5: User Story 3 - Direkt an eine Stelle springen (P2)

**Goal**: Callers open the settings where the user needs them.

**Independent Test**: quickstart S8, S10.

- [x] T031 [US3] Audit the callers of `openApp({ appId: 'system.settings' })` and `wm.app.open` for the settings (`src/components/apps/ChatApp.vue`, chat banners and dialogs): contextual hints (for example "no model installed") pass `at` with the matching location (`/models/download`, `/models/default`, `/agents/providers`); generic settings buttons stay at `/`
  - Done 2026-09-26: only two callers exist, the generic settings buttons in the chat header and thread sidebar; they stay at `/`. The chat's hints that mention the settings (`chat.empty.noModelsDescription`, `chat.model.delegateNotConnected`, `chat.effort.unknown`) are plain text or disabled select entries, so there is no contextual caller to point elsewhere yet.
- [x] T032 [US3] Operator: run quickstart S8 and S10
  - 2026-09-27: open — retry the deep links with `location.search = '?open=system.settings&at=…'`. Changing settings through a chat agent is not part of this spec: the actions are agent-callable, but haex-vault as an MCP server is spec 021.
  - 2026-09-27: `location.search` only reloaded the dev app; the log shows Vite failing to load its modules after the reload ("Importing a module script failed"), not the deep link. The workspace page now also consumes `?open=` that arrives while it is mounted (T071).
  - Done 2026-09-27: S8 (on load and with the settings open) and S10 run as `settings-deep-links` (T070).

---

## Phase 6: User Story 4 - Farbschema wählen (P2)

**Goal**: Light, dark or system, applied at once and readable everywhere.

**Independent Test**: quickstart S11, S12.

### Tests first

- [x] T033 [P] [US4] Write color-scheme tests in `scripts/check-settings.ts` (fail until T035): `parseColorScheme` accepts `light`/`dark`/`system` and maps anything else to `null`; `isDark('system', true)` true, `isDark('system', false)` false, `isDark('light', true)` false
  - Done 2026-09-26, plus a test that `toColorSchemeResult` leaves unset values out.
- [x] T034 [P] [US4] Add a palette-color denylist to `scripts/check-vue-templates.ts` (research R9): `(text|bg|border|ring|divide|fill|stroke|outline|from|to|via)-(white|black|neutral|gray|slate|zinc|stone|red|green|amber|yellow|blue|emerald|orange|sky|indigo|rose)` with optional shade and opacity in any `.vue` under `src/`; it fails now with about 170 hits; commit it together with T040 so no commit is red in CI
  - Done 2026-09-26 in the same commit as T040; a planted `text-neutral-500` fails the check.

### Implementation

- [x] T035 [US4] Implement `src/lib/settings/colorScheme.ts` (`parseColorScheme`, `isDark` and the `settings.get` projection); make T033 pass
  - Done 2026-09-26, plus `COLOR_SCHEME_KEY`, `colorSchemeState` and `toColorSchemeResult`.
- [x] T036 [US4] Implement `src/composables/useColorScheme.ts` per contracts §4: module-level state, vault-only `loadAsync()`/`setAsync(scheme)`, applying the `dark` class on `document.documentElement` and following `matchMedia('(prefers-color-scheme: dark)')` while `system` applies
  - Done 2026-09-26 with a deviation from contracts §4 (updated there): `loadAsync()` takes no argument and reads the device id itself, like `setAsync`, and `startSystem()` applies the system scheme before unlock.
- [x] T037 [P] [US4] Create `src/plugins/colorScheme.client.ts` that applies `system` at start (before unlock, FR-014); call `useColorScheme().loadAsync()` in `src/pages/workspace/[instance].vue` after opening, leaving `system` on a read error
  - Done 2026-09-26.
- [x] T038 [US4] Add `settings.appearance.setColorScheme` to `src/lib/actions/settingsActions.ts` (agent-callable, result without `null` per contracts §3), handler in `src/stores/settingsActionHandlers.ts` calling `useColorScheme().setAsync`, and `colorScheme` in `settings.get`; i18n `actions.settings.appearance.*`
  - Done 2026-09-26; `settings.get` returns `colorScheme` from the loaded state.
- [x] T039 [US4] Create `src/components/settings/ColorSchemeSetting.vue`: select Hell/Dunkel/System for the current Vault and save on selection through `settings.appearance.setColorScheme`; i18n `settings.colorScheme.*`
  - Done 2026-09-26 in the boxed-list style (group "Farbschema", rows "Dieses Gerät" and "Alle Geräte dieser Vault" reusing `settings.default.*Label`); wired as the view of `appearance` in `appRoutes.ts`; the search finds it as "Farbschema".
- [x] T040 [US4] In its own commit (`refactor(ui): use theme colors instead of palette colors`), replace the palette colors in the ~28 Vue files per the mapping in research R9; make T034 pass; `check:templates`, `typecheck`, `lint`, `format:check` green
  - Done 2026-09-26 (`refactor(ui): use theme colors instead of palette colors`): only about 40 palette colors in 16 files were left, the settings views had lost theirs in the COSMIC restyle. Extra mappings beyond research R9: `border-blue-500/20` → `border-primary/20`, `hover:border-blue-500` → `hover:border-primary`, `bg-blue-100/70` → `bg-primary/10`, `hover:bg-black/10 dark:hover:bg-white/10` → `hover:bg-foreground/10`.
- [x] T041 [US4] Operator: run quickstart S11 and S12, looking at chat, settings, onboarding and dialogs in dark mode
  - Done 2026-09-27: operator confirmed S11 and S12, dark mode readable after T064, T068, T069.

---

## Phase 7: User Story 5 - Föderation in den Einstellungen (P2)

**Goal**: The federation app is gone; the category lists the vault's devices.

**Independent Test**: quickstart S13, S13a, S14.

### Tests first

- [x] T042 [P] [US5] Create `src-tauri/src/storage/known_devices_tests.rs` (declared in `src-tauri/src/storage/mod.rs` like `maintenance_tests`; real vault like `maintenance_tests.rs`): `list_devices` returns this installation's row, skips the `VAULT_SCOPE_UUID` row, returns a second inserted device with `alias = NULL` as `None`
- [x] T043 [P] [US5] Create `src-tauri/src/device/commands_tests.rs` (declared via `#[cfg(test)] #[path]` in `src-tauri/src/device/mod.rs`): the pure ordering helper puts the current device first, then the others by name case-insensitively, unnamed devices last, and marks exactly one `is_current`
- [x] T044 [P] [US5] Add `resolveAppAlias` tests to `scripts/check-settings.ts`: `system.federation` → `{ appId: 'system.settings', at: '/federation' }`; `system.chat` → `{ appId: 'system.chat', at: null }`; `system.federation` is not in `WM_APPS`

### Implementation

- [x] T045 [US5] Implement `list_devices(conn)` in `src-tauri/src/storage/known_devices.rs` (research R12); make T042 pass
- [x] T046 [US5] Implement the command `list_vault_devices` and `VaultDevicePayload { vault_device_uuid, alias, is_current }` (serde camelCase) with the ordering helper in `src-tauri/src/device/commands.rs`, register it in `src-tauri/src/lib.rs`; make T043 pass; `cargo fmt --check`, `lint:rust`, `cargo test` green
- [x] T047 [US5] Add `VaultDevice` and `listVaultDevicesAsync()` to `src/composables/useDevice.ts`; add the read action `settings.devices.list` (scope `settings.read`, effect `read`) to `settingsActions.ts` with a handler that omits a `null` alias; i18n `actions.settings.devices.list`
- [x] T048 [US5] Create `src/components/settings/FederationView.vue`: one row per device with name or "Unbenanntes Gerät" and the mark "Dieses Gerät", loaded when the category opens; i18n `settings.federation.*`
  - Done 2026-09-27 with T042–T047: `KnownDevice` and `list_devices` in `known_devices.rs`, `VaultDevicePayload`, `order_vault_devices` and `list_vault_devices` in `device/commands.rs` (tests in `commands_tests.rs` via `#[path]` next to the command, like other modules); five new Rust tests. The action `settings.devices.list` returns `{ devices }`. E2E scenario `settings-federation` (T074).
- [x] T049 [US5] Add `LEGACY_APP_ALIASES` and `resolveAppAlias` to `src/lib/wm/apps.ts` and remove `system.federation` from `WM_APPS`; resolve the alias before `knownApp` in `wm.app.open` and `wm.tab.new` in `src/stores/wmActionHandlers.ts` (an input `at` wins over the alias `at`); make T044 pass
- [x] T050 [US5] Remove the federation entry from `src/components/wm/appRoutes.ts`, delete `src/components/apps/FederationApp.vue`, its two tests in `scripts/check-vault-lifecycle.ts` and the i18n key `wm.apps.federation`; point `src/pages/federation/[instance].vue` at `?open=system.settings&at=/federation`
- [x] T051 [P] [US5] Add a note to `specs/015-workspace-shell/spec.md` next to FR-003/FR-004: the federation app is replaced by the settings category "Föderation" (spec 023)
  - Done 2026-09-26 ahead of T042–T048 on operator request (the launcher still listed the federation app); the category stays empty until T048.
- [x] T052 [US5] Operator: run quickstart S13, S13a and S14
  - 2026-09-27: S13 confirmed (no federation app in the launcher); S13a waits for T042–T048, S14 is optional.
  - 2026-09-27: S13a without a second installation runs as `settings-federation`; the part with a copied vault on a second installation and S14 stay open for the operator.
  - Done 2026-09-27 (operator decision): the copied-vault part of S13a is dropped — haex-vault has no federation yet, and a copy only shows the `known_devices` rows the Rust tests already cover; a real multi-device check comes with the sync spec. S14 is dropped as a manual check: it needs a build from before this spec, and `check-wm-geometry.ts` covers dropping a tab whose app no longer exists.

---

## Phase 8: User Story 6 - Schmale Fenster (P2)

**Goal**: The sidebar shrinks to icons in narrow windows.

**Independent Test**: quickstart S15.

- [x] T053 [US6] Make the settings frame a container (`@container` on the root of `SettingsApp.vue`); in `Sidebar.vue` show icon and name from `@2xl` (42rem) up and only icons with the name as `UiButton` tooltip below, widths 16rem and 3.5rem (research R4); check that no view scrolls horizontally at 360 px
  - Done 2026-09-26 with T012/T013; the 360 px check is part of T054.
- [x] T053a [US6] Replace the icon rail with the GNOME-style sidebar (research R4, revised): hidden below `@2xl` and opened over the whole frame from a header icon, hideable beside the content from `@2xl` up, states `wideHidden`/`menuOpen` in `SettingsApp.vue`, not kept
  - Done 2026-09-26 on operator feedback.
- [x] T053c [US6] COSMIC/GNOME layout (FR-002): toolbar row with only the sidebar button and the search in `src/components/settings/Toolbar.vue`, large title below; boxed lists `src/components/settings/Group.vue`, `Row.vue`, `OptionRow.vue` in every settings view, overview rows as cards; `OverviewRow.vue` removed
  - Done 2026-09-26 on operator feedback.
- [x] T053b [US6] Settings search (FR-023, research R13): `keywordsKey`/`settingKeys` in `src/lib/settings/registry.ts`, `src/lib/settings/search.ts`, search field and hits in `Sidebar.vue`, keywords de/en, tests in `scripts/check-settings.ts`
  - Done 2026-09-26 on operator feedback.
- [x] T054 [US6] Operator: run quickstart S15 and S19
  - Done 2026-09-27: operator confirmed S15 and S19.

---

## Phase 8a: Einstellungen pro Vault (FR-024, research R14)

**Goal**: Settings apply to the vault on every device; only the device name and the default and speech models stay per device. Controls from haex-ui, readable in both schemes.

**Independent Test**: quickstart S20, S21.

- [x] T060 Rust: `fold_scoped_preferences` in `src-tauri/src/storage/maintenance.rs` (called from `run_after_open`) with tests in `maintenance_tests.rs`: device value → vault for `appearance.color_scheme`, `wm.session_restore`, `chat.autonomy_mode`, `cli_delegate.deny_rules`, `chat.reasoning_option.*` when the vault has none, then the device value is deleted; the vault value of `chat.default_model_id` → this device when it has none, then deleted; idempotent
  - Done 2026-09-26; `chat.permission_mode` joined the vault keys (found in `approval_bridge.rs`/`tool_round.rs`). Four new tests.
- [x] T061 Rust: session restore vault-only in `src-tauri/src/storage/wm_session_commands.rs` (`{ enabled }`), tests in `wm_session_commands_tests.rs`; `autonomy::get_deny_rules` reads the vault value; `cargo fmt --check`, `lint:rust`, `cargo test` green
  - Done 2026-09-26; `ScopedBool`/`get_scoped_bool` removed, `get_deny_rules` lost its device parameter, `chat.permission_mode` read from the vault. 571 Rust tests, fmt and clippy (both feature sets) green; one unrelated presence test is flaky and passed on rerun.
- [x] T062 Frontend: `useWmSession`, `lib/wm/sessionSync.ts`, the window manager store and their checks follow `{ enabled }`; `SessionRestoreSetting.vue` becomes one switch row
  - Done 2026-09-26; `SessionRestoreSetting.vue` is one `ShadcnSwitch` row with a one-line description.
- [x] T063 Frontend: actions without `scope` (contracts §3), handlers, `settings.get`; `useColorScheme` vault-only with `color-scheme` on `<html>`; autonomy (`AutonomyModeSetting.vue`, `useChatPermissionMode.ts`), deny rules and reasoning preference (`useReasoningPreference.ts`) read and write the vault value; default model device-only (`DefaultModelSetting.vue` one select)
  - Done 2026-09-26; `settings.sessionRestore.clear` and `settings.appearance.clearColorScheme` removed; `useChatPermissionMode` reads permission and autonomy mode from the vault.
- [x] T064 Frontend: controls from haex-ui in every settings view (`ShadcnSelect`, `ShadcnInput`, `ShadcnSwitch`, `ShadcnCheckbox`); native radio buttons keep the scheme through `color-scheme`
  - Done 2026-09-26 with `src/components/settings/Select.vue` (`SettingsSelect`; reka items cannot carry an empty value, so "Keins" travels as a sentinel); also the HuggingFace filters, the code input of "Abo verbinden" and the too-big confirmation in the file picker.
- [x] T065 Notes in specs 002, 009, 011/012 and 022 next to the scope requirements: settings apply to the vault since spec 023 (FR-024)
  - Done 2026-09-26: notes in specs 002 (FR-012), 009 (FR-014), 012 (effort preferences) and 022 (FR-002); 011 has no scope statement of its own.
- [x] T067 Header back returns to the previous station in the same category, otherwise to the parent (FR-009, research R3 revised): `headerBack(history)` in `src/lib/settings/registry.ts`, label names the target, tests in `scripts/check-settings.ts`
  - Done 2026-09-26 on operator feedback (Modelle → Installierte Modelle → Modelle herunterladen came back to Modelle).
- [x] T068 haex-ui: readable unchecked switch in dark mode (upstream in haex-space/haextension), then bump the pin in `nuxt.config.ts`
  - 2026-09-26: haex-space/haextension#61 open (unchecked thumb `bg-foreground`, track `bg-input/80` in dark mode, as in shadcn-vue); the pin bump follows the merge.
  - Done 2026-09-26: merged as `e380e18`, pin in `nuxt.config.ts` bumped.
- [x] T069 haex-ui: visible unchecked switch track (the knob was readable after T068, the track still vanished on the muted card): haex-space/haextension#62 with `bg-muted-foreground/35`, then bump the pin
  - Done 2026-09-26: merged as `b8549be`, pin bumped.
- [x] T066 Operator: run quickstart S20, S21 and S22
  - Done 2026-09-27: operator confirmed S20, S21 (earlier values kept) and S22.

---

## Phase 8b: End-to-End-Szenarien (SC-007)

**Goal**: The quickstart scenarios the operator checked by hand run against the built app (spec 016 rig).

**Independent Test**: `pnpm test:e2e --grep settings-`.

- [x] T070 E2E scenarios `scripts/e2e/scenarios/settings-{categories,header-back,deep-links,color-scheme,narrow-window,search,save-on-selection}.test.ts` with helpers in `scripts/e2e/lib/settings.ts`; hooks `settings-title`/`data-location`, `data-location` on search hits, `data-value` on `SettingsSelect` entries, `settings-autonomy-*`, `settings-deny-*`, `data-app-id` on launcher tiles (contracts "Test-Hooks")
  - Done 2026-09-27: all seven pass, the full suite 14 passed, 1 skipped (`relaunch-after-lock`, needs a relaunching build). The dark off switch's track measured 1.88:1 against its card at first; T073 raised it to 3:1.
- [x] T071 Deep link while the workspace is mounted (FR-011): `pages/workspace/[instance].vue` watches `route.query.open` once the session is restored
  - Done 2026-09-27; covered by `settings-deep-links`.
- [x] T073 haex-ui: the unchecked switch track reaches 3:1 against its card (WCAG 1.4.11; it measured 1.88:1 dark, about 1.5:1 light): haex-space/haextension#63 with `muted-foreground/85` light and `/65` dark, then bump the pin and raise `settings-color-scheme` to ≥ 3 in both schemes
  - 2026-09-27: PR open; a build against a local copy of the layer measured 3.33:1 dark and 3.30:1 light, the scenario passed with the raised threshold.
  - Done 2026-09-27: merged as `0407e10`, pin bumped; `settings-color-scheme` requires ≥ 3 in both schemes and measures 3.33:1 dark, 3.30:1 light against the pinned layer.
- [x] T074 E2E scenario `scripts/e2e/scenarios/settings-federation.test.ts` for S13a without a second installation (FR-022): this device listed once, first, marked, with the name set in "Allgemein"; `settings.devices.list` returns the same; hooks `settings-device` (`data-current`) and `settings-device-current`
  - Done 2026-09-27.
- [x] T072 Icons named only in TypeScript are bundled (`icon.clientBundle.scan.globInclude` with `.ts` in `nuxt.config.ts`): the e2e run showed the sidebar without category icons, the app fetched them from the iconify API
  - Done 2026-09-27; `settings-categories` checks that every category has its icon without network.

---

## Phase 9: Polish & Cross-Cutting

- [x] T055 [P] Add the terms "Einstellungskategorie", "Farbschema" and "Geräte der Vault" to `CONTEXT.md`
  - Done 2026-09-27; also corrected two stale statements there (Föderation as an App, session restore "device value over vault value").
- [x] T056 Run the automated part of `quickstart.md` in full: `check:settings`, `check:templates`, `check:wm-state`, `check:wm-navigation`, `check:chat-state`, `check:vault-lifecycle`, `typecheck`, `typecheck:scripts`, `lint`, `format:check`, `cargo fmt --check`, `lint:rust` (both feature sets), `cargo test`, `test:e2e`; record the counts here
  - Done 2026-09-27: `check:settings` 15, `check:templates` ok, `check:wm-state` 62, `check:wm-navigation` 77, `check:chat-state` 50, `check:vault-lifecycle` 10, `check:vault-passphrase-lifetime` 7, `check:e2e-lib` 161, `typecheck`, `typecheck:scripts`, `lint`, `format:check` ok; `cargo fmt --check` and `lint:rust` (both feature sets) ok; `cargo test` 576 passed, 8 ignored; `test:e2e` 15 passed, 1 skipped (`relaunch-after-lock` needs a relaunching build).
  - The first e2e run showed `settings-save-on-selection` clearing the name field before the view had filled in the stored name; both name-changing scenarios now wait for it (`clearAlias` in `scripts/e2e/lib/settings.ts`), three repeats green. Running `cargo test` in the e2e target overwrites the app binary with one without the embedded UI; build the app again before `test:e2e`.
- [x] T057 Operator: run quickstart S16 and S18
  - Done 2026-09-27: the operator confirmed that a download keeps running across category changes (S16) and that switching categories is smooth (S18).
- [x] T058 Operator: rerun the manual scenarios of the specs whose settings moved (SC-002): 002 default model and device name, 005 HuggingFace search and download, 009 autonomy mode, 010 speech model, 022 session restore S3–S7
  - Done 2026-09-27: the operator reported the tests done and working.
- [x] T059 After merge: refresh the graphify graph on `main` and rerun the queries from research.md (flagged there because the worktree snapshot predates the window manager work)
  - Done 2026-09-27: graph rebuilt on `main` (`d81bf86`); results in research.md, "Graphify-Konsultation".

---

## Dependencies & Execution Order

- Setup (T001–T003) → Foundational (T004–T011) → user stories.
- US1 (T012–T027) is the MVP and comes first; US2–US6 build on its frame (`SettingsApp.vue`, `Sidebar.vue`) and can then run in any order.
- Within US1: T021 before T023 and T025 (they read `models.downloads`); T022–T025 before T026.
- US4: T033 and T034 first; T035 → T036 → T037/T038 → T039; T040 last (it touches files of other stories, so run it after they are done or rebase it).
- US5: tests T042–T044 first; T045 → T046 → T047 → T048; T049 before T050.
- Polish after all stories.

## Parallel Opportunities

- T002, T003 in Setup.
- T009 next to T007/T008.
- In US1: T013–T020 touch different files; T023–T025 after T021.
- US4 T033/T034, US5 T042–T044 are independent test files.
- US2, US3, US5 and US6 can proceed in parallel once US1 is done; US4's T040 should come last.

## Implementation Strategy

1. Setup and Foundational: registry and tests green.
2. US1: the frame and every existing setting in its category, saved without buttons. Stop and let the operator run S1–S3, S17.
3. US2 and US3: header back, sub-view hints, context links.
4. US4: color scheme, then the theme-color migration as its own commit.
5. US5: device list and federation removal.
6. US6: narrow windows.
7. Polish: full checks, e2e, remaining manual scenarios.
