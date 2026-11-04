# Data model: Faster local answers with multi-token prediction

No database table changes. A drafter's state is derived from the catalog (what it should be) and from the
file system (what is there), like an installed model since migration 0012 (research R3, R5).

## CatalogDrafter (new, in `model_catalog.json`)

An optional `drafter` object on a catalog entry.

| Field                    | Type                  | Rule                                                                                                  |
| ------------------------ | --------------------- | ----------------------------------------------------------------------------------------------------- |
| `hf_repo`                | string                | Hugging Face repository                                                                               |
| `hf_revision`            | string, 40 hex        | Full commit SHA; never a branch (research R2)                                                         |
| `hf_filename`            | string, `*.gguf`      | File in the repository, without a directory                                                           |
| `size_bytes`             | integer > 0           | Exact size; a download of another size is refused                                                     |
| `sha256`                 | string, 64 hex        | Exact hash; a download with another hash is discarded                                                 |
| `architecture`           | string                | `general.architecture` of the drafter (`gemma4-assistant` for Gemma 4); checked at load               |
| `measured_partly_on_gpu` | bool, default `false` | Drafter allowed with the model partly on the GPU (research R7 rule 3); set only after a measured gain |

A catalog test checks every drafter: the revision is 40 hex characters, the hash 64, the file name ends in
`.gguf` and has no `/`, and the size is positive.

## CatalogEntry (extended)

| Field     | Change                                                                                   |
| --------- | ---------------------------------------------------------------------------------------- |
| `drafter` | New, optional `CatalogDrafter`. The first delivery sets it for the three Gemma 4 entries |

`approx_size_bytes` stays the model's own size. The size shown before a download is
`approx_size_bytes + drafter.size_bytes` (contracts).

## InstalledDrafter (derived)

For an installed model whose catalog entry names a drafter:

| State       | When                                                                      |
| ----------- | ------------------------------------------------------------------------- |
| `installed` | `models/<slug>/drafter/<hf_filename>` exists and has exactly `size_bytes` |
| `missing`   | otherwise; the model list offers the download (spec FR-010)               |

The hash is checked when a drafter is downloaded and again when a model loads with it (research R4, R6). A drafter
that fails the check at load:

- is not used, and the reason is logged (spec FR-008);
- has its `drafter/` folder removed.

The model list then shows it as `missing` and offers the download, so a damaged file never shows as an active
speed-up.

## InstalledModelPayload (extended)

| Field       | Change                                                                                      |
| ----------- | ------------------------------------------------------------------------------------------- |
| `sizeBytes` | Includes an installed drafter                                                               |
| `drafter`   | New, optional: `{ installed: bool, sizeBytes: number }`, present when the catalog names one |

## Placement (extended)

| Field     | Change                                                       |
| --------- | ------------------------------------------------------------ |
| `drafter` | New `bool`: whether this load uses the drafter (research R7) |

## ModelShape (extended)

| Field     | Change                                                                                      |
| --------- | ------------------------------------------------------------------------------------------- |
| `drafter` | New, optional: the drafter's file size and cache bytes per token, read from its GGUF header |

## State of one generation

```text
load ──► Placement.drafter = true ──► speculative loop ──► (error) ──► plain loop on the same context
     └─► Placement.drafter = false ─► plain loop (today)
```
