# Tasks: Allgemein mit Grundeinstellung und Erscheinungsbild

**Input**: Design documents from `/specs/042-settings-general-restructure/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/settings-contract.md, quickstart.md

**Tests**: Per ponytail rule (non-trivial logic leaves one runnable check) and plan R9: registry/search
checks in `scripts/check-settings.ts`, a Rust integration test for the rekey, `validate_new` unit tests,
and updated E2E scenarios. No new test framework.

## Phase 1: Setup

**Purpose**: Settle the one framework question before code depends on it.

- [x] T001 Probe `@nuxtjs/i18n` 10.6.0 language detection: set `defaultLocale: 'en'` and `detectBrowserLanguage: { useCookie: false, fallbackLocale: 'en' }` in `nuxt.config.ts`, start `pnpm dev`, load `/` in Chromium with `--lang=de-DE` and with `--lang=fr-FR` (Playwright MCP `browser_navigate` + `browser_evaluate('document.documentElement.lang')` or the visible `landing.welcome` text), confirm German / English and that no cookie or localStorage entry is written; record the result in `research.md` R2. If it fails, revert the `detectBrowserLanguage` change and instead add `systemLocale(languages)` to `src/lib/settings/language.ts` (`de*` → `de`, else `en`) plus `src/plugins/systemLocale.client.ts` calling `useNuxtApp().$i18n.setLocale(systemLocale(navigator.languages))`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The new settings structure every story hangs its controls into.

- [x] T002 In `src/lib/settings/registry.ts`: remove `'appearance'` from `SettingsCategoryId`, its `category(...)` entry and its `categoryLocation(...)`; give `subView()` an optional `settingKeys` option passed through to the location; drop the `settingKeys` of `categoryLocation('general')`; add `subView('general.basic', 'general/basic', 'general', { icon: 'lucide:settings-2', overviewRow: true, settingKeys: ['settings.alias.label', 'settings.sessionRestore.title'] })` and `subView('general.appearance', 'general/appearance', 'general', { icon: 'lucide:palette', overviewRow: true, settingKeys: [the seven appearance keys from the removed category] })` directly after `categoryLocation('general')`
- [x] T003 In `src/components/wm/appRoutes.ts` `SETTINGS_VIEWS`: map `general` to `OverviewView.vue`, `general.basic` to `GeneralView.vue` (renamed in T004), `general.appearance` to `AppearanceView.vue`; remove the `appearance` key
- [x] T004 Rename `src/components/settings/GeneralView.vue` to `src/components/settings/BasicView.vue` (update its doc comment to "Grundeinstellung", spec 042) and the import in `src/components/wm/appRoutes.ts`; grep for `SettingsGeneralView` and fix any user
- [x] T005 [P] In `src/i18n/locales/de.json` and `src/i18n/locales/en.json`: delete `settings.categories.appearance`; add `settings.locations.general.basic.{title,description,keywords}` ("Grundeinstellung" / "Basic settings", "Sprache, Passwort, Gerät" / "Language, password, device") and `settings.locations.general.appearance.{title,description,keywords}` ("Erscheinungsbild" / "Appearance", "Farbschema, Hintergrund, Farben" / "Color scheme, background, colors"); move the former appearance keywords to `general.appearance.keywords`
- [x] T006 Update `scripts/check-settings.ts`: `LOCATION_PATHS` (drop `appearance`, add `general.basic` → `/general/basic`, `general.appearance` → `/general/appearance`), the categories test (six categories, no appearance), add an overview-rows test for `general` (exactly `general.basic`, `general.appearance`), and change the search assertion `first('dunkel')?.path` to `/general/appearance`; run `pnpm check:settings` until green

**Checkpoint**: Settings open with "Allgemein" as an overview of two cards; every existing control is reachable.

---

## Phase 3: User Story 1 - Allgemein in zwei Unterpunkte gegliedert (Priority: P1) 🎯 MVP

**Goal**: Grundeinstellung and Erscheinungsbild show the right controls; search and E2E follow the new paths.

**Independent Test**: Spec US1 acceptance scenarios 1–5 (quickstart step 1).

- [x] T007 [US1] In `src/components/settings/AppearanceView.vue`: move `<SettingsColorSchemeSetting />` to the top of the view above the file buttons, and update the doc comment to "Erscheinungsbild" (spec 042); in `src/components/settings/ColorSchemeSetting.vue` change the doc comment's category name to "Erscheinungsbild"
- [x] T008 [P] [US1] Update E2E scenarios to the new paths and test ids: `scripts/e2e/scenarios/settings-categories.test.ts` (`CATEGORIES` without `appearance`; General is an overview — open `settings-row-general.basic` before expecting `settings-alias` and `session-restore-switch`), `settings-color-scheme.test.ts` and `appearance-basic.test.ts` (replace `settings-category-appearance` with `settings-category-general` + `settings-row-general.appearance`), `settings-narrow-window.test.ts` (category loop), `settings-search.test.ts` (`['gerätename', 'general']` → expected location `general.basic`), `settings-deep-links.test.ts`
- [x] T009 [US1] Run `pnpm typecheck`, `pnpm lint`, `pnpm check:settings`, `pnpm build`; commit `refactor(settings): fold appearance into general`

**Checkpoint**: US1 complete and shippable alone.

---

## Phase 4: User Story 2 - Sprache wählen (Priority: P1)

**Goal**: System language before unlock, start-screen picker, synced vault language.

**Independent Test**: Spec US2 acceptance scenarios 1–6 (quickstart step 2).

- [x] T010 [P] [US2] Create `src/lib/settings/language.ts`: `export const LANGUAGE_KEY = 'general.language'`, `export type Language = 'de' | 'en'`, `export const LANGUAGES: readonly Language[] = ['de', 'en']`, `export function parseLanguage(value: string | null | undefined): Language | null` (keep `systemLocale` here if T001 needed it); add `node --test` cases for `parseLanguage` (and `systemLocale` if present) to `scripts/check-settings.ts`
- [x] T011 [US2] Create `src/composables/useLanguage.ts` modeled on `src/composables/useColorScheme.ts`: uses `useNuxtApp().$i18n` (`locale`, `setLocale`) and `usePreferences`; `loadAsync()` reads `LANGUAGE_KEY` (vault scope) — valid → `setLocale`, missing/invalid → `setPrefAsync` with the active locale (FR-009); `refreshAsync()` re-reads and applies only when valid and different; `setAsync(lang)` writes then `setLocale` and returns `lang`; expose `language` as the current locale (computed)
- [x] T012 [US2] In `src/pages/index.vue`: call `await useLanguage().loadAsync()` in `onCreated` and `onUnlocked` before navigating (catch and `console.error('[settings] reading the language failed', error)` so a read error never blocks opening); add a compact language picker at `absolute top-4 right-4` inside `<main>` with `data-testid="landing-language"`, options Deutsch / English, bound to the i18n locale, calling only `setLocale` (contracts §4)
- [x] T013 [US2] In `src/pages/workspace/[instance].vue`: create `const language = useLanguage()`, add `onVaultTablesChanged(['preferences'], language.refreshAsync)` next to the color-scheme line, and in `onMounted` start `void language.loadAsync().catch(...)` next to `colorScheme.loadAsync()`
- [x] T014 [P] [US2] Create `src/components/settings/LanguageSetting.vue` modeled on `src/components/settings/ColorSchemeSetting.vue`: `SettingsGroup` + `SettingsRow` (`settings.language.label`, `label-for="settings-language"`) + `SettingsSelect` (`data-testid="settings-language"`, labels "Deutsch"/"English" untranslated), saving through `useActionOrThrow('settings.general.setLanguage')` with saved/failed status lines
- [x] T015 [US2] In `src/lib/actions/settingsActions.ts` add `setting({ id: 'settings.general.setLanguage', description: 'Set the interface language (German or English) for the whole vault, on all its devices. It applies at once.', input: { language: enum de/en, required }, result: { language }, scope: 'settings.device', effect: 'write' })` and add `language` to the `settings.get` result description; in `src/stores/settingsActionHandlers.ts` register the handler (calls `useLanguage().setAsync`) and add `language` to the `settings.get` result
- [x] T016 [US2] Put `<SettingsLanguageSetting />` first in `src/components/settings/BasicView.vue`; add `'settings.language.label'` to the `general.basic` `settingKeys` in `src/lib/settings/registry.ts`
- [x] T017 [P] [US2] i18n in `src/i18n/locales/de.json` / `en.json`: `settings.language.{label,saved,failed}`, `landing.language` (aria-label of the picker), `actions.settings.general.setLanguage`; append "Sprache Language" to the `general.basic` keywords
- [x] T018 [US2] Add an E2E scenario `scripts/e2e/scenarios/settings-language.test.ts` following `settings-color-scheme.test.ts`: change the language in Grundeinstellung → visible text switches; reopen the vault → language persists; run `pnpm export:eval-tools` if `settings.get`'s schema changed; run typecheck/lint/check:settings/build; commit `feat(settings): vault language with system default`

**Checkpoint**: US2 works on its own on top of Phase 2.

---

## Phase 5: User Story 3 - Vaultpasswort ändern (Priority: P2)

**Goal**: Change this device's vault passphrase after confirming the current one; agents can only open the view.

**Independent Test**: Spec US3 acceptance scenarios 1–6 (quickstart step 3).

- [x] T019 [US3] In `src-tauri/src/instances/passphrase.rs` add `pub fn validate_new(&self) -> Result<()>` returning `HaexError::WeakPassphrase { reason }` below `MIN_PASSPHRASE_LEN`; move `MIN_PASSPHRASE_LEN` from `create.rs:33` into `passphrase.rs`; replace the hand-written checks in `src-tauri/src/instances/create.rs:67-71` and `src-tauri/src/instances/link_vault.rs:49-52` with `passphrase.validate_new()?`; add cases (7 chars rejected, 8 accepted) to `src-tauri/src/instances/passphrase_tests.rs`
- [x] T020 [US3] Write the failing integration test `src-tauri/tests/vault_passphrase_change.rs` using the helpers and setup pattern of `src-tauri/tests/vault_single_session.rs` (`mock_app()`, `XDG_DATA_HOME` tempdir under the shared mutex): create a vault with passphrase A and write one preference → `change_vault_passphrase_core(A, B)` → close (`state.take()`) → open with A fails with `WrongPassphrase` → open with B succeeds and the preference is there; plus: wrong current → `WrongPassphrase` and A still opens; new of 7 chars → `WeakPassphrase`; new == current → `WeakPassphrase`
- [x] T021 [US3] Create `src-tauri/src/instances/passphrase_change.rs`: `ChangePassphraseArgs { current: Passphrase, new: Passphrase }` (`#[derive(Debug, Deserialize, TS)]`, `#[ts(export, export_to = "../../src/types/bindings/")]`, `#[ts(type = "string")]` per field, camelCase); `pub async fn change_vault_passphrase_core<R: Runtime>(app, state, chat, args)` doing, in order: `chat.acquire_operation()?`, `args.new.validate_new()?`, reject `new == current` as `WeakPassphrase`, resolve the path via `state.active_name()?` + `get_instance_path`, then inside the vault gate's `spawn_blocking` with the active `VaultDb`: verify `current` on a separate `Connection::open_with_flags(path, SQLITE_OPEN_READ_ONLY)` with `pragma_update(None, "key", ..)` and `SELECT COUNT(*) FROM sqlite_master` (map `NotADatabase` like `open.rs` `is_wrong_passphrase`), drop it, then `db.with_connection`: `query_row("PRAGMA wal_checkpoint(TRUNCATE)")` asserting `busy == 0`, `journal_mode=DELETE`, `pragma_update(None, "rekey", new)`, `journal_mode=WAL`; add a `ponytail:` comment on the rekey (key rendered into SQL text that is not zeroed; upgrade path `sqlite3_rekey_v2` via `rusqlite::ffi`); never log either passphrase; `#[tauri::command] pub async fn change_vault_passphrase(...)` wrapping the core
- [x] T022 [US3] Register the module in `src-tauri/src/instances/mod.rs` and the command in `src-tauri/src/lib.rs` `generate_handler!`; run `cargo test --test vault_passphrase_change` and `cargo test instances::passphrase` until green; `cargo fmt --check`, `cargo clippy`; revert the trailing-whitespace drift in `src/types/bindings/InstanceInfo.ts` (memory: ts-rs binding drift)
- [x] T023 [US3] Add `changePassphraseAsync(current: string, next: string)` to `src/composables/useInstance.ts` invoking `change_vault_passphrase` with `{ args: { current, new: next } }`
- [x] T024 [US3] Create `src/components/settings/PasswordChangeView.vue`: three `UiInputPassword` fields (current, new, repeat; `data-testid` `password-change-current|new|repeat`), hint `settings.password.deviceOnly`, submit disabled until new ≥ 8 chars, new == repeat and new ≠ current (inline reasons), busy state; on error map `kind` `WrongPassphrase` → `settings.password.wrongCurrent`, `WeakPassphrase` → `settings.password.weak`, otherwise `useErrorString`; on success clear the fields and show `settings.password.changed`
- [x] T025 [US3] Add `subView('general.basic.password', 'general/basic/password', 'general.basic', { icon: 'lucide:key-round', row: true })` to `src/lib/settings/registry.ts`, `'general.basic.password'` → `PasswordChangeView.vue` in `src/components/wm/appRoutes.ts` `SETTINGS_VIEWS`, `LOCATION_PATHS` in `scripts/check-settings.ts`; in `src/components/settings/BasicView.vue` add after the language setting a `SettingsGroup` with `SettingsRow to="/general/basic/password"` (pattern `FederationView.vue:82-90`, `data-testid="settings-row-general.basic.password"`)
- [x] T026 [P] [US3] i18n in de/en: `settings.locations.general.basic.password.{title,description,keywords}` ("Vaultpasswort ändern" / "Change vault password"), `settings.password.{current,new,repeat,deviceOnly,mismatch,same,tooShort,wrongCurrent,weak,changed,submit}`
- [x] T027 [US3] Verify agent reachability: `change_vault_passphrase` appears nowhere in `src/lib/actions`; `wm.app.open` with `{ appId: 'system.settings', at: '/general/basic/password' }` opens the view (unit-check the path resolves via `locationFor` in `scripts/check-settings.ts`); run typecheck/lint/check:settings/build; commit `feat(settings): change the vault passphrase`

**Checkpoint**: US3 works on its own on top of Phase 2.

---

## Phase 6: User Story 4 - Workspace-Hintergrund festlegen (Priority: P3)

**Goal**: One synced background image behind all workspaces, removable.

**Independent Test**: Spec US4 acceptance scenarios 1–5 (quickstart step 4).

- [x] T028 [US4] Extract the shared core of `scaledUrl` in `src/composables/usePasswordsThumbnails.ts:27-62` into `src/lib/images/downscale.ts` as `export async function downscaleToWebp(bytes: ArrayBuffer, mime: string, maxEdge: number): Promise<Blob>` (header pre-scale via `previewableSize`, `createImageBitmap`, canvas, `toBlob('image/webp', 0.8)`, `bitmap.close()`); make `scaledUrl` `URL.createObjectURL(await downscaleToWebp(bytes, mime, THUMBNAIL_EDGE))` (operator-approved in plan review)
- [x] T029 [P] [US4] Create `src/lib/settings/background.ts`: `BACKGROUND_KEY = 'appearance.background'`, `BACKGROUND_MAX_EDGE = 2560`, `isBackgroundValue(value): value is string` (prefix `data:image/webp;base64,`); add `node --test` cases to `scripts/check-settings.ts`
- [x] T030 [US4] In `src-tauri/src/storage/preferences_commands.rs` `validate_value`: for key `appearance.background` require the prefix `data:image/webp;base64,` and length ≤ 4 MiB, else `HaexError::InvalidInput`; add cases to the module's `*_tests.rs`
- [x] T031 [US4] Create `src/composables/useWorkspaceBackground.ts` (one module-level `ref<string | null>`, process holds one vault): `loadAsync()` (reset to null, then read; keep only `isBackgroundValue`), `refreshAsync()` (read; assign only if changed), `setFromFileAsync(file: File)` (`downscaleToWebp(await file.arrayBuffer(), file.type, BACKGROUND_MAX_EDGE)` → `FileReader.readAsDataURL` → `setPrefAsync`), `removeAsync()` (`clearPrefAsync`); add a `ponytail:` comment that every `preferences` change re-reads the whole value over IPC (upgrade path: own binary table with hash reference, `passwords/binaries.rs`)
- [x] T032 [US4] In `src/pages/workspace/[instance].vue`: `const background = useWorkspaceBackground()`, `onVaultTablesChanged(['preferences'], background.refreshAsync)`, `void background.loadAsync().catch(...)` in `onMounted`; in `src/components/wm/Desktop.vue` bind the root `div`'s `style` to `background-image: url(...)`, `background-size: cover`, `background-position: center` when set (keep `bg-muted/10` otherwise)
- [x] T033 [P] [US4] Create `src/components/settings/BackgroundSetting.vue`: `SettingsGroup` + `SettingsRow` (`settings.background.label`, description `settings.background.description`) with a hidden `<input type="file" accept="image/*">` opened by a "Bild wählen …" button (`data-testid="settings-background-choose"`) and a "Entfernen" button only while set (`data-testid="settings-background-remove"`, calls `useActionOrThrow('settings.appearance.removeBackground')`); decode errors show `settings.background.failed`, the background stays unchanged; busy state while scaling
- [x] T034 [US4] Put `<SettingsBackgroundSetting />` right after the color scheme in `src/components/settings/AppearanceView.vue`; add `'settings.background.label'` to the `general.appearance` `settingKeys` in `src/lib/settings/registry.ts`
- [x] T035 [US4] In `src/lib/actions/settingsActions.ts` add `setting({ id: 'settings.appearance.removeBackground', description: 'Remove the workspace background image for the whole vault; the default background shows again.', scope: 'settings.device', effect: 'write' })`; handler in `src/stores/settingsActionHandlers.ts` calling `useWorkspaceBackground().removeAsync()`
- [x] T036 [P] [US4] i18n in de/en: `settings.background.{label,description,choose,remove,failed,saved}`, `actions.settings.appearance.removeBackground`; append "Hintergrund Hintergrundbild background wallpaper" to the `general.appearance` keywords
- [x] T037 [US4] Add a two-device sync regression check for an approximately 500-KB background preference, then run typecheck/lint/check:settings/build and `cargo test storage::preferences_commands`; commit `feat(settings): workspace background image`

**Checkpoint**: All four stories work independently.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [x] T038 Search `src/` and `scripts/` for leftovers: `'/appearance'`, `settings-category-appearance`, `categories.appearance`, `GeneralView`; fix any hit
- [x] T039 Run the full check set from `quickstart.md` that works on this machine (`pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm check:settings`, `pnpm build`, `cd src-tauri && cargo fmt --check && cargo clippy && cargo test`); note that `pnpm test:e2e` needs the Arch host
- [x] T040 Diff the result against `docs/plans/2026-10-07-settings-general-restructure-design.md` decisions 1–10 and `research.md` R8 (memory: review drift); mark every task here `[x]`

---

## Dependencies & Execution Order

- **Phase 1 (T001)** → only blocks T010/T012 (language detection outcome).
- **Phase 2 (T002–T006)** blocks every story (registry, routes, i18n structure).
- **US1 (T007–T009)**: after Phase 2.
- **US2 (T010–T018)**, **US3 (T019–T027)**, **US4 (T028–T037)**: each only after Phase 2; independent of each other. Shared files (`registry.ts`, `BasicView.vue`, `AppearanceView.vue`, `settingsActions.ts`, `settingsActionHandlers.ts`, `[instance].vue`, locale JSONs) mean they are done one after another, not in parallel.
- **Within US3**: T019 → T020 (test red) → T021 → T022 (green) → T023 → T024 → T025.
- **Phase 7** after all stories.

## Parallel Opportunities

- Phase 2: T005 (i18n) alongside T002–T004.
- US2: T010, T014, T017 alongside each other.
- US3: T026 alongside T019–T022 (frontend strings vs. Rust).
- US4: T029, T033, T036 alongside each other; T030 (Rust) alongside all frontend tasks.

## Implementation Strategy

1. Phase 1 + 2 + US1 → the restructure alone is a shippable PR state (MVP).
2. US2 (language), then US3 (password), then US4 (background), one commit each on the same branch.
3. Phase 7, then one PR `feat(settings): general settings restructure (spec 042)` against `main`, rebase-merge.
