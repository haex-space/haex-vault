# Research: Frontend Onboarding

**Status**: Draft. Captures the rationale behind decisions in [`spec.md`](./spec.md) and [`plan.md`](./plan.md). Written alongside the spec so future readers can see what was considered and why.

## Origin

The frontend stack + landing flow was refined during a brainstorming session on 2026-09-04 (see this repo's `.claude/` transcript if retained). Two prior documents set the surrounding scope:

- [`docs/design/founding.md`](../../docs/design/founding.md) — full architecture (identity, transport planes, MCP).
- [`docs/plans/2026-09-04-v1-scope-design.md`](../../docs/plans/2026-09-04-v1-scope-design.md) — v1 scope line + identity/storage revisions.

This spec is the first surface-level feature spec; it consumes decisions from both.

## UI-Framework decision: shadcn-vue vs `@nuxt/ui`

`haex-vault 2.x (Legacy)` uses `@nuxt/ui` v4 (`package.json` deps, `UApp`, `UButton`, `UDrawer` throughout `src/`). The natural default for "orientate holzi at haex-vault 2.x UI/UX" would be `@nuxt/ui` — same component set, near-zero Look/Feel work, tight Tailwind v4 integration.

The operator chose **shadcn-vue** instead. Consequences accepted:

- Look/Feel of haex-vault 2.x is a _target_, not a _derivation_. Achieved by borrowing haex-vault 2.x's layout structure, spacing, and copy hierarchy, but with shadcn-vue's own visual defaults as the starting palette.
- Component maintenance: shadcn-vue is copy-in, so every added component is source we own and update deliberately. `@nuxt/ui` upgrades via `pnpm update`.
- More design control up front, more design work up front. Acceptable because holzi's surface is smaller than haex-vault 2.x's, so the upfront cost is bounded.

## Storage-file model: multi-file, single-active

`founding.md` §2.2 says "device equals relay equals Tauri application" — read strictly, that means one install holds exactly one federation state. During brainstorming this was refined to:

> **The active SQLite DB equals the running Nostr relay equals the running iroh peer equals the active instance.** The Tauri app is a container that may hold multiple `.db` files on disk, with exactly one active at a time.

Rationale:

- Portability. `.db` files are the natural export/backup unit (SQLCipher single-file), and holzi has no other export mechanism (nor needs one). Being able to hold several files without switching installs makes migration between machines trivial.
- Symmetric with `haex-vault 2.x`, which manages a `vaults/` directory the same way.
- Cost: two DBs open simultaneously would collide on relay ports and iroh endpoints. Enforcing single-active in `AppState` prevents that structurally.

This revision does not touch `founding.md`'s trust boundaries — the _active_ instance still binds one identity to one relay to one iroh peer.

## File-handling: haex-vault 2.x pattern verbatim

Studied [`haex-vault 2.x src-tauri/src/database/paths.rs`](../../../../haex-vault-legacy/src-tauri/src/database/paths.rs) and [`haex-vault 2.x src/stores/vault/lastVaults.ts`](../../../../haex-vault-legacy/src/stores/vault/lastVaults.ts). Adopted decisions:

- Files under `<AppLocalData>/instances/`. Backend resolves paths; frontend passes only names.
- Rust ensures the directory exists on every path resolution (haex-vault 2.x does this — cheap and idempotent).
- List is a directory scan, not a persisted JSON. This removes an entire class of drift bugs between persisted list metadata and actual files. Changes outside holzi are unsupported in v1 and become visible only on the next scan; in-scope imports go through the backend command and trigger a scan through `instance-list-changed`.
- Backend emits `instance-list-changed` on every mutation; frontend Pinia store subscribes.
- Soft-delete (`.trash/` subdirectory) is the only destructive path exposed in v1.

Departures from haex-vault 2.x:

- No `Import` component that reads a `.kdbx`-style file — holzi's "Öffnen" is the closest analog and always copies (never imports/converts).
- No `Connect` component talking to a sync backend — holzi's "Verbinden" is peer-pairing.
- No "recent" list that differs from "all instances" — the two are the same list, sorted by last-access.
- The backend refreshes the database mtime after each successful unlock; that mtime is the persisted `lastAccess` value returned by `list_instances`.

## QR scanner: single-stack `html5-qrcode` on every platform

QR and text entry carry the same opaque pairing-token string. The scanner uses `html5-qrcode` over the standard `navigator.mediaDevices.getUserMedia()` Web API on every platform Tauri targets — desktop (Linux/macOS/Windows) and mobile (Android/iOS). Rationale:

- **Consistency with haex-vault 2.x.** haex-vault 2.x ships `html5-qrcode` and targets the same platform matrix; matching keeps knowledge, patched versions, and issue-space shared.
- **No desktop path for `@tauri-apps/plugin-barcode-scanner`.** That plugin is mobile-only (MLKit on Android, Vision on iOS). Adopting it as the primary would force a dual-stack solution — two libraries, doubled tests, and platform-branching in `ConnectSheet` — for a QR-decoded string that is byte-identical on either path.
- **`getUserMedia` works in Tauri WebViews.** Android's system WebView (Chromium ≥ 43) supports it once the app has requested and received the OS `CAMERA` runtime grant and the WebView's `PermissionRequest` handler allows only `RESOURCE_VIDEO_CAPTURE`; iOS's WKWebView supports it from iOS 14.3 given `NSCameraUsageDescription`. The Android manifest declaration alone is insufficient because the runtime grant and WebView permission bridge are also required.

A single idempotent cleanup path awaits `stop()` before clearing the canvas on every exit path: successful decode, Sheet close, component unmount, and startup failure. If startup never reached the running state, the failed `stop()` is tolerated and cleanup completes without throwing.

Permission denial or unavailable hardware never blocks pairing because the text field remains available on every platform.

**Contingency plan.** If a concrete Tauri-WebView `getUserMedia` regression on a target OS/version emerges in E2E or field use and cannot be resolved via manifest/permission fixes, add `@tauri-apps/plugin-barcode-scanner` behind a runtime feature detect as a mobile fallback — not a rewrite, just a wrapping call in `ConnectSheet`'s scan-start path. This is not v1 work.

References: [Html5Qrcode lifecycle](https://scanapp.org/html5-qrcode-docs/docs/apis/classes/Html5Qrcode).

## `@nuxt/icon` local-bundle configuration

`@nuxt/icon`'s default `server` mode fetches icons from `api.iconify.design` at runtime. That is a hard non-starter for holzi: SC-006 requires zero external CDN traffic, and the app must be usable fully offline.

Chosen configuration:

```ts
icon: {
  mode: 'svg',
  serverBundle: false,
  clientBundle: { scan: true, includeCustomCollections: true },
}
```

With `@iconify-json/lucide` installed as a dependency, `@nuxt/icon`'s scan-mode reads all `<Icon name="lucide:xxx" />` occurrences at build time and inlines the SVG data from `node_modules/@iconify-json/lucide/icons.json`. No runtime request is made. A Playwright test asserts this (T066).

## Component auto-import with `pathPrefix: true`

Chosen (per operator direction) over explicit imports. Trade-offs:

- Pro: no import boilerplate in templates; renaming a component is one file operation.
- Pro: matches Nuxt community convention; readable for anyone familiar with Nuxt.
- Con: IDE navigation ("go to definition") is slightly worse; component discovery relies on the auto-import registry being reflected in the TS server.
- Con: name collisions require directory renames, not import-alias tweaks.

Net: the operator has weighed this and chosen auto-import. Documenting so future contributors don't relitigate.

## No copy-to-clipboard for paper-seed (Historical — superseded for v1)

This decision belongs to the superseded paper-seed onboarding design. V1 Genesis
has no paper-seed display or confirmation step; backup recovery opens the
copied database directly, without rekey or restore pairing. The direct-copy
flow authenticates with the copied vault identity and reuses or mints the local
`known_devices` row during pre-HLC bootstrap.

Paper-seed is displayed for the operator to _record physically_. Copy-to-clipboard would:

- Leave the seed in the clipboard history and any clipboard manager.
- Enable a silent screen-capture-based exfiltration if the host is compromised.
- The old design's "sole federation-scope recovery secret" property from `v1-scope-design.md §4` was superseded with that model.

Cost: manual transcription is slower. Acceptable for a one-time genesis step. If operators push back post-v1, a "reveal for 30s and clear clipboard" flow is one option.

## What is intentionally NOT in this spec

- Federation view (`/federation/<name>` beyond a placeholder page). Own spec.
- LLM chat UI, MCP surfaces, per-device policy screens. Own specs.
- Biometric unlock (`@choochmeque/tauri-plugin-biometry-api` in haex-vault 2.x). Post-v1.
- Instance rename / merge / export. Post-v1; export is intentionally absent (see above).
- List removal while retaining the file. The v1 list is a directory scan with no exclusion metadata, so the only removal action is moving an instance to `.trash`.
- Filesystem watcher on `<AppLocalData>/instances/`. All v1 mutations go through Tauri commands that emit `instance-list-changed`; no CLI, share-intent bypass, or third-party file-manager path is in scope. If one is later added, a watcher can reuse the event topic and payload envelope, with its producer and reason added explicitly to the event contract.
- Tour / onboarding walkthrough (`driver.js` in haex-vault 2.x). Post-v1.
- Font choice + hosting. Assumed system-font stack for v1; if we later add a custom font, it MUST ship locally (same SC-006 rule).
