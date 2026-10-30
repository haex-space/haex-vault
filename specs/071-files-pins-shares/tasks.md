---
description: 'Task list for spec 071-files-pins-shares'
---

# Tasks: Pinned folders and the share list

**Status**: implemented; the E2E scenario (T008) was type-checked but not run locally, CI runs it.

**Input**: [spec.md](./spec.md), [plan.md](./plan.md)

**Rules for every task**: consult graphify before a new named function/component; files under 500
lines; every text in `de.json` and `en.json` (vue-i18n refuses `<…>`); `UiButton` in a form needs
`type="button"`; run frontend commands through `nix develop --command`; no agent attribution.

## Phase 1: Pins (US1–US3)

- [x] T001 `src/lib/files/pins.ts`: `FilesPin`, `parsePins`, `formatPins`, `pinnableSource`, `addPins`,
      `removePin`, `isPinned`, `pinLabel` + `scripts/check-files-pins.ts`
- [x] T002 `useFilesPrefs`: `pins`, `pin(...)`, `unpin(...)` (load with the other prefs, save as JSON)
- [x] T003 `lib/files/menus.ts`: commands `pin`/`unpin`, `pinMenu()`; extend `entriesMenu`/`folderMenu`
      input `pin`; extend `check-files-clipboard.ts`; texts `files.actions.pin/unpin`
- [x] T004 `useFilesManage`: option `pins`, menu input, `run` cases; Panel wires it
- [x] T005 `components/files/PinnedPlaces.vue` (section, empty hint, click, context menu, drag of a pin,
      removal strip, drop target) and `useFilesDrag` export `currentDrag()`; `Sidebar.vue` uses it;
      `FilesApp.vue` opens a pin and passes the current place for the highlight

## Phase 2: Share list (US4)

- [x] T006 place key `sh`: `registry.ts`, `split.ts`, `useFilesSplit.ts` patch type; extend
      `check-files-registry.ts` and `check-files-split.ts`
- [x] T007 `SharesOverview.vue` → `SharesList.vue` (list, filter, live update, edit dialog kept); Panel
      shows it for `sh`; Sidebar button emits `pickShares`; FilesApp opens it; texts
- [x] T008 E2E `files-pins.test.ts` (pin by menu, drag, unpin by menu and strip, share list opens as a
      list, no dialog)

## Phase 3: Polish

- [x] T009 spec 056 FR-020 note, `prettier --write` on touched md/json, typecheck, lint, checks
- [ ] T010 manual check M1: pin by drag with a real mouse in the app; Esc cancels a pin drag
