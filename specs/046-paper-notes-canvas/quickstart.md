# Quickstart: Paper Notes to Editable Canvas

This guide validates the feature in the target repository
`https://github.com/haex-space/haextension.git` at revision
`1d24a491f24a20f5579ab16304c0a081fcd1fca8`, under `apps/haex-notes/`.

## Prerequisites

- A working haex-notes development environment with the existing haex runtime.
- A set of consented/de-identified reference photo fixtures containing handwritten text, drawings, spelling mistakes, crossed-out or ambiguous words, and quality variations; alternatively a runtime with an accessible camera and a file-selection dialog.
- A deterministic fixture recognition provider for automated checks.
- A real local or remote recognition provider only for the manual processing-location check.

## Automated checks

From `apps/haex-notes/`:

```sh
pnpm install
pnpm test
pnpm build
```

Expected results:

- Model and serialization tests preserve ordinary pages and imported pages.
- Recognition mapping rejects invalid boxes and retains low-confidence regions.
- Review commands are undoable and do not modify the original layer.
- Image assets are collected for sharing and missing assets produce placeholders.
- Text and image elements render in the canvas and in the export snapshot.
- The extension builds without requiring a concrete cloud-provider SDK or secret.

## Manual flow

1. Open or create a notebook and choose the paper-note import action.
2. Capture a photo directly in the runtime when camera access is available, or select one file through the file dialog. Confirm that the original asset is stored and shown on a visible, locked layer.
3. Verify that haex-notes creates a local working copy, reports poor quality with a retake/best-effort choice, and processes one image at a time.
4. Start processing with the fixture provider. Verify that text, drawing, unknown, and low-confidence regions are mapped to the canvas.
5. Open the review panel. For the spelling suggestion, accept it; for the ambiguous word, enter a custom correction; for the drawing, keep the original crop.
6. Toggle Original, Clean, and Compare views. The original pixels must remain unchanged.
7. Select the drawing and move/resize it. Select a text block and change its text and handwriting style.
8. Save, close, and reopen the page. Confirm that all layers, review states, positions, and asset references remain intact.
9. Export the visible canvas as PDF using the defaults, then repeat with changed paper size, orientation, quality, and original-layer visibility. Verify that the output matches the selected settings.
10. Repeat processing with a provider failure. Confirm that the original image remains accessible and manual editing remains possible.
11. Repeat with a remote provider configuration. Confirm that the destination is shown before image bytes leave the device.

## Acceptance evidence

Record for each reference fixture:

- elapsed time from import to first draft or quality warning for every reference fixture;
- number of annotated uncertain regions shown versus expected;
- whether one correction, one image move/resize, save/reopen, and each PDF setting combination succeeded;
- whether the original layer remained byte-identical;
- whether failed processing still left a usable manual canvas.

The results are used to evaluate SC-001 through SC-006 from the feature
specification. SC-007 is optional later pilot feedback, not a technical release
gate. The fixtures validate the contract and Canvas behavior; they are not a
substitute for evaluating a live recognition model on a representative German
handwriting dataset.
