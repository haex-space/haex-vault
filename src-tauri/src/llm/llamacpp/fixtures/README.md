# Chat template fixtures

The chat template text of the models the llama.cpp engine supports, exactly as the model file carries it
(`tokenizer.chat_template` in the GGUF metadata), used by the tests to render real prompts.

| File            | Model (GGUF)                                                      | Architecture | Call format |
| --------------- | ----------------------------------------------------------------- | ------------ | ----------- |
| `qwen3.jinja`   | `bartowski/Qwen_Qwen3-4B-GGUF`, `Qwen_Qwen3-4B-Q4_K_M`            | `qwen3`      | Hermes JSON |
| `qwen3_5.jinja` | `bartowski/Qwen_Qwen3.5-4B-GGUF`, `Qwen_Qwen3.5-4B-Q4_K_M`        | `qwen35`     | Qwen XML    |
| `gemma4.jinja`  | `lmstudio-community/gemma-4-E4B-it-GGUF`, `gemma-4-E4B-it-Q4_K_M` | `gemma4`     | Gemma 4     |

Extracted on 2026-10-30 by reading the metadata of each file; nothing was edited. The models are published
under Apache-2.0. A template changes when a model is re-released, so a new fixture is added next to the old one
when the catalog moves to a new file.
