---
description: 'Task list for Paper Notes to Editable Canvas'
---

# Tasks: Paper Notes to Editable Canvas

**Input**: Design documents from `/specs/046-paper-notes-canvas/`

**Implementation target**: `https://github.com/haex-space/haextension.git` at revision
`6dfb015198c99018236f5a8e958a916942ab971c`, under `apps/haex-notes/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/page-job.md, quickstart.md; holzi with
spec 055 and vault-sdk 4.7.1

**Tests**: Included; the project constitution requires runnable checks for non-trivial logic. Tests stay in separate
`*.test.ts` files.

> Regenerated 2026-10-21 for recognition through spec 055 and without the review part. The previous list (51 tasks,
> none built) was based on the agent task of spec 047.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no open dependency)
- **[Story]**: US1 (photo to draft), US3 (edit the draft), US4 (PDF export); US2 was dropped

---

## Phase 1: Setup

- [x] T001 (haex-space/haextension#116) Create an implementation worktree of `haex-space/haextension` at `6dfb015198c99018236f5a8e958a916942ab971c` and confirm `apps/haex-notes/package.json` is version 0.1.7
- [x] T002 (haex-space/haextension#116; `^4.7.1` like the other dependencies, the lockfile pins 4.7.1; `pdf-lib` comes with T036 so the package carries no unused dependency) Raise `@haex-space/vault-sdk` to exactly `4.7.1` and add `pdf-lib` in `apps/haex-notes/package.json`; update the lockfile; add no provider, OCR or model SDK
- [x] T003 [P] Add permissively licensed handwriting fonts with their license notices under `apps/haex-notes/public/fonts/` (Caveat, Patrick Hand, Kalam under the OFL, from `google/fonts@45e3d5269e4a17d823f6d84470bc084dc5ae46f6`)
- [x] T004 [P] Add the handwriting style registry with a neutral fallback in `apps/haex-notes/app/lib/paperNotes/fonts.ts`

---

## Phase 2: Foundational (blocking)

**Purpose**: the page model, the renderer for text, images and cells, and the storage every story needs.

### Tests first

- [x] T005 [P] (haex-space/haextension#116) Tests in `apps/haex-notes/app/lib/migratePage.test.ts`: a page without `paperImport` and a table without `cells` load unchanged; a page with both survives save and load; an unknown `paperImport.version` is kept
- [x] T006 [P] (haex-space/haextension#116) Tests in `apps/haex-notes/app/lib/tableActions.test.ts`: adding and removing rows and columns keeps `cells` at `rows` × `columns`, new cells empty
- [x] T007 [P] (haex-space/haextension#116) Tests in `apps/haex-notes/app/lib/render/elements.test.ts`: `text` with font, size and wrap width; `image` with opacity and rotation; a missing asset draws a placeholder; cell text inside its cell
- [x] T008 [P] (haex-space/haextension#116) Tests in `apps/haex-notes/app/lib/collectAssets.test.ts`: the original, the working copy and image crops of `paperImport` are collected

### Implementation

- [x] T009 (haex-space/haextension#116) Add `TableElement.cells` and the `PaperNoteImport`, `PaperNoteProcessing` and `RecognizedRegion` types (data-model.md) in `apps/haex-notes/app/types/document.ts`
- [x] T010 (haex-space/haextension#116) Add the column `paper_import` in `apps/haex-notes/app/database/schemas/index.ts` and the migration `apps/haex-notes/app/database/migrations/0005_*.sql` with its snapshot and journal entry
- [x] T011 (haex-space/haextension#116) Read and write `paperImport` and `cells` in `apps/haex-notes/app/lib/migratePage.ts` and `apps/haex-notes/app/stores/notebook.ts`
- [x] T012 (haex-space/haextension#116) Keep `cells` in step with rows and columns in `apps/haex-notes/app/lib/tableActions.ts`
- [x] T013 (haex-space/haextension#116; images load through `app/composables/useElementImages.ts`) Draw `text`, `image` (assets loaded asynchronously, placeholder when missing) and cell text in `apps/haex-notes/app/lib/render/elements.ts`
- [x] T014 (haex-space/haextension#116) Collect the new asset sources in `apps/haex-notes/app/lib/collectAssets.ts`

**Checkpoint**: Pages with text, images and filled tables draw, save and load; nothing reads a photo yet.

---

## Phase 3: User Story 1 - Photo to a clean draft (P1) 🎯 MVP

**Goal**: A photo becomes a page with the original on a locked layer and a draft of texts, tables and image crops,
read through holzi; every failure leaves a usable page with the original.

**Independent Test**: With a fake `runPageJob` that returns the fixture result, importing a fixture photo creates a
page with the original layer and one text, one table and one image element at the expected places; with each error
code it creates the page with the original and the contract's hint.

### Tests first

- [x] T015 [P] [US1] Tests in `apps/haex-notes/app/lib/paperNotes/pageJob.test.ts`: the job has the two steps, instructions, formats, efforts and `from` pointer of contracts/page-job.md; the image is the working copy as JPEG
- [x] T016 [P] [US1] Tests in `apps/haex-notes/app/lib/paperNotes/layout.test.ts`: boxes 0–1000 map to page coordinates; left edges within 2 % align; overlapping regions move down in reading order; font size follows box height and line count within bounds
- [x] T017 [P] [US1] Tests in `apps/haex-notes/app/lib/paperNotes/mapResult.test.ts`: text, table (rows padded to the longest) and image regions become elements; empty regions and invalid boxes are skipped; `fix` texts replace the texts in order and a count mismatch keeps the read texts; the provenance gives model and place of the `read` attempt with `outcome: "used"`
- [x] T018 [P] [US1] Tests in `apps/haex-notes/app/lib/paperNotes/workingCopy.test.ts`: turning by 90, 180 and 270 degrees; the longest edge is at most 2048 px; a dark or blurred photo gives the quality hint; the original bytes are never changed (turning is tested by the sizes of the copy; drawing it needs a canvas and is checked by hand)
- [x] T019 [P] [US1] Tests in `apps/haex-notes/app/lib/paperNotes/importPage.test.ts`: layers in the order of data-model.md, the original layer locked and never active, `paperImport` filled; a rotation hint starts exactly one second job with the turned copy; each error code (`no_capable_model`, `consent_denied`, `unusable`, `timeout`, `unavailable`) and a cancel give the page with the original and the state of data-model.md

### Implementation

- [x] T020 [US1] Build the job from contracts/page-job.md in `apps/haex-notes/app/lib/paperNotes/pageJob.ts`
- [x] T021 [US1] Map boxes to the page and tidy edges and overlaps in `apps/haex-notes/app/lib/paperNotes/layout.ts` (research R11)
- [x] T022 [US1] Map the result to elements, provenance and hints in `apps/haex-notes/app/lib/paperNotes/mapResult.ts` (research R9, R12)
- [ ] T023 [US1] Make the working copy (crop, perspective, turn, JPEG, quality hint) in `apps/haex-notes/app/lib/paperNotes/workingCopy.ts` — turn, size, JPEG and quality hint done; crop and perspective open (need corner detection)
- [x] T024 [US1] Create the page from original, working copy and result, with the one re-read after a rotation hint, in `apps/haex-notes/app/lib/paperNotes/importPage.ts` (research R10)
- [x] T025 [US1] `runPageJob(job)` over `sdk.models.runAsync`, the state while reading and the cancel over `cancelAsync` in `apps/haex-notes/app/composables/usePaperNoteRecognition.ts` (cancelled with `runAsync(job, { signal })` from vault-sdk 4.8.0, which calls `cancelAsync`)
- [x] T026 [US1] `apps/haex-notes/app/components/notes/PaperNoteImportDialog.vue`: camera or file, turn by 90°, quality hint with "take again" and "read anyway", reading with cancel, the hint of each error, the provenance (model and place) after reading; open it from the notebook page
- [x] T027 [P] [US1] Texts of the dialog and the hints in German and English in the locale files of haex-notes (in the `<i18n>` blocks of the components, as elsewhere in haex-notes)
- [ ] T028 [US1] Fixtures in `apps/haex-notes/app/lib/paperNotes/fixtures/`: de-identified photos (text, table, drawing, a sideways sheet) with the expected `read` and `fix` results and their manifest — open: no de-identified photos yet

**Checkpoint**: US1 works against the fake; quickstart steps 1–4, 9 and 10 against holzi.

---

## Phase 4: User Story 3 - Edit the draft as a canvas (P1)

**Goal**: Texts, cells, images and tables of the draft are edited like other canvas content; the original stays.

**Independent Test**: After a fixture import one text, one cell and one image are changed, the page is saved and
reopened, and every change and the unchanged original are there.

- [x] T029 [P] [US3] Tests in `apps/haex-notes/app/lib/elementEdits.test.ts`: editing a text, a cell, moving and resizing an image and changing the handwriting style are undoable commands that never touch the original layer
- [x] T030 [US3] Edit cell text by double click with the text editor in `apps/haex-notes/app/components/notes/PageCanvas.vue`
- [x] T031 [US3] Select, move and resize text, image and table elements of the draft in `apps/haex-notes/app/components/notes/PageCanvas.vue`
- [x] T032 [US3] Choose the handwriting style of a text block from `fonts.ts` in the toolbar of `apps/haex-notes/app/components/notes/PageCanvas.vue` (a picker above the selected text)
- [x] T033 [US3] Original, draft and compare views (layer visibility) in `apps/haex-notes/app/components/notes/PageCanvas.vue`
- [ ] T034 [P] [US3] Tests in `apps/haex-notes/app/lib/paperNotes/closeUp.test.ts`: the regions of a close-up fit into the rectangle with the rules of research R11; generated elements fully inside the rectangle are replaced, generated elements partly outside, the student's own elements and the locked original stay; `paperImport` gets the close-up and the new regions and loses the replaced ones; one undo restores elements and `paperImport`; a failed or cancelled job changes nothing and stores no asset (research R13)
- [ ] T042 [P] [US3] `setPaperImport(page, next, label)` command with tests in `apps/haex-notes/app/lib/commands.ts` and `apps/haex-notes/app/lib/commands.test.ts`
- [ ] T043 [US3] `PaperNoteCloseUp`, `RecognizedRegion.closeUpId` and the parsing of `paperImport.closeUps` in `apps/haex-notes/app/types/document.ts` and `apps/haex-notes/app/lib/migratePage.ts`; close-up assets in `apps/haex-notes/app/lib/collectAssets.ts`
- [ ] T044 [US3] Read a close-up and replace the elements of the rectangle in `apps/haex-notes/app/lib/paperNotes/closeUp.ts`, reusing the job, the working copy and the mapping of US1
- [ ] T045 [US3] "Photograph this area again": draw the rectangle on a photo page, then `PaperNoteImportDialog.vue` in close-up mode (camera or file, turn, quality hint, read with cancel, the hint of each error)

**Checkpoint**: Quickstart steps 5–7, including 6a.

---

## Phase 5: User Story 4 - Export a readable version (P2)

**Goal**: The visible canvas becomes a PDF with sensible defaults.

**Independent Test**: A fixture page exports to a PDF that opens and shows the visible layers; with the original
layer hidden, the original is not in it.

- [x] T035 [P] [US4] Tests in `apps/haex-notes/app/lib/paperNotes/exportPdf.test.ts`: defaults of plan.md, each setting changes the output, a hidden original layer is left out, one page per PDF page
- [x] T036 [US4] Add `pdf-lib` to `apps/haex-notes/package.json`, raster the visible page and embed it with `pdf-lib` in `apps/haex-notes/app/lib/paperNotes/exportPdf.ts`
- [x] T037 [US4] Export dialog with paper size, orientation, margins, quality and original visibility, opened from the notebook page

**Checkpoint**: Quickstart step 8.

---

## Phase 6: Polish

- [ ] T038 [P] Check every new or changed file of haex-notes stays under 500 lines
- [ ] T039 Run `pnpm test` and `pnpm build` in `apps/haex-notes/`
- [ ] T040 Walk through quickstart.md against holzi with Ollama and record the evidence of its last section in the PR description
- [ ] T041 Update the state of spec 046 in holzi (`spec.md`, ticks in this file) after the merge in haextension

---

## Dependencies & Execution Order

- The close-up tasks T042–T045 were added to US3 later (research R13); they replace "read again" and need US1 and
  T029–T033. T034 holds their tests.
- Phase 1 → Phase 2 → US1 → US3 → US4 → Polish. US3 and US4 need the draft of US1 to be useful, but T030–T033 and
  T035–T036 only need Phase 2 and can start alongside US1.
- Within a phase the tests come first and must fail before the implementation.
- T025 and T026 need holzi with spec 055 for the manual check only; the automated tests use the fake `runPageJob`.

### Parallel examples

- Phase 2 tests: T005, T006, T007, T008.
- US1 tests: T015, T016, T017, T018, T019; then T020, T021, T023 side by side.

## Implementation Strategy

1. MVP: Phases 1, 2 and US1. A photo becomes a draft, read through holzi; every failure leaves the original.
2. Then US3 (editing) and US4 (PDF), each its own PR in haextension.
3. Each delivery is its own PR in haextension; this list is ticked in holzi after each merge (T041).
