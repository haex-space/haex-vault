# Implementation Plan: The headers of Contacts, Passwords, Wallet and the Marketplace

**Branch**: `093-app-headers` | **Date**: 2026-11-13 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/093-app-headers/spec.md`

## Summary

The first row of four apps moves into the window header of spec 090. Each app describes its controls in a pure function
and hands them to `useWmHeader`; the old toolbar component is deleted. One mechanism change comes first: a menu control
gets an `active` flag that draws a dot (spec FR-008), carried to the overflow button when the control has moved into the
panel. The work is five steps, each its own pull request: **(0)** the dot, **(1)** Passwords, **(2)** Contacts,
**(3)** Wallet, **(4)** Marketplace. Steps 1–4 are independent of each other once step 0 is on `main`. Research R1–R9
holds the decisions; `contracts/header-dot.md` fixes the one protocol change.

**Prerequisite**: #986 (checked menu entries, the overflow panel on the window) must be on `main`. The steps branch off
`main` after it, or off its branch if it is not merged yet.

## Technical Context

**Language/Version**: TypeScript with Vue 3 / Nuxt (descriptions, renderer, apps); Rust (MSRV per `src-tauri/Cargo.toml`)
for the one field in `bridge/header.rs` and its validation

**Primary Dependencies**: spec 090's header (`useWmHeader`, `Header.vue`, `HeaderControl.vue`, `HeaderOverflow.vue`),
`ts-rs` bindings. **No new dependency.**

**Storage**: none

**Testing**: `cargo test --lib` (`header_tests.rs`); Node checks `check-wm-header.ts` (extended; it runs under
`check:wm-state`) and new `check-passwords-header.ts`, `check-contacts-header.ts`, `check-wallet-header.ts`,
`check-marketplace-header.ts` (pure description builders, collapse orders, test ids, no `headerProblems`; the globs of
`check:passwords`, `check:contacts`, `check:wallet`, `check:marketplace` pick them up, no `package.json` change); existing `check-passwords-*`/`check-contacts-*`/`check-marketplace-*`
scripts keep passing; E2E: the scenarios of each app with their selectors adapted, plus one header scenario per app
(narrow and wide) and the dot in `window-header`; a hand check of SC-005 in Passwords and Wallet

**Target Platform**: desktop and Android (a compact window). No platform-specific code.

**Project Type**: desktop/mobile app (Tauri)

**Performance Goals**: none beyond spec 090 (a control's effect within 100 ms; one description per animation frame). A
description is rebuilt on a state change only (`watchEffect`).

**Constraints**: every file at most 500 lines; Rust only in the 10 GB scope (`-j 1`,
`CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`); additive protocol; tests in separate files; no backward
compatibility (the old rows go, no setting returns them)

**Scale/Scope**: 4 apps, ≈ 700 lines of toolbar removed, ≈ 4 description builders (each well under 150 lines), 1 new field
across Rust, bindings, renderer and SDK

## Constitution Check

_GATE: before Phase 0; re-checked after Phase 1._

| Principle                                    | Check                                                                                             | Result |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                          | Labels and ids only                                                                               | Pass   |
| II No local absolute paths                   | Quickstart uses repo-relative commands                                                            | Pass   |
| III Device-independent identity              | Not touched                                                                                       | Pass   |
| IV Pinned cross-repo references              | The SDK change is cited as `haex-space/vault-sdk@<sha>:src/types/header.ts` once merged (R9)      | Pass   |
| V External sources opt-in                    | No harness content                                                                                | Pass   |
| VI Self-modifying instructions review-gated  | No instruction, skill or permission file changes; `.specify/feature.json` is the Spec Kit pointer | Pass   |
| VII Relay never blocks local work            | Not touched                                                                                       | Pass   |
| VIII No concealment instructions             | Not touched                                                                                       | Pass   |
| Workflow: PR to `main`, Conventional Commits | One branch and PR per step; no agent trailer in commits or PR bodies (operator rule)              | Pass   |
| spaex: 500 LoC per file                      | Builders in `src/lib/<app>/header.ts`; `PasswordsApp.vue` gains dialogs and one call, stays small | Pass   |
| spaex: graphify first                        | See candidates below; re-run `graphify query` before each new named artifact                      | Pass   |
| spaex: tests in separate files               | Rust in `header_tests.rs`; Node checks in `scripts/`                                              | Pass   |
| spaex: no new dependency                     | None                                                                                              | Pass   |

Checked against `.specify/memory/constitution.md` (and `.spaex/constitution.md` when the checkout has one).

**Graphify candidates evaluated (plan time)**: `src/lib/files/header.ts` and `useFilesPanelHeader.ts` are the model for
the four builders, not a thing to extend: each app's description is its own data. `SettingsApp.vue`'s inline
`useWmHeader` call is the model for the three apps with one sidebar button; the builder functions take the same shape so
the sidebar toggle is not written four times — if the second app repeats it verbatim, extract `sidebarToggle(visible,
label, testId)` into `src/lib/wm/headerParts.ts` (decide in step 2, not before: no abstraction for one use).
`useSidebarFrame` stays as it is. The old `TagFilter.vue`, `ListFilters.vue`, `CatalogToolbar.vue` and the three
`Toolbar.vue` files are the code being **moved and deleted**, not duplicated.

## Project Structure

### Documentation (this feature)

```text
specs/093-app-headers/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/header-dot.md
├── checklists/requirements.md
└── tasks.md                # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/extensions/bridge/
├── header.rs               # Menu { …, active: Option<bool> }
├── header_validate.rs      # flag(object, "active", own)? in the "menu" arm
└── header_tests.rs         # active accepted / refused / ignored

src/types/bindings/          # regenerated: WmHeaderControl.ts, WmHeaderDescription.ts (strip trailing spaces)

src/components/wm/
├── HeaderControl.vue        # the dot on a menu with `active` (row; group heading in the panel); testid on panel entries
├── Header.vue               # any overflowed control active → `HeaderOverflow` prop
└── HeaderOverflow.vue       # the dot on the ⋯ button

src/lib/wm/headerChecks.ts   # dev warning: `active` on a non-menu control

src/lib/passwords/header.ts  # NEW  pure description of the password manager (US1)
src/lib/contacts/header.ts   # NEW  (US2)
src/lib/wallet/header.ts     # NEW  (US3)
src/lib/marketplace/header.ts # NEW (US4)

src/components/apps/
├── PasswordsApp.vue         # useWmHeader + the three dialogs; no <PasswordsToolbar>
├── ContactsApp.vue          # useWmHeader; no <ContactsToolbar>
└── WalletApp.vue            # useWmHeader; no <WalletToolbar>
src/components/marketplace/CatalogGrid.vue   # useWmHeader; no <MarketplaceCatalogToolbar>

DELETED: passwords/Toolbar.vue, passwords/TagFilter.vue, contacts/Toolbar.vue, wallet/Toolbar.vue,
         wallet/ListFilters.vue (its state stays in the wallet store), marketplace/CatalogToolbar.vue

scripts/check-{passwords,contacts,wallet,marketplace}-header.ts   # NEW, run by the existing check:<app> globs
scripts/e2e/lib/{passwords,contacts,marketplace}.ts + scenarios   # selectors for moved controls (menu entries: <menu>-<entry>)
```

**Structure Decision**: each app keeps its state where it is and gains one pure builder next to the other
`src/lib/<app>/` modules; the renderer changes only in step 0. i18n keys stay (no new translation but
`wm.header.active` for the dot's accessible text, de/en).

## Steps

| Step | Pull request                                | Spec            | Verify                                                                                         |
| ---- | ------------------------------------------- | --------------- | ---------------------------------------------------------------------------------------------- |
| 0    | the dot: Rust, bindings, renderer, SDK type | FR-008          | `cargo test --lib bridge::header`; `check:wm-state`; `window-header` E2E sees the dot          |
| 1    | Passwords                                   | US1, FR-001–011 | `check-passwords-header`; `passwords-*` and a header scenario (narrow/wide); hand check SC-005 |
| 2    | Contacts                                    | US2             | `check-contacts-header`; `contacts-basic` adapted                                              |
| 3    | Wallet                                      | US3             | `check-wallet-header`; `wallet-*` adapted; hand check SC-005                                   |
| 4    | Marketplace                                 | US4             | `check-marketplace-header`; `marketplace-*` adapted (R7 open point)                            |

## Risks

- **Menu entry test ids change** (`<menu>-<entry>`): every E2E selector for a Passwords menu entry is adapted in step 1.
  Mitigated by listing them in the step's tasks from `scripts/e2e/lib/passwords.ts`.
- **A dialog that depends on the toolbar's lifetime** (Passwords): R2; the E2E for "open tags manager from the
  menu while narrow" covers it.
- **Menu without headings or colours** (R10): Wallet's group headings and the tags' colour have no place in a header
  menu yet; the operator decides (heading entry in step 0, or accept) before steps 1 and 3.
- **Marketplace grid lifetime** (R7): checked at spec review (the grid is a route component and unmounts on a detail
  page); step 4 confirms it.
- **#986 not merged**: steps wait or branch off it; if its review changes the `checked` shape, step 0's field follows.

## Post-design constitution check

No violation introduced by Phase 1 (the dot is one optional boolean; no new dependency; no file over 500 lines
expected).
