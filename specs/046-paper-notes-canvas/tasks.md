---
description: 'Task list for Paper Notes to Editable Canvas'
---

# Tasks: Paper Notes to Editable Canvas

**Input**: Design documents from `/specs/046-paper-notes-canvas/`

**Implementation target**: `https://github.com/haex-space/haextension.git` at revision `1d24a491f24a20f5579ab16304c0a081fcd1fca8`, under `apps/haex-notes/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/recognition-provider.md, quickstart.md

**Tests**: Included because the project constitution requires runnable checks for non-trivial logic. Tests remain in separate `*.test.ts` files.

> **Stand 2026-10-16:** Alle Haken sind zurückgesetzt. Im Zielrepo `haex-space/haextension` gibt es weder `app/lib/paperNotes/` noch `pdf-lib` in `apps/haex-notes/package.json`, nichts davon ist gebaut. Diese Liste wird neu erzeugt, sobald Spec 055 geplant ist; sie folgt dann den Änderungen in `spec.md` (Abschnitt „Änderungen nach Messung und Entscheidung“).

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare the target extension and the small set of new runtime assets.

- [ ] T001 Prepare a dedicated implementation worktree for `haex-space/haextension` at revision `1d24a491f24a20f5579ab16304c0a081fcd1fca8` and verify `apps/haex-notes/package.json` is based on that revision.
- [ ] T002 Add `pdf-lib` to `apps/haex-notes/package.json` and update the target lockfile without adding a provider-specific OCR or KI SDK.
- [ ] T003 [P] Add only permissively licensed handwriting font files and their license notices under `apps/haex-notes/public/fonts/`.
- [ ] T004 [P] Add the initial handwriting style registry and fallback names in `apps/haex-notes/app/lib/paperNotes/fonts.ts`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish the persisted page model, asset/rendering primitives, and provider boundary required by all user stories.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Tests first

- [ ] T005 [P] Add model validation tests for `PaperNoteProcessing`, `RecognitionRegion`, normalized source boxes, review states, and the rule that failed processing retains `originalAssetId` in `apps/haex-notes/app/lib/paperNotes/model.test.ts`.
- [ ] T006 [P] Add page migration and serialization tests for ordinary pages and pages with `paperImport` in `apps/haex-notes/app/lib/migratePage.test.ts`.
- [ ] T007 [P] Add provider contract tests for preparation/capability responses, valid and invalid recognition results, low-confidence and unknown regions, and the typed failure envelope in `apps/haex-notes/app/lib/paperNotes/recognition.test.ts`.
- [ ] T008 [P] Add text/image renderer tests covering font selection, opacity, asset loading, missing assets, and page-coordinate placement in `apps/haex-notes/app/lib/render/elements.test.ts`.
- [ ] T009 [P] Extend asset collection tests to cover original-photo and drawing-crop asset IDs in `apps/haex-notes/app/lib/collectAssets.test.ts`.

### Implementation

- [ ] T010 Extend `PageDoc`, `PageElement` metadata, and page-layer types with the optional `paperImport` model while preserving existing `TextElement`, `ImageElement`, and `PageLayer` compatibility in `apps/haex-notes/app/types/document.ts`.
- [ ] T011 Add schema storage for the optional `paper_import` page field and keep the existing legacy stroke/table/background columns in `apps/haex-notes/app/database/schemas/index.ts`.
- [ ] T012 Implement migration and serialization for `paperImport`; ordinary pages without the field must load unchanged and unknown import versions must not become empty pages in `apps/haex-notes/app/lib/migratePage.ts`.
- [ ] T013 Implement the two-stage provider-neutral contract (`prepare`/`recognize`), capability and normalized-box validation, processing-state transitions, typed failures, and the deterministic no-network fixture provider in `apps/haex-notes/app/lib/paperNotes/recognition.ts` and `apps/haex-notes/app/lib/paperNotes/fixtureProvider.ts`.
- [ ] T014 Extend asset reference collection and page-sharing preparation so every original and generated image asset remains discoverable in `apps/haex-notes/app/lib/collectAssets.ts`.
- [ ] T015 Implement asynchronous image loading and a missing-asset placeholder using the existing asset store in `apps/haex-notes/app/composables/useAssetStore.ts` and `apps/haex-notes/app/lib/paperNotes/assets.ts`.
- [ ] T016 Extend the shared renderer to draw `TextElement` and `ImageElement` with page coordinates, rotation, opacity, handwriting font fallback, and cached asset images in `apps/haex-notes/app/lib/render/elements.ts`.
- [ ] T017 Extend notebook commands and store integration so adding, replacing, mutating, hiding, and locking imported elements remains undoable and persists through `apps/haex-notes/app/lib/commands.ts` and `apps/haex-notes/app/stores/notebook.ts`.

**Checkpoint**: Existing pages still render, imported model data has deterministic tests, and text/image elements can be rendered and saved without a recognition provider.

---

## Phase 3: User Story 1 - Foto in einen sauberen Entwurf umwandeln (Priority: P1) 🎯 MVP

**Goal**: Import one paper photo, preserve the original, and create a first clean draft with text and visual regions separated.

**Independent Test**: Import a representative single-page photo with text, a drawing, and a crossed-out passage; verify that a page with a locked original layer and separated draft elements is created even when automatic processing fails.

### Tests for User Story 1

- [ ] T018 [P] [US1] Add import mapping tests for local working-copy preprocessing, quality warnings, original asset creation, page-fit geometry, locked original layer, text regions, drawing crops, and manual fallback in `apps/haex-notes/app/lib/paperNotes/importPage.test.ts`.
- [ ] T019 [P] [US1] Add provider lifecycle tests for preparation before byte transfer, capability negotiation, consent metadata, local/remote processing status, schema rejection, success, and typed failure in `apps/haex-notes/app/lib/paperNotes/recognitionAdapter.test.ts`.

### Implementation for User Story 1

- [ ] T020 [US1] Implement single-image import, local working-copy preprocessing, quality warnings with retake/best-effort choices, original-asset persistence, page-size normalization, and creation of the visible locked original layer in `apps/haex-notes/app/lib/paperNotes/importPage.ts` and `apps/haex-notes/app/lib/paperNotes/preprocess.ts`.
- [ ] T021 [US1] Implement local cropping of drawing/unknown regions into asset-store images and map recognized text regions into `TextElement`s with source-region IDs in `apps/haex-notes/app/lib/paperNotes/importPage.ts`.
- [ ] T022 [US1] Implement the provider adapter lifecycle with preflight before byte transfer, processing status, remote-destination metadata, validation failure, and manual fallback in `apps/haex-notes/app/composables/usePaperNoteRecognition.ts`.
- [ ] T023 [US1] Build the import dialog with mandatory file selection, optional in-runtime camera capture, image preview, quality warning, processing destination disclosure, explicit remote-processing consent, progress, and failure recovery in `apps/haex-notes/app/components/notes/PaperNoteImportDialog.vue`.
- [ ] T024 [US1] Add the paper-note import action to the notebook toolbar and connect successful imports to the current page/notebook flow in `apps/haex-notes/app/pages/notebook/[id].vue`.
- [ ] T025 [US1] Add localized labels and error messages for import, processing location, fallback, original layer, and unsupported images in the page-local `<i18n>` block of `apps/haex-notes/app/pages/notebook/[id].vue` and the import dialog.

**Checkpoint**: User Story 1 is independently usable: a student can import a photo, see a preserved original and a first structured draft, or continue manually after provider failure.

---

## Phase 4: User Story 2 - Erkannte Inhalte prüfen und korrigieren (Priority: P1)

**Goal**: Make recognition uncertainty, spelling suggestions, crossed-out passages, and source provenance easy to review and correct.

**Independent Test**: With a fixture page containing one low-confidence word, one spelling suggestion, and one ambiguous crossed-out region, accept one suggestion, edit one word, reject one candidate, and open each original source crop without changing the original layer.

### Tests for User Story 2

- [ ] T026 [P] [US2] Add review-state tests for accept, edit, reject, undo, source-region lookup, and preservation of `recognizedText` in `apps/haex-notes/app/lib/paperNotes/review.test.ts`.
- [ ] T027 [US2] Add UI mapping cases for confidence markers and candidate lists to `apps/haex-notes/app/lib/paperNotes/review.test.ts`.

### Implementation for User Story 2

- [ ] T028 [US2] Implement undoable review operations that apply or reject suggestions, update `TextElement.text`, preserve the recognized candidate, and set the documented review state in `apps/haex-notes/app/lib/paperNotes/review.ts`.
- [ ] T029 [US2] Build the review panel with low-confidence markers, spelling/recognition suggestions, custom correction input, review notes, and explicit crossed-out ambiguity in `apps/haex-notes/app/components/notes/PaperNoteReviewPanel.vue`.
- [ ] T030 [US2] Add original-region lookup and a crop preview that reads the original asset without mutating the original layer in `apps/haex-notes/app/components/notes/PaperNoteReviewPanel.vue` and `apps/haex-notes/app/lib/paperNotes/assets.ts`.
- [ ] T031 [US2] Connect region selection, review overlays, and source-crop opening to the canvas without allowing selection or mutation of the locked original layer in `apps/haex-notes/app/components/notes/PageCanvas.vue`.
- [ ] T032 [US2] Add localized review labels, confidence explanations, suggestion actions, and provider-error messages in the page-local `<i18n>` block of `apps/haex-notes/app/pages/notebook/[id].vue` and the review panel.

**Checkpoint**: User Story 2 is independently testable with the fixture provider and produces a reversible, user-confirmed clean text result.

---

## Phase 5: User Story 3 - Den Entwurf als Canvas weiterbearbeiten (Priority: P1)

**Goal**: Let students edit text, move/resize drawings, switch original/clean/compare views, and reopen the saved page without data loss.

**Independent Test**: Open a fixture page with at least one text and one image object, edit the text, move and resize the image, toggle the three views, save/reopen, and verify that all objects and visibility states remain intact.

### Tests for User Story 3

- [ ] T033 [P] [US3] Add hit-testing and transform tests for text/image selection, move, resize, rotation, locked-layer exclusion, and BBox updates in `apps/haex-notes/app/lib/paperNotes/geometry.test.ts`.
- [ ] T034 [P] [US3] Add handwriting-style registry tests for available styles, missing-font fallback, and persisted `fontFamily` values in `apps/haex-notes/app/lib/paperNotes/fonts.test.ts`.
- [ ] T035 [P] [US3] Add save/reopen tests for text/image layers, original visibility, review metadata, and asset references in `apps/haex-notes/app/lib/paperNotes/persistence.test.ts`.

### Implementation for User Story 3

- [ ] T036 [US3] Add selection, hit testing, drag, resize, and locked-layer protection for text and image elements in `apps/haex-notes/app/components/notes/PageCanvas.vue`.
- [ ] T037 [US3] Implement the handwriting-style registry, system-font fallback, and user-selectable styles in `apps/haex-notes/app/lib/paperNotes/fonts.ts`, `apps/haex-notes/app/components/notes/PaperNoteReviewPanel.vue`, and `apps/haex-notes/app/components/notes/PageCanvas.vue`.
- [ ] T038 [US3] Add Original, Clean, and Compare visibility controls while keeping the original layer visible-but-locked by default in `apps/haex-notes/app/components/notes/PageCanvas.vue` and `apps/haex-notes/app/pages/notebook/[id].vue`.
- [ ] T039 [US3] Ensure text/image transforms, visibility changes, and style changes use existing undoable commands and survive save/reopen in `apps/haex-notes/app/stores/notebook.ts` and `apps/haex-notes/app/lib/commands.ts`.
- [ ] T040 [US3] Add accessible labels, keyboard focus, and localized controls for selecting, moving, resizing, hiding, and comparing imported content in `apps/haex-notes/app/components/notes/PageCanvas.vue` and `apps/haex-notes/app/i18n/locales/de.json`.

**Checkpoint**: User Story 3 is independently testable with fixture data and delivers the editable Canvas experience even without a live recognition service.

---

## Phase 6: User Story 4 - Eine lesbare Version exportieren (Priority: P2)

**Goal**: Export the currently visible page composition to a readable PDF with original visibility respected.

**Independent Test**: Export one page with the original layer hidden and once with it visible; both PDFs open successfully and contain exactly the respective visible composition.

### Tests for User Story 4

- [ ] T041 [P] [US4] Add PDF export tests for visible-layer filtering, page dimensions, image embedding, export failure, and non-empty PDF output in `apps/haex-notes/app/lib/paperNotes/exportPdf.test.ts`.

### Implementation for User Story 4

- [ ] T042 [US4] Implement a configurable high-resolution export renderer with sensible defaults for paper size, orientation, margins, quality, visible layers, page coordinates, text fonts, image assets, and opacity in `apps/haex-notes/app/lib/paperNotes/exportPdf.ts`.
- [ ] T043 [US4] Implement configurable PDF creation, default settings, download naming, and user-visible export errors with `pdf-lib` in `apps/haex-notes/app/lib/paperNotes/exportPdf.ts` and `apps/haex-notes/app/pages/notebook/[id].vue`.
- [ ] T044 [US4] Add the export action and disabled/loading/error states to `apps/haex-notes/app/pages/notebook/[id].vue`.
- [ ] T045 [US4] Add localized export labels, progress, success, and error messages in the page-local `<i18n>` block of `apps/haex-notes/app/pages/notebook/[id].vue`.

**Checkpoint**: User Story 4 is independently testable with a saved fixture page and produces a valid PDF matching the selected visibility state.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Validate the complete feature, harden failure handling, and document the pilot workflow.

- [ ] T046 [P] Add unsupported-mime, decode/preprocessing-failure, missing-asset, malformed-provider-response, capability, consent, and provider-timeout tests in `apps/haex-notes/app/lib/paperNotes/errorHandling.test.ts`.
- [ ] T047 [P] Add a set of consented/de-identified representative reference photos with a manifest and expected region annotations under `apps/haex-notes/app/lib/paperNotes/fixtures/`, covering clear writing, poor lighting, perspective, crossed-out text, drawings, tables, and unknown regions without including student-identifying data.
- [ ] T048 Review all user-visible processing disclosures and confirm no provider credential, local absolute path, or personal handwriting sample is persisted in `apps/haex-notes/app/lib/paperNotes/` and `apps/haex-notes/app/database/schemas/index.ts`.
- [ ] T049 Run `pnpm test` and `pnpm build` from `apps/haex-notes/`, fixing regressions without weakening original-preservation or review invariants.
- [ ] T050 Execute the manual scenarios for one image at a time against the target extension and record deterministic fixture evidence for SC-001 through SC-006; record SC-007 only as optional later pilot feedback.
- [ ] T051 Update `specs/046-paper-notes-canvas/quickstart.md` with any environment-specific prerequisites discovered during the manual run.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No feature-code dependency; must pin the target revision and prepare the PDF/font assets.
- **Foundational (Phase 2)**: Depends on Setup and blocks all user-story phases.
- **User Story 1 (Phase 3)**: Depends on Foundational; this is the strict MVP increment.
- **User Story 2 (Phase 4)**: Depends on Foundational and the imported region/provenance output from US1, but can be tested with the fixture provider.
- **User Story 3 (Phase 5)**: Depends on Foundational and the element/layer shape from US1; its editing tests use fixture pages and do not require a live provider.
- **User Story 4 (Phase 6)**: Depends on the shared renderer from Foundational and the visible-layer behavior from US3.
- **Polish (Phase 7)**: Depends on all desired stories being complete.

### User Story Dependencies

```text
Foundational
    ├── US1 Import and first draft
    │      └── US2 Review and correction
    ├── US3 Canvas editing
    │      └── US4 PDF export
    └── Polish / pilot validation
```

### Parallel Opportunities

- T003 and T004 can run in parallel after T001.
- T005–T009 are independent test additions and can run in parallel before T010–T017.
- T018 and T019 can run in parallel; T020–T022 then implement their tested boundaries.
- T026 and T027 are separate review-test groups but share `review.test.ts`; complete them before T028–T030 implement review behavior and UI.
- T033–T035 can run in parallel; T036–T039 then implement Canvas editing and persistence.
- T046 and T047 can run in parallel with final documentation review.

## Parallel Example: User Story 1

```text
T018: import mapping tests in apps/haex-notes/app/lib/paperNotes/importPage.test.ts
T019: provider lifecycle tests in apps/haex-notes/app/lib/paperNotes/recognitionAdapter.test.ts
T023: import dialog in apps/haex-notes/app/components/notes/PaperNoteImportDialog.vue
```

T023 can be developed against the fixture provider while T020–T022 stabilize the
mapping and adapter implementation; integration is required before the story
checkpoint.

## Implementation Strategy

### Strict MVP

1. Complete Setup and Foundational phases.
2. Complete User Story 1.
3. Validate that a paper photo becomes a structured draft and that provider failure leaves a usable original/manual Canvas.
4. Stop for a pilot review before adding personalization or a live provider model.

### Usable classroom increment

1. Add User Story 2 so students can correct uncertain words safely.
2. Add User Story 3 so text and drawings can be edited and saved.
3. Validate with real student-created, de-identified sample pages.

### Delivery after the classroom increment

1. Add User Story 4 for PDF export.
2. Run the complete quickstart and collect the success-criterion evidence.
3. Consider a separate later feature for a device-local personal handwriting profile; do not add it to this task list retroactively.

## Notes

- `[P]` tasks touch separate files and have no dependency on incomplete work in the same phase.
- Every user-story task includes its story label and an exact target path.
- Tests are kept in separate files from production code.
- The original image is never overwritten; reprocessing requires an explicit user action.
