# Implementation Plan: Qwen3-TTS as haex's natural voice

**Branch**: `081-qwen3-tts` | **Date**: 2026-11-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/081-qwen3-tts/spec.md`; the spike of 2026-11-04 (spec 066 research
R23) that ran Qwen3-TTS 1.7B through haex's pinned llama.cpp.

## Summary

Qwen3-TTS becomes a second implementation of the existing `tts::local::Engine` trait, next to Pocket TTS.

- **Engine**: a new `tts/qwen/` module drives llama.cpp's audio generation helper (`mtmd_helper_gen_audio_*`)
  through a small FFI layer on `llama-cpp-sys-2`. It shares the process's llama.cpp backend with the chat engine
  (spec 070) and adds no new inference engine (FR-002, research R1).
- **Files**: the Q8_0 backbone and audio part from `ggml-org/Qwen3-TTS-12Hz-1.7B-Base-GGUF`, pinned by commit and
  SHA-256, downloaded like the other speech models (FR-003, R2).
- **Voices**: every shipped and own reference recording becomes an audio bitmap the model reads as its voice print
  (FR-004, R3). Nothing about voices or their storage changes.
- **Parts and checks**: the speaker splits sentences into parts of at most 20 words, checks each part before it
  plays and generates a failed part again (FR-010, FR-010a, R5–R7). The rules live in `tts/parts.rs` and
  `tts/check.rs`, outside the engine.
- **Choice and speed**: the catalog learns several languages per entry, a quality rank and the measured speed; the
  "Stimme" settings page gets a speech model row with "hält mit" / "langsamer als Echtzeit"; haex measures every
  part it speaks (FR-006, FR-007, FR-011, FR-012, R10).
- **Graphics card**: Qwen3-TTS is placed on the GPU only through one memory ledger shared with the chat model's
  planner; the chat model wins (FR-008, FR-009, FR-009a, R8). This touches `llm/llamacpp/memory.rs`, where spec
  080 works; the contract is fixed here and built after 080's planner change.
- **Pocket TTS**: German moves to a haex export of Kyutai's retrained model under a new catalog id; the
  temperature becomes a catalog field (FR-013, FR-015, R11).

## Technical Context

**Language/Version**: Rust (edition and MSRV of `src-tauri/Cargo.toml`); TypeScript and Vue 3 for the settings
page.

**Primary Dependencies**:

- `llama-cpp-sys-2` =0.1.160 with its `mtmd` feature: `mtmd_init_from_file`, `mtmd_gen_audio_get_info`,
  `mtmd_bitmap_init_from_audio`, `mtmd_helper_gen_audio_{init,set_input,step_prompt,step_gen,get_output,reset}`,
  `llama_get_embeddings_ith`, the sampler chain. The helper is marked "early-stage experimental" upstream (R1).
- `llama-cpp-2` =0.1.160: only `llm::llamacpp::model::backend()` (process backend) and the device probe.
- `sherpa-onnx` 2.0.1: unchanged, Pocket TTS and the voice activity check of R6.
- No new crate.

**Storage**:

- Model files in the shared models directory (`models::paths::slug_dir`), as for Pocket TTS.
- Device preferences `assistant.speechModel.<language>` and `assistant.speechModelHint` (`PrefScope::Device`).
- Speed measurements in a JSON file beside the model files, never in the vault (R10).

**Testing**:

- `cargo test --lib` for `tts::parts`, `tts::check`, `tts::speed`, `tts::catalog`, the speaker's part loop with a
  fake engine, the auto choice and the ledger.
- Ignored tests with the real files in a new `src-tauri/tests/qwen_tts_eval.rs` (`QWEN_TTS_DIR`): load, speak the
  R23 sentences, speed on CPU and GPU, parts, the 200-sentence robustness run of SC-006.
- `node --test` for the settings page's pure logic (`scripts/check-speech-model.ts` in `check:settings`); there is
  no e2e scenario for the assistant yet, and a 2.3 GB model does not fit CI, so the UI is checked manually
  (quickstart M1–M9).

**Target Platform**: desktop (Linux, Windows, macOS) and Android wherever the build has the llama.cpp engine;
iOS not on the roadmap.

**Project Type**: desktop and mobile app (Tauri)

**Performance Goals**: SC-001 (GPU ≤ 0.5 s per second of speech, first audio no later than Pocket), SC-005 (CPU:
no pause over 3 s), SC-007 (chat model ≤ 20 % slower while Qwen3-TTS speaks).

**Constraints**:

- No load of a chat model or of Qwen3-TTS may abort the app for lack of GPU memory (FR-009a).
- A part is never played unchecked and never skipped (FR-010a).
- Builds in the 10 GB memory scope with `-j 4`; the CUDA build per the spec 070 quickstart.

**Scale/Scope**: one new engine module (about 5 files), two engine-agnostic modules (parts, checks), the speed
store, catalog and settings changes, one shared ledger in the llama.cpp engine.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                                                                         | Result |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | No secrets involved; Hugging Face downloads need no token                                                                                     | Pass   |
| II No local absolute paths                              | Model paths come from `QWEN_TTS_DIR` in the ignored tests; docs use placeholders                                                              | Pass   |
| III Device-independent identity                         | Not touched                                                                                                                                   | Pass   |
| IV Pinned cross-repo references                         | Model files pinned by Hugging Face commit and SHA-256; llama.cpp by the crate version that vendors it                                         | Pass   |
| V External sources opt-in                               | No harness content                                                                                                                            | Pass   |
| VI Self-modifying instructions review-gated             | No agent instruction file changes                                                                                                             | Pass   |
| VII Relay never blocks local work                       | Speech runs fully on the device                                                                                                               | Pass   |
| VIII No concealment instructions                        | Not touched                                                                                                                                   | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | One topic branch per delivery step (see Delivery order)                                                                                       | Pass   |
| Phasing discipline                                      | Builds on 066 (in daily use) and 070 (in daily use); the GPU step waits for 080's planner change                                              | Pass   |
| spaex: 500 LoC per file                                 | `tts/local.rs` (390) gets the part loop through a new `tts/speaker.rs`; `voice.rs` (528) only changes the type of one field, split is planned | Pass   |
| spaex: graphify first                                   | `graphify query` before each new function; `graphify update` first in each worktree                                                           | Pass   |
| spaex: tests in separate files                          | `*_tests.rs` beside every new module; real-model tests ignored in `tests/qwen_tts_eval.rs`                                                    | Pass   |
| spaex: ponytail comments                                | The part length (20 words) and the plausible-length factors (R6) are fixed numbers from one measurement; marked with `ponytail:`              | Pass   |

Checked against `.specify/memory/constitution.md` and `.spaex/constitution.md`. No ADR: the engine stays the one
of ADR 0023 (spec 070); a second speech model behind the existing trait changes no principle. Re-check after
Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/081-qwen3-tts/
├── plan.md
├── research.md          # R1–R13
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── tauri-commands.md
│   └── gpu-ledger.md
└── tasks.md             # $speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/Cargo.toml                       # feature voice-qwen = voice + llm-llamacpp + llama-cpp-sys-2/mtmd
src-tauri/src/tts/
├── qwen/
│   ├── mod.rs           # QwenModel: load(dir, placement), impl Engine, bitmap cache per voice
│   ├── ffi.rs           # RAII wrappers: model, context (embeddings on), mtmd context, gen-audio helper, bitmap
│   ├── generate.rs      # one part: prompt, frame loop with cancel and frame cap, PCM out
│   └── *_tests.rs
├── parts.rs             # split a sentence into parts (FR-010)
├── check.rs             # plausible length, too short, speech present (FR-010a)
├── speaker.rs           # the worker's per-sentence loop moved out of local.rs: parts, checks, retries, fallback
├── speed.rs             # measurements per model and acceleration, "keeps up", slow-answer streak (FR-007, FR-011)
├── choice.rs            # chosen or automatic speech model per language (FR-012)
├── catalog.rs, tts_catalog.json           # languages[], quality_rank, files with sha256, temperature, Qwen entry
├── files.rs             # file lists per engine, SHA-256 check for entries that pin hashes
├── local.rs             # Engine trait gains parts and cancel; Pocket reads its temperature from the catalog
├── commands.rs, preview.rs, credits.rs   # speech model choice, preview with the chosen model, credits
└── *_tests.rs
src-tauri/src/voice.rs                     # speech slot holds Arc<dyn Engine> keyed by model id and placement
src-tauri/src/assistant/commands.rs        # open_audio resolves the speech model through tts::choice
src-tauri/src/llm/llamacpp/memory.rs       # the shared ledger (contracts/gpu-ledger.md), after spec 080
src-tauri/src/llm/llamacpp/model.rs        # chat load asks the ledger to make room
src-tauri/tests/qwen_tts_eval.rs           # ignored real-model runs
src/components/settings/assistant/VoiceSetting.vue   # speech model row, speed marks, hint
src/components/settings/assistant/SpeechModelRow.vue # new
src/composables/useTtsVoices.ts, src/lib/assistant/settings.ts
src/i18n/locales/{de,en}.json
```

**Structure Decision**: Qwen3-TTS lives in the speech module (`tts/qwen/`) and reaches into the llama.cpp engine
only for the process backend and the memory ledger. No new command family, table or migration.

## Delivery order

Each step is its own PR. Steps 1–3 and 5 do not touch `llm/llamacpp/`; step 4 waits for spec 080's planner change
and is coordinated with whoever builds 080 (contracts/gpu-ledger.md).

1. **Engine on the processor** (US1, US2): Cargo feature, `tts/qwen/`, catalog entry with hashes, download with the
   SHA-256 check, the Engine trait's cancel, the preference `assistant.speechModel.<language>` read by
   `open_audio` and the preview, credits. Ignored eval test: load, speak R23, Q8_0 against Q4_K_M (R2). Android
   build check with `mtmd` (R12).
2. **Parts and checks** (FR-010, FR-010a): `parts.rs`, `check.rs`, `speaker.rs`, Pocket fallback per part; the
   SC-006 run.
3. **Choice and speed** (US3, US5): `speed.rs`, `choice.rs`, the catalog wire fields, the speech model row with
   its marks, the one-time hint, the automatic proposal.
4. **Graphics card** (FR-008, FR-009, FR-009a): the ledger in `memory.rs`, GPU placement of Qwen3-TTS, eviction to
   the processor when the chat model needs the room; SC-001 and SC-007 runs on the reference laptop.
5. **Pocket TTS German** (US4, FR-013, FR-015): export spike, new catalog id, cleanup of the old files, temperature
   per entry; SC-008.
6. **Listening tests and results** (SC-002 to SC-004): `specs/081-qwen3-tts/results.md`.

## Complexity Tracking

No violations. `voice.rs` (528 lines) is over the spaex 500-LoC boundary before this feature; it only changes the
type of the speech slot here. Split, as a follow-up PR:

| File       | Lines | Split                                                                               | Remains                          |
| ---------- | ----- | ----------------------------------------------------------------------------------- | -------------------------------- |
| `voice.rs` | 528   | `voice/stt_choice.rs`: the STT preference and catalog resolution (lines 391 to 500) | the voice state and push-to-talk |
