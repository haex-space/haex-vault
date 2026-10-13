# Recognition Provider Contract

This contract is the boundary between haex-notes and Holzi's structured
paper-note agent task. haex-notes owns image selection, local preprocessing,
quality warnings, original preservation, Canvas mapping, and export. Holzi
selects the configured harness/model and returns only a validated structured
recognition result. The contract deliberately does not name a vendor SDK or
transport.

## Preparation

The client performs a preparation call before sending image bytes. This makes
the processing destination known before remote processing can occur.

```text
schemaVersion: 1
task: "paper-note-recognition"
operation: "prepare"
profileId?: string
locale: string
options:
  detectDrawings: boolean
  suggestSpelling: boolean
```

The response is:

```text
schemaVersion: 1
status: "ready" | "unsupported"
processing:
  mode: "local" | "remote"
  destination?: string
capabilities:
  vision: boolean
  drawingDetection: boolean
  spellingSuggestions: boolean
```

`destination` is user-readable and required for remote processing. The client
must request explicit consent before sending image bytes when `mode` is
`remote`.

## Recognition request

```text
schemaVersion: 1
task: "paper-note-recognition"
operation: "recognize"
requestId: string
profileId?: string
locale: string
image:
  bytes: binary image payload
  mimeType: string
  width: positive integer
  height: positive integer
options:
  detectDrawings: boolean
  suggestSpelling: boolean
```

The image is the locally prepared working copy, not a filesystem path. The
original image remains under haex-notes control. Credentials, local paths, and
personal handwriting profiles must never be included in the request.

## Successful response

```text
schemaVersion: 1
status: "completed"
requestId: string
provenance:
  profileId?: string
  mode: "local" | "remote"
  destination?: string
regions: [
  {
    id: string,
    kind: "text" | "drawing" | "unknown",
    sourceBox: [x, y, width, height],   // normalized to 0..1
    text?: string,
    confidence?: number,                 // 0..1
    suggestions?: [
      { text: string, kind: "spelling" | "recognition", confidence?: number }
    ]
  }
]
warnings: [
  { code: string, message: string }
]
```

Regions must be bounded, non-empty, and lie within the normalized source
image. Text and suggestions are candidates; the client must not replace
user-visible text without confirmation. A drawing region only describes its
source box; the client creates the cropped image asset locally from the
original photo. Unknown or low-confidence regions must remain in the response
so the review UI can mark them.

## Failure response

```text
schemaVersion: 1
status: "failed"
requestId: string
error:
  code: "unsupported-capability" | "invalid-input" | "timeout" |
        "unavailable" | "invalid-response" | "processing-failed" |
        "cancelled"
  message: string
  retryable: boolean
```

Provider failure must not invalidate the imported original asset. A response
that violates the schema may be rejected and must fall back to manual editing.

## Lifecycle and invariants

```text
idle → preparing → awaiting-consent? → processing → completed | failed
```

- The task must validate the structured response before it reaches the Canvas mapper.
- A text-only model is not sufficient; unsupported capabilities must be explicit.
- Free-form chat text must never be interpreted directly as a Canvas document.
- The client stores only non-secret provenance such as profile ID, processing mode, and status.
- The contract is versioned independently from provider/model selection.

## Test fixture

The deterministic fixture provider returns a fixture page containing text, a
drawing, a spelling suggestion, and one low-confidence/crossed-out region. It
never makes network requests and is used by the annotated reference-fixture
tests and the manual quickstart.
