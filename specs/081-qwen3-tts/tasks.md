---
description: 'Task list for spec 081, Qwen3-TTS as haex natural voice'
---

# Tasks: Qwen3-TTS as haex's natural voice

**Input**: Design documents from `specs/081-qwen3-tts/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/tauri-commands.md,
contracts/gpu-ledger.md, quickstart.md

**Tests**: included. The spaex constitution asks for one runnable check per non-trivial piece of logic. Rust unit
tests go in `*_tests.rs` next to their module; tests with the real model files are `#[ignore]` in
`src-tauri/tests/qwen_tts_eval.rs`; the settings page's pure logic is checked with `node --test` in
`scripts/check-speech-model.ts`.

**Organization**: phases follow the plan's delivery order. Phases 1–3 are delivery step 1 and land as one PR; from
Phase 4 on each phase is its own PR: Phase 4 is step 2, Phase 5 the own-voice dialog part of US2 (step 1 already
covers the preview), Phase 6 step 3, Phase 7 step 4, Phase 8 step 5 and Phase 9 step 6, with the `voice.rs` split
(T053) as a separate PR. Each task carries the user story of spec.md it serves. US1 and US2 are P1, US3 and US4 are P2, US5 is P3.

**Working rules for every task**:

- **graphify first**: before a new named function, run `graphify query "<concept>"` and extend an existing
  candidate where one matches (spaex).
- **Builds**: from `src-tauri`,
  `nix develop --command bash -c '../scripts/with-nix-host-bridge.sh cargo test -j 4 --lib --no-default-features --features llm-llamacpp,voice,voice-qwen -- <filters>'`;
  after a cargo test run `git checkout -- src/types/bindings`. Clippy with `CARGO_BUILD_JOBS=4 pnpm lint:rust`. The
  CUDA build per the spec 070 quickstart (`CUDAARCHS=86`, `CUDA_LIBRARY_PATH`), run through `cargo test`, never the
  bare binary.
- **Real files**: the two Q8_0 files of research R2 (and Q4_K_M for T021) in one folder outside the repository,
  read from `QWEN_TTS_DIR`; outputs to `QWEN_TTS_OUT`.
- **Not your area**: `src-tauri/src/llm/llamacpp/` belongs to the chat engine work (specs 079, 080). Only Phase 7
  changes it, after 080's planner change is on `main`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: the user story of spec.md

---

## Phase 1: Setup

**Purpose**: the knowledge graph, the build feature, and the platform check of research R12.

- [x] T001 Run `graphify update .` in the worktree root, so later `graphify query` calls see the current tree.
- [x] T002 Add the feature `voice-qwen = ["voice", "llm-llamacpp", "llama-cpp-sys-2/mtmd"]` to
      `src-tauri/Cargo.toml` with a comment citing spec 081 research R1, and add it to `default`
      (`["llm-llamacpp", "voice", "voice-qwen"]`). Check that `cargo tree -e features -i llama-cpp-sys-2` shows
      `mtmd` and that `Cargo.lock` gains no new package.
- [ ] T003 Build Android with the new feature (`pnpm tauri android build --debug` in the Android devShell of spec
      043, or the CI Android job via `gh workflow run`). If `mtmd` fails to build there, add
      `voice-qwen` only to the desktop targets' feature use and record the result in research R12.

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: the catalog shape, the file check, the Engine trait and the speech model resolution that every story
needs. No user-visible change yet: with only Pocket entries, everything behaves as today.

- [x] T004 [P] In `src-tauri/src/tts/catalog.rs` and `tts_catalog.json`: add `TtsEngine::Qwen3Tts` (`"qwen3-tts"`);
      replace `language` by `languages: Vec<String>` (data-model.md, "at least one of the six languages"); add
      `quality_rank: u32` (Pocket 10), `files: Option<Vec<CatalogFile { name, bytes, sha256 }>>`,
      `temperature: Option<String>` (Pocket entries `"0.7"`). Rename `default_for` to `pocket_for(language)` (standard
      before 24l). Update every caller
      (`grep -rn "default_for\|\.language" src-tauri/src/tts src-tauri/src/assistant`) and `catalog_tests.rs`: every
      entry has a full-commit `hf_revision`, at least one language of `LANGUAGES`, and `files` sums to
      `approx_size_bytes` when present.
- [x] T005 [P] In `src-tauri/src/tts/files.rs`: `files_for(entry)` returns the names of `entry.files` when set, the
      Pocket list otherwise; `ensure_model_files` downloads `entry.files` and, after each file, checks its SHA-256
      once, writing `<file>.sha256` beside it; a mismatch deletes the file and returns
      `TtsError::Unavailable { reason: "die Datei … ist beschädigt" }`. `is_complete_model` requires the marker for
      hashed files. Allow an empty `hf_folder` in `file_url`. Tests in `files_tests.rs` with a small temp file and
      its known hash.
- [x] T006 In `src-tauri/src/tts/local.rs`: extend `Engine` with `fn part_words(&self) -> Option<usize>` (default
      `None`) and `fn generate_cancellable(&self, text, reference, rate, cancelled: &dyn Fn() -> bool)` (default:
      calls `generate`). `PocketModel` reads its temperature from its catalog entry (passed to `load`) instead of
      the constant `TEMPERATURE` (FR-015). Update `local_tests.rs`.
- [x] T007 Create `src-tauri/src/tts/choice.rs` (+ `choice_tests.rs`, registered in `tts/mod.rs`):
      `pub fn resolve(language, preference: Option<&str>, installed: &[String], hw, speeds) -> Resolved` with
      `Resolved { chosen: Option<&Entry>, proposed: &Entry, speaking: &Entry }`. `proposed` = highest `quality_rank`
      among entries of the language that keep up (measured) or are presumed to (Pocket always; Qwen3-TTS only with a
      GPU of at least 3 GB free, FR-012); `speaking` = the chosen entry when installed and enabled, else the installed
      Pocket entry of the language. Pure, no `AppHandle`; `speeds` is a trait object so tests pass fixed values.
- [ ] T008 In `src-tauri/src/voice.rs`: the speech slot becomes `Option<(SpeechKey, Arc<dyn Engine>)>` with
      `SpeechKey { model_id, acceleration }` (data-model.md "Speech slot"); `speech_model` takes the entry and
      dispatches on `entry.engine` (Pocket today; Qwen3-TTS from T018). Behaviour unchanged for Pocket.
      Done in #826 without the acceleration in the key: the slot is keyed by model id; `SpeechKey` with the acceleration comes with the GPU in T044.
- [x] T009 Wire the resolution: `assistant/commands.rs::open_audio`, `tts/preview.rs` and
      `tts/commands.rs::voice_options` read `assistant.speechModel.<language>` (device scope) and use
      `choice::resolve(..).speaking` instead of the language default. Add the key to
      `src/lib/assistant/settings.ts` next to `voicePrefKey`. Tests: `voice_options` names the chosen model;
      without a preference everything is as before.
      The key in `src/lib/assistant/settings.ts` comes with its first frontend user, the speech model row (T038).
- [x] T010 Add `tts_speech_models(language)` to `src-tauri/src/tts/commands.rs` per contracts/tauri-commands.md
      (fields `speed`/`ratio` return `unknown`/`null` until Phase 5), register it in the command list, and add
      `speechModelsAsync(language)` to `src/composables/useTtsVoices.ts`.
      Moved to Phase 6 with the speech model row (T036), which is its only user.

**Checkpoint**: everything speaks with Pocket as before; the catalog, files and resolution are ready for a second
engine.

---

## Phase 3: User Story 1 - Qwen3-TTS on the processor (P1) 🎯 MVP part 1

**Goal**: Qwen3-TTS speaks answers, previews and secrets on the processor (delivery step 1).

**Independent Test**: with `assistant.speechModel.de = qwen3-tts-1.7b` and the files downloaded, an answer with a
time and an amount is spoken by Qwen3-TTS in the chosen voice, numbers as words (quickstart M2 without the speed
claim).

- [x] T011 [P] [US1] Add the entry `qwen3-tts-1.7b` to `src-tauri/src/tts/tts_catalog.json`: engine `qwen3-tts`,
      six languages, `quality_rank` 20, `hf_repo` `ggml-org/Qwen3-TTS-12Hz-1.7B-Base-GGUF`, `hf_revision`
      `ca27d74bc954b73dadab5b71ca265d87fc861a7c`, `hf_folder` `""`, the two Q8_0 files with bytes and SHA-256 of
      research R2, licence `Apache-2.0`, attribution of data-model.md, `sample_rate` 24000. Update `_meta.note`.
- [x] T012 [P] [US1] Credits: `src-tauri/src/tts/credits.rs` lists Qwen3-TTS with licence and origin (FR-003);
      `credits_tests.rs` checks it is there; `src/components/settings/LicensesView.vue` needs no change if it
      renders the list generically (check).
- [x] T013 [US1] `engine_enabled` in `src-tauri/src/tts/commands.rs`: `Qwen3Tts => cfg!(feature = "voice-qwen")`.
- [x] T014 [US1] Create `src-tauri/src/tts/qwen/ffi.rs` (module doc: the only `unsafe` of spec 081, research R1):
      RAII owners for `llama_model` (load with `n_gpu_layers` from a `Placement` argument, CPU = 0), `llama_context`
      (`n_ctx` 4096, `n_batch` 512, `embeddings = true`, `n_threads` = `n_threads_batch` =
      `llm::llamacpp::cpu::default_threads()`), `mtmd_context` (`use_gpu` per placement), the gen-audio helper,
      an audio bitmap from `&[f32]` (`mtmd_bitmap_init_from_audio`), and a sampler chain (top-k 40, top-p 0.95,
      min-p 0.05, temp 0.8, dist with a seed; research R4). Call `llm::llamacpp::model::backend()` before the first
      load. `ffi_tests.rs`: the owners free in reverse order (drop-order test with a counter behind
      `#[cfg(test)]` hooks) and a missing file returns an error, not a crash.
- [x] T015 [US1] Create `src-tauri/src/tts/qwen/generate.rs`: one part → PCM. `set_input` (seq 0, prompt, voice
      bitmap, `lang` = the voice's language, top-k/top-p/seed of R4, `out_type` PCM), `step_prompt(512)` until 0,
      then the frame loop `sample → step_gen` until stop, `cancelled()` or the frame cap passed in (R6), then
      `get_output`; `llama_memory_clear` and `mtmd_helper_gen_audio_reset` before each part. Returns samples and
      the computing time. Seed: hash of the part text for the first attempt, caller-supplied otherwise.
- [x] T016 [US1] Create `src-tauri/src/tts/qwen/mod.rs` (`#[cfg(feature = "voice-qwen")]` in `tts/mod.rs`):
      `QwenModel::load(dir, entry, placement)` checks `mtmd_gen_audio_get_info` reports `QWEN3TTS` (else
      `TtsError::Unavailable`), reads the sample rate from it; holds the FFI owners in a `Mutex`; `impl Engine`
      (`part_words` = `Some(20)`; `generate_cancellable` applies `tts::spoken` then `generate.rs`, then
      `audio::stretch` for the rate, research R13).
- [x] T017 [US1] Voice bitmaps: in `qwen/mod.rs`, a cache of up to 8 bitmaps keyed by a hash of the reference's
      samples and sample rate (not its address, which a later voice can reuse), built with
      `audio::resample::resample_filtered` to `mtmd_get_audio_sample_rate` (research R3). Tests: two calls with the
      same reference build one bitmap; two different references of the same length build two.
- [x] T018 [US1] In `src-tauri/src/voice.rs::speech_model`: `TtsEngine::Qwen3Tts` loads `QwenModel` on CPU
      (Placement CPU until Phase 7) under `VaultGate::spawn_blocking`; the slot key carries `acceleration: Cpu`.
- [x] T019 [US1] Secrets: check that the spelled secret of 066 FR-026 reaches only the local engine with Qwen3-TTS
      selected (the `assistant/` secret path is engine-agnostic; read it and note the result in the PR). That a
      spelled secret stays one part is tested in T024.
      066 US6 (reading secrets) is not built yet; when it is, it speaks through the same `Speaker` and engine as every other sentence, so nothing leaves the device.
- [x] T020 [US1] Create `src-tauri/tests/qwen_tts_eval.rs` (`#![cfg(feature = "voice-qwen")]`, module doc with the
      quickstart commands), ignored tests: `loads_and_reports_qwen3tts`; `speaks_r23_sentences` (the six German
      sentences of spec 066 R23 with the shipped voice `de-lena`, WAVs and a timing table to `QWEN_TTS_OUT`);
      `cancel_stops_within_one_frame`.
- [x] T021 [US1] Measurement for research R2: run `speaks_r23_sentences` with Q8_0 and with Q4_K_M on 8 threads,
      give the operator both WAV sets (a listening page like the 066 comparison) and record ratio and verdict in
      research R2. Switch the catalog entry to Q4_K_M only if it is at least 20 % faster and the operator hears no
      difference.
      Result 2026-11-05 (research R2): Q8_0 1.49, Q4_K_M 1.29 on the reference laptop's processor with
      `cpu::default_threads()` (16), not 8 threads (research R9 open point); 13 % faster, below the 20 % threshold, so
      the entry stays on Q8_0.

---

## Phase 4: User Story 1 - parts and checks (P1) 🎯 MVP part 2

**Goal**: long sentences in parts, every part checked before it plays, nothing cut or skipped (FR-010, FR-010a;
delivery step 2).

**Independent Test**: a 45-word sentence is spoken completely in balanced parts; a forced failure (test engine)
is regenerated and never heard (quickstart M3, SC-006).

- [x] T022 [P] [US1] Make `src-tauri/src/tts/spoken.rs` return the spans it wrote out (one span per number, time,
      date, amount or unit expansion) next to the text:
      `spoken_with_spans(text, language) -> (String, Vec<Range<usize>>)`; `spoken()` stays as a wrapper. Tests in
      `spoken_tests.rs` for a time, an amount with unit and a date.
- [x] T023 [P] [US1] Conjunction lists per language for the break rule (research R5), next to the `tts::spoken`
      word lists (`src-tauri/src/tts/spoken/`), six languages, with a test that each list is non-empty and
      lower-case.
      Done in `tts/parts.rs` itself, beside the rule that uses the lists.
- [x] T024 [US1] Create `src-tauri/src/tts/parts.rs` (+ `parts_tests.rs`):
      `split(text, spans, protected: &[Range<usize>], language, max_words) -> Vec<String>`. Break order punctuation →
      before a conjunction → between words; never inside a span or a protected range; balanced parts (30 words → 15 +
      15); `ponytail:` on 20. Tests: short sentence unchanged; 45 words with commas; no punctuation; a time and an
      amount at the break point stay whole; a spelled secret is one part even above 20 words.
- [x] T025 [US1] Create `src-tauri/src/tts/check.rs` (+ `check_tests.rs`): `frame_cap(chars)` =
      `(1.5 + 0.16 × chars) × 12.5`; `judge(samples, rate, chars, vad) -> Verdict { Ok, TooLong, TooShort, NoSpeech }`
      with the bounds of research R6 (`ponytail:` constants). The speech check is a trait so tests use a fake; the
      real one measures the level of 20 ms frames (`Level`; Silero VAD did not fit, research R6). Tests per verdict
      with synthetic audio and the fake.
- [x] T026 [US1] Create `src-tauri/src/tts/speaker.rs` and move the worker of `tts/local.rs` there (keeping
      `Speaker`'s public API and ticket semantics). Per job: `parts = engine.part_words()` → `parts::split`, else
      the whole sentence; per part: `generate_cancellable` (cancel = stale or closing), `check::judge`, then the
      retry chain of research R7 (new seed → split once more → Pocket of the voice's language if installed → the
      attempt nearest the expected length with speech, trimmed to the cap); trim leading/trailing silence to
      150 ms (R5); queue to the mixer; stale checks between parts. Log every failed attempt with model id,
      acceleration, part length and reason, never the text. The Pocket fallback engine comes from the speech slot
      (a second, lazily loaded Pocket instance).
      Done as `speaker::speak_sentence`; the worker stayed in `tts/local.rs` (it only calls it), and the voice sample uses it too.
- [x] T027 [US1] `speaker_tests.rs` with a fake engine: parts play in order under one ticket; a `TooLong` first
      attempt is regenerated and its audio never reaches the mixer; two failures lead to the split, then to the
      fallback engine; without a fallback the nearest attempt plays; `stop()` during part 2 drops parts 2..n and
      plays nothing of them (edge case "Interruption").
- [x] T028 [US1] The reserve (FR-010a, operator 2026-11-05): wherever haex installs the chosen speech model
      (`tts/commands.rs::ensure_installed` callers: the voice sample, the assistant downloads, the settings
      download), it also installs the language's Pocket model when the chosen one is Qwen3-TTS. Test that choosing
      Qwen3-TTS without Pocket fetches both.
      Also: the assistant counts a missing reserve as a missing voice (`assistant_readiness`), so its download
      fetches it; a speech model that fails to load or fails on a part gives way to the Pocket model.
- [x] T029 [US1] Ignored eval `two_hundred_sentences` in `src-tauri/tests/qwen_tts_eval.rs`: 200 sentences (180
      from agent-style templates with numbers and times, 20 of more than 40 words), counts of parts, second
      attempts, splits, fallbacks; asserts at most 2 second attempts (SC-006). Record the run in
      `specs/081-qwen3-tts/results.md`.
      Result 2026-11-05 (processor, Q8_0, level check): 200 sentences in 260 parts, 0 second attempts, 1215 s of
      speech in 2142 s. Listening for babbling or missing words is still open (SC-006).

**Checkpoint**: US1 works on the processor; Qwen3-TTS can be chosen through the preference.

---

## Phase 5: User Story 2 - own voices with Qwen3-TTS (P1)

**Goal**: own voices work with Qwen3-TTS without a new recording, also in the own-voice dialog.

**Independent Test**: quickstart M4.

- [ ] T030 [US2] `src-tauri/src/tts/clone_commands.rs`: the sample before saving uses `choice::resolve` for the
      voice's language (not the Pocket default); test in `clone_commands_tests.rs` with a chosen Qwen3-TTS entry
      and a fake engine.
- [ ] T031 [US2] Ignored eval `own_voice_from_file` in `qwen_tts_eval.rs`: build a reference the way
      `stored_reference` does from a 20 s WAV in `QWEN_TTS_DIR/own.wav` and speak two sentences; output for the
      SC-004 listening test.
- [ ] T032 [US2] Synced own voices: a test in `voices_tests.rs` that an own voice whose row came from another
      device resolves to a `Reference` usable by both engines (no engine-specific data in the voice row).

---

## Phase 6: User Story 3 and User Story 5 - speed, choice and the automatic proposal (P2, P3)

**Goal**: the speech model row with honest speed marks, the one-time hint, and the automatic proposal (delivery
step 3).

**Independent Test**: quickstart M1, M5 and M8 (without the GPU part).

- [x] T033 [P] [US3] Create `src-tauri/src/tts/speed.rs` (+ `speed_tests.rs`) per data-model.md "Speed
      measurement" (sentences with their parts, last 10 sentences per acceleration, ratio over all parts, keeps up
      at < 1.0 with ≥ 3 sentences, `slowStreak`, version 1, unreadable = empty), reading and writing
      `models/<slug>/speed.json` atomically (write temp + rename). Tests: ratio, the 3-sentence threshold, streak
      reset, unknown version.
- [x] T034 [US3] The speaker records per sentence the parts' `[compute, audio]` pairs including retries and, at the
      end of an answer (the last ticket of a conversation turn), hands them to `speed.rs`; at most one write per
      answer. Pocket answers are measured too.
      The assistant closes an answer when its transcript entry is written (`Ports::record` with
      `EntryKind::Answer`); a part's computing time includes its retries and the reserve.
- [x] T035 [US3] The hint (FR-011): when `slowStreak` reaches 3 for the chosen model and `choice::resolve` names a
      different model that keeps up or is presumed to, emit `tts-speech-model-hint` (contracts/tauri-commands.md)
      unless `assistant.speechModelHint` is `dismissed`; emitted at most once per app run. Closing the toast writes
      `dismissed` too (T039), so the hint shows once per device.
      Counted for the model that speaks, chosen or default, since Qwen3-TTS is the default (FR-012); not
      offered when the language's Pocket model itself is slow or speaks already.
- [x] T036 [US3] Fill `speed`, `ratio`, `acceleration`, `chosen`, `proposed` in `tts_speech_models` from
      `speed.rs` and `choice.rs`; tests in `commands` tests with fixed speeds.
- [x] T037 [P] [US3] Create `src/lib/assistant/speechModel.ts`: `speedLabelKey(option)` (`hält mit` /
      `langsamer als Echtzeit` / `noch nicht gemessen`), `soundPhraseKey(engine)` ("klingt am natürlichsten" for
      Qwen3-TTS, "klein und schnell" for Pocket), `hintActions()`; `scripts/check-speech-model.ts` with `node --test`,
      added to the `check:settings` script in `package.json`.
- [x] T038 [US3] Create `src/components/settings/assistant/SpeechModelRow.vue` and use it in `VoiceSetting.vue`
      above the voices: one option per `SpeechModelOption` with name, size, licence, languages, sound phrase and
      speed mark; "Automatisch" first with the proposed model's name; choosing writes the preference on selection
      (no save button, errors as toast); a model not installed asks before downloading like a voice today. Follow
      the COSMIC/GNOME row style (Group/OptionRow). i18n keys in `{de,en}.json`.
      Note (operator 2026-11-06): no own "Automatisch" row; the default model is marked "Standard" in its row and
      choosing it removes the preference (`preferenceFor`); the download needed beyond the model names only the
      Pocket reserve; Pocket's sound phrase is "klein, klingt einfacher" (it was "klein und schnell", which
      contradicted a "langsamer als Echtzeit" mark).
      Also (FR-006a): an installed model shows an "Installiert" badge and, except Pocket, a delete button with
      a question (`delete_tts_model`, `may_delete`); a finished download no longer stays on screen
      (`modelDownloads.running`), which also kept the voice rows green with the model's size.
- [x] T039 [US3] The hint toast in the frontend (listener for `tts-speech-model-hint`): "Wechseln" writes the
      preference, "Nicht mehr anzeigen" and closing write `assistant.speechModelHint = dismissed`.
- [x] T040 [US3] The fallback event: `open_audio` and the speaker emit `tts-speech-model-fallback` once per
      conversation when they speak with Pocket instead of the chosen model (memory, not installed, load failed); the
      frontend shows it as a toast naming the reason (contracts/tauri-commands.md).
      `memory` comes with the graphics card ledger (T044); `open_audio` emits `notInstalled` and `loadFailed`
      only when the user chose a model, so the default not downloaded yet (US5) stays quiet.
- [x] T041 [US5] US5 behaviour (FR-012 as changed 2026-11-05): without a preference the default of
      `choice::resolve` is Qwen3-TTS wherever the build runs it; the settings page shows it first with the download
      size of Qwen3-TTS plus the language's Pocket model; Pocket speaks until Qwen3-TTS is downloaded. Tests in
      `choice_tests.rs` (done with the change in phase 4): Qwen3-TTS is the default, a build without it defaults to
      Pocket, a user choice is never replaced (US5 scenario 3).
      Android (operator 2026-11-05): `commands::engine_may_default` keeps Qwen3-TTS from being the default
      there; `choice::resolve` takes it as `may_default`, Qwen3-TTS stays selectable.
- [x] T058 [US5] Operator 2026-11-06: Pocket TTS is the default on every platform (`engine_may_default`: Qwen3-TTS
      never), Qwen3-TTS speaks only when chosen; the onboarding ends with a voice step
      (`src/components/onboarding/VoiceStep.vue`, `src/pages/onboarding/[instance].vue`) that offers the Pocket model
      of the interface language with the download question, skippable and left out when it is installed (FR-012,
      FR-012a, FR-014, US5). Voice rows no longer show their model's download progress; the speech model row shows
      it. Tests: `pocket_is_the_default_everywhere_and_qwen_only_by_choice` in `choice_tests.rs`, the voice rows in
      `scripts/check-assistant-ui.ts`.

---

## Phase 7: User Story 1 and User Story 3 - the graphics card (P1, P2)

**Goal**: Qwen3-TTS on the GPU through one memory ledger shared with the chat model (FR-008, FR-009, FR-009a;
delivery step 4). **Starts only when spec 080's planner change is on `main`**; coordinate with whoever builds 080.

**Independent Test**: quickstart M6 and the GPU run of SC-001; SC-007.

- [ ] T042 [US1] Implement the ledger in `src-tauri/src/llm/llamacpp/memory.rs` (or a new `ledger.rs` beside it)
      per contracts/gpu-ledger.md: `Occupant`, `ledger()`, `placed`, `released`, `place_speech`,
      `make_room_for_chat`, `on_evict_speech`. Tests in `ledger_tests.rs` as listed in the contract.
- [ ] T043 [US1] Chat side: `llm/llamacpp/model.rs` calls `make_room_for_chat` before `placement_for` and
      `placed(Chat, …)` / `released(Chat)` around the model's life; the kept context (080) and drafter (079) are
      in its bytes.
- [ ] T044 [US1] Speech side: `voice.rs::speech_model` asks `place_speech` with the measured need (files plus
      buffers, research R8), loads with GPU layers when the answer is `Gpu`, registers `on_evict_speech` (the slot
      swaps to a CPU instance at the next part boundary), and retries once on the CPU if the GPU load fails.
- [ ] T045 [US3] Speed per acceleration: the slot key's acceleration selects the `speed.json` run; the speech
      model row shows the mark for where the model would run now (`acceleration` of `tts_speech_models`).
- [ ] T046 [US1] Ignored evals in `qwen_tts_eval.rs`: `gpu_speed` (SC-001: ratio ≤ 0.5 and first audio not later
      than Pocket for the same first sentence, measured from the sentence end, including the check);
      `chat_while_speaking` (SC-007: a local chat model's tokens per second with and without Qwen3-TTS speaking on
      the same GPU, ≤ 20 % slower), with the CUDA build. Record both in `results.md`.

---

## Phase 8: User Story 4 - Pocket TTS with Kyutai's new German model (P2)

**Goal**: the retrained German Pocket model (FR-013, FR-015; delivery step 5).

**Independent Test**: quickstart M7; SC-008.

- [ ] T047 [US4] Export spike (research R11): export `kyutai/pocket-tts` German weights (release v3.3.0 or later,
      pinned commit) with the k2-fsa sherpa-onnx Pocket export script, apply the encoder patch of the
      `haex-space/pocket-tts-sherpa-onnx` `tools/` script, load with `PocketModel` and speak the R23 sentences. If
      the export fails, stop and report to the operator (FR-013 amendment); do not ship a third-party export.
- [ ] T048 [US4] Publish the export as folder `de-v4` in `haex-space/pocket-tts-sherpa-onnx` (new commit, CC BY
      4.0 attribution naming the change), add the entry `pocket-de-v4` (temperature `"0.3"`, `quality_rank` 11)
      and remove `pocket-de` from `tts_catalog.json`; update credits.
- [ ] T049 [US4] Transition: `choice::resolve` and the speaker fall back to the old `pocket-de` directory while it
      is complete and `pocket-de-v4` is not; a startup step removes the old directory once the new model is
      complete. Tests with temp directories.
- [ ] T050 [US4] SC-008 run: old and new German model on the R23 sentences, ratio on 4 threads; the new one must not
      be slower. Record in `results.md` and give the operator both WAV sets.

---

## Phase 8b: Sharing the machine with the chat model (operator 2026-11-06)

**Goal**: no pauses from two models fighting over one compute unit (FR-016), and playback without gaps where the
speech model is slower than real time (FR-017).

- [x] T056 FR-016: `src-tauri/src/compute.rs` (`ComputeUse`, `COMPUTE`, `Unit`, `units_of_layers`): the llama.cpp
      chat model claims the units of its placement for each model call (`LoadedModel.units`, set at load from
      `gpu_layers` and `n_layer`; claimed in `LlamaCppAdapter::stream_chat` while the call computes);
      `Engine::compute_unit` (Pocket: processor; Qwen3-TTS: its placement); `speak_sentence` waits before every
      sentence and every part (`ComputeUse::claim_speech`, ends on a stop), outside the timed span. Tests in
      compute_tests.rs and speaker_tests.rs.
- [x] T057 FR-017: `tts::local::Worker` reads `Meter::ratio` at the start of every answer (nothing sounds or
      waits); above 1.0 it keeps the spoken sentences (`Held`) and looks ahead into the queue (`pending`) until
      `may_start(ratio, held, rest)` says the rest can be computed before the held audio has played, then queues
      them back to back and plays the rest as it comes. A stop drops what is held, reported in ticket order.
      `speed.json` version 2: version-1 files measured the slowdown beside the chat model and count as empty.
      Tests in local_tests.rs (`may_start`, a slow model holds, a model that keeps up starts at once).
- [x] T059 FR-017 (operator 2026-11-06, second pass): `may_start(ratio, held, rest, answer_known)` holds a model
      slower than real time until the answer's end marker is queued (the chat model may still be writing);
      `Worker::holding` is `Option<Option<f64>>`: an unmeasured model is held too and `hold_ratio` takes the
      pace of its computed sentences (`Held::compute_secs`/`audio_secs`) until a measurement is stored. Tests
      in local_tests.rs (a slow model waits for the end, an unmeasured model that keeps up plays its first
      sentence, an unmeasured slow model is held by its own pace).

---

## Phase 9: Polish & cross-cutting concerns

- [ ] T051 Listening tests (SC-002, SC-003, SC-004): a blind comparison page from the eval WAVs (as the 066
      comparison), handed to the operator; results in `results.md`.
- [ ] T052 [P] Update `specs/066-voice-assistant/research.md` R23 with a pointer to spec 081 and the final file
      choice (Q8_0 or Q4_K_M).
- [ ] T053 [P] Split `src-tauri/src/voice.rs` per plan Complexity Tracking (`voice/stt_choice.rs`), as its own
      PR.
- [ ] T054 Run quickstart.md end to end (M1–M9) in the dev app; note results in `results.md`.
- [ ] T055 `CARGO_BUILD_JOBS=4 pnpm lint:rust`, the Android clippy, `pnpm check:settings`, `pnpm lint` before each
      PR.

---

## Dependencies & Execution Order

- **Setup (Phase 1)** → **Foundational (Phase 2)** → everything else.
- **Phase 3 (US1 engine)** before Phase 4 (parts need the engine's `part_words`), Phase 5 (US2) and Phase 7.
- **Phase 4** before Phase 6 (the speed measurement lives in the speaker's part loop).
- **Phase 5 (US2)** only needs Phase 3; it can run beside Phase 4.
- **Phase 7 (GPU)** needs Phase 3, Phase 6 (speed per acceleration) and spec 080's planner change on `main`.
- **Phase 8 (US4)** only needs Phase 2 (catalog temperature, new id); it can run any time after it, beside the
  Qwen3-TTS phases.
- **Phase 9** last.

### Parallel opportunities

- T004 and T005 (different files).
- T011 and T012; T022 and T023.
- T033 and T037 (Rust store and TS helper).
- Phase 8 entirely in parallel with Phases 3–7.

## Implementation Strategy

- **MVP**: Phases 1–4. Qwen3-TTS speaks on the processor, chosen through the preference, with parts and checks;
  the operator can listen to it in the real app.
- **Then**: Phase 5 (own voices), Phase 6 (the visible choice and honest speed marks), Phase 7 (GPU, once 080 has
  landed), Phase 8 (German Pocket) whenever the export spike is done.
- One PR per phase from Phase 3 on, each with its own tests green and the operator's listening check where the
  phase changes what is heard.
