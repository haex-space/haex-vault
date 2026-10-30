# Implementation Plan: Marketplace for Extensions

**Branch**: `069-marketplace` | **Date**: 2026-10-28 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/069-marketplace/spec.md`

## Summary

HaexVault gets a catalog for extensions. All network and verification logic sits in the host, in a new
module `src-tauri/src/marketplace/`; the frontend only displays and calls Tauri commands.

- **Catalog**: The host queries all enabled marketplaces in parallel (`GET /extensions`,
  `/categories`, `/extensions/{slug}`), with a fixed deadline per source, and combines the answers
  (simultaneously, every tile with its source, **no** merging across sources, errors per source). The host
  adds the install state from the extension registry (spec 017), so there is one place for it.
- **Download**: `marketplace_download` fetches the signed download address, streams the `.xt`
  (≤ 64 MiB, exactly `bundleSize` bytes), checks SHA-256 against `bundleHash` and puts the bytes into **one
  memory slot in the host** (nothing on disk, nothing in the webview).
- **Install**: `extension_install_preview` and `extension_install` take a `BundleSource` (`file` or
  `marketplace`) instead of `file: PickedFile`. The marketplace case reads the bytes from the memory slot
  and calls the same `install_preview` and `install` as a file does. `InstallDialog.vue` gets a `source`
  property; there is no second preview.
- **Marketplace list (generic)**: new CRDT table `haex_marketplaces`; every row is equal (name, address
  with any path part, on/off, rank, auth type, credentials reference). There is **no special case in the
  code**: the project's public marketplace is only the **preset**, an ordinary row with the fixed id
  `preset-public`, which a new vault brings along and which the user can change or delete. A vault without
  a marketplace offers it in the empty state with one tap.
- **Open protocol**: `contracts/marketplace-api.md` is the normative contract "Marketplace protocol v1"
  for anyone who wants to run a marketplace; the host assumes nothing about a marketplace beyond this
  contract (FR-034).
- **Credentials**: reference `credentials_item_id` to a password entry (spec 034/052), tag
  `haex-marketplace`; a module of its own, `marketplace/credentials.rs`, reads it (Bearer: the password
  field is the token; Basic: user name and password) and reports `present`/`missing`/`syncing`. The
  selection uses `CredentialsEntryPicker.vue`, which gets a source as a property instead of `protocol`
  for this.
- **UI**: new wm app `system.marketplace` ("Marktplatz" / "Marketplace"), new sub-view
  `extensions/marketplaces` in the settings with form `extensions/marketplaces/:id`, an update hint in the
  list of installed extensions.

No new dependency: `reqwest`, `sha2`, `semver`, `url`, `futures`, `tokio-util` are there; test server
through `hyper` (dev dependency, already present).

## Technical Context

**Language/Version**: Rust 2021 (MSRV 1.95, `src-tauri`), TypeScript (Nuxt 4, Vue 3)

**Primary Dependencies**:

- **Existing**:
  - `reqwest` 0.13 (rustls, stream, json, query), `sha2` 0.11, `semver` 1, `url`, `futures`, `tokio-util`
  - `PasswordsService`, `passwords::usage::EntryUsage` (delete warning, spec 034 FR-034)
  - `extensions::registry::{install_preview, install, list}`, `limits::ARCHIVE_BYTES` (64 MiB)
  - `tauri::ipc::Channel` for the progress (like `files_watch`, `TransferManager`)
  - Patterns: `extensions/web.rs` (reqwest client, redirects), `remote_storage/credentials.rs`
    (credentials entry), `vault-data-changed` and `onVaultTablesChanged` (spec 028 live data)
- **New**: none.

**Storage**:

- New CRDT table `haex_marketplaces` (migration `0048_marketplaces`, `0047` was taken by spec 065;
  `HOLZI_TRIGGER_VERSION` + 1). No secret in the table.
- The memory slot for the downloaded bundle is only memory in the `AppState`, at most one bundle.
- Image cache (icons) only in memory, per session.

**Testing**:

- **Rust**: `*_tests.rs` next to every module, never inline. A test marketplace built on `hyper` (as in the
  WebDAV tests) serves catalog, download and error cases: checksum wrong/missing, size wrong, too large,
  redirect to another host, `http` with credentials, deadline, broken answers, 401.
- **Pure parts without network**: merging and paging, version comparison (`semver`), compatibility
  (`minAppVersion`/`maxAppVersion`), preset as an ordinary row.
- **Vault test**: table, preset (create, delete, do not re-create), credentials state against a test vault.
- **Frontend**: no logic of its own that would need unit tests (it sits in the host); type check, lint,
  `check:templates` and e2e.
- **E2E** (tauri-driver rig, spec 016): a local test marketplace (Node, in the rig) with two extensions,
  one of them signed and installable; desktop and Android emulator (SC-008); two-device case for
  SC-007 (`marketplace-two-devices.test.ts`, pattern `contacts-two-devices.test.ts`).

**External release prerequisite**: Checked against the public marketplace on 2026-10-29 (9 extensions): six
of the newest versions are in the old bundle format (no `haextension/signature.json`, signature inside the
manifest, `public.key`) and cannot be installed through the install preview of spec 017: `haex-tetris`
0.1.3, `haex-image` 0.1.6, `haex-game-unicorn` 1.0.1, `haex-draw` 0.2.7, `haex-code` 0.2.11 and
`haex-calendar` 1.0.9. `haex-mail`, `haex-notes` and `haex-doom` are already `haextension-bundle/2`.
The six were republished on 2026-10-29 as `haex-tetris` 0.1.4, `haex-image` 0.1.7, `haex-game-unicorn`
1.0.2, `haex-draw` 0.2.8, `haex-code` 0.2.12 and `haex-calendar` 1.0.10 (hash, size and `haex verify`
checked); what is left is to check every download with the install preview of a release build. That lies outside this repository and is no substitute
for the signed test marketplace in the E2E tests; the checks are manual step M16 of the quickstart.

**Target Platform**: Linux, Windows, macOS, Android, iOS (no platform-dependent code except the version
header `X-Platform`)

**Project Type**: Desktop and mobile app (Tauri 2 with Nuxt frontend)

**Performance Goals**: first catalog page visible in < 3 s with a reachable marketplace; a failed
marketplace does not extend that beyond the deadline (R5); icons load afterwards.

**Constraints**:

- No credentials over `http://` or to another host (FR-027).
- Checksum mandatory, 64 MiB, exactly `bundleSize` (FR-013, FR-014).
- No agent and no extension reaches the catalog (FR-030).
- Files ≤ 500 lines, tests separate, Clippy per repo configuration.

**Scale/Scope**: one to a few sources, a few hundred extensions per source, up to 20 entries per source
and page (a shared page accordingly contains more entries with more sources).

## Constitution Check

_GATE: checked before phase 0, re-checked after phase 1._

| Principle                                   | Status                                                                                                                                                                                                          |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. No secrets in Git                        | Met. Spec, plan and test server contain only placeholders; test tokens are random values at run time. The example addresses in the contracts are public and carry no token (the signed address is abbreviated). |
| II. No local absolute paths                 | Met. References are repo-relative.                                                                                                                                                                              |
| III. Project identity is device-independent | Not affected.                                                                                                                                                                                                   |
| IV. Cross-repo references pin a fixed SHA   | Met. haex-vault 2.x is given as `8dce379d…`, the `marketplace-sdk` with version 0.1.6; the service itself is a live endpoint (see R1, risk).                                                                    |
| V. External sources only by allowlist       | Not affected (no new harness source).                                                                                                                                                                           |
| VI. Self-modifying instructions             | Not affected.                                                                                                                                                                                                   |
| VII. Relay outage blocks nothing            | Correspondingly: a marketplace outage blocks nothing local; installing from a file remains (FR-031, edge cases).                                                                                                |
| VIII. No concealment                        | Not affected.                                                                                                                                                                                                   |

Further MUST rules of the spaex constitution that apply here:

- **Worktree and PR**: Work lives in `.worktrees/069-marketplace`; docs PR, then staged PRs, no
  squash merge, Conventional Commits.
- **Graphify first**: a `graphify query` before every new name. Checked for the plan: credentials
  (`remote_storage/credentials.rs`, `CredentialsEntryPicker.vue`) are **extended**, not copied;
  the redirect rules from `extensions/web.rs` are tied to permissions and are not reused (R6).
- **One implementation per rule**: install preview and installation remain one implementation
  (`BundleSource`); version comparison uses `semver` like the registry.
- **Tests separate, deterministic, without network**: test server local, clock and deadline injectable.
- **Not over 500 lines**: the module is cut into seven files in advance (see structure).
- **ADR**: not needed, no core-principle decision. The decisions are in `research.md`.

After phase 1: re-checked, no violation; Complexity Tracking stays empty.

## Project Structure

### Documentation (this feature)

```text
specs/069-marketplace/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── marketplace-api.md   # What HaexVault expects from the service (contract to the outside)
│   ├── commands.md          # Tauri commands, types, progress
│   └── ui.md                # App, settings, test ids
├── checklists/requirements.md
└── tasks.md                 # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/marketplace/
├── mod.rs              # State (AppState part), deadline and size constants
├── model.rs            # Catalog and list types (ts-rs), inputs
├── store.rs            # Table haex_marketplaces, preset (preset-public), reading and writing
├── client.rs           # reqwest: requests, headers, redirect rule, error kinds
├── catalog.rs          # parallel query, merging, paging, install state, compatibility
├── download.rs         # Stream, checksum, size, memory slot, progress, cancel
├── credentials.rs      # Credentials entry (state, reading, tag), auth header
├── updates.rs          # known updates in memory for the list of installed extensions
├── commands.rs         # Tauri commands
└── *_tests.rs          # one per file
src-tauri/src/identity/migrations_marketplaces.rs   # Migration 0048
src-tauri/src/extensions/commands/install.rs        # BundleSource (one place)
src-tauri/src/extensions/registry/install.rs        # read_bundle: file or memory slot

src/lib/marketplace/        # registry.ts (routes), compat/display helpers
src/composables/useMarketplaceCatalog.ts, useMarketplaces.ts, useMarketplaceInstall.ts
src/components/marketplace/ # Catalog, tile, details, filter, source errors
src/components/apps/MarketplaceApp.vue
src/components/settings/extensions/ (marketplaces list, form)
src/components/extensions/InstallDialog.vue         # source property
src/lib/wm/apps.ts, src/components/wm/appRoutes.ts  # register the app
src/i18n/locales/{de,en}                            # texts
scripts/e2e/                                        # test marketplace and scenarios
```

**Structure Decision**: one Rust module per responsibility (network, merging, download,
credentials, storage), the shared install routine stays in `extensions/`, the frontend follows the
pattern of the existing apps (`lib/<app>/registry.ts`, `components/apps/<App>.vue`).

### Stages (PR cut)

The assignment of the tasks is in `tasks.md` (shipping table):

1. **A – Core** (T004–T024): migration, `store` including the preset and creating it for a new vault,
   `client`, `catalog`, types, commands `marketplace_list`, `marketplace_catalog`, `marketplace_categories`,
   `marketplace_details`, `marketplace_icon`; Rust tests with a test server. No UI.
2. **B – Download and install** (T032–T039): `download`, memory slot, `BundleSource`, all callers,
   `InstallDialog` source, progress, cancel. Rust tests.
3. **C – Catalog app** (T025–T031, T040–T041): `MarketplaceApp`, tiles, details, search, categories,
   source, paging, icons, install flow. e2e desktop.
4. **D – Updates** (T042–T046): known updates per entry, update check for all installed extensions,
   compatibility lock, remove, hint in the list of installed extensions.
5. **E – Management** (T047–T055): settings view, form with test, order, on/off, several
   sources at once, empty state with the preset.
6. **F – Private marketplaces** (T056–T063): credentials entry, picker extension, delete warning.
7. **G – Wrap-up** (T064–T067): two-device e2e, Android, glossary, manual check (quickstart).

## Complexity Tracking

No violations; nothing to justify.
