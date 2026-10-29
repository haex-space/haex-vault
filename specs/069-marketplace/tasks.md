---
description: 'Task list for spec 069-marketplace'
---

# Tasks: Marketplace für Erweiterungen

**Input**: Design documents from `/specs/069-marketplace/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/marketplace-api.md](./contracts/marketplace-api.md),
[contracts/commands.md](./contracts/commands.md), [contracts/ui.md](./contracts/ui.md),
[quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires tests with every behaviour change.

- **Rust**: tests go into a `*_tests.rs` file next to the module, never inline (the module gets
  `#[cfg(test)] #[path = "x_tests.rs"] mod tests;`). A test marketplace served by `hyper` (dev
  dependency already in `Cargo.toml`) replaces the network; no test talks to the internet.
- **Frontend**: the logic lives in the host, so the frontend has no unit tests of its own; it is
  covered by `pnpm typecheck`, `pnpm lint`, `pnpm check:templates` and the E2E scenarios.
- **Behaviour in the app**: E2E scenarios `scripts/e2e/scenarios/marketplace-*.test.ts` against a
  local test marketplace (T030).

**Organization**: Grouped by user story in the order of spec.md: US1 (P1), US2 (P1), US3 (P2), US4 (P2),
US5 (P3). US1 and US2 are both P1 and meet in the install button: the catalog app (US1) is read-only
until the safe download (US2) exists, so the install flow tasks (T040, T041) sit in US2 and ship with
the app.

**Shipping note** (plan.md "Stufen"; each PR is reviewed and merged before the next starts on `main`):

| PR   | Tasks                | Content                                                      |
| ---- | -------------------- | ------------------------------------------------------------ |
| docs | T001–T003            | spec, plan, tasks                                            |
| A    | T004–T024            | migration, store, client, catalog and its commands (no UI)   |
| B    | T032–T039            | safe download, `BundleSource`, install dialog source         |
| C    | T025–T031, T040–T041 | catalog app and the install flow (US1 + US2 end to end)      |
| D    | T042–T046            | updates, compatibility, remove (US3)                         |
| E    | T047–T055            | managing marketplaces, preset, several sources at once (US4) |
| F    | T056–T063            | private marketplaces with credentials (US5)                  |
| G    | T064–T067            | two devices, Android, glossary, wrap-up                      |

**Rules for every task**:

- **Before writing code**:
  - Before authoring a named function, component, store or module, query graphify (`graphify query` on
    `graphify-out/`) and extend a candidate if one fits.
  - Read a file before changing it, and keep hand-written files under 500 lines.
- **Rust**:
  - Never block the async executor; no `unwrap()`/`expect()` on network, file or DB input.
  - Secrets (tokens, passwords) live in `zeroize::Zeroizing`, never in `Debug`, logs (`tracing`
    fields included), error messages, argv or frontend state.
  - New `#[ts(export)]` types carry the `Marketplace` or `Catalog` prefix, or sit in `model.rs`. After
    `cargo test`, run `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts` like the CI.
- **Builds**: Cargo builds can drive this machine out of memory. Build only the affected targets with
  `-j 4` inside `nix develop`, and ask the operator before a full `cargo build` or `cargo test` of the
  whole crate.
- **UI text**:
  - Every new text goes into `src/i18n/locales/de.json` and `en.json`. vue-i18n refuses `<…>` in
    messages, so write `{name}`. Product name "HaexVault", never "haex-vault"; no word "Layout".
  - `UiButton` in a form needs `type="button"`.
  - A missing UI building block goes into haex-ui, not a local copy. Reuse `SettingsGroup`, `Row`,
    `OptionRow`.
- **Before every push**:
  - `pnpm exec prettier --write` on touched Markdown/JSON, then `pnpm format:check`
  - `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm check:templates` and the touched
    `pnpm check:*`
  - `pnpm lint:rust` (clippy with `-D warnings`, also `--no-default-features`)
  - code with `cfg` branches: clippy it for `aarch64-linux-android` too
- **Commits**: Conventional Commits, no agent attribution in commits or PR text.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US5)

---

## Phase 1: Setup (docs PR)

- [x] T001 Commit spec, plan, research, data model, contracts, quickstart, checklist and this file in `specs/069-marketplace/` and `.specify/feature.json` (`"feature_directory": "specs/069-marketplace"`)
- [x] T002 [P] Add a row to the table in `plans/README.md`: "069: Marketplace für Erweiterungen (`specs/069-marketplace/`)", priority P2, effort L, status "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-29; Lieferungen PR A–G", gate "017 und 034 im Einsatz; öffentlicher Marketplace unter `https://marketplace.haex.space`"
- [ ] T003 [P] Settle how the E2E build points the preset at a local test marketplace (research R15). Read how `HOLZI_E2E_SYNC_LOOPBACK` is gated in `src-tauri/src/sync/endpoint.rs`, pick the same kind of gate for `HOLZI_E2E_MARKETPLACE_URL` (never active in a release build), and write the exact place and gate into research R15; T016 implements it

---

## Phase 2: Foundational (PR A, blocks all user stories)

**Purpose**: The table, the module, the types, the HTTP client and the test marketplace, with no visible
change yet. No new crate: `reqwest`, `sha2`, `semver`, `url`, `futures`, `tokio-util` and `hyper` (dev)
are in `src-tauri/Cargo.toml` already.

- [ ] T004 Check the next free migration number (`ls src-tauri/src/identity/migrations_*.rs`, `git branch -a` for other open migrations; main ends at `0046_sync_rules_decrypt`). Create `src-tauri/src/identity/migrations_marketplaces.rs` with `pub const MARKETPLACES_0047` (or the next free number) and `#[cfg(test)] pub const SYNCED_TABLES: [&str; 1] = ["haex_marketplaces"]`, modelled on `migrations_browser_clients.rs`. Columns verbatim from data-model.md: `id` TEXT PRIMARY KEY NOT NULL; `name` TEXT NOT NULL; `base_url` TEXT NOT NULL; `enabled` INTEGER NOT NULL DEFAULT 1; `sort_order` INTEGER NOT NULL; `auth_type` TEXT NOT NULL DEFAULT 'none'; `credentials_item_id` TEXT; `created_at` INTEGER NOT NULL; `updated_at` INTEGER NOT NULL. **No UNIQUE and no foreign key** (a conflict would halt the sync; the credentials entry may arrive later, research R11, R14). Header comment cites spec 069 and research R14
- [ ] T005 Register the migration in `src-tauri/src/identity/migrations.rs`: `use` the constant, `m.insert(MigrationName::from("0047_marketplaces"), MARKETPLACES_0047.to_owned())` next to the other late migrations, raise `HOLZI_TRIGGER_VERSION` by one (29 → 30) and add its doc line "30: `0047_marketplaces` (spec 069) added the CRDT-tracked `haex_marketplaces`". Declare the module in `src-tauri/src/identity/mod.rs`
- [ ] T006 [P] Tests in `src-tauri/src/identity/migrations_marketplaces_tests.rs`, modelled on `migrations_browser_clients_tests.rs`: a fresh vault has `haex_marketplaces`; the table is CRDT-tracked (every name in `SYNCED_TABLES` has triggers); `enabled` defaults to 1 and `auth_type` to `none`; two rows with the same `name` and `base_url` are accepted (no UNIQUE)
- [ ] T007 Create the module skeleton `src-tauri/src/marketplace/mod.rs` and declare `mod marketplace;` in `src-tauri/src/lib.rs`. `mod.rs` holds the constants (one place, no magic numbers elsewhere): `SOURCE_TIMEOUT` 8 s (catalog, details, test), `CONNECT_TIMEOUT` 10 s, `DOWNLOAD_IDLE_TIMEOUT` 20 s, `PAGE_LIMIT` 20, `PENDING_IDLE` 10 min, `ICON_MAX_BYTES` 256 KiB, `ICON_CACHE_ENTRIES` 200, `ICON_PARALLEL` 4, `PROGRESS_INTERVAL` 100 ms, and re-uses `crate::extensions::registry::limits::ARCHIVE_BYTES` (64 MiB) for the download limit. Also `pub struct MarketplaceState` (empty for now, filled by T023, T033, T042) and `app_version()` (`env!("CARGO_PKG_VERSION")` parsed with `semver`) and `platform()` (`linux|windows|macos|android|ios` from `cfg`, pure function)
- [ ] T008 Create `src-tauri/src/marketplace/model.rs`: the types of contracts/commands.md and data-model.md with `#[derive(Serialize, TS)]`, camelCase, `#[ts(export, export_to = "../../src/types/bindings/")]`:
  - `MarketplaceAuthType` (`none`, `bearer`, `basic`), `MarketplaceInput { id?, name, baseUrl, authType, credentialsItemId? }`, `MarketplaceView { id, name, baseUrl, enabled, rank, authType, credentials }`, `CredentialsState` (`none`, `present`, `missing`, `syncing`)
  - `SourceErrorKind` with exactly `unreachable`, `timeout`, `auth_rejected`, `credentials_missing`, `credentials_syncing`, `bad_response`, `insecure_credentials`; `SourceError { sourceId, name, kind, detail? }` (`detail` cut to 200 characters)
  - `CatalogQuery { page, search?, category?, sourceId? }`, `CatalogEntry`, `CatalogVersion { version, bundleSize, compatible, incompatibleReason? }`, `CatalogPage { entries, page, totalPages, total, sourceErrors }`, `CatalogDetail`, `MarketplaceTestResult { ok, count?, error? }`, `KnownUpdate`, `DownloadProgress { loaded, total }`, `PendingBundleRef { id }`
- [ ] T009 [P] Create `src-tauri/src/marketplace/wire.rs`: the protocol structs of contracts/marketplace-api.md for `serde` only (`Deserialize`, camelCase), tolerant: `#[serde(default)]` on every optional field, no `deny_unknown_fields`, `Option` for `iconUrl`, `averageRating`, `tags`, `publishedAt`, `publisher`, `category`, `changelog`, `minAppVersion`, `maxAppVersion`, `publicKey`. Required per entry: `extensionId`, `name`, `slug`, `shortDescription`; `versions` may be empty. Required per version for an install: `version`, `bundleHash`, `bundleSize`. Test file `wire_tests.rs`: parses a real-shaped list response (fixture with random hashes), ignores unknown fields, skips an entry without `slug` and keeps the rest
- [ ] T010 Add `HolziError::Marketplace { kind: String }` to `src-tauri/src/error.rs` (look at how `ExtensionInstall { reason }` and its kind reach the frontend and do the same), with the kinds of contracts/commands.md "Fehlerarten" as constants in `marketplace/mod.rs`. Add a test next to the existing error tests that the kind survives serialization
- [ ] T011 Create `src-tauri/src/marketplace/store.rs` over the `Query`/`CrdtTransaction` helpers (look at `browser_bridge/approvals.rs` and `remote_storage/store.rs` for the read/write pattern):
  - `list(q) -> Vec<MarketplaceRow>` ordered by `(sort_order, name, id)`; a row whose `base_url` fails validation is left out and logged, not an error (data-model.md)
  - `normalize_base_url(&str) -> Result<String>`: scheme `http` or `https`, host required, **path prefix kept**, trailing `/`, fragment and query removed
  - `insert` (new row gets `sort_order` = highest existing + 1), `update`, `set_enabled`, `reorder(ids)` (rewrites every `sort_order`), `delete`
  - rules at write time: `auth_type` other than `none` needs `credentials_item_id` **and** an `https` address (error `marketplace_insecure_credentials`)
  - `PRESET_ID = "preset-public"`, `PRESET_NAME = "Haex Marketplace"`, `PRESET_URL = "https://marketplace.haex.space"`: the **only** place in the code with this address (FR-034). `add_preset(tx)` inserts the row with the fixed id and does nothing when a row with that id exists; it is called by T017 (new vault), T047 (command) and T052 (empty state), never on open
- [ ] T012 [P] Tests in `src-tauri/src/marketplace/store_tests.rs` against a test vault: normalization table (`https://example.org/market/`, `HTTP://Example.org`, with query and fragment, without scheme, with `ftp://`, empty host, a path prefix survives); invalid rows are skipped on read; new rows append; `reorder` rewrites ranks; `delete` of any row works; `add_preset` is idempotent; a deleted preset is **not** back after `list`; bearer/basic with `http://` is refused; bearer without a credentials id is refused
- [ ] T013 Create the test marketplace in `src-tauri/src/marketplace/test_support.rs` (`#[cfg(test)]`): a `hyper` 1 server on `127.0.0.1:0` with scripted routes (`/health`, `/categories`, `/extensions`, `/extensions/{slug}`, `/extensions/{slug}/download`, a bundle path), a path prefix option, optional required `Authorization`, a recording of every request's headers, delays, redirects (same host and other host), and raw bodies for broken answers. Fixtures are shaped like the live answers of 2026-10-28 with **random** hashes and ids, never real tokens
- [ ] T014 Create `src-tauri/src/marketplace/client.rs`:
  - one `reqwest::Client` with rustls, `CONNECT_TIMEOUT`, the per-call `SOURCE_TIMEOUT`, `User-Agent: HaexVault/<version>`, `X-Platform`, `X-App-Version`, `Accept: application/json` and **no other** identifying header (FR-007)
  - `join_url(base, path)` that appends the protocol path **behind** the path prefix of the base address
  - `get_json<T>(base, path, query, auth: Option<&AuthHeader>) -> Result<T, SourceError>`; `AuthHeader` is a `Zeroizing` wrapper without `Debug`/`Display`; it is only sent when the base is `https` (second barrier, FR-027) and is dropped on any redirect to another host or port
  - redirect policy: at most 5, never `https` to `http`
  - status mapping: `401`/`403` → `auth_rejected`; other `>= 400`, connection, DNS, TLS → `unreachable` (detail `HTTP n` or the cause); timeout → `timeout`; invalid JSON or missing required fields of the answer → `bad_response`; the `error` text of the service cut to 200 characters, never rendered as HTML
- [ ] T015 [P] Tests in `src-tauri/src/marketplace/client_tests.rs` with T013: success; the exact header set sent (nothing identifying beyond the contract); path prefix; `401` → `auth_rejected`, `404` and `500` → `unreachable`, broken JSON → `bad_response`; a delayed answer hits the timeout using `tokio::time::pause` (no real sleeping); a redirect to another host drops `Authorization`; a redirect `https` → `http` is refused; credentials are never sent over `http`; `platform()` returns the right value for each `cfg`
- [ ] T016 Implement the E2E address override decided in T003 in `src-tauri/src/marketplace/store.rs`: in an E2E build `PRESET_URL` can be replaced by `HOLZI_E2E_MARKETPLACE_URL`; in every other build the constant is used and the variable is ignored. Test: a unit test that a release-like configuration ignores the variable (gate expressed as a function taking the flag, so it is testable without rebuilding)

- [ ] T017 Seed the preset for a **new vault** (FR-022, SC-006, US4-1, research R10). Find where a vault gets its first data (graphify `genesis`; read `src-tauri/src/sync/genesis.rs` `ensure_sync_state` and `src-tauri/src/identity/bootstrap.rs`) and call `store::add_preset` exactly once at creation: not on every open, not in a migration. The bootstrap hook writes rows without an HLC that stay invisible to the sync until they are updated, so decide which of the two places gives a row that reaches the other devices, and write the decision and the reason into research R10. Tests in `src-tauri/src/marketplace/preset_seed_tests.rs`: a new vault has exactly one row `preset-public` with the values of `PRESET_*`; opening the same vault again adds nothing; after the row is deleted, reopening does **not** bring it back; two vaults created separately that then sync converge on **one** row (same id, no duplicate)

**Checkpoint**: `cargo test -j 4 -p holzi marketplace::` and `identity::migrations_marketplaces` are green; nothing is visible in the app.

---

## Phase 3: User Story 1 - Erweiterung im Katalog finden und installieren (Priority: P1)

**Goal**: The app "Erweiterungen entdecken" shows the common catalog of all enabled marketplaces, with
search, category, source filter and pages, and opens details.

**Independent Test**: With the test marketplace of T030 serving two extensions, open the app, find a
tile with its source, search, filter, page, open the details. (Installing needs US2.)

### Backend (PR A)

- [ ] T018 [US1] Create `src-tauri/src/marketplace/catalog.rs`:
  - `fetch_catalog(sources, query, installed, app_version)` asks **all enabled sources at the same time** (`futures::future::join_all`), each with its own `SOURCE_TIMEOUT`; with `sourceId` only that one
  - page N is page N of every source, appended in rank order; entries of different sources are **never merged**, even with the same `extensionId` or name (FR-012); an entry is identified by `(sourceId, slug)`
  - `totalPages` is the maximum over the sources, `total` the sum; `limit` is `PAGE_LIMIT`, `sort` is `downloads`
  - an entry missing a required field is skipped (log the source and slug), not the whole answer
  - pure helpers, each testable without network: `latest_compatible(versions, app_version)` using `semver` (not the order of the answer, research R1), `is_compatible(version, app_version)` from `minAppVersion`/`maxAppVersion` (a missing bound does not apply; research R9), `installed_state(entry_name, installed)` from `extensions::registry::list`, matching by **name** only (the details add the publisher key, research R8; the preview decides in the end)
  - `catalog` carries `updateTo` only when the latest compatible version is higher than the installed one
- [ ] T019 [P] [US1] Tests in `src-tauri/src/marketplace/catalog_tests.rs` with the test marketplace: two sources, one failing (error per source, the other's entries present); one source timing out (`tokio::time::pause`) and the **total duration is at most `SOURCE_TIMEOUT`** while the fast sources answer at once (SC-003); the **same slug and `extensionId` in two sources gives two entries**, each with its source; totals and `totalPages`; stable paging; an entry without `slug` is skipped; `versions[0]` is not the newest → semver wins; `maxAppVersion` below the app version → not compatible and not installable; empty `versions`; `sourceId` filter; a disabled source is not asked at all (the server records zero requests)
- [ ] T020 [US1] In `catalog.rs` (or `details.rs` if it grows past 500 lines) add `fetch_categories(sources, source_id?)` (union by `slug`, counts summed, per-source errors) and `fetch_details(source, slug)`: `publicKey` as fingerprint through `extensions::registry::install::publisher_fingerprint`, and `publisherMatchesInstalled` (research R8). Tests in `catalog_details_tests.rs`: union and counts, details of an installed extension of the same publisher and of another publisher with the same name
- [ ] T021 [US1] Create `src-tauri/src/marketplace/icons.rs` (research R12): `fetch_icon(source, url)` loads with **no credentials**, at most `ICON_MAX_BYTES`, only PNG, JPEG or WebP decided by the **magic bytes** (not the `Content-Type`, never SVG), returns a `data:` URL; in-memory cache of `ICON_CACHE_ENTRIES` (oldest out first); at most `ICON_PARALLEL` loads at once (a `tokio::sync::Semaphore`); error `marketplace_icon_unavailable`
- [ ] T022 [P] [US1] Tests in `src-tauri/src/marketplace/icons_tests.rs`: PNG, JPEG, WebP accepted; SVG, HTML and a `Content-Type: image/png` lie (magic bytes of text) refused; 256 KiB + 1 refused; no `Authorization` sent even for an authenticated source; the cache serves the second call without a request; the fifth parallel load waits
- [ ] T023 [US1] Create `src-tauri/src/marketplace/commands.rs` part 1: `marketplace_list`, `marketplace_catalog` (a newer call cancels a running one through an `AbortHandle` kept in `MarketplaceState`), `marketplace_categories`, `marketplace_details`, `marketplace_icon` with the signatures of contracts/commands.md. `MarketplaceState` is managed in `src-tauri/src/lib.rs` next to the other `.manage(...)` calls; add the commands to the `generate_handler!` list there (the handler is wrapped by the gate, so they need an open vault). Read sources from the store; pass installed extensions from the registry
- [ ] T024 [P] [US1] FR-030 guard in `src-tauri/src/marketplace/commands_tests.rs`: find the three registries with graphify (agent actions: `src/stores/*ActionHandlers.ts` and the list in `src-tauri/src/mcp_server/`; extension host methods: `src-tauri/src/extensions/bridge/dispatch.rs`) and write a test that fails when a name containing `marketplace` appears in any of them. Where a registry cannot be enumerated at run time, the test reads that module's source text. Failure message tells the author to decide on purpose

### Catalog app (PR C)

- [ ] T025 [US1] Register the app `system.marketplace`: graphify for every place `system.contacts` is registered and mirror each (known sites: `src/lib/wm/apps.ts` with `titleKey: 'wm.apps.marketplace'`, icon `lucide:store`, `iconUrl: '/app-icons/marketplace.svg'`, size 960 × 640, minimum 360 × 360, `multiInstance: false`, `tabTitle: 'app'`; `src/components/wm/appRoutes.ts` with an async `MarketplaceApp`; new `src/lib/marketplace/registry.ts` with the routes `/` and `/:slug`, modelled on `src/lib/contacts/registry.ts`; new `public/app-icons/marketplace.svg` in the style of the other icons; `scripts/check-extensions-apps.ts`, `scripts/lib/wm-fixtures.ts`, `scripts/check-wm-*.ts`, `src/stores/wmActionHandlers.ts`). Decide in the PR whether an agent may open the window at all (opening is harmless, FR-030 forbids querying and installing) and keep the decision in `contracts/ui.md`
- [ ] T026 [US1] Create `src/composables/useMarketplaceCatalog.ts`: state for search (300 ms debounce), category, source and page; loading through `marketplace_catalog` where only the newest answer counts; categories through `marketplace_categories`; reload on `onVaultTablesChanged` for `haex_marketplaces` and for the extension tables (look at `src/composables/useVaultData.ts` and the Contacts composables for the pattern); a filter change goes to page 1 (US1-2, US1-3)
- [ ] T027 [US1] Create `src/components/apps/MarketplaceApp.vue` and `src/components/marketplace/` (`CatalogGrid.vue`, `CatalogTile.vue`, `SourceErrors.vue`, `CatalogToolbar.vue`) per contracts/ui.md: toolbar with search, category, source and "Aktualisieren"; one-column grid below `@lg`; the tile shows icon (through `marketplace_icon`, placeholder while loading or on error), name, short description, publisher with the "geprüft" mark as display only, rating, downloads, **its source**; one error line per source with "Erneut versuchen" (FR-031); empty states "lädt", "Keine Erweiterungen", "Keine Verbindung" (all sources failed), "Alle Marketplaces sind ausgeschaltet" with a jump to the settings; pager. Text from the marketplace as plain text only (FR-006); all `data-testid` values of ui.md
- [ ] T028 [US1] Create `src/components/marketplace/CatalogDetails.vue` for the route `/:slug?source=<id>`: description as plain text, publisher, category, tags, source, the versions with change note; "Installieren", "Aktualisieren", "Entfernen" are wired in T040 and T044. Back to the catalog keeps search, category and page
- [ ] T029 [P] [US1] Add the texts under `marketplace.*` in `src/i18n/locales/de.json` and `en.json` (app title "Erweiterungen entdecken" / "Discover extensions", toolbar, tile, details, empty states, per-source error kinds `unreachable`, `timeout`, `auth_rejected`, `credentials_missing`, `credentials_syncing`, `bad_response`, `insecure_credentials`) and `wm.apps.marketplace`
- [ ] T030 [US1] Create the E2E test marketplace `scripts/e2e/lib/marketplace.ts`: a Node `http` server on a free port with an optional path prefix and an optional required bearer token (random per start), serving `/health`, `/categories`, `/extensions`, `/extensions/{slug}`, `/extensions/{slug}/download` and the bundle for two extensions, the signed fixtures `fixture('e2e', 'probe')` (version 1.0.0, and `probe-v2` for the update scenario) and `fixture('e2e', 'agent-tools')` from `scripts/e2e/lib/extensions.ts` (`src-tauri/tests/fixtures/extension_e2e/`); size and SHA-256 are computed from the file when serving; switches to make the hash wrong, missing, or the size wrong; per-request recording. Unit check for the helper in `scripts/e2e/lib/marketplace.test.ts` run by `pnpm check:e2e-lib`
- [ ] T031 [US1] E2E scenario `scripts/e2e/scenarios/marketplace-catalog.test.ts` (copy `contacts.test.ts`/`create-and-unlock.test.ts` for the shape): start the test marketplace, point the preset at it (T016), open the app, see both tiles with the source, search and category and page, open the details, kill a second source and see its error line next to the intact first one

**Checkpoint**: the app lists the catalog of several sources at once, read-only.

---

## Phase 4: User Story 2 - Sicher: nur geprüfte Bytes werden installiert (Priority: P1)

**Goal**: A download is checked (size, hash, limit), kept in the host, and goes through the one existing
install preview and install of spec 017.

**Independent Test**: Test marketplace with wrong hash, missing hash, short and long bodies, a body over
64 MiB, a bundle without signature: nothing is installed in any case, the reason is shown.

### Backend (PR B)

- [ ] T032 [US2] Create `src-tauri/src/marketplace/download.rs`: `download_bundle(source, slug, version, progress, cancel)`:
  - ask `/extensions/{slug}/download?version=<v>`; refuse (`marketplace_no_hash`) when `bundleHash` is missing; refuse a version that is not compatible (`marketplace_incompatible_version`, FR-018)
  - load `downloadUrl` **immediately**, **without** `Authorization`, streamed (`bytes_stream`); `http` is allowed only when the marketplace address itself is `http` (research R6)
  - stop as soon as more than `ARCHIVE_BYTES` or more than `bundleSize` bytes arrived (`marketplace_too_large`, `marketplace_size_mismatch`) **without reading the rest**; fewer bytes than `bundleSize` is `marketplace_size_mismatch` as well
  - running `sha2::Sha256` over the stream, compared case-insensitively with `bundleHash` (`marketplace_hash_mismatch`)
  - `DOWNLOAD_IDLE_TIMEOUT` between two chunks, no total timeout; progress to the channel at most every `PROGRESS_INTERVAL`; a `tokio_util::sync::CancellationToken` ends it (`marketplace_download_cancelled`)
  - the bytes are held in `Zeroizing`-free memory only inside the pending slot (T033); nothing is written to disk
- [ ] T033 [US2] Create `src-tauri/src/marketplace/pending.rs`: the single pending slot in `MarketplaceState` (`id` from `uuid`, `bytes`, `source_id`, `slug`, `version`, `last_used`); `put` replaces and drops the old one; `take` consumes it on a successful install; `peek(id)` for the preview refreshes `last_used`; cleared after `PENDING_IDLE` without use (a `tokio::time` task, testable with `pause`) and when the vault closes or locks (find the close path through `close_instance` in `src-tauri/src/state.rs` and `vault_gate/`, and clear there, **and trigger every running download's `CancellationToken`** so an in-flight download ends with `marketplace_download_cancelled`); missing or expired slot is `marketplace_pending_missing`
- [ ] T034 [P] [US2] Tests in `src-tauri/src/marketplace/download_tests.rs` and `pending_tests.rs` with the test marketplace: every case of quickstart M6 (wrong hash, missing hash, size short, size long, body over 64 MiB **and** the server records that fewer bytes were served than the whole body, an incompatible version); a signed download URL on another host receives no `Authorization`; an `http` download URL from an `https` marketplace is refused; the idle timeout (`tokio::time::pause`); cancel mid-stream leaves the slot empty; progress is monotonic and not more often than every 100 ms; a second download replaces the first; expiry after 10 minutes; closing or locking the vault clears the slot **and cancels a running download** (nothing installed)
- [ ] T035 [US2] In `src-tauri/src/extensions/registry/install.rs` add `pub enum BundleSource { File(PickedFile), Marketplace { id: String } }` (`Deserialize`, `TS`, tagged `kind` as in contracts/commands.md) and `read_bundle(opener, state, &BundleSource) -> Result<Vec<u8>>` (file: the existing `read_bundle_file`; marketplace: `peek` the slot). In `src-tauri/src/extensions/commands/install.rs` change `extension_install_preview(file)` and `ExtensionInstallArgs.file` to `source: BundleSource`, consume the slot with `take` after a **successful** `install`, and add `origin: Option<InstallOrigin { sourceName }>` to `InstallPreview` (`ts(optional)`). `extension_dev_load` stays a file. Grep every Rust caller of `install_preview`, `install` and `read_bundle_file` (`src-tauri/src/extensions/registry/install_*_tests.rs`, `src-tauri/src/mcp_server/`, others) and fix them all
- [ ] T036 [P] [US2] Tests in `src-tauri/src/extensions/registry/install_marketplace_tests.rs`: the preview of a bundle from the slot equals the preview of the same bytes from a file; the slot is consumed after the install and kept after a failed one; a missing slot is `marketplace_pending_missing`; an unsigned bundle shows the same error as the file path; downgrade and same-version replacement still need `confirmed`; `origin` is set for the marketplace source only
- [ ] T037 [US2] Add the commands `marketplace_download` (with `tauri::ipc::Channel<DownloadProgress>`), `marketplace_download_cancel`, `marketplace_download_discard` to `commands.rs`, keep a `CancellationToken` per running download in `MarketplaceState`, and register them in the `generate_handler!` list of `src-tauri/src/lib.rs`. Extend the FR-030 guard (T024) if it needs the new names
- [ ] T038 [US2] **Every call site** of the changed install commands (memory note: a task list once missed two of seven paths). Run `grep -rn "extension_install" src scripts` (excluding `src/types/bindings`) and change each hit to `source`:
  - `src/components/extensions/InstallDialog.vue` (two `invoke` calls)
  - `scripts/e2e/lib/extensions.ts` (two calls)
  - `scripts/e2e/scenarios/extension-agent-tools.test.ts` (two calls)
  - `scripts/e2e/scenarios/extension-contacts.test.ts`
  - any further hit; list the final grep result in the PR description. Regenerate bindings and strip trailing whitespace; `pnpm typecheck:scripts` must pass
- [ ] T039 [US2] `src/components/extensions/InstallDialog.vue`: a `source` prop (`{ kind: 'file' | 'marketplace', … }`) replaces the file-only flow; show the source line from `preview.origin` (FR-011) and the key of the bundle as publisher with a note when it differs from the catalog's publisher (US2-3); closing without installing calls `marketplace_download_discard`; texts `extensions.install.fromMarketplace` and `extensions.install.publisherDiffers` in `de.json` and `en.json`

### Install flow in the app (PR C)

- [ ] T040 [US2] Create `src/composables/useMarketplaceInstall.ts` and `src/components/marketplace/InstallProgress.vue`: one install at a time (a second request while one runs is refused with a hint, never two dialogs), progress bar fed by the channel and "Abbrechen" (FR-015), then the `InstallDialog` with `{ kind: 'marketplace', id }`; map every error kind of contracts/commands.md to a text (`marketplace_hash_mismatch` → "Die geladene Datei stimmt nicht mit der Angabe des Marketplace überein", `marketplace_no_hash` → "Der Marketplace liefert keine Prüfsumme", and the rest); wire "Installieren" in `CatalogTile.vue` and `CatalogDetails.vue`; a chosen older version goes through the same preview (Downgrade confirmation of spec 017)
- [ ] T041 [US2] E2E scenario `scripts/e2e/scenarios/marketplace-install.test.ts`: install the signed bundle from the test marketplace, see the preview with publisher, permissions and the source, confirm, the extension runs as an app and the tile says "Installiert (x)"; cancel the preview and see that nothing is installed; the three switches of T030 (wrong hash, missing hash, wrong size) show their message and install nothing

**Checkpoint**: from catalog to a running extension, end to end, with every check of FR-013 and FR-014.

---

## Phase 5: User Story 3 - Aktualisieren und Entfernen (Priority: P2)

**Goal**: A newer compatible version shows "Update verfügbar", updates go through the preview, remove uses
the existing removal, the installed list shows what the catalog last saw.

**Independent Test**: Test marketplace with two versions of an installed extension: the tile says
"Update verfügbar", the update runs through the preview, "Entfernen" removes after the data question.

- [ ] T042 [US3] Create `src-tauri/src/marketplace/updates.rs`: the in-memory map `name → KnownUpdate { installed, latestCompatible, sourceId, slug }` in `MarketplaceState`, merged **per entry** and never replaced wholesale: an entry is set when a source answered for that name with a higher compatible version, removed when the answer says it is up to date or the extension is gone, and **kept** when the source failed or the entry was not part of the answer. A catalog page touches only the entries it contains (a page with a filter must not erase the rest). No background fetching (FR-017). Add the command `marketplace_known_updates` to `commands.rs` and the handler list. Tests in `updates_tests.rs`: a catalog page sets and clears only its own entries; a page with a filter keeps the others; a failed source keeps its earlier entries; an up-to-date extension is not in the map; nothing fetches by itself
- [ ] T043 [US3] Add `check_updates` to `updates.rs` and the command `marketplace_check_updates` (`commands.rs`, handler list, contracts/commands.md): for every installed extension name (`extensions::registry::list`) ask every enabled source with `GET /extensions?search=<name>&limit=5` (all names and sources in parallel, at most 4 requests in flight, each with `SOURCE_TIMEOUT`), accept only an entry whose `name` equals the installed name exactly, compute `latest_compatible` with the helpers of T018 and merge the result into the map of T042. It runs when the catalog app opens and on "Aktualisieren" (FR-017, called from `src/composables/useMarketplaceCatalog.ts`), never at start-up and never in the background. Tests in `updates_check_tests.rs`: an installed extension that is **not on the current catalog page** still gets its update; a name that only partly matches is ignored; a failing source keeps its earlier entries and reports its error; at most 4 requests at a time; no request when nothing is installed or no source is enabled
- [ ] T044 [US3] In `CatalogTile.vue` and `CatalogDetails.vue`: state marks "Installiert (x)" and "Update verfügbar (x → y)", the "Aktualisieren" button (same flow as T040, "Update von A auf B" comes from the preview), versions with "kompatibel" or "Benötigt HaexVault ab/bis x" and a disabled install for an incompatible one (FR-018; a compatible older version stays selectable), "Entfernen" calling `extension_remove` like `src/components/settings/extensions/ExtensionManageView.vue` does, **reusing** its data question (extract it into a shared component if it is inline there; do not copy it). After install, update and remove the tile states reload through the live-data subscription of T026 (also when another device did it)
- [ ] T045 [US3] `src/components/settings/extensions/ExtensionsListView.vue`: a row with a known update shows "Update verfügbar (y)" (`data-testid="extension-update-hint-<id>"`) from a new `src/composables/useKnownUpdates.ts` (`marketplace_known_updates`; it neither fetches nor starts a check, FR-017: the hint shows what the last check or catalog call found) and opens the catalog at `/:slug?source=<id>`. Texts in `de.json`/`en.json`
- [ ] T046 [US3] E2E scenario `scripts/e2e/scenarios/marketplace-update.test.ts`: install 1.0.0, serve 1.1.0, "Update verfügbar" on the tile and in the settings list, update through the preview ("Update von 1.0.0 auf 1.1.0", data kept), the mark disappears; choose the older version and see the downgrade confirmation; open the catalog with a search that does **not** show the installed extension and still see the update in the settings list; mark the newest version incompatible and see it locked; remove through the details

**Checkpoint**: updates and removal work from the catalog; the settings list shows what the catalog saw.

---

## Phase 6: User Story 4 - Marketplaces verwalten (Priority: P2)

**Goal**: Settings → Erweiterungen → Marketplaces: every row is equal, several run at once, a self-hosted
one works by entering its address; the preset is an ordinary row.

**Independent Test**: Add a second test marketplace (other port, path prefix), switch it off and on, move
it, delete it; delete the preset and get it back only through the empty-state button.

- [ ] T047 [US4] Add to `commands.rs` and register: `marketplace_save` (create or update, normalizes and validates through the store: scheme, host, path prefix, `https` for credentials), `marketplace_set_enabled`, `marketplace_reorder`, `marketplace_delete` (every row, including the preset), `marketplace_add_preset` (T011 `add_preset`; returns the row), `marketplace_test` (`GET /health` and the first page with the entered values; `{ ok, count }` or the `SourceErrorKind`; credentials only over `https`, FR-021). The writes notify the live-data listeners (`vault_events`, look at how `haex_browser_clients` writes do it)
- [ ] T048 [P] [US4] Tests in `src-tauri/src/marketplace/commands_manage_tests.rs`: save and normalize, reject `ftp://` and a missing host, reorder, delete the preset and see that `marketplace_list` does not bring it back, `add_preset` creates the row with the fixed id and a second call changes nothing (also if the user switched it off in between), `marketplace_test` against the test marketplace (ok with a count, `auth_rejected`, `unreachable`, `bad_response`)
- [ ] T049 [US4] Settings registry: in `src/lib/settings/registry.ts` add the sub-views `extensions.marketplaces` (`extensions/marketplaces`) and `extensions.marketplaces.edit` (`extensions/marketplaces/:id`, `new` creates) like `extensions.browserBridge` (around lines 249–262), a row in the extensions list, entries in `src/lib/settings/search.ts` if the search lists sub-views, and the components in the settings route table
- [ ] T050 [US4] Create `src/components/settings/extensions/MarketplacesView.vue`: `SettingsGroup` with one `Row` per marketplace (name, address, a switch for on/off that applies at once, up and down buttons with labels and keyboard use, "Bearbeiten", the credentials state line), **no special row and no badge for the preset**, the empty list text; subscribes with `onVaultTablesChanged('haex_marketplaces')`. Test ids of contracts/ui.md
- [ ] T051 [US4] Create `src/components/settings/extensions/MarketplaceForm.vue`: name, address (a hint when it starts with `http://`), "Testen und speichern" showing the result of `marketplace_test` (reachable with a count, or the cause) and allowing to save anyway (FR-021), "Löschen" with a confirmation on every existing row. Auth type and credentials are added in T061. `UiButton` with `type="button"`
- [ ] T052 [US4] Catalog empty state "Kein Marketplace eingerichtet" in `MarketplaceApp.vue`: "Öffentlichen Marketplace hinzufügen" (`marketplace_add_preset`) and "Eigenen Marketplace hinzufügen" (opens the settings form `extensions/marketplaces/new` in the settings app; look at how other apps open settings locations)
- [ ] T053 [P] [US4] Texts `settings.extensions.marketplaces.*` in `de.json` and `en.json` (list, form, test result, delete confirmation, `http` hint, empty states)
- [ ] T054 [P] [US4] Tests for SC-009 and FR-034: (a) a Rust test starts **two** test marketplaces on different ports, one with a path prefix and one requiring a (random) bearer token, and fetches one catalog from both **without any code that knows either address**; (b) a test that reads the source files under `src-tauri/src` and `src` and fails when `marketplace.haex.space` appears in `src-tauri/src` or `src` outside `marketplace/store.rs`, `*_tests.rs`, `test_support.rs` and `src/i18n/locales/` (docs and specs are not scanned)
- [ ] T055 [US4] E2E scenario `scripts/e2e/scenarios/marketplace-manage.test.ts`: add a second test marketplace with a path prefix through the form, both appear in the catalog at once with their sources and the source filter works; switch one off (no entries, no error), reorder (the order in the catalog follows), delete; delete the preset, restart, it is **not** back, the empty state offers it, one tap restores exactly one row (no duplicate on a second tap)

**Checkpoint**: any number of equal marketplaces, a self-hosted one by its address.

---

## Phase 7: User Story 5 - Privater Marketplace mit Zugangsdaten (Priority: P3)

**Goal**: A closed marketplace works with Bearer or Basic through a password entry, no secret in the
table, never sent to another host or over `http`.

**Independent Test**: Test marketplace that answers `401` without the token: without credentials the
source shows "Anmeldung nötig", with the chosen entry it loads; deleting the entry warns.

- [ ] T056 [US5] Create `src-tauri/src/marketplace/credentials.rs`, modelled on `src-tauri/src/remote_storage/credentials.rs` (research R11): `CALLER = Caller::Internal { feature: "marketplace" }` with a fixed grant, `TAG = "haex-marketplace"` (the one place of the name), `state(db, item_id)` → `present` / `missing` (deleted, or the fields the auth type needs are empty) / `syncing` (same rules as `state_in` there), `read(…)` → `AuthHeader` (bearer: the password field is the token; basic: user name and password; `Zeroizing`), `tag_selected`, `candidates(auth_type)` (entries with the needed fields, tagged ones first). Register the entry as used in `src-tauri/src/passwords/usage.rs` (`EntryUsage`) so the delete warning names the marketplace (spec 034 FR-034, FR-029). Check how the owner string is handled in `src-tauri/src/passwords/access.rs` (Z14) and extend it only if a new owner needs it
- [ ] T057 [P] [US5] Tests in `src-tauri/src/marketplace/credentials_tests.rs` against a test vault: the three states including an entry that arrives later (`syncing` → `present`), bearer and basic headers built from entries, an entry with an empty password is `missing`, the delete warning of `usage.rs` names the marketplace, an entry chosen by the user keeps its owner and its shares
- [ ] T058 [US5] Use the credentials in every call of the client: `fetch_catalog`, `fetch_categories`, `fetch_details`, `marketplace_test` and the download request for the download address (**only** that request, not the signed URL) get `AuthHeader` from `credentials::read`; a source without `present` credentials fails with `credentials_missing` or `credentials_syncing` **without a request**; credentials with an `http` address are `insecure_credentials` and are not sent
- [ ] T059 [US5] Add the command `marketplace_credentials_candidates` and the creation of a new entry from the form (look at how `src/components/settings/storage/ConnectionForm.vue` and the `storage_credentials_*` commands create the entry in spec 052 and do the same with title "Marketplace: <name>", the tag `haex-marketplace` and the owner decided in T056; if the shape differs from `MarketplaceInput` in contracts/commands.md, update the contract in the same PR). Register in the handler list
- [ ] T060 [US5] Generalize `src/components/settings/storage/CredentialsEntryPicker.vue`: replace the `protocol` prop by `source: { command: string, args: Record<string, unknown> }` and a `labelKey`, and update its caller `ConnectionForm.vue` so the storage behaviour and its test ids (`storage-entry-picker`, `storage-entry-search`) stay identical. Check the E2E scenarios that use them (`grep -rn "storage-entry" scripts`)
- [ ] T061 [US5] `MarketplaceForm.vue`: auth type select (Keine, Bearer, Basic) and the picker (T060) with "neu eingeben" like spec 052; the form refuses Bearer or Basic together with `http://` with the hint that credentials only travel over `https://` (US5-6); the state line "Zugangsdaten fehlen" / "wird synchronisiert" in `MarketplacesView.vue` and the per-source error lines. Texts in `de.json`/`en.json`
- [ ] T062 [P] [US5] SC-004 and SC-005 tests in `src-tauri/src/marketplace/secrets_tests.rs`: after saving a marketplace with an entry, the `haex_marketplaces` row holds only the item id; a `tracing` subscriber capturing a catalog call and a failed call sees neither token nor password; `Debug` of `AuthHeader` and of every model type holds no secret; the client test of T015 for the redirect and `http` rules runs again with a real `AuthHeader`
- [ ] T063 [US5] E2E scenario `scripts/e2e/scenarios/marketplace-private.test.ts`: the test marketplace with the random bearer token; without credentials the source shows "Anmeldung nötig"; create the password entry, choose it, the catalog loads; delete the entry in the password manager and see the warning naming the marketplace, confirm, the source shows "Zugangsdaten fehlen" while the second source stays intact; `http://` with credentials is refused in the form

**Checkpoint**: closed marketplaces work and leak nothing.

---

## Phase 8: Polish & cross-cutting (PR G)

- [ ] T064 [P] E2E scenario `scripts/e2e/scenarios/marketplace-two-devices.test.ts` (SC-007, pattern of `contacts-two-devices.test.ts`): add a marketplace on device A, see it on device B after the sync; switch the preset off on A and see it stay off on B; delete the preset on A, it is not back on B
- [ ] T065 Android: run the clippy of the whole module for `aarch64-linux-android`; run the catalog and install scenarios in the Android emulator (devShell `holzi-emulator`, memory note on the Android e2e rig); check the one-column grid at 400 px, touch targets of 44 px, and that the back gesture closes the install preview before it leaves the app. iOS is covered by the CI build (compile only; the `platform()` value for iOS is tested in T015). Real-phone checks stay manual (quickstart M12)
- [ ] T066 [P] Add the glossary terms "Marketplace" and "Voreinstellung" to `CONTEXT.md` and link `specs/069-marketplace/contracts/marketplace-api.md` as the protocol for operators from `README.md` (one short section "Einen Marketplace betreiben")
- [ ] T067 Wrap-up: run the manual checks of quickstart M1–M14 that need a person (M10, M12, M13), update `plans/README.md` with the shipped status, mark finished tasks here, run all "Before every push" commands, and run `/speckit-analyze` once over spec, plan and tasks

---

## Dependencies & Execution Order

- **Phase 1** (T001–T003) is the docs PR; T003 before T016.
- **Phase 2** blocks everything: T004 → T005 → T006; T017 after T005 and T011 (before PR A merges: a new vault needs the preset); T007 → T008, T009 → T010 → T011 → T012; T013 → T014 → T015; T016 after T011 and T003.
- **US1 backend** (T018–T024): T018 → T019, T020; T021 → T022; T023 after T018, T020, T021; T024 after T023. **US1 app** (T025–T031): T025 → T026 → T027 → T028; T029 any time; T030 → T031 (needs T027).
- **US2** (T032–T041): T032 → T033 → T034; T035 after T033 → T036; T037 after T032, T033, T035; T038 and T039 after T035; T040 after T027, T037, T039; T041 after T030, T040. **US2 backend does not need the app**, the app's install button needs US2.
- **US3** (T042–T046): T042 after T018 → T043 (after T018, T019) and T044 (after T040); T045 after T042, T043; T046 last.
- **US4** (T047–T055): T047 → T048; T049 → T050 → T051; T052 after T047 and T027; T053 any time; T054 after T047; T055 last.
- **US5** (T056–T063): T056 → T057; T058 after T056 and T014; T059 after T056; T060 any time; T061 after T051, T059, T060; T062 after T058; T063 last.
- **Phase 8**: T064 after T047; T065 after the scenarios exist; T067 last.
- **Stories**: US1 and US2 first (MVP together). US3, US4, US5 are independent of each other after US1+US2 (US3 needs the tile states, US5 needs the form of US4).

## Parallel Examples

- **Phase 2**: T006 (migration tests), T009 (wire types), T012 (store tests), T015 (client tests) are
  separate files.
- **US1 backend**: T019 (catalog tests), T022 (icon tests) while T023 is written.
- **US2**: T034 (download tests) and T036 (install tests) are different modules.
- **US4**: T048 (command tests), T053 (texts), T054 (SC-009 tests).
- **After MVP**: US3, US4 and US5 by different people in different worktrees.

## Implementation Strategy

- **MVP**: Phases 1–4 (PR A, B, C).
  - The app lists the catalog of every enabled marketplace at once, installs through the one preview,
    refuses anything whose hash, size or signature is wrong.
  - The preset is the only marketplace a new vault has until US4 ships the settings; adding a
    marketplace works from PR E.
- **Then**: PR D (updates), PR E (manage), PR F (private), PR G (two devices, Android, wrap-up). Each PR
  keeps the app releasable.
- **Manual-only checks**: quickstart M10 (network off), M12 (Android with a real phone), M13 (Windows and
  macOS by hand), and SC-001 with a stopwatch.
