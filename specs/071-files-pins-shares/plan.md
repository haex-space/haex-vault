# Implementation Plan: Pinned folders and the share list

**Branch**: `071-files-pins-shares` | **Date**: 2026-10-30 | **Spec**: [spec.md](spec.md)

## Summary

Two front-end changes in the file browser; no Rust, no schema, no new dependency.

1. **Pins** are a device preference (`files.pins`, JSON) read and written through the existing
   `useFilesPrefs` (shared module state, same load/save path as view, sort, hidden). The rules (parse,
   add, remove, "is it a known place", "may this source be pinned", label) are pure in
   `src/lib/files/pins.ts`. The sidebar section is a new component `components/files/PinnedPlaces.vue`
   (Sidebar.vue stays under 500 lines). The context menu gets two commands `pin`/`unpin`
   (`lib/files/menus.ts`); `useFilesManage` only asks an optional `pins` option what to offer and calls
   `set` for the command. Drag: the section is a drop target for the entries drag of
   `useFilesDrag` (new tiny export `currentDrag()` of the drag under way, since browsers hide drag data
   while hovering); a pinned entry starts a drag of its own MIME and the removal strip takes it.
2. **Share list**: the drawer `SharesOverview.vue` becomes the panel list `SharesList.vue` (same data
   and actions, `UiDrawerModal` and the `open` model gone, row layout and filter like
   `SyncRulesList.vue`). The panel shows it for the new place key `sh` (registry, split `OTHER_PASSED`,
   `FilesQueryPatch`), the sidebar button emits `pickShares` like `pickSyncRules`.

## Technical Context

**Language**: TypeScript (Nuxt 4, Vue 3.5). **Storage**: `usePreferences` device scope. **Testing**:
`node --test scripts/check-files-pins.ts` (new), `check-files-registry.ts`, `check-files-clipboard.ts`
(menus), `check-files-split.ts` (extended); E2E `scripts/e2e/scenarios/files-pins.test.ts`.

## Constitution Check

- Smallest correct change: extends `useFilesPrefs`, `FilesMenu`, `entriesMenu`, the `sr` view pattern
  instead of parallel mechanisms (graphify candidates: `useFilesPrefs`, `Menu.vue`, `SyncRulesList.vue`,
  `useFilesDrag`, `shareableSource`).
- 500 line boundary: `useFilesManage.ts` (488) gets only a few lines (its `shareableSource` is replaced
  by the shared predicate of `pins.ts`); new logic goes to new files.
- Every device and OS: no platform code; paths via `lib/files/state.ts` helpers (Windows `\`).
- Texts German/English; no "Relay", no "Layout". No agent attribution.

## Slicing

One PR, one commit: the two parts are small and independent but touch the same sidebar, panel and texts.
