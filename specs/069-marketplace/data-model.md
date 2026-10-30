# Data model: Marketplace for extensions

## Table `haex_marketplaces` (vault data, CRDT-tracked, synced)

Migration `0048_marketplaces` (`0047` was taken by `agent_browser`, spec 065), `HOLZI_TRIGGER_VERSION` + 1. No UNIQUE
and no foreign key to credentials (see R11, R14).

| Column                | Type                         | Meaning                                                                                              |
| --------------------- | ---------------------------- | ---------------------------------------------------------------------------------------------------- |
| `id`                  | TEXT PK                      | random UUID; only the preset carries the fixed id `preset-public` (R10)                              |
| `name`                | TEXT NOT NULL                | display name                                                                                         |
| `base_url`            | TEXT NOT NULL                | address without trailing `/`, with any path prefix                                                   |
| `enabled`             | INTEGER NOT NULL DEFAULT 1   | on/off                                                                                               |
| `sort_order`          | INTEGER NOT NULL             | rank, smaller = earlier; on a tie `(sort_order, name, id)`                                           |
| `auth_type`           | TEXT NOT NULL DEFAULT `none` | `none`, `bearer` or `basic`                                                                          |
| `credentials_item_id` | TEXT                         | password entry (Spec 034); required for `bearer`/`basic`, otherwise NULL; **no** secret in the table |
| `created_at`          | INTEGER NOT NULL             | Unix milliseconds                                                                                    |
| `updated_at`          | INTEGER NOT NULL             | Unix milliseconds                                                                                    |

**Rules**

- `base_url` MUST start with `http://` or `https://` and have a host; a path prefix is allowed
  (`https://example.org/market`, FR-024). It is normalized (without trailing `/`, without fragment and
  query); paths of the protocol are appended **behind** the path prefix.
- `auth_type` ≠ `none` requires `credentials_item_id` and an `https://` address (FR-027); the check sits
  in the host command on save **and** in the client on send.
- A new row gets `sort_order` = highest existing value + 1 (at the end); the user moves it with up/down,
  and the UI then rewrites the full order (`marketplace_reorder`).
- All rows are equal: each can be changed, switched off and deleted. There is no default marker and no
  row that may not be deleted.
- A row without a valid address (from a future version) is left out on read and logged, not rejected.

## Derived view `MarketplaceView` (no table)

From the rows, sorted by `(sort_order, name, id)`: `id`, `name`, `baseUrl`, `enabled`, `rank`,
`authType`, `credentials: none | present | missing | syncing` (R11).

## Transient types (not stored)

- **CatalogEntry** (determined by `(source.id, slug)`, never merged across sources): `source: { id, name }`,
  `extensionId`, `name`, `slug`, `shortDescription`, `iconUrl`, `verified`, `totalDownloads`,
  `averageRating`, `reviewCount`, `tags`, `publisher`, `category`, `versions[]` (each `version`,
  `bundleSize`, `compatible`, `incompatibleReason?`), plus `installed: { version, enabled } | null`,
  `updateTo: string | null`, `latestCompatible: string | null`.
- **CatalogPage**: `entries[]`, `page`, `totalPages`, `total`, `sourceErrors[]`.
- **SourceError**: `{ sourceId, name, kind, detail? }` with `kind` ∈ `unreachable`, `timeout`,
  `auth_rejected`, `credentials_missing`, `credentials_syncing`, `bad_response`, `insecure_credentials`.
- **PendingBundle** (memory in the host only): `id`, `bytes`, `sourceId`, `slug`, `version`, `fetchedAt`,
  `lastUsedAt`. At most one; for when it is freed see R3.
- **DownloadProgress** (channel): `{ loaded, total }`.
- **KnownUpdate** (memory): candidates are separated by `(installedName, sourceId, slug)` and contain
  `{ installed, latestCompatible, sourceId, slug }`. `marketplace_check_updates` and catalog calls update
  only the candidates of the answering source for the entries contained; a source that fails keeps its
  earlier candidates. The installation list picks, per extension name, the highest compatible version
  from the known candidates, and on a tie the source with the better rank. There is no query of its own
  accord (FR-017).

## State transitions of the download

```text
idle ──download──▶ fetching-url ──▶ streaming ──▶ verifying ──▶ ready(id)
  ▲                    │              │             │             │
  └──── cancel / error / vault closed / idle 10 min ◀─────────────┘
ready(id) ──preview──▶ ready(id) ──install ok──▶ idle      ready(id) ──discard──▶ idle
```

`verifying` checks the size (exactly `bundleSize`, at most 64 MiB) and SHA-256; if it fails, the state
goes to `idle` and the slot is empty. A new download replaces `ready` and discards the old one.
