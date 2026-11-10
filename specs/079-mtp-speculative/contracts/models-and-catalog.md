# Contract: catalog, model commands and events

Changes visible to the frontend and to the catalog file. Everything not named here stays as it is.

## `model_catalog.json`: a drafter on an entry

```json
{
  "id": "gemma-4-e4b-instruct-q4_k_m",
  "hf_repo": "unsloth/gemma-4-E4B-it-GGUF",
  "hf_filename": "gemma-4-E4B-it-Q4_K_M.gguf",
  "approx_size_bytes": 4980000000,
  "drafter": {
    "hf_repo": "unsloth/gemma-4-E4B-it-GGUF",
    "hf_revision": "bfc15c382204943c3a8fff0c750b94ae2364d7a3",
    "hf_filename": "mtp-gemma-4-E4B-it.gguf",
    "size_bytes": 98653248,
    "sha256": "b6a723115efa510d3b3215db1e26790dae84cd08c2134a764f3d194f1f0c3376",
    "architecture": "gemma4-assistant"
  }
}
```

`architecture` is the drafter's `general.architecture`, read from the three files on 2026-11-04: `gemma4-assistant`
(with a hyphen). The load checks that the file says the same before using it.

`measured_partly_on_gpu` (optional, default `false`) allows the drafter when the model fits the GPU only partly
(research R7 rule 3). No entry sets it until task T031 has measured the 12B.

## `list_catalog` (unchanged command, extended entry)

Each `CatalogEntryWithFit` carries the optional `drafter` object above. The frontend shows
`approx_size_bytes + drafter.size_bytes` as the model's size. The `fit` stays classified on the model's own size: a drafter that does not fit is left out at load (research
R7 rule 2), so it never makes a model too big.

## `model_download_check` (unchanged command)

For a catalog target, the size it checks is the model plus its drafter.

## `download_model_from_catalog` (unchanged signature, extended behaviour)

- **Drafter**: if the entry names one, it is downloaded as part of the same operation (research R4).
- **`model-download-progress`**: `{ modelId, bytesDownloaded, bytesTotal }` as today.
  - `bytesTotal` = the model's announced size + `drafter.size_bytes`.
  - `bytesDownloaded` counts both files.
- **`model-download-complete`**: the `InstalledModelPayload` below.
- **A drafter that fails its size or hash check**: it is discarded, the model is installed, and the command
  succeeds. The payload says `drafter.installed: false`.
- **`cancel_model_download(modelId)`**: cancels both files; nothing is published.

## `download_model_drafter(modelId)` (new command)

- **Purpose**: fetches the drafter of an installed model whose catalog entry names one (spec US5).
- **Progress and cancel**: the same `model-download-progress` and `model-download-complete` events, keyed by
  `modelId`. `cancel_model_download(modelId)` cancels it.
- **Errors**:
  - The model is not installed: `ModelNotFound`.
  - The catalog names no drafter: `InvalidInput`.
  - The hash or size does not match: `ModelDownload` with the reason.
- **Registration**: in `lib.rs` next to `download_model_from_catalog`. It touches the vault, so it stays off the vault
  gate's app-scoped allow-list (default deny).

## `InstalledModelPayload` (list and complete event)

```ts
interface InstalledModel {
  // … unchanged fields …
  sizeBytes: number // includes an installed drafter
  drafter?: { installed: boolean; sizeBytes: number } // only when the catalog names one
}
```

## `delete_installed_model(modelId)` (unchanged signature)

Removes the model file and the `drafter/` folder of the model.

## Settings, installed model details (UI)

- **Drafter installed**: one line, "Beschleunigung: aktiv" / "Speed-up: on".
- **Drafter missing**: "Beschleunigung verfügbar (98 MB)" / "Speed-up available (98 MB)", with a download
  button. The download uses the existing download bar of that model.
- **i18n**: German and English keys under `models.installed.drafter.*`.
