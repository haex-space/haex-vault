# Quickstart: Checking the Marketplace

Shows that the feature holds up from start to finish. Prerequisites: the worktree of this spec, the
nix devshell (`cargo` only there, memory `holzi-cargo-in-worktrees`), a test marketplace (below).

## Automated

Always build in a targeted way (`-j 4`, only the affected target), never the whole tree:

```sh
nix develop --command cargo test -j 4 -p holzi marketplace::                  # core, download, credentials
nix develop --command cargo test -j 4 -p holzi extensions::registry::install  # source of the installation
pnpm lint:rust                                                                # Clippy with -D warnings
pnpm typecheck && pnpm lint && pnpm check:templates && pnpm format:check
```

The logic (merging, versions, compatibility, credentials) sits in the host; the frontend has no unit
tests of its own, but is covered by type check, lint and e2e.

E2E (spec 016; desktop with Xvfb, Android in the emulator `holzi-emulator`):

```sh
nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e --grep marketplace
```

The scenarios are called `marketplace-*.test.ts`; `marketplace-two-devices.test.ts` covers SC-007, the
others SC-008.

## Test marketplace

The rig starts it itself (`scripts/e2e/lib/marketplace.ts`, R15). By hand: a Node server that serves
`/health`, `/categories`, `/extensions`, `/extensions/{slug}`, `/extensions/{slug}/download` according to
[contracts/marketplace-api.md](contracts/marketplace-api.md), with a signed throw-away bundle
(`haextension-test-bundles`). Credentials for the private case are random values at start-up.

## Manual (check points, T numbers follow in `tasks.md`)

| #   | Step                                                                                                                                                                | Expectation                                                                                                                                                                                                                         |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M1  | New vault, open the app "Erweiterungen entdecken"                                                                                                                   | Catalog of the preset visible, categories, no setup needed (SC-006)                                                                                                                                                                 |
| M2  | Search, choose a category, page                                                                                                                                     | List follows after a short pause, page jumps to 1 (US1-2, -3, -4)                                                                                                                                                                   |
| M3  | "Installieren" for a small, signed extension from the test marketplace                                                                                              | Progress, then preview with publisher, permissions and source; after "Installieren" it runs                                                                                                                                         |
| M4  | Cancel the preview                                                                                                                                                  | nothing installed, tile unchanged                                                                                                                                                                                                   |
| M5  | Choose an older version while the newer one is installed                                                                                                            | Downgrade confirmation                                                                                                                                                                                                              |
| M6  | Test marketplace with wrong checksum / without checksum / file over 64 MiB                                                                                          | Abort with message, nothing installed (US2)                                                                                                                                                                                         |
| M7  | Create a second marketplace (also with a path part), switch off, switch on, move up, delete; delete the preset                                                      | Catalog follows at once, both sources simultaneously, every tile names its source; the preset does not come back, the empty state offers it (US4)                                                                                   |
| M8  | Marketplace with Bearer token and a chosen password entry; delete the entry in the password manager                                                                 | Warning before deleting names the marketplace; source shows "Zugangsdaten fehlen" (US5)                                                                                                                                             |
| M9  | Address `http://` with login                                                                                                                                        | Saving is refused (US5-6)                                                                                                                                                                                                           |
| M10 | Disconnect the network                                                                                                                                              | "Keine Verbindung", installing from a file stays usable                                                                                                                                                                             |
| M11 | Two devices: marketplace on A, then B; switch the preset off on A                                                                                                   | appears on B (SC-007); the switch stays off on B                                                                                                                                                                                    |
| M14 | Two test marketplaces with the same extension (different publisher)                                                                                                 | two tiles with source; the preview names the publisher key from the bundle (SC-009)                                                                                                                                                 |
| M15 | Install an extension whose newer version the test marketplace carries; search for something else in the catalog; open Settings → Extensions                         | the row shows "Update verfügbar" although the extension is not on the displayed catalog page (US3-7)                                                                                                                                |
| M12 | Android: catalog, install, back gesture                                                                                                                             | Grid in one column, installation runs; back closes the preview first                                                                                                                                                                |
| M13 | Windows and macOS once by hand                                                                                                                                      | as M1 to M4                                                                                                                                                                                                                         |
| M16 | For every extension in the public marketplace, download the newest version (`GET /extensions/{slug}/download`) and run `haex verify <file>` and the install preview | all nine pass the spec 017 preview and can be installed; six (`haex-tetris`, `haex-image`, `haex-game-unicorn`, `haex-draw`, `haex-code`, `haex-calendar`) were in the old format on 2026-10-29 and must be republished before PR C |

## Success measures in view

SC-001 (under a minute), SC-002 and SC-004/SC-005 can be automated and belong in the Rust tests;
SC-006, SC-007, SC-008 in e2e; the M points cover UI and platforms.
