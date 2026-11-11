# Quickstart: checking wallet passes

Run from the worktree of the branch. Rust through the nix shell and the host bridge, as in the other specs
(`nix develop --command scripts/with-nix-host-bridge.sh ...`). Build only the test targets you need.

## Gates per slice

```sh
pnpm generate:ts-types                      # after any change to a ts-rs type
pnpm check:passwords                        # node --test scripts/check-passwords-*.ts
pnpm check:templates && pnpm typecheck && pnpm lint && pnpm format:check
pnpm lint:rust                              # clippy, both feature sets
cargo test --manifest-path src-tauri/Cargo.toml --lib passwords::   # targeted; add -j 4
cargo test --manifest-path src-tauri/Cargo.toml --lib identity::migrations_passwords_pass
cargo test --manifest-path src-tauri/Cargo.toml --test vault_upgrade
cargo test --manifest-path src-tauri/Cargo.toml --test passwords_sync
```

## Slice A: foundation (US1)

1. Open the frozen vault of `src-tauri/tests/pre_rename_vault.rs` (`cargo test --test pre_rename_vault`): every old entry is a
   `login`, and every field, tag, folder, attachment and history stand reads as before (SC-003).
2. `cargo test --test vault_upgrade`: a vault built without `0061` reopens with the column, the two tables and the trigger
   covering `kind` (research R1).
3. Seed a pass in a test vault; run the credential lookups of `contracts/external-callers.md`: 0 passes (SC-005); run every read
   path with a seeded code marker: the marker is nowhere (SC-004).
4. `cargo test --lib passwords::snapshots`: a stand without version reads as 1, a version-2 stand with a pass block round-trips,
   restore does not move or re-link.
5. A newer-build entry (unknown `kind`) in a test vault shows the "needs a newer holzi" notice and cannot be edited.

## Slice B: pass core (US2, US4)

1. `cargo test --lib passwords::pass::codes`: for each of the five formats a code is drawn, rasterised and decoded back to the
   stored value (SC-002); a wrong EAN-13 check digit is refused by hand and kept flagged from import (FR-003).
2. `node --test scripts/check-passwords-pass-date.ts`: the start of a London concert at 20:00 shows 20:00 and, on a Berlin
   device, 21:00 labelled; on a London device only one; the days around the 2026 clock changes (SC-008).
3. `node --test scripts/check-passwords-sort-filter.ts`: twenty tickets of three providers and five loyalty cards: filter
   type "pass" then kind "ticket", sort by date gives the nearest upcoming first, then past (most recent first), then undated;
   hide expired leaves valid ones; a search word and the filter together (SC-009).
4. `node --test scripts/check-passwords-search.ts`: a ticket in `Tickets/Deutsche Bahn` is found by "bahn"; after a rename the
   new name finds it at once; a root entry has no folder name.
5. App: `pnpm tauri:dev` (not from a worktree another dev app runs from), unlock, "New" shows login, card/ticket, folder; create a
   pass with a QR and a Code 128, open it, "Show large" fills the screen dark-on-white; leave and the brightness returns.
6. E2E: `scripts/with-nix-host-bridge.sh pnpm test:e2e --grep passwords-pass-basic`.

## Slice C: linking (US3)

1. `node --test scripts/check-passwords-links.ts`: self and descendant links are refused; a loop made by two devices is found with
   a visited set and every pass on it is flagged; states "(in trash)" and "(not available)".
2. `cargo test --test passwords_sync`: two replicas link in opposite directions and converge to a flagged loop; purge of a
   parent clears the link on both (SC-007).
3. App: link two vouchers with the same free fields to one card: two separate rows in "Linked"; trash the card: both stay with
   "(in trash)"; restore: normal; delete for good: the links are gone and the vouchers stay.

## Slice D: pkpass import (US6)

1. `cargo test --lib passwords::pass::pkpass`: one generated file per style (boarding pass, event ticket, store card, coupon,
   generic) gives the right kind, dates, codes and fields; a ZIP with `..` paths, too many files or an inflated entry is refused
   with the reason and leaves nothing behind (SC-006).
2. `cargo test --lib passwords::pass::place`: ticket-like with a date gives `Tickets/<provider>` and the tag `Ticket`;
   everything else the root; an existing folder of another case is reused.
3. App: import a file twice: the second asks; turn on "Check signature": the result shows before saving and the import still
   works when it is invalid; the report names what was not kept and holds no code.
4. The network is off during these steps; nothing calls the pass's update address (FR-029).

## Slice E: capture (US7)

1. `cargo test --lib passwords::pass::codes::scan`: a photo with two codes yields both; the user picks one.
2. Desktop: scan a printed code of each format by the native Linux camera (spec 085 path); Windows and macOS stay "untested"
   until a device is at hand (FR-036 notes the limit).
3. A PDF page rendered by `pdfjs-dist` is decoded; a PDF whose code is not found says so and offers photo or typing.

## Slice F: Android (US8)

1. Emulator: the intents of `contracts/android-intents.md`, vault locked and unlocked.
2. Phone (manual): brightness and keep-awake behaviour, camera formats, pinned shortcut label.

## Always

- 360 px width: every new view without horizontal scrolling (SC-010).
- German and English texts exist for every new key; no `<` and `>` in a message (the build refuses it).
- The build used for e2e is `nuxt generate`, which catches i18n problems that `typecheck` does not.
