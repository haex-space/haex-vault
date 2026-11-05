# Quickstart: validating Qwen3-TTS in haex

## Prerequisites

- A worktree with `pnpm install` done; Rust commands run inside `nix develop` from `src-tauri`, with the host
  bridge as in the spec 070 quickstart.
- The model files (research R2) in one folder, referred to as `$QWEN_TTS_DIR`:
  `Qwen3-TTS-12Hz-1.7B-Base-Q8_0.gguf` and `mmproj-Qwen3-TTS-12Hz-1.7B-Base-Q8_0.gguf` from
  `ggml-org/Qwen3-TTS-12Hz-1.7B-Base-GGUF` at commit `ca27d74bc954b73dadab5b71ca265d87fc861a7c`.
- For GPU runs: the CUDA build of the spec 070 quickstart (`llm-llamacpp-cuda`).

## Automated checks

```bash
# Unit tests of the new modules (CPU, no model files)
cargo test -j 4 --lib --no-default-features --features llm-llamacpp,voice,voice-qwen -- \
  tts::parts tts::check tts::speed tts::choice tts::catalog tts::speaker tts::qwen llm::llamacpp::memory

# Real model, processor: load, speak the R23 sentences, write WAVs and timings
QWEN_TTS_DIR=… QWEN_TTS_OUT=… cargo test -j 4 --release --test qwen_tts_eval \
  --no-default-features --features llm-llamacpp,voice,voice-qwen -- --ignored speaks_r23_sentences

# Real model, graphics card (SC-001)
QWEN_TTS_DIR=… cargo test -j 4 --release --test qwen_tts_eval \
  --no-default-features --features llm-llamacpp-cuda,voice,voice-qwen -- --ignored gpu_speed

# Robustness, 200 sentences including 20 of more than 40 words (SC-006)
QWEN_TTS_DIR=… cargo test -j 4 --release --test qwen_tts_eval \
  --no-default-features --features llm-llamacpp,voice,voice-qwen -- --ignored two_hundred_sentences

pnpm lint:rust && pnpm check:settings
```

Expected: the R23 run reports a ratio near 1.35 on 8 threads and near 0.36 on the GPU; the 200-sentence run
reports no part that was played unchecked and at most 2 second attempts.

## Manual checks (dev app)

| #   | Story | Steps                                                                                                   | Expected                                                                                                |
| --- | ----- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| M1  | US1   | Settings → KI → Sprachassistent → Stimme: open the speech model row                                     | Qwen3-TTS once, with size, licence, six languages and a speed mark                                      |
| M2  | US1   | Choose Qwen3-TTS, confirm the download, ask the assistant "Wie spät ist es und was kostet 3,50 €?"      | Spoken by Qwen3-TTS in the chosen voice, numbers as words, first words as soon as with Pocket           |
| M3  | US1   | Ask for an answer with a sentence of more than 40 words                                                 | Heard in full, breaks like at a comma, no repeated or missing words                                     |
| M4  | US2   | With Qwen3-TTS chosen, press "Probe" on an own voice created before; create a new own voice from a file | Both samples spoken by Qwen3-TTS, no new recording needed                                               |
| M5  | US3   | Run on a machine or build without GPU support                                                           | Mark "langsamer als Echtzeit" after 3 sentences; long answer complete; hint after 3 slow answers, once  |
| M6  | US3   | Load a large local chat model on the GPU while Qwen3-TTS is on the GPU                                  | Chat model gets the GPU, speech continues on the CPU, no crash                                          |
| M7  | US4   | Start with the old German Pocket model installed                                                        | Download offered for the new model; old one speaks until it is done, then its folder is gone            |
| M8  | US5   | Fresh vault on the reference laptop, turn on the assistant                                              | Download offers Qwen3-TTS plus the Pocket model with their size; the first answer after it is Qwen3-TTS |
| M9  | edge  | Interrupt the assistant while Qwen3-TTS speaks a long answer                                            | Silence at once; no part of that answer plays later                                                     |
