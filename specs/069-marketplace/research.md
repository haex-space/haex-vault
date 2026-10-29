# Research: Marketplace for extensions

All items of the Technical Context are decided; no `NEEDS CLARIFICATION` remains. Sources:
`@haex-space/marketplace-sdk` 0.1.6 (types and paths, read from the npm package), the live service
`https://marketplace.haex.space` (read-only calls on 2026-10-28), haex-vault 2.x @ `8dce379d` and the
holzi code (references per item).

## R1 – The interface of the service

**Decision**: HaexVault speaks the same interface as the SDK 0.1.6, directly under the marketplace
address (no `/v1`): `GET /health`, `/categories`, `/extensions?page&limit&category&search&sort`,
`/extensions/{slug}`, `/extensions/{slug}/download[?version]`, `/extensions/{slug}/versions`. Answers are
JSON in `camelCase`. The fields HaexVault needs are listed in
[contracts/marketplace-api.md](contracts/marketplace-api.md).

**Confirmed against the live service** (2026-10-28):

- `bundleHash` is SHA-256 in hex (64 characters) and matched the downloaded file for `haex-tetris 0.1.3`;
  `bundleSize` matched to the byte.
- `downloadUrl` is a **signed, short-lived address on another host**
  (`api.marketplace.haex.space/storage/v1/object/sign/…`, valid 300 s according to the token). It is
  loaded immediately after it is fetched and never cached; no `Authorization` is sent to this host.
- `iconUrl` also lives on another host (public storage).
- `extensionId` has the form `<publisher slug>/<name>` (`haex/haex-tetris`) and is **not** the publisher
  key. The key (`publicKey`) appears only in the details.
- `versions` in the list call arrive newest first, and the extension's `publishedAt` is often `null`.
  The newest version is therefore determined with `semver`, not by order (the legacy took `versions[0]`).
- In practice a bundle is between 1.7 MiB and 24 MiB; the limit of 64 MiB fits.

**The protocol belongs to this repository** (operator decision, 2026-10-28): The operator provides a
public marketplace, and users may host their own. Therefore
[contracts/marketplace-api.md](contracts/marketplace-api.md) is, from 069 on, the **normative contract**
("Marketplace Protocol v1") and no longer a description of the haex-space service; required and optional
fields, error behaviour and requirements for operators (TLS, path prefix, authentication) are stated
there. The host knows no address and no operator except as a row in the list.

**Alternatives**: use the SDK in the frontend (rejected: second HTTP path, operator decision); invent a
different interface (rejected: the service and the legacy speak this one).

**Risk**: The service is a live endpoint, not a fixed revision (Constitution IV applies to harness
content, not to this). Mitigation: the host reads tolerantly (ignore unknown fields, check required
fields per entry) and the contract lives in the repo, with test data from the live answer (random values
substituted).

## R2 – Where the types come from

**Decision**: The types are rebuilt **in Rust** (`marketplace/model.rs`, `serde`, only the needed fields)
and brought into the frontend via `ts-rs` (like all bindings, `src/types/bindings/`). The
`marketplace-sdk` is **not** taken as a dependency.

**Rationale**: The client in the SDK consists of one file and a `fetch` loop; the types are small. An own
implementation in Rust avoids a second type model and the dependency on the package.

**Alternatives**: SDK for types only (rejected: ties the frontend to a version the host does not use at
all).

## R3 – Where the downloaded bundle lives

**Decision**: In the host's memory, in exactly one slot (`PendingBundle { id, bytes, source, name,
version }`, ≤ 64 MiB). It is freed on: cancelling/closing the preview, successful installation, a new
download, closing the vault, after 10 minutes without access. The webview receives only the `id`.

**Rationale**: "Nothing stays on disk" (spec US1-6) without cleanup logic for temp files, even after a
crash; the bytes never reach the webview (the legacy copied them as `Array.from(Uint8Array)` through the
IPC, which for 24 MiB is a doubling and a trap). 64 MiB is bearable on Android.

**Alternatives**: temp file in `app_cache_dir` (rejected: leftovers after a crash, path handling on
Android); bytes into the webview and back (rejected, see above).

## R4 – How the installation gets the source

**Decision**: `extension_install_preview` and `extension_install` replace `file: PickedFile` with
`source: BundleSource` (`{ kind: "file", file }` or `{ kind: "marketplace", id }`). A function
`read_bundle(source)` in `extensions/registry/install.rs` returns the bytes (file as before via
`read_bundle_file`, otherwise from the slot); after that the same `install_preview` and `install` run.
`InstallDialog.vue` gets `source` and calls the same two commands with it.

**Rationale**: One implementation per rule; there is no second installation routine and no second
dialog. All callers are searched for and changed (see task "every call site", memory
`holzi-every-call-site`): `InstallDialog.vue`, dev loading (`extension_dev_load` stays a file), e2e
scripts and every place with `extension_install`.

**Alternatives**: two new commands `marketplace_install_*` (rejected: duplicates the arguments and the
confirmation logic); an artificial `PickedFile` such as `pending:<id>` (rejected: smuggles a source into
a type that means "file chosen by the user").

## R5 – Timeout, parallelism, progress

**Decision**:

- Catalog: all sources in parallel (`futures::future::join_all`), an overall timeout of **8 s** per
  source (connection and answer). A source that does not finish counts as "timeout", the others do not
  wait any longer. `marketplace_catalog` can be aborted by a newer call (typing a search).
- Download: connection timeout 10 s, **idle timeout 20 s** between two chunks, no fixed overall timeout
  (large files, slow networks); progress through a `Channel<DownloadProgress>` (bytes loaded, total size)
  at most every 100 ms. Abort through `marketplace_download_cancel(id)`.
- Timeout and idle timeout are kept as constants in `mod.rs` and shortened in tests through a clock or an
  adjustable value (no waiting in tests).

**Alternatives**: overall timeout for the download (rejected: aborts legitimate slow downloads).

## R6 – Authentication, redirects, `http`

**Decision** (FR-027):

- One `reqwest::Client` per kind of marketplace call, `redirect::Policy::custom`: redirects are
  **followed only if a scheme downgrade (`https`→`http`) is ruled out** and, as soon as credentials are
  involved, only within the same host and port. Up to 5 redirects. `reqwest` drops `Authorization` on a
  host change anyway; the test checks it nevertheless (no trust in defaults).
- Credentials are sent only to `https://`; the form rejects "Bearer/Basic with `http://`" (FR-027, US5-6)
  and the client does not send them even if the row allows it (second barrier).
- The signed download address and icons are loaded **without** credentials. An `http://` download address
  is allowed only if the marketplace's own address is `http://` (self-hosted on the local network);
  otherwise it is rejected.
- The redirect logic from `extensions/web.rs` is **not** reused: it is tied to the permission check for
  extensions (a question before every redirect). A shared core would be an abstraction without a second
  real user (Rust rule of the constitution).

**Residual risk**: A malicious marketplace could point a download address into an internal network
(SSRF). The answer is only hashed and checked as a bundle, it does not reach the user; no credentials
are sent. Accepted and noted in the contract.

## R7 – Several marketplaces at once: ordering and paging

**Decision**: All enabled sources are asked at the same time. Page N is page N of **every** source (with
up to 20 entries per source), one after the other in rank order. The combined list is explicitly **not**
sorted by downloads again across sources: that would cancel the rank order and, when download counts
change, move entries between sources and pages. Within each source the sort by downloads, descending,
applies. Entries of different sources are **never** merged (FR-012): an entry is identified by
`(sourceId, slug)`, the tile names the source, a "source" filter (`sourceId` in the call) restricts to
one. `totalPages` is the maximum over the sources, `total` the sum. This is stable (no entry disappears
while paging).

**Rationale**: The legacy way (dedupe by `extensionId`, lowest rank wins) lets any self-hosted source
hide the entry of another, because `extensionId` is a claim of the marketplace and cannot be verified.
Without merging, the special rule also disappears; which extension is really installed is decided by the
user in the preview from the publisher key in the signed bundle (Spec 017).

**Alternatives**: dedupe by `extensionId` (legacy, rejected: hiding); merge only for an identical
`publicKey` (rejected: the key appears only in the details, a detail call per entry would be expensive,
the gain small).

## R8 – When is something "installed" and what is an update

**Decision**: The host compares with `extensions::registry::list` (name and, where known, the publisher
key): list entries carry only the name, details additionally `publicKey`. A tile is "installed" if an
installed name equals the entry's name; the preview decides in the end (publisher key plus name, "same
name, other publisher", Spec 017). "Update available" if the **newest compatible** version (`semver`, R9)
is higher than the installed one.

**Rationale**: one place for the rule (host), not in Vue and not once more for the list of installed
extensions. The legacy compared only the name, in the frontend.

**Known limit**: Two publishers with the same name both show "installed" until the details (`publicKey`)
or the preview clarify it; the details show the key and mark the difference.

**Update check** (FR-017): A catalog call sees only a filtered page. Therefore there is an operation of
its own, `marketplace_check_updates`: for **every installed name**, every enabled source is asked with
`GET /extensions?search=<name>&limit=20` (at most 4 requests at a time), and only exact name matches
count. Candidates are kept per `(installedName, sourceId, slug)`. A source updates or removes only its
own candidates; on an error they are kept. A catalog call changes only the source's candidates for
entries that occur on the page. For the installation list the highest known compatible version is chosen,
and on a tie the source with the better rank. The check is started when the app opens and on "Aktualisieren",
never in the background. (The legacy also searched by name for the update, with `limit: 10`, but only on
request per extension.)

## R9 – Compatibility with the HaexVault version

**Decision**: The HaexVault version is the one from `tauri.conf.json`/`Cargo.toml`
(`env!("CARGO_PKG_VERSION")`), as `semver`. `minAppVersion` and `maxAppVersion` of a version bound it (if
one is missing, it does not apply). Versions that do not fit are marked in the catalog and cannot be
installed (FR-018). Requests carry `X-Platform` (`linux`, `windows`, `macos`, `android`, `ios`) and
`X-App-Version`, as the SDK provides; otherwise no trait of the user or device (FR-007).

**Alternatives**: no check, as in the legacy (rejected: the service supplies the fields, Spec 017 knows
manifests with requirements).

## R10 – Preset instead of a special default

**Decision**: There is no built-in marketplace. The **preset** (name and address of the public
marketplace, in a single place in `marketplace/store.rs`, to be set by the operator) is an ordinary row
with the fixed id `preset-public`:

- **New vault**: The row is created once, when the vault gets its initial data (genesis,
  `sync::genesis`; the exact place is task T017). Not on every open.
- **Vault without a marketplace** (created before this feature or everything deleted): the empty state
  of the catalog offers "Öffentlichen Marketplace hinzufügen"; this writes the same row with the same id.
- Delete, switch off, rename, change the address: like any row. Nothing creates it again.

**Rationale**: The fixed id lets two devices that both create it converge on **one** row (no duplicate,
SC-006). Because nothing creates it on open, a late-arriving sync cannot silently switch the marketplace
back on (the legacy case `ensureDefaultMarketplaceAsync`). The address in the code is no special
treatment: the rest of the code does not know it (FR-034), a test searches the source for the address
outside `store.rs`.

**Alternatives**: default as a constant in the code with an override row (earlier state of this plan,
rejected: a special case that could not be deleted and pulls the address into the logic); row in the
migration (rejected: runs per device, same race).

## R11 – Credentials entry

**Decision**: The marketplace has `credentials_item_id` (password entry, no foreign key, because the
entry may arrive later via sync, as in 038/052). `marketplace/credentials.rs` (new) reads the entry as
`Caller::Internal { feature: "marketplace" }` with a fixed grant, returns `present`/`missing`/`syncing`
with the same rules as `remote_storage::credentials::state_in` and builds the header (Bearer: the
password field is the token; Basic: user name and password, `Zeroizing`). Tag `haex-marketplace` (for
newly created entries and when choosing, like 052 FR-006). The entry is reported as in use in
`passwords::usage::EntryUsage` (delete warning, FR-029).

**Reuse**: `CredentialsEntryPicker.vue` gets a property `source: { command, args }` instead of
`protocol` (callers: storage connections as before, marketplace new); **no** second picker is built. The
Rust side shares the helpers (`from_item`-like field mapping) only where the code is the same; a shared
module for "credentials of a service" would be an abstraction over two cases with different owners and
fields (storage: protocols, marketplace: Bearer/Basic) and is drawn only when a third one comes.

## R12 – Icons

**Decision**: The frontend asks `marketplace_icon(source_id, url)`; the host loads (without credentials,
≤ 256 KiB, only `image/png|jpeg|webp`, **no SVG**) and returns a `data:` URL, cached in memory (at most
200 entries). At most 4 icons at the same time. Every destination must be publicly routable, even if it
lies on the configured marketplace origin; this keeps internal marketplace addresses usable for catalog
calls, but their icons are not loaded through this fetch. External URLs must be `https`. The public
storage `api.marketplace.haex.space` remains permitted. Private, reserved, loopback, link-local,
multicast and unspecified addresses are rejected. Every redirect is checked again, `https` → `http` is
forbidden. The connection uses a checked, pinned DNS address (no DNS rebinding). If an icon is missing or
invalid, the tile shows a default icon.

**Rationale**: FR-006 (the webview calls no marketplace address; the CSP stays closed). SVG is left out
so that no active content lands in `<img>` (even though `<img>` does not run scripts). The public-address
check limits SSRF to public image servers.

## R13 – Agents and extensions do not see the catalog

**Decision**: The commands live only in the Tauri handler of the main window. They are **not** entered in
the registry of the agent actions (`*ActionHandlers`, Spec 020/021), not in the MCP server and not in the
checkpoint of the extensions (`extensions/bridge/dispatch.rs`). A test counts: no action, no MCP tool and
no host method carries the name `marketplace*` (FR-030); it fails when someone adds one and forces a
deliberate decision.

## R14 – Live data and sync

**Decision**: `haex_marketplaces` is an ordinary CRDT table (`HOLZI_TRIGGER_VERSION` + 1, entry in the
list of synced tables). The settings view subscribes to changes through the existing
`vault-data-changed`/`onVaultTablesChanged` (Spec 028 live data); on/off and rank write immediately on
tap (Spec 023). No UNIQUE (a conflict would halt the sync, pattern of the other tables); sort gaps and
equal ranks are resolved stably on read with `(rank, name, id)`.

## R15 – e2e and test marketplace

**Decision**: The rig (Spec 016) gets a small Node server that serves catalog, categories, details and
download for two extensions, one of them with the existing signed test bundle (`haextension-test-bundles`,
throw-away key). The address of the preset can be overridden in the e2e build
(`HOLZI_E2E_MARKETPLACE_URL`, only in e2e builds, like the other switches of the rig; the exact place is
settled by task T003 in `tasks.md`). In the release build there is no environment switch.

## R17 – Comparison with haex-vault 2.x (legacy)

Read in haex-vault @ `8dce379d`: `useMarketplaces.ts`, `useMarketplaceCatalog.ts`,
`useExtensionUpdate.ts`, `system/marketplace*.vue`, `settings/extensions/marketplaces.vue`,
`MarketplaceEditDialog.vue`, migration `0000_jazzy_chat.sql` (table) and `useMarketplaces.test.ts`.

**Adopted**

- Several sources in parallel with an error per source (`Promise.allSettled`, `sourceErrors`); catalog,
  search, category, page, sort by downloads, page size 20.
- The download goes through the source that supplied the entry (`sourceMarketplaceId`), never through
  "any".
- Settings: list with an on/off switch per row, menu edit/delete, form for creating, auth types none,
  Bearer, Basic; rank as priority (smaller = earlier).
- After install, update and remove the catalog refreshes (legacy fix "refresh catalog after extension
  update"): tile states follow `vault-data-changed` (ui.md).
- Choose a version in the details; an update keeps the data, a reinstall is a confirmation of its own
  (in holzi already part of the preview, Spec 017).

**Deliberately different**

| Legacy                                                        | holzi                                                                                       | Reason                                              |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| Dedupe by `extensionId`, first source wins (test `dedupes…`)  | No merging, every tile with its source (R7)                                                 | a source must not hide another (self-hosted)        |
| Token/password as columns of the table                        | Reference to a password entry (R11)                                                         | no secrets in vault tables, sync and logs           |
| `is_default` column, default not deletable, row on open       | ordinary row `preset-public` (R10)                                                          | generic configuration, no sync race                 |
| Network in the webview via `plugin-http` and SDK              | Host (Rust, `reqwest`)                                                                      | one HTTP path, redirect and auth rules in one place |
| `bundleHash` is passed, the check is a `TODO`                 | Checksum and size are mandatory (FR-013)                                                    | a third party supplies the file                     |
| Bytes as `Array.from(Uint8Array)` through the IPC and back    | Bytes stay in the host (R3)                                                                 | 24 MiB bundles, no doubling, nothing in the webview |
| Update: search by name (`limit: 10`) on request per extension | Check of all installed names on app open and on request, candidates per source (R8, FR-017) | known updates also outside the current catalog page |
| newest version = `versions[0]`                                | `semver` (R1)                                                                               | the order is not guaranteed                         |
| "installed" by name only, in the frontend                     | Host, name plus publisher key in details and preview (R8)                                   | one place for the rule                              |
| DID authentication                                            | later                                                                                       | operator decision, 2026-10-28                       |
| Form with "Erstellen/Speichern" without a test                | "Testen und speichern" (FR-021)                                                             | show errors early; pattern from Spec 038            |

## R16 – Open points outside the spec

- DID authentication (legacy) and writing reviews/ratings: later, not part of 069.
- Automatic updates and background queries: deliberately not (FR-017).
- An ADR is not necessary; should it turn out during implementation that a core principle is touched, it
  will be submitted afterwards.
