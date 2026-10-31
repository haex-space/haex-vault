---
description: 'Task list for spec 071-files-pins-shares'
---

# Tasks: Pinned folders and the share list

**Status**: implemented, revised 2026-10-31 (Phase 4); the E2E scenario runs in CI (not locally).

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

## Phase 4: Revision 2026-10-31 (added folders live in "Orte", any source)

- [x] T011 `pins.ts` `pinnableSource`: every source but a `device:` storage; update
      `check-files-pins.ts`
- [x] T012 `PinnedPlaces.vue` → `Places.vue`: the whole "Orte" section (known places, then added
      folders with their source's name, greyed out while unreachable), drop target and removal strip;
      no "Favoriten" section, no empty hint; `Sidebar.vue` uses it
- [x] T013 texts `files.actions.pin/unpin` ("Zu Orten hinzufügen", "Aus Orten entfernen"),
      `files.sidebar.unpinDrop`; drop `files.sidebar.pinned/pinnedHint`
- [x] T014 E2E `files-pins.test.ts`: drop target `files-places`, no empty hint

## Phase 5: Second revision 2026-10-31 (drop in the app, known places removable)

- [x] T015 drop on "Orte" uses `dropEffect = 'copy'` (the entries drag allows copy/move only, `link`
      was refused by the webview); the E2E `dragTo` helper now refuses a drop as a webview does
      (hover not taken, or an effect the drag does not allow)
- [x] T016 `pins.ts`: `FilesPlaces`, `parseHidden`, `addPlaces`, `removePlace`, `offerPin` over both;
      `useFilesPrefs` stores `files.hiddenPlaces`; tests in `check-files-pins.ts`
- [x] T017 `Places.vue`: known places get the context menu and drag to the strip; hidden ones are left
      out; E2E step: remove a known place, add it back from the open folder's menu

## Phase 6: Icons of added folders (operator, 2026-10-31)

- [x] T018 `components/files/FolderIcon.vue`: the icon-set icon of a folder by its path; added folders in
      "Orte" use it (FR-016)
- [x] T019 `pins.ts`: compare known-place paths without trailing separators; `shownKnown`/`shownPins`
      show a known place stored as an added folder too once, as the known place; removing it drops the
      added copy; tests in `check-files-pins.ts`
- [x] T020 the known places of "Orte" use `FolderIcon` too (operator: "in Orte die gleichen Icons wie
      im Rest des Dateibrowsers", also after a change of the icon set); the fixed symbols are gone
