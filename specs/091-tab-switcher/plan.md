# Implementation Plan: Switching tabs by hand

**Branch**: `091-tab-switcher` | **Date**: 2026-11-11 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/091-tab-switcher/spec.md`

## Summary

Spec 090 took the tab strip out of the window header. This spec gives a person back the way to open, see, switch and
close tabs: a host-owned button beside the window buttons (a "+" with one tab, the number of tabs with two or more), a
tile overview of the window's tabs as an overlay over its content, and the apps shown as tiles behind the "+" instead of
a list. The app tiles are the launcher's grid extracted into one component that scrolls (in COSMIC's look: large icons, the name below), so 30 to 50 apps stay
reachable; the launcher uses it too.

Two pull requests, because only one needs spec 090: **Part A** extracts `AppTiles.vue` and gives the launcher the scrolling grid
(User Story 6, FR-003a, FR-022); **Part B** adds the tab button, the overlay and the "+" tiles after 090 is merged
(User Stories 1–5). Research R1–R10 records every decision.

## Technical Context

**Language/Version**: TypeScript with Vue 3 / Nuxt. **No Rust change.**

**Primary Dependencies**: the haex-ui layer (`UiDrawerModal`, `UiButton`, `Shadcn*`). **No new dependency** (spaex
ponytail rule): a scrolling grid needs none.

**Storage**: none; the switcher state is never persisted (FR-018)

**Testing**: Node check scripts (`check-wm-tab-switcher.ts`, `check-wm-tile-grid.ts`, the existing
`check-wm-navigation.ts`), WebDriver E2E (`tab-switcher`, `launcher-grid`, moved tab scenarios), a hand check on a phone

**Target Platform**: desktop (Linux, macOS, Windows) and Android (compact window, back gesture)

**Project Type**: desktop/mobile app (Tauri)

**Performance Goals**: the overlay opens in under 300 ms with 50 tabs or 50 apps (SC-002, SC-007); the overlay is mounted
only while open

**Constraints**: every file at most 500 lines (`windowManager.ts` is at 501: no growth, new state in
`wmTabSwitcher.ts`); the tab model, `wm.tab.*` actions and session restore unchanged (FR-018); build on spec 090's header

**Scale/Scope**: up to 50 tabs and 50 apps; 5 new components, 3 pure modules, 1 store module

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                                         | Result |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | No secrets involved                                                                                           | Pass   |
| II No local absolute paths                              | Scripts and docs use repo-relative paths                                                                      | Pass   |
| III Device-independent identity                         | Not touched                                                                                                   | Pass   |
| IV Pinned cross-repo references                         | No external content; spec 090 is referred to by number                                                        | Pass   |
| V External sources opt-in                               | No harness content                                                                                            | Pass   |
| VI Self-modifying instructions review-gated             | No instruction, skill or permission file changes; `.specify/feature.json` is the Spec Kit pointer, in the PR  | Pass   |
| VII Relay never blocks local work                       | Not touched                                                                                                   | Pass   |
| VIII No concealment instructions                        | Not touched                                                                                                   | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | Two topic branches, two PRs; no agent trailer                                                                 | Pass   |
| spaex: 500 LoC per file                                 | `Launcher.vue` shrinks; new components are small; the state is in `wmTabSwitcher.ts`, not `windowManager.ts`   | Pass   |
| spaex: graphify first                                   | Candidates below; the launcher grid is **extended** (extracted), not duplicated                               | Pass   |
| spaex: tests in separate files                          | Node checks in `scripts/`; no Rust                                                                            | Pass   |
| spaex: no new dependency                                | None needed (a scrolling grid)                                                                                          | Pass   |

Checked against `.specify/memory/constitution.md` and `.spaex/constitution.md`.

**Graphify candidates evaluated** (research R8): `Launcher.vue` is the match and is extracted; COSMIC's app library is the visual and behavioural reference (large icons, a scrolling grid);
`WindowOverview.vue` is similar in spirit for windows but a different data set and layout; `NewTabMenu.vue` is removed
by spec 090 and its "already open" marking moves into `AppTiles.vue`. Re-run `graphify query` before writing
`AppTiles.vue`, `TabSwitcher.vue` and `TabOverview.vue`.

## Project Structure

### Documentation (this feature)

```text
specs/091-tab-switcher/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── tab-button.md         # faces, place, behaviour, accessibility, attention
│   ├── switcher-overlay.md   # structure, tiles, pick/close, keyboard, inert, ends
│   └── app-tiles.md          # props, look, scrolling, the launcher and the "+"
├── checklists/requirements.md
└── tasks.md                  # /speckit-tasks, not this command
```

### Source Code (repository root)

```text
src/lib/wm/
├── tabSwitcher.ts          # NEW  tabButtonFace, mode transitions (pure), moveFocus
└── tileGrid.ts             # NEW  gridColumns (pure)

src/stores/
├── wmTabSwitcher.ts        # NEW  Map<windowId, {mode, appsFrom}>, open/toggle/close/back
└── wmNavigation.ts         # +2 deps: tabSwitcherOpen, closeTabSwitchers (system back closes the overlay)

src/components/wm/
├── AppTiles.vue            # NEW  the launcher's grid extracted; scrolling, COSMIC-style tiles (Part A)
├── Launcher.vue            # uses AppTiles; fixed strip with assistant and lock tiles (Part A)
├── TabButton.vue           # NEW  the "+" or the number, attention dot (Part B)
├── TabSwitcher.vue         # NEW  the overlay and its two modes (Part B)
├── TabOverview.vue         # NEW  the tab tiles and the trailing "+" tile (Part B)
└── Window.vue              # renders TabButton in the window-button group and TabSwitcher over the content; inert while open

src/i18n/locales/{de,en}.json     # button names, overlay titles, "+" tile

scripts/
├── check-wm-tab-switcher.ts, check-wm-tile-grid.ts                # NEW
├── check-wm-navigation.ts                                         # + the closeOverlay case
└── e2e/scenarios/{tab-switcher,launcher-grid}.test.ts            # NEW
    e2e/scenarios/{compact-tab-close,passwords-tabs,tab-content-isolation,android-phone-layout,android-back-gesture}.test.ts   # moved to the button/overview where they used the strip
```

**Structure Decision**: the existing Tauri app layout; the logic that has rules (face, transitions, focus movement, grid
and column math) is pure TypeScript under `src/lib/wm/` so the repo's Node check scripts test it; the Vue components
only draw and call it. No new top-level directory.

## Phases and steps

### Step A — App tiles and the launcher (US6; FR-003a, FR-022; independent of spec 090)

1. `tileGrid.ts` + `check-wm-tile-grid.ts` first.
2. `AppTiles.vue` extracted from `Launcher.vue` as a scrolling grid in COSMIC's tile look (large icon, name in up to two
   lines, no frame); `Launcher.vue` uses it with the fixed strip; keep the test ids and the pin menu.
3. Plain regression check on the emulator (research R6): a long press opens the pin menu, a scroll that starts on a tile
   does not open anything.
4. `launcher-grid.test.ts`; `knip`; texts.

**Exit**: the launcher scrolls 40 apps with the lock tile always visible; with six apps it shows no scroll bar.

### Step B — Tab button, overview and "+" tiles (US1–US5; needs spec 090 merged)

1. `tabSwitcher.ts` + `check-wm-tab-switcher.ts` first (face, transitions, `moveFocus`, attention rule).
2. `wmTabSwitcher.ts`; the two dependencies in `wmNavigation.ts` and the system-back case in `check-wm-navigation.ts`.
3. `TabButton.vue`, `TabOverview.vue`, `TabSwitcher.vue`; `Window.vue` wiring and `inert` (header app areas through
   090's prop); texts.
4. Move the E2E scenarios that used the tab strip; new `tab-switcher.test.ts`; notes in specs 015, 020, 040 that
   spec 090 added are updated (FR-019).
5. Hand check on a phone (scroll feel, 44 px targets, back gesture).

**Exit**: SC-001–SC-005 and SC-006 pass; a window with several tabs can be driven with mouse, keyboard and touch.

### Step C — Clean-up

`knip`; `gh workflow run ci.yml --ref <branch>` for the whole E2E set; set the spec's status.

## Risks

| Risk                                                                                  | Mitigation                                                                                               |
| ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| The tile look (two-line names, large icons) does not fit the drawer's height on a phone | The grid scrolls, the fixed strip keeps its size; checked in `launcher-grid.test.ts` and by hand |
| `windowManager.ts` (501 lines) tempts adding state there                              | The state is `wmTabSwitcher.ts`; the system-back hook is two dependencies in `wmNavigation.ts`           |
| Spec 090's header is not merged when Part B starts                                    | Part A first; Part B on top of the 090 branch or after its merge                                          |
| 50 apps are slow to set up in E2E                                                      | A test-only app-list override, decided by measuring (R9)                                                 |
| Moving the tab scenarios loses coverage                                                | Each moved scenario keeps its assertion; none is deleted without a replacement                           |

## Complexity Tracking

No constitution violations; nothing to justify.

## Post-design re-check

Constitution check repeated after Phase 1: unchanged, all Pass. No dependency, no secret, no permission, no Rust.
