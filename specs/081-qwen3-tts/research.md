# Research: Qwen3-TTS as haex's natural voice

Measurements of 2026-11-04 on the reference laptop (i7-12800HX, 62 GB RAM, RTX A2000 8 GB); the spike test
`qwen3tts_speed` ran in a throwaway worktree against `llama-cpp-sys-2` 0.1.160 with the `mtmd` feature.

## R1 — Binding: llama.cpp's audio generation helper through `llama-cpp-sys-2`

**Decision**: a small FFI layer `tts/qwen/ffi.rs` calls `llama-cpp-sys-2` directly: `llama_model_load_from_file`,
`llama_init_from_model` with `embeddings = true`, `mtmd_init_from_file`, `mtmd_gen_audio_get_info` (must report
`MTMD_GEN_AUDIO_TYPE_QWEN3TTS`), `mtmd_bitmap_init_from_audio`, the `mtmd_helper_gen_audio_*` steps and a sampler
chain. Each handle gets an RAII owner; the module is the only `unsafe` code of the feature. The process backend
comes from `llm::llamacpp::model::backend()`, so llama.cpp is initialised once for chat and speech.

**Rationale**: `llama-cpp-2` 0.1.160 has an `mtmd` feature, but its `LlamaModel`, `LlamaContext` and
`MtmdContext` keep their raw pointers `pub(crate)`, and it wraps none of the audio generation helper. The helper
needs the raw `llama_context *` and `mtmd_context *`. The spike ran exactly this sequence and produced the audio
the operator judged best.

**Risk**: upstream marks the helper "early-stage experimental, subjected to breaking changes". haex pins the
crate with `=`; a version bump of `llama-cpp-2`/`llama-cpp-sys-2` (spec 070) must run `qwen_tts_eval` before it
merges. The FFI layer is small, so an interface change stays local.

**Alternatives**: a fork or upstream PR of `llama-cpp-2` exposing raw pointers or wrapping the helper (good
follow-up, not a blocker); llama.cpp's `llama-tts` tool as a separate process (violates FR-002); the Python
reference package (4.7 s per second of speech on CPU, a second runtime).

## R2 — Files, hosting and quantisation

**Decision**: catalog entry `qwen3-tts-1.7b` pinned to `ggml-org/Qwen3-TTS-12Hz-1.7B-Base-GGUF` at commit
`ca27d74bc954b73dadab5b71ca265d87fc861a7c`:

| File                                        | Bytes         | SHA-256                                                            |
| ------------------------------------------- | ------------- | ------------------------------------------------------------------ |
| `Qwen3-TTS-12Hz-1.7B-Base-Q8_0.gguf`        | 1 847 874 400 | `ac7931aeb2e7aad1a6ed6602d353a5679c9d096b18ce8204ac730a8408d572e1` |
| `mmproj-Qwen3-TTS-12Hz-1.7B-Base-Q8_0.gguf` | 446 422 912   | `6fd65188839bcd6ecc91b277ad471e22a0edfada4699a0fe82f1165c18cfcce2` |

haex downloads from that commit directly; no mirror. The files are checked against the SHA-256 once after the
download (a marker file records the verified hash), not at every load. Licence: Apache-2.0 (Qwen), the GGUF
conversion by ggml-org.

**Open measurement (delivery step 1)**: the same repository has `Q4_K_M` (1 035 965 280 bytes,
`8d18c94acb2addd042f97da63c98be144eafa76d0d9495177eab65130cf85129`). The eval test speaks the R23 sentences with
both. If Q4_K_M is at least 20 % faster on the processor and the operator hears no difference, the entry switches
to it (about 1.5 GB instead of 2.3 GB). Otherwise Q8_0 stays, as measured.

**Result (2026-11-05, `qwen_tts_eval`, processor, `cpu::default_threads()` = 16)**: Q8_0 needs 1.49 s and
Q4_K_M 1.29 s of computing per second of speech on the R23 sentences; loading takes 2.4 s and 2.2 s. Q4_K_M is
13 % faster, below the 20 % threshold, so the entry stays on Q8_0.

**Not available**: ggml-org publishes no 0.6B GGUF at the time of writing; the 0.6B follow-up (spec Assumptions)
would need its own conversion.

## R3 — Voices as voice prints

**Decision**: the reference recording of a voice (`tts::local::Reference`, at most 30 s, stored at up to 24 kHz)
is resampled with `audio::resample::resample_filtered` to `mtmd_get_audio_sample_rate` and turned into an audio
bitmap. `QwenModel` keeps up to 8 bitmaps keyed by a hash of the reference's samples and sample rate (same
capacity as Pocket's voice cache), not by the reference's address: a freed reference's memory can be reused by
another voice of the same length, which would then speak with the wrong voice print. The helper computes the speaker embedding from the bitmap in `set_input`; the spike's prompt step (0.03–0.42 s)
includes it.

**Rationale**: llama.cpp's Qwen3-TTS pipeline clones from the speaker embedding only and takes no reference
transcript; the operator heard that this is good enough. Shipped and own voices are both reference recordings
already, so no voice needs a new recording (FR-004).

## R4 — Sampling

**Decision**: the backbone sampler chain of the spike: top-k 40, top-p 0.95, min-p 0.05, temperature 0.8, then
`dist` with a seed. The seed of the first attempt is fixed per part text (stable output for the same sentence); a
second attempt uses a different seed (R7). The helper's own `top_k`/`top_p`/`seed` fields (code predictor) get the
same values.

**Rationale**: these values gave the audio the operator preferred. Fixing the first seed makes the eval and the
listening tests repeatable.

## R5 — Parts instead of whole answers

**Decision**: `tts/parts.rs` splits the text of one sentence (after `tts::spoken`) into parts of at most 20
words. Break order: sentence-internal punctuation (`,;:–—` and closing brackets), then before a conjunction from a
per-language list kept with the `tts::spoken` word lists (de: und, oder, aber, denn, weil, dass, wenn, sondern, …),
then between words. Protected spans that never split: the words `tts::spoken` produced from one number, time,
date, amount or unit (it returns their spans), and a spelled secret (one span). Parts are balanced (a 30-word
sentence becomes 15 + 15, not 20 + 10). Pocket TTS keeps whole sentences: its engine returns no part limit.

**Rationale**: the helper only hands out audio at `get_output`, after the last frame, so a whole sentence would
start late on the processor. Short parts start sooner, and an autoregressive model drifts less on short input.
20 words is about 6–8 s of German speech.

**Pause between parts**: the parts are queued back to back in the mixer; the model's own trailing silence (about
100–200 ms) gives the comma pause. haex trims leading and trailing silence of each part to at most 150 ms.

## R6 — Checking a part before it plays

**Decision**: `tts/check.rs` judges one generated part:

- **Too long**: the frame loop stops at a cap of `1.5 s + 0.16 s × characters` of the spoken part (Qwen3-TTS
  makes 12.5 frames per second); hitting the cap is a failed attempt.
- **Too short**: audio shorter than `0.035 s × characters` (about half the normal pace) is a failed attempt,
  which catches parts that end after a few words.
- **No speech**: less than 40 % of the part's 20 ms frames are louder than about −40 dBFS (silence), or the
  loud frames vary less than 0.3 (standard deviation over mean) and so are steady noise rather than syllables
  (`tts::check::Level`).

**Rationale**: German speech in the R23 runs averaged about 0.07 s per character; the bounds give a factor of two
either way plus a start allowance. Numbers are `ponytail:` constants, checked against the 200-sentence run of
SC-006. Real parts measured 65–78 % loud frames (2026-11-05, `qwen_tts_eval::verdicts`). The 200-sentence run then needed no second attempt in 260 parts.

**Tried first, 2026-11-05**: Silero VAD, the assistant's pause detection. It finds stretches of speech between
pauses in a live stream and reported about 1.3 s of speech in every part, however long, so every part failed;
reusing one detector after `finish` also crashed inside sherpa-onnx after about a hundred parts.

## R7 — What happens after a failed part

**Decision**, per part:

1. generate again with another seed;
2. split the part once more (at the best break inside it) and generate both halves, each with this same rule up
   to step 2;
3. speak the part with Pocket TTS of the voice's language in the same voice, if that model is installed;
4. otherwise play the attempt whose length is nearest the expected length and that contains speech, trimmed to
   the cap.

Every failed attempt is logged with model id, acceleration, part length in characters and the reason, never the
text (FR-010a).

**Rationale**: the operator's rule is that nothing is cut or lost. The operator settled the reserve on
2026-11-05 (spec clarification, FR-010a): while Qwen3-TTS is chosen for a language, haex keeps that language's
Pocket model installed and downloads it together with Qwen3-TTS. Step 4 is then left for a reserve that is
missing anyway (its download failed, or files were removed by hand); it plays an attempt that failed its check
rather than skip the part, and logs that it did.

## R8 — One memory ledger for the graphics card

**Decision**: `llm/llamacpp/memory.rs` gets a process-wide ledger of what sits on the GPU: the chat model (with
its kept context of spec 080 and the drafter of 079) and the speech model. Rules (contracts/gpu-ledger.md):

- **Speech asks first**: before Qwen3-TTS loads, it asks the ledger for its device bytes (file sizes plus the
  measured context and compute buffers, about 2.6 GB in the spike). The answer is GPU only if the free device
  memory minus that amount still holds the last chat model's full-offload plan (or the device margin when no
  local chat model was loaded since start); otherwise CPU.
- **Chat wins**: when a chat model's plan needs memory the speech model holds, the chat load asks the ledger to
  evict the speech model. The speech slot drops its GPU instance at the next part boundary and loads a CPU
  instance; a part that is generating finishes first. The chat load then plans with the freed memory.
- **Back to GPU**: when the chat model is unloaded, the next speech load may ask for the GPU again.
- **Nothing aborts**: both sides plan against the device memory llama.cpp reports at that moment (as `plan()`
  does today); the ledger only adds what it knows is reserved.

**Rationale**: llama.cpp aborts the process when GPU memory runs out (spec 080 US5). Two planners would not see
each other. The chat engine's planner already reads free device memory at load time, so a resident speech model
would silently push the chat model off the GPU; the ledger turns that into the explicit FR-009 rule.

**Coordination**: `memory.rs` is changed by spec 080 (kept context) and 079 (drafter). This step lands after 080's
planner change, and the ledger API is fixed in contracts/gpu-ledger.md so the 080 work can leave room for it.

## R9 — Processor threads

**Decision**: Qwen3-TTS uses `llm::llamacpp::cpu::default_threads()` for both the batch and the generation
threads, the same number the chat model may use (FR-008). Never all logical cores.

**Rationale**: the spike measured 1.65 at 4 threads and 1.35 at 8 threads; more threads help, but speech and a
local chat model can run at the same time and must not starve the UI.

**Open point (2026-11-05)**: `default_threads()` counts physical cores, 16 on the reference laptop including its
efficiency cores. The spike was faster with 8 threads (1.35) than the app with 16 (1.49). Phase 7 measures 8 against
16 and, if 8 wins clearly, caps the speech model's threads at the performance cores.

## R10 — Measuring speed

**Decision**: the speaker records, per generated part, the computing time (from `set_input` to `get_output`, retries
included) and the audio length. `tts/speed.rs` groups the parts of each sentence and measures the ratio over the
last 10 sentences per (model id, acceleration): the sum of their computing time divided by the sum of their audio
length. This matches FR-007 and prevents a long sentence split into several parts from outweighing other sentences.
The slow streak counts consecutive answers whose aggregate ratio is 1.0 or more. These measurements are stored in
`speed.json` in the model's directory (`models/<slug>/speed.json`), written at most once per answer.

**Rationale**: speed is a property of the device and the model, not of the vault: a JSON file beside the model
files is device local by construction, needs no migration and causes no sync traffic (a device-scoped preference
would sync its row after every answer). The acceleration is part of the key because the same model keeps up on
the GPU and not on the CPU.

## R11 — Pocket TTS German: Kyutai's retrained model

**Decision**: delivery step 5 starts with a spike: export `kyutai/pocket-tts` (German weights of the release
v3.3.0 or later, pinned by commit; at the time of writing the repository head is
`db04f0e653473e5c7333c69686ac92b7460b0c1d`) with the k2-fsa sherpa-onnx export script, apply haex's existing
encoder patch (start signal, 066 research R8), and compare speed and sound against the April export. The result
goes to `haex-space/pocket-tts-sherpa-onnx` at a new commit as folder `de-v4` and to the catalog as a new id
`pocket-de-v4`, so its directory differs from the old `pocket-de`. Until the new files are complete, `open_audio`
and the preview fall back to the old directory if it is complete; afterwards the old directory is removed.

**Temperature**: the catalog entry gets a `temperature` field. New exports carry Kyutai's current recommendation
(0.3 at the time of writing); the old entries keep 0.7 until their own re-export (FR-015).

**Not available**: no third-party export of the retrained German model was found that haex would trust, and
ggml-org publishes no Pocket TTS GGUF; llama.cpp's Pocket pipeline therefore stays a later step (spec
Assumptions).

**Fallback if the export fails**: the old German model stays, and FR-013 goes back to the operator as an amendment.

## R12 — Platforms

**Decision**: the `mtmd` feature is enabled wherever `llm-llamacpp` is (desktop and Android). Delivery step 1
builds Android with it; if `mtmd` does not build for Android with the pinned crate, the Qwen3-TTS entry is
hidden there (`engine_enabled` is false) and Android keeps Pocket TTS only (FR-001 allows that).

**Rationale**: Android is expected to stay on Pocket TTS anyway (FR-014), but the choice should not be removed
without a reason.

## R13 — Speech rate

**Decision**: Qwen3-TTS has no speed parameter; haex applies the user's rate with `audio::stretch` (WSOLA), as it
already does for Pocket TTS (066 research R8).
