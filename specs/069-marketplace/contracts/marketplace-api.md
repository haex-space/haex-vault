# Marketplace protocol v1 (normative)

This contract describes what a **marketplace** MUST provide so that HaexVault can use it. It applies to
every operator in the same way: the project's public marketplace, company-owned marketplaces and
self-hosted ones. HaexVault assumes nothing about a marketplace beyond this contract (FR-034).

Origin: `@haex-space/marketplace-sdk` 0.1.6 and the service `marketplace.haex.space`, confirmed on
2026-10-28 with read-only calls; from 069 on, **this text** is authoritative, not the service. The host
reads tolerantly: unknown fields are ignored (FR-035), and an entry with a missing required field is left
out (not the whole answer).

## Requirements for an operator

- **Address**: `https://` (recommended) or `http://` (only sensible on a local network), with any
  path prefix. All paths of this contract are appended **behind** the path prefix
  (`https://example.org/market` → `https://example.org/market/extensions`).
- **Responses** are JSON (`Content-Type: application/json`) in `camelCase`; character set UTF-8.
- **Authentication** (optional): bearer token or Basic in the `Authorization` header; HaexVault sends it only
  over `https://` and only to this host (FR-027). A marketplace that requires it answers a request without
  valid authentication with `401` or `403`.
- **Download address** (`downloadUrl`) MAY be on another host (object storage, signed
  address); it is loaded without authentication and MUST remain valid at least as long as the call takes (the
  public service: 300 s).
- **Checksum and size** of every version are required (`bundleHash`, `bundleSize`); without them the version
  cannot be installed (FR-013).
- **No requirement** on response time beyond a limit of 8 s per call, on CORS (the host makes the request,
  not a browser) or on a fixed address.

## Requests

All requests are `GET`, `Accept: application/json`, with `User-Agent: HaexVault/<version>`,
`X-Platform: linux|windows|macos|android|ios` and `X-App-Version: <version>`. No other header that
identifies the user, the vault or the device (FR-007). For `auth_type` ≠ `none`, additionally
`Authorization: Bearer <token>` or `Basic <base64>`, only over `https://` and only to the host of the
marketplace address (FR-027).

| Call                                          | Purpose                       | Response                                           |
| --------------------------------------------- | ----------------------------- | -------------------------------------------------- |
| `/health`                                     | Reachability (form test)      | `{ "status": "ok" }`                               |
| `/categories`                                 | Categories                    | `{ categories: [CategoryWithCount] }`              |
| `/extensions?page&limit&category&search&sort` | Catalog page                  | `{ extensions: [ListItem], pagination }`           |
| `/extensions/{slug}`                          | Details                       | `ExtensionDetail`                                  |
| `/extensions/{slug}/download[?version]`       | Download address of a version | `{ downloadUrl, version, bundleSize, bundleHash }` |

`sort` ∈ `downloads` (HaexVault's default), `rating`, `newest`, `updated`. HaexVault always sends `limit=20`; a marketplace MAY cap it lower and then reports the effective value in `pagination.limit`, which HaexVault uses for paging.

The response shapes and field types are binding:

```ts
type HealthResponse = { status: 'ok' }
type CategoryWithCount = { name: string; slug: string; count: number }
type CategoriesResponse = { categories: CategoryWithCount[] }
type Pagination = {
  page: number
  limit: number
  total: number
  totalPages: number
}
type ListResponse = { extensions: ListItem[]; pagination: Pagination }
type DownloadResponse = {
  downloadUrl: string
  version: string
  bundleSize: number
  bundleHash: string
}
```

`page` is 1-based. `total` counts all hits after filters; `totalPages` is `ceil(total / limit)` and
`0` when there are no hits. `bundleSize` is a non-negative whole number of bytes; `bundleHash` is a
64-character SHA-256 hex value. An absent optional field is either omitted or delivered as `null`.
Unknown fields are permitted.

## Fields HaexVault reads

**ListItem**: `id: string`, `extensionId: string` (`<publisher-slug>/<name>`), `name: string`,
`slug: string`, `shortDescription: string`, `iconUrl?: string`, `verified: boolean`,
`totalDownloads: number`, `averageRating?: number`, `reviewCount: number`, `tags?: string[]`,
`publishedAt?: string`, `publisher?: { displayName: string; slug: string; verified: boolean }`,
`category?: { name: string; slug: string }`, `versions: Version[]`.

**Version**: `version: string` (semver), `changelog?: string`, `bundleSize?: number` (bytes),
`bundleHash?: string` (SHA-256, 64 hex characters), `permissions?: string[]` (display only:
**the declared permissions in the bundle are authoritative**, spec 017), `minAppVersion?: string`,
`maxAppVersion?: string`, `publishedAt?: string`.

**ExtensionDetail**: `ListItem` plus `publicKey: string` (publisher key), `description: string`,
`author?: string`, `latestVersion?: string`.

**Required fields per entry**: `extensionId`, `name`, `slug`, `shortDescription`, `versions` (may be empty;
then nothing is installable). To install a version, `bundleHash` and `bundleSize` must be
present; if either is missing, the version stays visible but cannot be installed.

## Download

1. `GET /extensions/{slug}/download?version=<v>` returns `downloadUrl` (signed, short-lived, **possibly on
   another host**), `bundleSize`, `bundleHash`.
2. HaexVault loads `downloadUrl` **immediately** and **without** `Authorization`, streamed, up to at most
   64 MiB and at most `bundleSize` bytes; more bytes or fewer bytes are an error.
3. The SHA-256 of the bytes MUST equal `bundleHash` (case-insensitive); without `bundleHash`
   nothing is installed (FR-013).
4. The bytes go into the install preview (spec 017); its signature check is independent.

## Errors

Every status ≥ 400 is an error of the source: `401/403` → `auth_rejected`; timeout →
`timeout`; connection error, TLS, DNS → `unreachable`; invalid JSON or missing required fields of the
response → `bad_response`. The client discards the service's response text entirely: it may contain an
echo of credentials. Error details contain only locally generated, sanitized information such as a
status code or the error kind; never the response text, the full URL, request headers or credentials.

## Images

The host loads `iconUrl` (R12): without credentials, only `http` or `https`, `image/png|jpeg|webp`, ≤
256 KiB (icons) or ≤ 1 MiB (screenshots), **no SVG**. Every target must be publicly routable,
even if it lies on the configured marketplace origin. Private, reserved, loopback,
link-local, multicast and unspecified IP addresses are blocked. A URL on a
different origin must use `https`. The public storage `api.marketplace.haex.space` remains
permitted. Redirects are checked one by one under the same rules and must not change from `https`
to `http`. DNS addresses are validated for the connection and pinned, so that the
check cannot be bypassed by DNS rebinding.
