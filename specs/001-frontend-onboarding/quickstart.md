# Quickstart: Frontend Onboarding

**Goal (post-implementation)**: after the frontend scaffold, `haex-crdt` extraction, and the implementation tasks are complete, reach a running Tauri window from a fresh clone of `holzi` that renders the landing page with the three primary CTAs (Anlegen, Öffnen, Verbinden) and the "Zuletzt verwendet" list, in under 5 minutes on a warm machine.

**V1 contract note**: Genesis creates a fresh vault identity without a
paper-seed display or confirmation step. Imported databases open directly:
`DatabaseBootstrap` reuses or inserts a `known_devices` row keyed by the local
installation UUID and returns the vault-device UUID for HLC; no rekey, no
attestation, no restore-pairing handshake.

**Prerequisites** on the host:

- Node.js LTS + `pnpm` in `PATH`.
- Rust toolchain matching `rust-toolchain.toml` (once added — the extraction task provides this).
- Tauri build prerequisites for the host OS (Linux: `webkit2gtk-4.1`; macOS: Xcode CLI Tools; Windows: MSVC + WebView2).
- `haex-crdt` available as the git dependency pinned in [`plan.md`](./plan.md); do not use a workspace-path override.

## Steps

1. **Clone and install dependencies**.

   ```bash
   git clone git@github.com:haexmas/holzi.git
   cd holzi
   pnpm install
   ```

2. **Verify Nuxt + Tailwind + shadcn-vue setup**.

   ```bash
   pnpm dev
   ```

   Open the dev URL (`http://localhost:3030`, pinned in `nuxt.config.ts` and `src-tauri/tauri.conf.json`) — you should see the landing shell, three CTA buttons (disabled or empty labels until i18n keys are filled), and the version footer.

3. **Run the Tauri app**.

   ```bash
   pnpm tauri dev
   ```

   A native window opens with the same page. Verify:

   - No network requests to `api.iconify.design` or `fonts.googleapis.com` (open DevTools → Network, refresh).
   - No console errors.
   - "Zuletzt verwendet" section is hidden (empty `instances/` directory).

4. **Create a first instance (Anlegen — Genesis)**.

   - Click **Anlegen**.
   - Choose "Neue Federation".
   - Name: `test-01` (any alphanumeric name).
   - Passphrase: any string meeting the min-length policy, entered twice.
   - Submit.
   - Fresh Nostr and iroh identities are generated inside the encrypted database.
   - App navigates to `/federation/test-01` (placeholder page for this spec).

   Verify on disk:

   ```bash
   ls -la ~/.local/share/holzi/instances/    # Linux path — adjust per OS
   # Expect: test-01.db
   ```

5. **Return to landing and unlock**.

   - Navigate back to `/` (a "Zur Übersicht"-button will be added in a later spec; for now use the browser back button in `pnpm dev`, or restart `pnpm tauri dev`).
   - The instance `test-01` appears in "Zuletzt verwendet".
   - Click it → Unlock sheet opens → enter the passphrase → arrive at `/federation/test-01`.

6. **Import an external `.db` (Öffnen)**.

   - Close `test-01` cleanly before taking the copy, so committed data is not left in an active WAL. Copy `~/.local/share/holzi/instances/test-01.db` to `/tmp/other.db`.
   - In the app, click **Öffnen**.
   - File picker opens; select `/tmp/other.db`.
   - Import succeeds; `other.db` appears in the list. The source at `/tmp/other.db` is untouched.

7. **Unlock the imported copy**.

   - Select `other.db` and enter the original passphrase.
   - Verify the app opens `/federation/other` directly — no restore-pairing prompt, no token, no reachable parent required.
   - Because this copy was made on the same installation, verify (via a quick SQL peek or a debug log) that `known_devices` reuses the existing local-installation row and its vault-device UUID. Close and reopen — the same row is reused; no new one is inserted.
   - To verify minting for a different installation, repeat Steps 6–7 with a second `XDG_DATA_HOME` (or another machine). That installation has a different `installation-id`, so its copied database gets a fresh vault-device UUID while the source remains untouched.

8. **Verbinden and two-device flows.** Verbinden is the explicit QR/token pairing path; the direct-copy check above is independent of it. Use the second-install setup to exercise multi-replica behaviour — you should get an independent vault-device UUID on the other side.

## Common failure modes

- **Blank window in Tauri**: Nuxt dev server not yet up. Wait for `pnpm dev` in another terminal, or use `pnpm tauri dev` which starts both.
- **`instances/` not created**: backend calls `create_dir_all` on first path resolution — check the app has write permission to `AppLocalData`.
- **Unlock error after a correct passphrase**: use the specific message shown by the unlock dialog. A missing file, unreadable storage, or migration/compatibility problem is reported separately; verify the `haex-crdt` version and inspect the vault copy/synchronization state before retrying.
- **Icons render as boxes**: `@iconify-json/lucide` not installed, or `nuxt.config.ts` icon settings not committed. Re-run `pnpm install` and restart dev server.

## Definition of "onboarding-complete"

This spec is done when:

- A fresh clone reaches step 5 (create + unlock loop) without deviations.
- Steps 6–7 import a same-installation copy and confirm local-row reuse; the second-installation variant confirms a fresh `known_devices` row with a distinct vault-device UUID.
- All E2E tests in `e2e/onboarding.spec.ts` pass.
- Playwright network-assertion test (T066) passes with zero external requests.
