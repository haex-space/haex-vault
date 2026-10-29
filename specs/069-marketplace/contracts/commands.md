# Contract: Tauri commands and types

All commands live only in the main window's handler (R13). Types reach the frontend through `ts-rs` in
`src/types/bindings/` (`camelCase`). Errors are `HolziError` with stable kinds (see below); the
UI translates them.

## List and management (settings, FR-020 to FR-029)

| Command                              | Arguments                                                               | Result                                         | Note                                                                                                       |
| ------------------------------------ | ----------------------------------------------------------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `marketplace_list`                   | –                                                                       | `MarketplaceView[]`                            | all rows, by rank (all equal, no special row)                                                              |
| `marketplace_save`                   | `MarketplaceInput { id?, name, baseUrl, authType, credentialsItemId? }` | `MarketplaceView`                              | creates or updates; checks the address, `https` for authentication (FR-024, FR-027)                        |
| `marketplace_set_enabled`            | `id`, `enabled`                                                         | –                                              | takes effect at once                                                                                       |
| `marketplace_reorder`                | `ids: string[]` (complete order)                                        | –                                              | rewrites `sort_order`                                                                                      |
| `marketplace_delete`                 | `id`                                                                    | –                                              | every row can be deleted, including the preset; nothing creates it again                                   |
| `marketplace_add_preset`             | –                                                                       | `MarketplaceView`                              | creates the preset (`preset-public`) if it is missing; otherwise it switches nothing (empty state, FR-022) |
| `marketplace_test`                   | `MarketplaceInput`                                                      | `MarketplaceTestResult { ok, count?, error? }` | `GET /health` and the first page; sends credentials only over `https` (FR-021)                             |
| `marketplace_credentials_candidates` | `authType`                                                              | `CredentialsCandidate[]`                       | matching entries, tagged ones first (R11)                                                                  |

## Catalog (FR-001 to FR-005, FR-017, FR-018, FR-031)

| Command                     | Arguments                                                                             | Result                                                                                                                                                                                                                                    |
| --------------------------- | ------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `marketplace_catalog`       | `CatalogQuery { page, search?, category?, sourceId? }` (`limit` 20, `sort` downloads) | `CatalogPage`                                                                                                                                                                                                                             |
| `marketplace_categories`    | `sourceId?`                                                                           | `{ categories, sourceErrors }`                                                                                                                                                                                                            |
| `marketplace_details`       | `sourceId`, `slug`                                                                    | `CatalogDetail` (entry + description, key fingerprint, `publisherMatchesInstalled`)                                                                                                                                                       |
| `marketplace_icon`          | `sourceId`, `url`                                                                     | `string` (`data:` URL) or error `marketplace_icon_unavailable`; the target and every redirect must be publicly routable (R12)                                                                                                             |
| `marketplace_known_updates` | –                                                                                     | `{ candidates: KnownUpdate[], selected: KnownUpdate[] }` (only the candidates known from the last run, FR-017)                                                                                                                            |
| `marketplace_check_updates` | –                                                                                     | `{ updates: KnownUpdate[], sourceErrors }` (asks every enabled source, for **every installed extension**, with `search=<name>`, exact name matches only; runs when the app opens and on "Aktualisieren", never in the background, FR-017) |

Without `sourceId`, all enabled marketplaces are asked at the same time and their entries are returned one
after the other in rank order, each with its source, **without merging** (FR-012). Each source returns the
requested page number with up to 20 entries; `totalPages` is the maximum of the page counts, `total`
the sum of the hits. A combined page can therefore hold more than 20 entries. Within each
source the sort order is downloads, descending (FR-003). Known updates are kept per
`(installedName, sourceId, slug)`: an answer changes only the candidates of its source and the
entries it answered for; a source that fails deletes nothing. In the installed list, the highest known
compatible version is shown per name; on a tie the marketplace rank decides. A new call to
`marketplace_catalog` cancels one that is still running.

The combined list is **not** sorted by downloads across sources: sources stay in their
configured rank, and the requested page of each source is placed in that order. Sorting
by downloads applies within each source.

## Download and installation (FR-010 to FR-016)

| Command                        | Arguments                                                                    | Result                                                           |
| ------------------------------ | ---------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `marketplace_download`         | `sourceId`, `slug`, `version`, `id`, `onProgress: Channel<DownloadProgress>` | `PendingBundleRef { id }`                                        |
| `marketplace_download_cancel`  | `id`                                                                         | –                                                                |
| `marketplace_download_discard` | `id`                                                                         | – (closing the preview)                                          |
| `extension_install_preview`    | `source: BundleSource`                                                       | `InstallPreview` (+ `origin?: { sourceName }` for a marketplace) |
| `extension_install`            | `ExtensionInstallArgs { source: BundleSource, accepted, confirmed }`         | `ExtensionSummary`                                               |

```ts
type BundleSource =
  | { kind: 'file'; file: PickedFile } // as before
  | { kind: 'marketplace'; id: string } // PendingBundleRef.id
```

`id` is a UUID chosen by the caller: the download can then be cancelled by `marketplace_download_cancel`
before it has ended, and the same `id` names the pending bundle afterwards (`PendingBundleRef.id`). An
`id` that is already running is refused. `marketplace_download_discard` also ends a download that
is still running. A closing vault ends running downloads and drops the pending bundle (the app process
ends with the vault, spec 013); the pending bundle also expires 10 minutes after its last use.

`extension_install` with `marketplace` consumes the pending slot on success. Removal
(`extension_remove`, spec 017) stays unchanged and is called from the catalog (FR-016).

## Events

- `vault-data-changed` (existing): the marketplace list and the tile states subscribe to
  `haex_marketplaces` and to the extensions (`onVaultTablesChanged`, spec 028 live data).
- No new event.

## Error kinds (`HolziError::Marketplace { reason }`)

The field is called `reason` because `kind` is the tag of the error enum (`{ kind: "Marketplace", reason }`).

`marketplace_invalid_url`, `marketplace_credentials_required`,
`marketplace_insecure_credentials`, `marketplace_not_found`, `marketplace_no_hash`,
`marketplace_hash_mismatch`, `marketplace_size_mismatch`, `marketplace_too_large`,
`marketplace_incompatible_version`, `marketplace_download_cancelled`, `marketplace_source_*`
(`unreachable`, `timeout`, `auth_rejected`, `bad_response`), `marketplace_icon_unavailable`,
`marketplace_pending_missing` (pending slot empty or expired), `marketplace_credentials_syncing`
(the credentials entry has not arrived yet), `marketplace_superseded` (a newer `marketplace_catalog` call took
the place of this one), `marketplace_destination_refused`.

## Not present (FR-030)

No action in `*ActionHandlers`, no MCP tool, no method of the extension checkpoint (ADR-0004). A test
in `marketplace/commands_tests.rs` checks this by name.
