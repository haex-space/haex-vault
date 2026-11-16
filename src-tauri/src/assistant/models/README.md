# Models shipped with the voice assistant (spec 066)

| File                   | Source                                                                                                                                                                                                          | Licence | SHA-256                                                            |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------ |
| `silero_vad.int8.onnx` | [k2-fsa/sherpa-onnx release `asr-models`](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.int8.onnx), Silero VAD by [snakers4/silero-vad](https://github.com/snakers4/silero-vad) | MIT     | `c36d490aff5ab924ca6c7aeec4d8f6bd3d22db6fa17611b9c5b17eae58ac3a20` |

The pause detection model is 212 KB, so it ships inside the binary (`assistant::assets::SILERO_VAD`)
instead of being downloaded; `assistant::assets` writes it next to the downloaded models on first
use. The pause detection tests and the platform probe read the same file.

## Wake word models (`../wakeword/models/`)

| File                    | Source                                                                                                                                  | Licence    | SHA-256                                                            |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ------------------------------------------------------------------ |
| `speech_embedding.onnx` | [haex-space/haex-wakeword@d0d2dc6](https://huggingface.co/haex-space/haex-wakeword/tree/d0d2dc6225c3bb1103756ebaa15a4e1d16cd77b8) (Google speech_embedding/1) | Apache-2.0 | `092fa584098a0c0e5c5382d028eed7e7e7d5e41ccf23786e7417248d0aabe8c6` |
| `hex.onnx`              | same revision, the classifier for "Haex" / "Hey Haex" (spoken "Hex")                                                                    | Apache-2.0 | `c67e7a3686741c9ff2ea14de714611b54c64e7ddae3eb4522015e02c0e9ec70a` |

Both ship inside the binary (3 MB, `BUNDLED_VERSION` 1 in `assistant::wakeword::update`), so the
default wake word needs no download. A newer version reaches devices without an app release: push
the new files to `haex-space/haex-wakeword` and add `update.json` at its `main` with the next
`version`, the commit `revision` and `bytes` + `sha256` of both files (shape in the module docs of
`update.rs`). When a release bundles a newer version, replace the two files here, raise
`BUNDLED_VERSION` and recompute the expected values in `wakeword/model_tests.rs`.
