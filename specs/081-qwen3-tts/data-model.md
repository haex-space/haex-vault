# Data Model: Qwen3-TTS as haex's natural voice

No table and no migration. Everything new is either part of the built-in catalog, a device preference or a file
beside the model.

## Speech model entry (`tts_catalog.json`, `tts::catalog::TtsCatalogEntry`)

| Field                                 | Type                        | Change              | Rule                                                                                                                                        |
| ------------------------------------- | --------------------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`                                  | string                      | as today            | Unique. New: `qwen3-tts-1.7b`, `pocket-de-v4`; `pocket-de` is removed (R11).                                                                |
| `name`                                | string                      | as today            |                                                                                                                                             |
| `engine`                              | `pocket` \| `qwen3-tts`     | extended            | `qwen3-tts` only offered where `engine_enabled` (build with `voice-qwen`).                                                                  |
| `languages`                           | string[]                    | replaces `language` | At least one of the six languages of 066 FR-052. Pocket entries: one; Qwen3-TTS: six.                                                       |
| `quality_rank`                        | integer                     | new                 | Higher sounds better; Qwen3-TTS 20, Pocket 10. Used by the automatic choice (FR-012).                                                       |
| `hf_repo`, `hf_revision`, `hf_folder` | string                      | as today            | `hf_revision` is a full commit; `hf_folder` may be empty (repository root).                                                                 |
| `files`                               | `{ name, bytes, sha256 }[]` | new, optional       | When present, the download fetches exactly these files and checks each SHA-256 once. Pocket entries keep the fixed file list of `files.rs`. |
| `approx_size_bytes`                   | integer                     | as today            | Sum of all files.                                                                                                                           |
| `license`, `attribution`              | string                      | as today            | Qwen3-TTS: `Apache-2.0`, "Qwen3-TTS by the Qwen team (Apache-2.0); GGUF conversion by ggml-org".                                            |
| `variant`                             | `standard` \| `24l`         | as today            | Qwen3-TTS: `standard`.                                                                                                                      |
| `sample_rate`                         | integer                     | as today            | Qwen3-TTS: read from `mtmd_gen_audio_get_info` at load; the catalog value (24000) is a hint.                                                |
| `temperature`                         | string                      | new, Pocket only    | Pocket sampling temperature (FR-015); `0.3` for `pocket-de-v4`, `0.7` for the others.                                                       |

`default_for(language)` becomes `pocket_for(language)`: the Pocket entry of the language (standard before 24l).
The default (Qwen3-TTS where the build runs it, else Pocket; FR-012) is in `tts::choice`.

## Device preferences (`PrefScope::Device`, saved on selection)

| Key                                | Value                            | Rule                                                                                                       |
| ---------------------------------- | -------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `assistant.speechModel.<language>` | catalog id, absent = the default | Must name an entry whose `languages` contains `<language>`; an unknown or disabled id counts as automatic. |
| `assistant.speechModelHint`        | `dismissed`, absent = not shown  | One per device; once dismissed the hint of FR-011 never shows again on this device.                        |

## Speed measurement (`models/<slug>/speed.json`, device local)

```json
{
  "version": 2,
  "runs": {
    "gpu": {
      "sentences": [
        {
          "parts": [
            [1.21, 3.4],
            [0.22, 0.8]
          ]
        },
        { "parts": [[0.88, 2.5]] }
      ],
      "slowStreak": 0,
      "measuredAtMs": 1793870000000
    },
    "cpu": {
      "sentences": [{ "parts": [[4.6, 3.4]] }],
      "slowStreak": 2,
      "measuredAtMs": 1793822400000
    }
  }
}
```

- `sentences`: the last 10 sentences, newest last; each sentence holds its generated parts as
  `[compute seconds, audio seconds]` pairs; a part's computing time includes its retries and, when the
  reserve spoke it, the reserve's time. `measuredAtMs` is Unix milliseconds.
- **Ratio** = sum of compute / sum of audio across all parts in `sentences`; **keeps up** when ratio < 1.0 and at
  least 3 sentences exist;
  fewer sentences means "noch nicht gemessen".
- `slowStreak`: consecutive answers whose own ratio was ≥ 1.0; reset by an answer below 1.0. At 3, the hint fires
  once (FR-011).
- Unknown versions or unreadable files count as empty. The file is deleted with the model.
- Version 2 (2026-11-06, FR-016): computing time excludes waiting for the compute unit, and speech no longer
  computes beside the chat model on the same unit; version-1 files measured that slowdown and count as empty.
- FR-017 reads the ratio of the run at the start of every answer to decide how long to hold playback.

## Speech slot (`voice::VoiceState.speech`, memory only)

`Option<(SpeechKey, Arc<dyn Engine>)>` with `SpeechKey { model_id, acceleration: Gpu | Cpu }`. A different key
drops the old engine before the new one loads (as today). The ledger's eviction (contracts/gpu-ledger.md) replaces
a `Gpu` key by a `Cpu` key at the next part boundary.

## Part (`tts::parts::Part`, memory only)

`{ text, protected: bool }` in order; at most 20 words; never splits a protected span (R5). The speaker's ticket
stays per sentence: 066's transcript ("spoken up to") is unchanged.
