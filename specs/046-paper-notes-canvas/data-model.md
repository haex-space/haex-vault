# Data Model: Paper Notes to Editable Canvas

## Existing model to extend

The implementation target already stores a page as `PageDoc` with `PageLayer[]`,
`PageElement[]`, a `PageBackground`, and page dimensions. `TextElement` and
`ImageElement` are the canonical visual elements. Existing commands provide
undoable add, remove, replace, and mutate operations.

## `PaperNoteImport`

Optional metadata on a page. It is absent for ordinary handwritten or blank pages.

| Field | Type | Rules |
|---|---|---|
| `version` | integer | Starts at `1`; unknown future versions must not be silently discarded. |
| `originalAssetId` | string | References the unchanged imported image in the asset store. |
| `sourceMimeType` | string | Must describe an imported image supported by the import flow. |
| `sourceWidth` / `sourceHeight` | positive integer | Original pixel dimensions used to interpret normalized source boxes. |
| `processing` | `PaperNoteProcessing` | Tracks readiness, failure and processing destination. |
| `regions` | `RecognitionRegion[]` | One entry per recognized or reviewable source area. |
| `createdAt` | ISO timestamp | Set once when the import is created. |
| `updatedAt` | ISO timestamp | Updated when recognition or review state changes. |

The object is persisted with the page and must be included when a page is shared,
provided the referenced assets are included by `collectAssetIds()`.

## `PaperNoteProcessing`

```text
state: "manual" | "processing" | "ready" | "failed"
mode: "local" | "remote" | "unknown"
destination?: string
provider?: string
error?: string
startedAt?: ISO timestamp
finishedAt?: ISO timestamp
```

Validation rules:

- `destination` is user-facing metadata only; credentials and access tokens are never stored here.
- `state = "failed"` requires a non-empty `error` while retaining the original asset.
- `state = "ready"` requires a non-empty region list or an explicit empty-page result.
- A remote `mode` requires the import UI to show the destination before processing.

## `RecognitionRegion`

Each region links source coordinates to one or more generated canvas elements.

| Field | Type | Rules |
|---|---|---|
| `id` | string | Stable within the import. |
| `kind` | `"text" \| "drawing" \| "unknown"` | Unknown regions remain reviewable. |
| `sourceBox` | normalized `BBox` | `[x, y, width, height]`, each coordinate between `0` and `1`; width and height must be positive. |
| `elementIds` | string[] | IDs of generated `TextElement`/`ImageElement`s; may be empty for unresolved regions. |
| `recognizedText` | string? | Raw text returned by recognition for text regions. |
| `suggestions` | `TextSuggestion[]` | Ordered spelling or interpretation suggestions; never applied automatically. |
| `confidence` | number? | Provider confidence in `[0, 1]`; absent means unknown, not zero. |
| `review` | `"unreviewed" \| "accepted" \| "edited" \| "rejected"` | `rejected` hides the generated candidate but keeps the source reference. |
| `reviewNote` | string? | Optional user note for unresolved content. |

`TextElement.text` is the current canonical text after a user accepts or edits a
candidate. `recognizedText` is retained to make the change reversible and
auditable. A drawing region points to an `ImageElement` cropped from the original
asset; the original region is never deleted.

## `TextSuggestion`

```text
text: string
kind: "spelling" | "recognition" | "manual"
confidence?: number
```

Suggestions are display data. Applying one is an undoable element mutation and
changes the region review state to `accepted` or `edited`.

## Layer arrangement

An imported page is composed in this order:

```text
paper background
original import layer (visible, locked by default)
clean text layer (visible, editable)
visual objects layer (visible, editable)
user additions layer (visible, editable)
```

The exact order may be represented by existing `PageLayer[]` order. The original
layer must not be the active drawing layer and must be excluded from element
mutations originating in the editor.

## State transitions

```text
manual ──start processing──> processing
processing ──success────────> ready
processing ──failure────────> failed
failed ──manual editing─────> manual
ready ──user edits──────────> ready (region review state changes independently)
```

All transitions retain `originalAssetId`. Reprocessing must create a new candidate
set or replace only generated elements after an explicit user action; it must not
overwrite the original layer or confirmed user edits silently.

## Migration and compatibility

- Ordinary pages without `paperImport` load exactly as before.
- Page serialization must preserve legacy stroke/table/background columns while adding the new JSON field.
- Unknown `paperImport.version` values are kept as opaque metadata or surfaced as unsupported; they must not be converted to an empty page.
- Missing image assets render a placeholder and keep the element metadata so the page can recover if the asset becomes available.
