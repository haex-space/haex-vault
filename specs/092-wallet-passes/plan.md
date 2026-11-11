# Implementation Plan: Cards and tickets in the password manager (wallet passes)

**Branch**: `092-wallet-passes` | **Date**: 2026-11-12 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/092-wallet-passes/spec.md`

## Summary

The password manager gets a second entry type, the **pass** (kind of pass, start, parent, codes), next to the login. The two
share the identity table `haex_passwords_item_details`, which gains a `kind` column (`login` default); the pass-only fields sit
in a 1:1 table `haex_passwords_pass_details` and a codes table `haex_passwords_pass_barcodes`. Folders, tags, trash, history,
attachments, grants and sync keep working through the existing item id.

The work falls into one foundation and five slices that follow the user stories:

1. **Foundation** (US1): one migration (`0061`), the type on every header, a login-only default at every outside reader
   (autofill, bridge, agent, MCP, extensions, passkeys, remote-storage credentials, drive unlock, import duplicates),
   history format version 2, and the hand-kept lists of satellite tables (purge, copy). Nothing visible changes for logins.
2. **Pass core** (US2, US4): the pass service, codes (validate, draw, decode in one Rust module on `rxing`), the pass view
   and editor, "New", list icons, filters by type and kind, sort by date, hide expired, search over folder names.
3. **Linking** (US3): the parent link, cycle refusal and repair, "Belongs to", "Linked", "(in trash)", "(not available)".
4. **Import** (US6): a pkpass reader and a per-pass dialog (place proposal, link, duplicate check, optional signature check).
5. **Capture** (US7): camera, photo and PDF to code.
6. **Android** (US8): brightness and keep-awake, the share target and the pinned shortcut, both through the existing
   park-until-unlock inbox.

US5 (history, attachments) is covered by the foundation (version 2) and the pass core (the history view of a pass).

The decisions and their alternatives are in [research.md](research.md); tables, columns and state in
[data-model.md](data-model.md); the interfaces in [contracts/](contracts/); the checks to run in
[quickstart.md](quickstart.md).

## Technical Context

**Language/Version**: Rust 2021 (Tauri 2 backend, `holzi_lib`), TypeScript with Nuxt 3 / Vue 3 / Pinia (frontend), Kotlin
(Android plugin `src-tauri/plugins/holzi-android`).

**Primary Dependencies**: existing: `zip` 8 (pkpass), `image` 0.25, `rqrr`/`qrcode` (link QR, unchanged), `pdfjs-dist` (PDF
pages), `jsqr`, `@tauri-apps/plugin-barcode-scanner`, Fuse.js, ts-rs. New: `rxing` (encode, decode and check digits for QR,
Code 128, EAN-13, PDF417 and Aztec, one implementation, R3), `cms` and `x509-cert` (RustCrypto, pkpass signature check,
only in the slice that offers it, R6).

**Storage**: the vault database (SQLCipher, CRDT-tracked tables), migration `0061_passwords_pass`; no new file store. Temporary
files of import and capture live in the vault's own temporary space (spec 084).

**Testing**: Rust `*_tests.rs` next to the modules and `src-tauri/tests/` integration tests (migration upgrade, two-replica
sync, access); frontend logic as pure modules under `src/lib/passwords/` with `node --test scripts/check-passwords-*.ts`;
e2e scenarios in `scripts/e2e/scenarios/`; the Android parts on the emulator where it can and manual steps otherwise.

**Target Platform**: Linux, Windows, macOS and Android (iOS is not on the roadmap).

**Project Type**: desktop and mobile app (Tauri), one repository.

**Performance Goals**: opening a pass and drawing its code is immediate (no network, a code draws in well under 100 ms);
the list stays smooth with 1 000 entries (filter and sort are linear passes over headers already in memory).

**Constraints**: offline everywhere; no code in any list, header, log or error (FR-025, SC-004); 360 px width; German and
English; no `<…>` in a vue-i18n message; no UNIQUE constraints and no `WITHOUT ROWID` (034 FR-037); one implementation per
rule across Rust and TypeScript (memory: one-implementation-per-rule).

**Scale/Scope**: a user holds tens to a few hundred passes; one account may carry hundreds of linked tickets (edge case).

## Constitution Check

_Gate: passes before Phase 0 and again after Phase 1._

| Principle                                         | Check                                                                                                                                                                                                                                 |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. No secrets in git                              | The Apple root and intermediate certificates bundled for the signature check are public certificates, not key material. Test pkpass files are built in the tests with throwaway keys; no real pass, code or signing key is committed. |
| II. No local absolute paths                       | None added. Plan and tasks use repository-relative paths only.                                                                                                                                                                        |
| III. Identity is device-independent               | Not touched.                                                                                                                                                                                                                          |
| IV. Cross-repo references pin revisions           | No external harness content is referenced. The new crates are pinned by `Cargo.lock`.                                                                                                                                                 |
| V. External sources opt-in                        | `.haex-hive.json` is not changed.                                                                                                                                                                                                     |
| VI. Self-modifying instructions review-gated      | No constitution, skill or permission file is changed; the amendments of spec 034 go through this pull request.                                                                                                                        |
| VII. Relay unavailability never blocks local work | Everything is local; the pkpass import and the signature check never use the network (FR-029, FR-030).                                                                                                                                |
| VIII. No concealment instructions                 | None.                                                                                                                                                                                                                                 |
| Workflow                                          | Work lands through pull requests from topic branches (stacked, one per slice), rebase- or merge-commit, Conventional Commits. An ADR records the entry-type decision (task T001).                                                     |

**Post-design re-check**: the data model adds three tracked tables/columns and one trigger-version bump, all through one
migration; no principle is touched. The only new bundled data are public certificates. No violation; the Complexity table is
empty.

## Project Structure

### Documentation (this feature)

```text
specs/092-wallet-passes/
├── plan.md              # This file
├── research.md          # Phase 0: decisions R1 to R16
├── data-model.md        # Phase 1: tables, columns, header and snapshot shapes, state
├── quickstart.md        # Phase 1: how to check each slice
├── contracts/
│   ├── tauri-commands.md      # commands the user interface calls
│   ├── external-callers.md    # what agents, MCP, extensions, bridge and autofill see
│   └── android-intents.md     # share target, shortcut, brightness commands
├── checklists/requirements.md
└── tasks.md             # created by /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── identity/
│   ├── migrations_passwords_pass.rs         # new: 0061 (kind column, pass_details, pass_barcodes)
│   ├── migrations_passwords_pass_tests.rs   # new
│   └── migrations.rs, mod.rs                # register 0061, HOLZI_TRIGGER_VERSION 39 -> 40
├── passwords/
│   ├── model.rs, items.rs, access.rs        # EntryType, kind on headers, login-only default
│   ├── snapshots.rs                         # SnapshotData version 2, default for missing version stays 1
│   ├── trash.rs, copy.rs                    # satellite lists gain the two tables
│   ├── pass/                                # new module
│   │   ├── mod.rs, model.rs                 # PassInput, PassDetail, PassHeader fields, Code
│   │   ├── codes.rs                         # validate, draw (SVG) and decode on rxing
│   │   ├── parent.rs                        # cycle check (recursive query), clear on delete
│   │   ├── pkpass.rs                        # read a pkpass (zip, pass.json, images), limits
│   │   ├── signature.rs                     # optional offline signature check
│   │   ├── place.rs                         # folder and tag proposal rule (FR-019)
│   │   └── *_tests.rs
│   ├── service/pass.rs, commands/pass.rs    # service and Tauri commands for passes
│   └── service/items.rs, by_site.rs, ...    # login-only filters at the outside readers
├── agent_actions / extensions/passwords.rs / browser_bridge/methods/{items,passkeys}.rs
├── agent_browser/{act_credentials,act_entry_fill}.rs, remote_storage/credentials.rs,
│   files/local/drives/unlock.rs             # login-only
├── deep_link/                               # pass shortcut link, shared-file inbox (Android and desktop)
src-tauri/plugins/holzi-android/             # brightness, keep-awake, share intent, pinned shortcut (Kotlin + Rust)
src-tauri/gen/android/app/src/main/AndroidManifest.xml, MainActivity.kt   # intent filters, forwarding
src-tauri/tests/                             # passwords_pass_*.rs, vault_upgrade.rs, passwords_sync.rs, pre_rename_vault.rs

src/
├── components/passwords/
│   ├── PassView.vue, PassEditor.vue, PassDetails.vue, PassCodes.vue, PassCodeLarge.vue
│   ├── PassLinks.vue (Belongs to, Linked), PassParentPicker.vue
│   ├── PassImportDialog.vue, PassCapture.vue, ListFilters.vue
│   └── EntryPage.vue, List.vue, ListItem.vue, Toolbar.vue, ReferencePicker.vue, HistorySnapshot.vue   # type-aware
├── lib/passwords/
│   ├── passType.ts, passDate.ts (zone display), sortFilter.ts (date sort, type, kind, hide expired),
│   │   links.ts (cycle detection, parent state), search.ts (folder names), registry.ts (new location + query key)
├── stores/passwordsListView.ts              # sort and filter state while the vault is open
├── composables/usePasses.ts, useScreenBright.ts, useSharedFile.ts
├── i18n/locales/{de,en}.json
scripts/check-passwords-{pass-date,sort-filter,links,search,routes}.ts, scripts/e2e/scenarios/passwords-pass-*.test.ts
docs/adr/0025-entry-types-share-identity.md
```

**Structure Decision**: one repository, the existing layout. All logic that can be decided without the UI (codes, dates,
sorting, cycle detection, the place rule) sits in Rust modules or pure `src/lib/passwords/*.ts` modules so Linux CI tests it
(FR-036); the components stay thin.

## Slices (pull requests)

Each slice is one pull request stacked on the one before; a slice is shippable alone and leaves logins unchanged.

| Slice | Content                                                                                                                                   | Stories                | Depends on                  |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------- | ---------------------- | --------------------------- |
| A     | Migration 0061, `EntryType` on headers, login-only default at every reader, history v2, purge and copy, ADR, 034 amendments               | US1, US5 (format)      | none                        |
| B     | Pass service and commands, codes on `rxing`, pass view and editor, "New", list icon, filters, date sort, hide expired, folder-name search | US2, US4               | A                           |
| C     | Parent link: cycle check and repair, "Belongs to", "Linked", trash and missing states, "Add linked pass"                                  | US3                    | B                           |
| D     | pkpass reader, import dialog, place proposal, duplicate check, optional signature check                                                   | US6                    | B (C for the link choice)   |
| E     | Capture: camera (all formats), photo, PDF page                                                                                            | US7                    | B                           |
| F     | Android: brightness and keep-awake, share target, pinned shortcut                                                                         | US8, FR-008 on Android | B (D for the shared pkpass) |

E2e and quickstart checks of a slice ship in that slice.

## Complexity Tracking

No constitution violations to justify.
