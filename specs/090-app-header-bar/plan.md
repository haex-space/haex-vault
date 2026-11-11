# Implementation Plan: The header bar belongs to the app

**Branch**: `090-app-header-bar` | **Date**: 2026-11-11 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/090-app-header-bar/spec.md`

## Summary

The title row of a window becomes one header bar the app fills. An app (a system app through `useWmHeader`, an
extension through the new bridge method `extension_header_set`) hands holzi a plain description of three areas of
controls from a small catalog; holzi draws it, sends events back, and collapses controls as the window narrows by the
order and form each control names. The tab strip and the tab-list menu leave the header, the tab model stays.
Settings and the file browser move their first row into the header as the pilots.

The work has four steps, each its own pull request, in this order because each proves the next one's ground: (1) the
header frame, the layout function and `useWmHeader`, with **Settings** as the first user (US1, US2, US5);
(2) the **extension channel**: Rust validation, the frame composable, the vault SDK change (US3, US6); (3) the **file
browser** (US4); (4) clean-up and the remaining E2E. Research R1–R16 records every decision; the contracts fix the
description, the collapse algorithm and the extension side.

## Technical Context

**Language/Version**: TypeScript with Vue 3 / Nuxt (the frame, the header, the system apps, the check scripts); Rust
(edition 2021, MSRV per `src-tauri/Cargo.toml`) for the one bridge method and its types

**Primary Dependencies**: the haex-ui layer (`UiButton`, `ShadcnPopover`, `ShadcnDropdownMenu`, Nuxt Icon), Tauri
events, `ts-rs` for the exported types. **No new dependency** (spaex ponytail rule).

**Storage**: none; the description lives in `TabRuntime` and is never persisted

**Testing**: `cargo test --lib` in `bridge/header_tests.rs`; Node check scripts (`check-wm-header.ts`,
`check-files-header.ts`, `check-extensions-header.ts`) in the existing `check:*` suites; WebDriver E2E scenarios
(`window-header`, `extension-header`, moved `settings-*` and `files-*`); a hand check of SC-008

**Target Platform**: desktop (Linux, macOS, Windows) and Android (a compact window). No platform-specific code.

**Project Type**: desktop/mobile app (Tauri)

**Performance Goals**: a control's effect within 100 ms (SC-006); at most one description applied per animation
frame; one `ResizeObserver` per window header; no layout thrash while resizing (the function is O(n log n) in at most
32 controls)

**Constraints**: header height equals the title row's today (36 px + border, R8); every file at most 500 lines
(spaex); Rust builds only in the 10 GB scope with `-j 1` (R16); additive protocol (haex-vault 2.x ignores the call)

**Scale/Scope**: ≤ 32 controls per header; 2 pilot apps; 1 new bridge method; 1 SDK addition in another repository

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                                                                  | Result |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | No secrets; descriptions carry labels and ids only                                                                                      | Pass   |
| II No local absolute paths                              | Quickstart and scripts use repo-relative paths                                                                                          | Pass   |
| III Device-independent identity                         | Not touched                                                                                                                            | Pass   |
| IV Pinned cross-repo references                         | The SDK change is referenced by repository and PR; when merged, the spec cites it as `haex-space/vault-sdk@<sha>:<path>`               | Pass   |
| V External sources opt-in                               | No harness content                                                                                                                      | Pass   |
| VI Self-modifying instructions review-gated             | No instruction, skill or permission file changes; `.specify/feature.json` is the Spec Kit pointer and goes in the spec PR              | Pass   |
| VII Relay never blocks local work                       | Not touched                                                                                                                            | Pass   |
| VIII No concealment instructions                        | Not touched                                                                                                                            | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | One topic branch per step, each a PR; no agent trailer in commits or PR bodies                                                         | Pass   |
| spaex: 500 LoC per file                                 | `Header.vue`, `HeaderControl.vue`, `HeaderOverflow.vue`, `headerLayout.ts`, `header.rs` are separate; `useExtensionFrame.ts` (460) only gains one call, the rest is in `useExtensionHeader.ts` | Pass   |
| spaex: graphify first                                   | `graphify query` before each new named artifact; candidates evaluated below                                                            | Pass   |
| spaex: tests in separate files                          | Rust tests in `header_tests.rs`; Node checks in `scripts/`                                                                              | Pass   |
| spaex: no new dependency                                | None added                                                                                                                              | Pass   |

Checked against `.specify/memory/constitution.md` and `.spaex/constitution.md`.

**Graphify candidates evaluated (plan time)**: `SettingsToolbar`, `FilesToolbar` and `PanelBar` are the controls being
**moved**, not duplicated (R12, R13; the one-panel/two-panel twin of the file browser is a recorded ceiling).
`notifications.rs::icon` (bundle path or `data:` URL, 256 KiB, verified read) is a **match** and is extracted and shared, not copied (R9). `useSidebarFrame` (container width for the sidebar) does not match: it decides one boolean from a container, the
layout function decides per control with an order. The path bar's `max-md` swap is replaced by the breadcrumb
control's `shrink`. No existing resize-driven collapse utility exists; `useSidebarFrame` and `FolderView` observe a
size but do not lay out a row. Re-run `graphify query` before writing `headerLayout.ts` and `Header.vue`.

## Project Structure

### Documentation (this feature)

```text
specs/090-app-header-bar/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── header-description.md   # shape, limits, checks, what the frontend does with it
│   ├── collapse.md             # the layout function and its test cases
│   └── extension-header.md     # bridge method, events, SDK, forbidden attempts
├── checklists/requirements.md
└── tasks.md                    # /speckit-tasks, not this command
```

### Source Code (repository root)

```text
src-tauri/src/extensions/bridge/
├── header.rs              # NEW  WmHeader* types (ts-rs), validate, set handler, icon table
├── header_icons.rs        # NEW (only if header.rs would pass 500 lines) icon resolving and the per-frame cache
├── ../icon_file.rs        # NEW  shared: a bundle file or data: URL as bytes (notifications + header)
├── ../notifications.rs    # icon() moves into icon_file.rs; behaviour and tests unchanged
├── header_tests.rs        # NEW  every check of contracts/header-description.md, boundaries
├── allowlist.rs           # +1 entry: extension_header_set
└── mod.rs                 # +mod header; +mod header_tests

src/types/bindings/        # generated: WmHeaderDescription, WmHeaderControl, WmHeaderCollapse, WmHeaderEvent, …

src/lib/extensions/header.ts  # NEW  pure: accept only the frame's own payload, build the event message

src/lib/wm/
├── headerLayout.ts        # NEW  layoutHeader (pure)
├── headerSearch.ts        # NEW  input merge (100 ms) and typed-text rule (pure; FR-012, FR-013)
├── types.ts               # TabRuntime + header, headerHandler
└── (apps.ts, …)           # unchanged

src/stores/wmTabHeader.ts     # NEW  header state of a tab (windowManager.ts is at 501 lines; R5)

src/composables/
├── useWmHeader.ts         # NEW  the system-app API
├── useExtensionHeader.ts  # NEW  event → tab, events → SDK port, clearing
├── useExtensionFrame.ts   # +1 call to useExtensionHeader
└── useWmTab.ts            # WmTabApi + setHeader, registerHeaderHandler (+ inert defaults)

src/components/wm/
├── Header.vue             # NEW  the row: areas, drag region, measure ghost, overflow button
├── HeaderControl.vue      # NEW  one control by type, row form and menu form
├── HeaderOverflow.vue     # NEW  the popover with the moved controls
├── HeaderBreadcrumb.vue   # NEW (only if HeaderControl.vue would pass 500 lines)
├── Window.vue             # title row block → <WmHeader>; no tab strip, no tab-list menu
├── TabPanel.vue           # provides setHeader / registerHeaderHandler
├── TabBar.vue, TabListMenu.vue, NewTabMenu.vue   # DELETED if unused (R11)
└── WindowControls.vue     # unchanged (belongs to holzi, FR-002)

src/components/apps/SettingsApp.vue     # useWmHeader; first row removed
src/components/settings/Toolbar.vue     # DELETED
src/components/apps/FilesApp.vue        # useWmHeader; first row removed
src/components/files/Toolbar.vue        # DELETED
src/components/files/PathBar.vue        # max-md swap removed from the one-panel use (kept for PanelBar's two-panel use)
src/lib/files/header.ts                 # NEW  one-panel description builder

src/i18n/locales/{de,en}.json           # header strings that holzi adds (overflow button, fallback labels)

scripts/
├── check-wm-header.ts, check-files-header.ts, check-extensions-header.ts   # NEW
├── e2e/scenarios/{window-header,extension-header}.test.ts                  # NEW
└── e2e/scenarios/{settings-categories,settings-narrow-window,files-*,…}    # moved to the header controls

specs/017-extension-host/contracts/bridge.md      # +1 method row (the spec PR notes it; the code PR edits it)
specs/015-workspace-shell, 020, 040               # a one-line note each: tab strip suspended by spec 090 (FR-029)
```

**Structure Decision**: the existing Tauri app layout. One bridge module in Rust because the checkpoint is Rust; the
layout and its tests in plain TypeScript under `src/lib/wm/` because the repo's logic tests are Node scripts; the
Vue components only measure and apply. No new top-level directory.

## Phases and steps

Each step is one pull request from a topic branch (`--no-track` from `origin/main`), ends green on the fast checks, and
does not leave the app between two designs.

### Step 0 — Spec and plan (this PR)

`spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`, `plan.md`, later `tasks.md`.

### Step 1 — Header frame, layout, system-app API, Settings (US1, US2, US5; FR-001–007, 011, 014–017, 024–025, 028–031)

1. T001 measure the title row (R8); `pnpm typecheck:scripts` decision on how `headerLayout.ts` types its input (R16).
2. Types: `header.rs` with the types only (no handler, no allowlist entry yet), so the bindings exist for the system
   apps; Step 2 adds `validate` and the handler to the same file. If T002 finds the bindings unusable in the Node
   checks, `headerLayout.ts` declares a structural subset (R16).
3. `headerLayout.ts` + `check-wm-header.ts` first (tests before the component).
4. `TabRuntime`, `WmTabApi`, `TabPanel.vue`, `useWmHeader.ts`; `Header.vue`, `HeaderControl.vue`,
   `HeaderOverflow.vue`; `Window.vue` swaps the block; tab strip gone.
5. Settings: `useWmHeader`, `SettingsToolbar` deleted, E2E moved.
6. Tab model checks (`check:wm-navigation`, `check:wm-state`) adjusted to the model; restore scenario for SC-007.

**Exit**: Settings shows the COSMIC-style header; narrowing collapses as ordered; two-tab windows restore.

### Step 2 — Extension channel (US3, US6; FR-018–023)

1. Extract `notifications.rs::icon` into the shared `icon_file.rs` (tests of notifications stay green, unchanged); `header.rs` handler + validation + icon table + `header_tests.rs` (including the icon limits); allowlist; bindings; `bridge.md` row.
2. `useExtensionHeader.ts`, one call in `useExtensionFrame.ts`; `check-extensions-header.ts`.
3. E2E `extension-header` with the `probe` fixture (raw `request`, no SDK release needed).
4. **vault-sdk PR** (separate repository): `HeaderAPI`, types, event constant, tests as in `src/api/__tests__/tab.test.ts`.

**Exit**: the probe's description shows, events arrive, forged input changes nothing (SC-003, SC-004).

### Step 3 — File browser (US4; FR-026)

`src/lib/files/header.ts` + `check-files-header.ts`; `FilesApp.vue` through `useWmHeader`; `FilesToolbar` deleted;
breadcrumb `shrink` replaces the `max-md` swap for one panel; split on/off hand-over; E2E moved.

**Exit**: every control of spec 044's first row works from the header; two panels keep their own bars.

### Step 4 — Clean-up (FR-029, FR-031)

knip run; delete the unused tab components (R11); notes in specs 015/020/040; the operator's hand check (SC-008);
`gh workflow run ci.yml --ref <branch>` for the whole E2E set.

## Risks

| Risk                                                                                 | Mitigation                                                                                                   |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| The recursive type does not typecheck in `tsconfig.scripts.json` (R16)               | T002 first; fallback is a structural subset in `headerLayout.ts`                                              |
| Measuring with a ghost row flickers on first paint                                   | The first frame renders the row with `visibility: hidden` until the first layout result; checked in `window-header` |
| Tab strip gone leaves users with no way to switch tabs by hand                        | Accepted (decision 2); noted in the PR and in specs 015/020/040; follow-up spec                              |
| `useExtensionFrame.ts` outgrows 500 lines                                            | The header logic is its own composable, the frame gets one call                                              |
| One-panel and two-panel controls of the file browser drift apart (R13)               | `ponytail:` marker with the upgrade path; the builder's check script pins the one-panel list                  |
| An extension's SVG icon with script or external references | Drawn only through `<img>`/`mask-image` from a `data:` URL, never inlined; `extension-header` E2E loads such an SVG and checks nothing runs or loads |
| E2E is not run on pull requests                                                      | `gh workflow run ci.yml --ref <branch>` before every merge in Steps 1–3                                       |

## Complexity Tracking

No constitution violations; nothing to justify. Two recorded ceilings (`ponytail:` in the code): the file browser's
controls exist as description and as `PanelBar` (R13), and an extension's header cannot use holzi's own icons (R9, by the operator's rule).

## Post-design re-check

Constitution check repeated after Phase 1: unchanged, all Pass. The design adds no dependency, no secret, no
permission, and keeps the extension protocol additive (R14).
