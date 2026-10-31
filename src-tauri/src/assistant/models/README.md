# Models shipped with the voice assistant (spec 066)

| File                   | Source                                                                                                                                                                                                          | Licence | SHA-256                                                            |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------ |
| `silero_vad.int8.onnx` | [k2-fsa/sherpa-onnx release `asr-models`](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.int8.onnx), Silero VAD by [snakers4/silero-vad](https://github.com/snakers4/silero-vad) | MIT     | `c36d490aff5ab924ca6c7aeec4d8f6bd3d22db6fa17611b9c5b17eae58ac3a20` |

The pause detection model is 212 KB, so it ships inside the binary (`assistant::assets::SILERO_VAD`)
instead of being downloaded; `assistant::assets` writes it next to the downloaded models on first
use. The pause detection tests and the platform probe read the same file.
