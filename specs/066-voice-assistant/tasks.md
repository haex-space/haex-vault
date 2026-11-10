---
description: 'Task list for spec 066-voice-assistant'
---

# Tasks: Sprachassistent

**Input**: Design documents from `/specs/066-voice-assistant/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md),
[contracts/agent-and-actions.md](./contracts/agent-and-actions.md), [quickstart.md](./quickstart.md),
[ADR-0020](../../docs/adr/0020-spoken-secrets-bypass-the-model.md)

**Tests**: Included. The constitution requires an executable check for non-trivial logic:

- Rust unit tests go in sibling `*_tests.rs` files, registered with
  `#[cfg(test)] #[path = "…_tests.rs"] mod tests;`.
- Rust integration tests go in `src-tauri/tests/` (`assistant_conversation.rs`,
  `assistant_secret_contract.rs`) and drive the assistant with a WAV `Source`, a recording `Sink`,
  a fake STT, a fake voice, a scripted model and an injected clock.
- Measurements (`assistant_wakeword_eval.rs`, `assistant_echo_eval.rs`) are `#[ignore]` and run
  only in a release build.
- Catalog and UI rules go in `scripts/check-assistant-*.ts`.
- e2e scenarios go in `scripts/e2e/scenarios/` and use the `e2e-audio` feature.
- Tests come first inside each story and fail before the code exists.

**Organization**: Grouped by user story and shipped in the plan's deliveries. Each delivery is one
PR from its own worktree off `main`.

| Delivery | Phases     | Stories                                                           |
| -------- | ---------- | ----------------------------------------------------------------- |
| PR A     | –          | spec, plan, ADR-0020 (proposed), this file                        |
| PR B     | Phases 1–3 | Setup + spike, Foundational (audio, VAD, voice, tones), US8 local |
| PR C     | Phases 4–5 | US2 (conversation core), US5 (answers you can hear)               |
| PR D     | Phase 6    | US1 (wake word, launcher, dock, tray)                             |
| PR E     | Phases 7–8 | US3 (tasks, approvals, background), US4 (questions, suggestions)  |
| PR F     | Phase 9    | US6 (secrets)                                                     |
| PR G     | Phase 10   | US2 barge-in with echo cancellation (FR-016)                      |
| PR H     | Phase 11   | US9 (Android service, iOS foreground)                             |
| PR I     | Phase 12   | US7 (own wake word)                                               |
| PR J     | Phase 13   | US8 external voice                                                |
| –        | Phase 14   | Polish, manual checks                                             |

Priorities: P1 US1, US2, US3; P2 US4, US5, US6, US9; P3 US7, US8.

**Shipping notes**:

- PR B and PR C ship hidden: the launcher tile and the mode switch appear only in PR D. Until
  then, a device setting `assistant.preview` (KI → Sprache, "Sprachassistent (Vorschau)", default
  off) enables the talk shortcut and the floating indicator, so C can be tried without a wake word.
  PR D removes the preview setting.
- PR C lands the migration `chat_threads.origin`; later PRs add no migration.
- ADR-0020 changes from "proposed" to "accepted" in PR F.
- The default wake word model is trained outside this repo (T060–T063). PR D waits for its
  published revision.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US9)

**Conventions for every task**:

- **File size**: Files stay ≤ 500 lines. `chat/commands.rs` (743) only loses code;
  `chat/turn/tool_round.rs` (499) gets no new line; `migrations.rs` gets only the registration
  hook. New code goes into new files.
- **Test code**: lives in separate files.
- **Rust**:
  - No `unwrap`/`expect` on input.
  - No localized text in Rust; everything haex says or recognizes comes from the one word list
    `assistant/lexicon.json` (one row per meaning, one column per language, research R22), never
    from string literals in code.
  - No audio, transcript of pre-wake-word audio, secret or credential in a log line, error string,
    event or `Debug` output.
  - Audio callbacks neither lock nor allocate; heavy work runs on dedicated threads or
    `spawn_blocking`, everything started through `VaultGate`.
- **Commits**: Conventional Commits without agent attribution.
- **Running Rust**: only through `nix develop --command scripts/with-nix-host-bridge.sh cargo …`
  with `CARGO_BUILD_JOBS=4` and `-j 4`, one test target at a time.
- **Before each commit**:
  - Run `pnpm generate:ts-types` after every Rust type change and commit `src/types/bindings/`.
  - Run `pnpm exec prettier --write` on touched Markdown/JSON.
- **New named artifacts**: Before the first new function, module, component or store of a
  delivery, run `graphify query` for it. Record reused names in the PR description.

---

## Phase 1: Setup and spike (PR B)

**Purpose**: Prove the native stack on every target before any code depends on it (research R3,
R8). Record every result in research.md.

- [x] T001 Add `sherpa-onnx` (exact pin of the newest 2.0.x, `default-features = false`; no `shared` on desktop, see research R3 "Stand nach T001") as an optional dependency behind the `voice` feature in src-tauri/Cargo.toml; confirm with `cargo tree -i sherpa-onnx` that no GPL crate and no second onnxruntime enters the lockfile
- [x] T002 Ship the shared `libonnxruntime` next to the binary: bundle resources in src-tauri/tauri.conf.json for Linux, macOS and Windows and `jniLibs` for Android in src-tauri/gen/android/app/build.gradle.kts; add what the Nix devshell needs in flake.nix — originally not needed (research R3 "Stand nach T001"): desktop linked sherpa statically and Android's build script copied `.so` files into `jniLibs`. Spec 070 adds a Linux-only shared Sherpa build because its static ONNX Runtime (GCC 11) is incompatible in the same executable with the newer compiler used for llama.cpp; `sherpa-onnx-sys` copies the shared runtime libraries beside the Cargo binary and sets relative rpaths. macOS and Windows remain static; Android keeps its existing `jniLibs` handling; iOS waits for an iOS project.
- [x] T003 [P] Create empty modules `src-tauri/src/assistant/mod.rs` and `src-tauri/src/tts/mod.rs` behind `#[cfg(feature = "voice")]` with stub commands for `--no-default-features` (pattern of src-tauri/src/voice.rs), and register them in src-tauri/src/lib.rs — reconciled 2026-11-08: `lib.rs` uses `assistant` under feature `voice`, `assistant_stub.rs` without it
- [x] T004 [P] Create `scripts/check-assistant-actions.ts` and `scripts/check-assistant-ui.ts` (empty rule sets) and the script `check:assistant` in package.json; wire it into the CI job of `check:agent-actions` in .github/workflows/ — reconciled 2026-11-08: `check:assistant` in package.json, run by CI next to `check:agent-actions`
- [ ] T005 Spike build: build the app with sherpa on Linux, macOS, Windows, Android (arm64) and iOS (simulator) in CI; record build time and added binary size per target in research.md R3
  - Status 2026-11-08: archive sizes per target in R3, CI `platform-probe` runs `speech-detection` on Windows, macOS, Android; per-target build time and added binary size not recorded
- [ ] T006 Spike VAD: run Silero VAD through sherpa on `src-tauri/src/stt/fixtures/de_sample.wav` in a throwaway test; record CPU per second of audio on the reference desktop and the reference phone in research.md R6
  - Status 2026-11-08: Silero runs on de_sample.wav (vad_tests.rs); CPU per second of audio on desktop and phone not recorded in R6
- [x] T007 Spike ONNX sharing: load a small ONNX model through `ort` with `load-dynamic` pointed at the shipped `libonnxruntime` while sherpa runs in the same process (no duplicate symbols, same version); decide whether `ort` is needed for R4 and record it in research.md R3/R4 — done 2026-11-09: tract instead of `ort` (pure Rust, no second runtime), see R3 "Result T007"
- [ ] T008 Spike voice: synthesize five sentences per language (de, en, fr, es, it, pt; standard and `24l` variant) with Kyutai Pocket TTS through sherpa, each with a native-speaker reference recording, on the reference desktop and phone; record time to first audio, real-time factor, model size, a listening verdict incl. accent, and which languages k2-fsa ships as sherpa exports, in research.md R8; if German fails, evaluate the Piper `de_DE-thorsten` fallback with a free G2P and record the licence check
  - Status 2026-11-08: R8 "Ergebnis T008" has desktop timings and de/en verdicts; phone measurement and fr/es/it/pt listening verdicts missing
- [ ] T130 Parakeet validation (research R7): measure Parakeet TDT 0.6B v3 (sherpa `OfflineRecognizer`, `nemo_transducer`, int8) on `src-tauri/src/stt/fixtures/de_sample.wav`, `jfk.wav` and 20 own utterances of 2–15 s in each of de, en, fr, es, it, pt; record word error rate, real-time factor, time from end of speech to text and memory on the reference desktop and phone in research.md R7; evaluate Parakeet language detection for short utterances (research R22)
  - Status 2026-11-08: R7 "Ergebnis T130" has WER and time for six languages on FLEURS, desktop only; phone, memory, end-of-speech latency and own utterances missing
- [ ] T009 Spike background: with the window minimized (Linux WebKitGTK, Windows WebView2, macOS WKWebView) and the Android app in the background, confirm that `action-call-request` events still reach `useAgentActions` and are answered within 1 s; record the result in research.md R1

**Checkpoint**: The stack builds everywhere and its numbers are in research.md; if a spike fails,
update the plan before continuing.

---

## Phase 2: Foundational (PR B, blocking)

**Purpose**: Continuous audio, pause detection, local speech output, tones and model download.

### Tests first

- [x] T010 [P] Ring buffer tests in src-tauri/src/audio/ring_tests.rs: 16 kHz mono, capacity "≤ 2 s", pre-roll of 1.5 s returned in order after wrap-around, overrun drops oldest samples, no allocation after construction
- [x] T011 [P] Resampler tests in src-tauri/src/audio/resample_tests.rs: 44.1 kHz and 48 kHz stereo blocks of arbitrary size → 16 kHz mono with the same result as the one-shot path of `audio::Capture::stop` within tolerance; state carries across block borders
- [x] T012 [P] Mixer tests in src-tauri/src/audio/mixer_tests.rs: speech and tones mix with separate volumes (`assistant.tones.volume` "int 0–100"), the reference signal equals what was written to the device, `stop_speech()` silences speech within one block but lets a tone finish
- [x] T013 [P] Source/Sink tests in src-tauri/src/audio/source_tests.rs: a WAV script (files with pauses in between) yields the same samples as the files; the recording sink stores what the mixer outputs
- [x] T014 [P] Utterance tests in src-tauri/src/assistant/listen_tests.rs with the WAV source: end of utterance after `assistant.pauseMs` ("int 500–2000", default 800); utterances shorter than 250 ms are dropped; maximum 60 s cuts and delivers; pre-roll included when listening starts on a trigger
- [x] T015 [P] Voice catalog tests in src-tauri/src/tts/catalog_tests.rs: `tts_catalog.json` parses, every entry has a pinned 40-hex `hf_revision`, `language` in `"de"|"en"|"fr"|"es"|"it"|"pt"`, `variant` in `"standard"|"24l"`, a licence and attribution; one standard model per language; hardware fit like `stt/catalog.rs`
- [x] T131 [P] STT catalog tests in src-tauri/src/stt/catalog_tests.rs: the only entry uses `engine = "sherpa-parakeet"`, has a pinned 40-hex `hf_revision`, all its files with sizes, `languages` including `de` and `en`, licence `CC-BY-4.0` and attribution; removed Whisper IDs are rejected
- [x] T132 [P] Parakeet adapter tests in src-tauri/src/stt/sherpa_tests.rs: `de_sample.wav` and `jfk.wav` give the expected words (tolerant comparison like local_tests.rs); silence gives an empty text; a missing model file gives `SttError` without panic
- [x] T016 [P] Assets tests in src-tauri/src/assistant/assets_tests.rs: ~~`assets.json` lists the VAD and wake word models with pinned revisions~~ the shipped VAD model is the documented file and is written/repaired on disk (research R6; the wake word entry in `assets.json` comes with T061); `assistant_readiness` reports exactly the missing items of `Missing` in contracts/tauri-commands.md
- [x] T017 [P] Tone tests in src-tauri/src/assistant/tones_tests.rs: every `ToneId` of contracts/tauri-commands.md renders, all except `working` are shorter than 400 ms, all share one base pitch family, peak level below clipping; `working` repeats every 2 s

### Implementation

- [x] T018 Implement `audio::ring`, `audio::resample` and `audio::source` (traits `Source`, `Sink`; WAV source and recording sink only with feature `e2e-audio` or in tests) in src-tauri/src/audio/{ring,resample,source}.rs
- [x] T019 Implement the duplex stream in src-tauri/src/audio/stream.rs: one cpal input stream feeding the ring buffer through the stateful resampler and one cpal output stream driven by src-tauri/src/audio/mixer.rs; start and stop through `VaultGate`; lock-free hand-over between callback and worker (e.g. an SPSC queue)
- [x] T020 Let the push-to-talk recording of spec 008 read from the stream when it runs, and keep the current one-shot capture when it does not, in src-tauri/src/audio/mod.rs and src-tauri/src/voice.rs; existing voice tests stay green
- [x] T021 Implement `assistant::listen` in src-tauri/src/assistant/listen.rs: Silero VAD through sherpa on the ring buffer, utterances with pre-roll, pause length from the device setting, maximum 60 s
- [x] T022 Implement the voice catalog and download in src-tauri/src/tts/{catalog.rs,tts_catalog.json,commands.rs} reusing `models::download` and `models::paths::slug_dir`; commands `list_tts_catalog`, `list_installed_tts_voices`, `download_tts_voice` with `assistant-download-progress` (since T160 the shared `model-download-progress`)
- [x] T023 Implement local synthesis in src-tauri/src/tts/local.rs: sherpa `OfflineTts` per installed voice, sentence-wise synthesis on a dedicated thread, rate from `assistant.speechRate` ("float 0.75–1.5"), output into the mixer — Pocket ignores sherpa's `speed`, so the rate is a WSOLA time stretch in src-tauri/src/audio/stretch.rs (research R8)
- [x] T024 [P] Implement `assistant::tones` in src-tauri/src/assistant/tones.rs (synthesized at runtime, research R16) and the event `assistant-tone`
- [x] T025 Implement `assistant::assets` in src-tauri/src/assistant/{assets.rs,assets.json} with `download_assistant_assets` and `assistant_readiness` — both take `{language}` (contracts/tauri-commands.md); the VAD model ships in the binary, so `assets.json` starts with T061; readiness reports the wake word missing until then
- [x] T026 Wire shutdown: stop the stream, listener and synthesis in `begin_close`/`finish_close` next to `voice.cancel_for_close()`/`reset_for_close()` in src-tauri/src/instances/close.rs — stream and listener done in PR B part 3 (`VoiceState::cancel_for_close`/`reset_for_close` stop the stream; a listener ends with it); synthesis: `tts::local::Speaker` runs under `VaultGate` and ends on close (`closing_the_vault_ends_the_speaker`); its output stops with the stream

- [x] T133 Add `engine` and `languages` to src-tauri/src/stt/catalog.rs and stt_catalog.json, the Parakeet entry with its files, and download through `models::download`
- [x] T134 Implement the Parakeet adapter in src-tauri/src/stt/sherpa.rs (`SttAdapter` over sherpa `OfflineRecognizer`, `spawn_blocking`); `VoiceState` in src-tauri/src/voice.rs keeps the adapter of the selected entry warm (`Arc<dyn SttAdapter>` instead of the Whisper-only cache), so push-to-talk (008) and the assistant share it; switching the model drops the old one
- [x] T135 Release measurement `src-tauri/tests/stt_compare_eval.rs` (`--ignored`): both engines on the fixtures and a folder from `HOLZI_STT_EVAL_DIR` (WAV + reference text), prints word error rate, real-time factor and time to text per model, so the operator can repeat the comparison on own recordings

**Checkpoint**: haex can listen continuously, cut utterances and speak sentences and tones;
nothing is user-visible yet except the settings of Phase 3.

---

## Phase 3: User Story 8 - Stimme wählen, lokal (Priority: P3) (PR B)

**Goal**: Users pick a local voice per language, hear a sample, set the rate and the tones.

**Independent Test**: Pick another voice, play a sample, change the rate; the next sample uses it
without restart. Play every tone.

- [x] T027 [P] [US8] Check rules in scripts/check-assistant-ui.ts: every setting key of data-model.md has a de and en label; the voice and tone pages exist under the route `ai/speech`
  - Done 2026-11-01: the keys and their labels live in src/lib/assistant/settings.ts (`ASSISTANT_SETTING_KEYS`, `assistantLabelKey`); the check holds them against the data-model.md table and both locales. The voice page is `ai/speech/voice`; the tone page joins the check's page list with T030.
- [x] T028 [US8] Command `assistant_tts_preview {voiceId, rate}` in src-tauri/src/tts/commands.rs; downloads a missing voice first
  - Done 2026-11-01: takes `{voice: VoiceRef, rate}` as the contract says; playback in src-tauri/src/tts/preview.rs. It speaks through the assistant's stream while one runs, otherwise through a stream with only the speaker (no microphone). A newer sample stops the one playing. `VoiceState` keeps the loaded speech model warm for PR C. `model_download_check` knows a `tts` target for the download question.
- [x] T138 [US8] Built-in voices (research R8): pick 2–3 native-speaker reference recordings per language (de, en, fr, es, it, pt) from CC0 (Common Voice, Kyutai `voice-donations`) or CC-BY sources (VCTK, CML-TTS), or record volunteers with written consent as CC0; never Expresso, EARS or other NC sources; trim to 5–20 s; store in src-tauri/src/tts/voices/ with `voices.json` (`id`, `name`, `language`, `default`, source, licence, attribution) and provenance in voices/README.md
- [x] T139 [P] [US8] Voice list tests in src-tauri/src/tts/voices_tests.rs: every language of de, en, fr, es, it, pt has 2–3 voices and exactly one default; licence in `CC0-1.0`|`CC-BY-4.0`; every file decodes, is mono and 5–20 s long; `VoiceRef` parsing of `builtin:`, `clone:`, `model:<id>#<speaker>`; an unknown or deleted ref resolves to the default of its language
- [x] T140 [US8] Implement src-tauri/src/tts/voices.rs (`include_bytes!` of the references, `VoiceRef` resolution with fallback) and `tts_voices_list`; `tts::local` passes the reference as `reference_audio` and sets `voice_embedding_cache_capacity`; `assistant_tts_preview` takes a `VoiceRef`
  - Done 2026-11-01: `voices::{builtin, resolve, decode_wav, reference}`, `VoiceRef` and `tts_voices_list`. `VoiceOption` also carries `modelId`, the catalog entry to download (contract updated). Own voices and imported models resolve to the language default until PR K. `assistant_tts_preview` with a `VoiceRef` follows with T028.
  - T138 done 2026-11-01: per language one female voice (default) and one male voice, see `tts/voices/README.md`. The operator left the pick open because users choose their own voices with PR K.
- [x] T029 [US8] Settings page src/components/settings/assistant/VoiceSetting.vue under KI → Sprache: voices per language for all six languages from `tts_voices_list` (built-in now, own voices and imported models from PR K) with size and fit (OptionRow, pattern of SttModelSetting.vue), "Probe", rate slider; saved on selection as device preferences `assistant.voice.<sprache>` (a `VoiceRef`) and `assistant.speechRate`
  - Done 2026-11-01: route `ai/speech/voice` (row "Stimme" in the group "Sprache" of the KI overview). Size and fit belong to the speech model of a language, so the page shows them once under each language instead of on every voice. A sample of a voice whose model is missing asks before the download (043 FR-028).
- [x] T136 [US8] STT model list in src/components/settings/SttModelSetting.vue shows the engine and, for Parakeet, "25 europäische Sprachen"; texts in de.json and en.json; selection saved as today
- [x] T030 [US8] Settings page src/components/settings/assistant/TonesSetting.vue: list of tones with "Anhören", on/off `assistant.tones.enabled`, volume `assistant.tones.volume`; note that "Hört zu" and "Geheimnis" always play (FR-045)
  - Done 2026-11-01: route `ai/speech/tones`. "Anhören" is the new command `assistant_tone_preview {tone, volume}` (contract updated); it plays at the slider's volume, also with tones off, through the same speaker path as the voice sample.
- [x] T031 [P] [US8] Register both pages in src/lib/settings/registry.ts and src/components/wm/appRoutes.ts below `ai.speech`; texts in src/i18n/locales/de.json and en.json under `assistant.*`
  - Done 2026-11-01 with T029 and T030: `ai.speech.voice` and `ai.speech.tones`. The texts are under `settings.voice.*` and `settings.tones.*`, the labels of all assistant settings under `settings.assistant.labels.*`, like the other settings pages.
- [x] T032 [P] [US8] Credits for CC-BY voice weights in the app's licence view (where third-party licences are listed) from the `attribution` fields of `tts_catalog.json` and `tts/voices/voices.json`
  - Done 2026-11-01: command `speech_credits` (src-tauri/src/tts/credits.rs) lists every built-in voice, every speech model and the speech recognition models whose catalog entry carries an attribution (parakeet-primeline is CC BY too); "Allgemein → Lizenzen" shows them below the icon sets.

**Checkpoint (PR B done)**: Voice and tone settings work; the assistant itself is still hidden.

---

### Voice page: language tabs and the shared download (operator 2026-11-02)

- [x] T160 [US8] Voice page src/components/settings/assistant/VoiceSetting.vue: speaking rate, then one tab per language (`ShadcnTabs`, opens on the interface language); per tab a speech model row with the shared model download components (`ModelsDownloadBar`, `ModelsDownloadStatus`, `ModelsDownloadConfirmStep`, `useModelDownloadsStore`) and an explicit "Laden", then the voices of that language with "Probe" (enabled once the model is here). Speech model downloads emit the shared `model-download-progress` instead of `assistant-download-progress` (src-tauri/src/tts/commands.rs)

### Tones: distinguishable and your own (operator 2026-11-02, FR-043, FR-045a)

- [x] T155 [US8] Redesign the built-in tones in src-tauri/src/assistant/tones.rs: per tone its own colour (`Timbre`: bell, wood, soft, buzz, tick), glides between scale steps, two and a half octaves of the pentatonic; test in tones_tests.rs that no two tones share colours and direction; per-tone WAVs from the ignored listening test
- [x] T156 [US8] Migration `0051_assistant_tones` (`haex_assistant_tones` exactly as data-model.md, CRDT-tracked, no UNIQUE) in a new src-tauri/src/identity/migrations_assistant_tones.rs; `HOLZI_TRIGGER_VERSION` +1; migration test
- [x] T157 [US8] Import in a new src-tauri/src/assistant/custom_tones.rs: decode MP3/WAV/OGG/FLAC/M4A with symphonia, mix to mono, trim silence, limit 3 s, normalize the peak to the built-in tones, store as 16-bit WAV ≤ 48 kHz; tests in custom_tones_tests.rs (each format from a generated fixture, silent, too long, garbage)
- [x] T158 [US8] Commands `assistant_tones_list`, `assistant_tone_import`, `assistant_tone_assign` (1 s limit for `working`), `assistant_tone_delete`, `customId` on `assistant_tone_preview`; device pref `assistant.tones.custom`; `tones::play` uses the assigned tone and falls back to the built-in one
- [x] T159 [US8] Tones page src/components/settings/assistant/TonesSetting.vue: per tone a choice "Mitgeliefert" or an own tone with "Anhören", a group "Eigene Töne" with "Datei hinzufügen…" and delete; strings in de/en
  - Done 2026-11-02: own tones are a synced vault library (like the icon sets of 053), the choice per occasion is the device pref `assistant.tones.custom`; the managed `ToneBank` loads and follows assignments with the vault lifecycle. Fixtures in src-tauri/src/assistant/fixtures/tones/ are the new built-in "done" tone encoded by ffmpeg in five formats.

## Phase 4: User Story 2 - Ganz normal weitersprechen (Priority: P1) (PR C)

**Goal**: A conversation started by shortcut or indicator runs over several turns with follow-up
window, stop words, end phrases and continuation; answers are spoken.

**Independent Test**: With the preview setting on, start a conversation by shortcut, ask, follow up
without trigger, say "Stopp" while it speaks, end with "Danke, das war's"; the transcript is a chat
conversation marked as voice.

### Tests first

- [x] T033 [P] [US2] State machine tests in src-tauri/src/assistant/conversation_tests.rs with an injected clock: every transition of the diagram in data-model.md; follow-up window (`assistant.followUpSecs` "int 3–30", default 8) runs only when no turn runs and no question is open; continuation (`assistant.continueMins` "int 0–60", default 5) reuses the thread
  - 2026-11-02 (PR C part 3): the clock is injected as the `now: Instant` of every event. Besides the diagram the tests cover mute and pause as cross states (FR-040: no window while muted, a fresh one after unmuting), questions asked while speaking, output that ran dry before the turn ended, and continuation windows of 0, exactly 5 min and 5 min + 1 ms.
- [x] T153 [P] [US2] Word list tests in src-tauri/src/assistant/lexicon_tests.rs: every entry has a non-empty cell for every language in `languages`; `hear.*` cells are lists and have `match` in `"whole"|"start"|"contains"`; `say.*` cells use the same `{placeholders}` in every column; `hear.yes` and `hear.no` do not share a word within a language; ids are unique
- [x] T034 [P] [US2] Phrase tests in src-tauri/src/assistant/phrases_tests.rs: end phrases (rows `hear.end` of the word list, all six languages) are recognized only as whole utterances or with small additions ("danke, das war's erstmal"), never inside a longer request; stop words delegate to `stt::interrupt::match_interrupt`
- [x] T035 [P] [US2] Sentence splitting tests in src-tauri/src/assistant/sentences_tests.rs: token deltas are cut at sentence ends (., !, ?, :, line break) but not inside numbers ("3.5"), abbreviations ("z. B.", "e.g.") or URLs
- [ ] T036 [P] [US2] Integration test in src-tauri/tests/assistant_conversation.rs: WAV source + fake STT + scripted model + recording sink; two turns with context; "Stopp" during speech aborts the turn within the budget of SC-006 and keeps the conversation open; an empty utterance gives a request to repeat and no model call (FR-019); a conversation without understandable utterance leaves no thread
  - Status 2026-11-08: equivalent unit tests with fake ports (conversation_tests.rs, driver_tests.rs); the integration file and the SC-006 budget are missing
- [ ] T037 [P] [US2] Integration test in src-tauri/tests/assistant_conversation.rs: a chat turn already running in a window → the assistant says it is busy, waits up to 10 s, then gives up (research R12)
  - Status 2026-11-08: busy wait built (turn.rs `BUSY_WAIT`, `say.busy`, `say.busyGaveUp`); no test of wait-then-give-up

### Implementation

- [x] T038 [US2] Extract the body of `send_message` into `chat::send::start_turn(ctx, StartTurn { thread_id, content, origin, emit })` in src-tauri/src/chat/send.rs; `send_message` in src-tauri/src/chat/commands.rs calls it; all chat turn tests stay green
  - 2026-11-01 (PR C part 2): `chat::send::start_turn(app, StartTurn { thread_id, content, origin, idempotency_key, …, emit })`; `emit` is an `Arc<dyn Fn(&'static str, Value) + Send + Sync>` because it is used before and after the turn is spawned. The tool-availability event now goes through `emit` too. `send_message` passes `origin: None` and forwards to the window.
- [x] T039 [US2] Migration with the next free number: `ALTER TABLE chat_threads ADD COLUMN origin TEXT` ("NULL = getippt, 'voice' = Sprachgespräch") in a new src-tauri/src/identity/migrations_chat_origin.rs, registered in src-tauri/src/identity/migrations.rs; `HOLZI_TRIGGER_VERSION` 29 → 30; read/write `origin` in src-tauri/src/storage/chat_threads.rs; migration test in migrations_chat_origin_tests.rs
  - 2026-11-01 (PR C part 2): migration `0050_voice_conversations` in src-tauri/src/identity/migrations_voice.rs (main had moved on: trigger version 31 → 32, not 29 → 30). `origin` is set only when a turn creates the thread; `ThreadOrigin::from_column` reads unknown values as typed. Test in migrations_voice_tests.rs.
- [x] T124 [US2] Same migration as T039: table `voice_transcript_entries` exactly as in data-model.md (device-local contiguous `seq`, ties allowed across devices, `speaker` "'user' | 'haex'", `kind` list, `outcome` "'approved'|'declined'|'open'|'cancelled'", no cascading foreign key), CRDT-tracked; explicitly delete transcript rows before a thread in the same CRDT write; storage functions `append`, `set_outcome`, `list_for_thread` in a new src-tauri/src/storage/voice_transcript.rs with tests in voice_transcript_tests.rs
  - 2026-11-01 (PR C part 2): in the same migration `0050_voice_conversations`. `speaker`, `kind` and `outcome` read unknown values of newer builds as `Other(text)` instead of failing. `chat_threads::delete_thread_and_messages` deletes the transcript first. Tests in src-tauri/src/storage/voice_transcript_tests.rs.
- [x] T125 [US2] Write every utterance and every output to the transcript (FR-047) from src-tauri/src/assistant/conversation.rs and turn.rs: `utterance`, `answer` (full text, `spoken_until` on barge-in or stop), `announcement`, `stop`, `closing`, `notUnderstood`; never pre-wake-word audio
  - 2026-11-02 (PR C part 6): the notes come from `assistant::driver::Core` (it sees every utterance and every output), and `assistant::commands::write_notes` stores them one after the other, then emits `assistant-transcript`.
    - Notes wait until the conversation has a thread; a conversation without one stores nothing (FR-019).
    - An answer is noted once it is over: when its turn ended and its sentences sounded, before a question, on a stop, a barge-in or the end. It keeps the text of all its sentences in speakable form; `spoken_until` counts the characters of the sentences that had sounded, `0` for a muted answer. The raw answer stays in `chat_messages`.
    - Spoken questions are `approvalQuestion`/`choiceQuestion` with `outcome = open` and `ref_id`; an answer given in the AwaitingAnswer phase is `approvalAnswer`/`choiceAnswer` with the same `ref_id`. Updating the outcome comes with PR E.
    - `assistant_transcript_list {threadId}` (src-tauri/src/assistant/transcript.rs) is compiled in every build.
    - Also fixed: `tones::play` returns how long the tone sounds, so the driver waits for an own tone of up to 3 s before it listens.
- [ ] T126 [P] [US2] Integration test in src-tauri/tests/assistant_conversation.rs for SC-015: a scripted conversation with stop and closing yields transcript entries in order with timestamps and kinds; deleting the thread deletes them
  - Status 2026-11-08: pieces in voice_transcript_tests.rs and driver_tests.rs; no scripted conversation test with stop and closing
- [x] T127 [US2] Transcript view in the chat window for voice conversations (toggle "Mitschrift", time stamps, speaker, kind as small label, interrupted answers greyed after `spoken_until`) in a new src/components/chat/VoiceTranscript.vue; search over transcripts with `assistant_transcript_search {query, cursor?, limit?}` in a new src-tauri/src/storage/voice_transcript_search.rs using `passwords::ids::fold_for_search` (no FTS exists; 006 excludes it), enforcing the page limit and stable cursor ordering from contracts/tauri-commands.md, with tests in voice_transcript_search_tests.rs
  - 2026-11-03 (PR C part 10): "Mitschrift" is a toggle in the session header of a spoken session (AgentHeader.vue, `aria-pressed`); ChatApp.vue shows VoiceTranscript.vue instead of the messages while it is on, and a session opened otherwise starts with its messages. The view reads `assistant_transcript_list`, merges `assistant-transcript` entries of its thread and reads again on synced changes of `voice_transcript_entries`; kinds other than utterance and answer get a small label, approvals and choices their outcome, the unspoken rest of an answer is greyed with a screen-reader hint. Search lives in "Verlauf" (AgentSessionsPopover.vue, `useTranscriptSearch`, debounced 250 ms, "Weitere Treffer" pages on); a hit opens the session's transcript scrolled to the entry. Rust: `storage::voice_transcript_search` folds text and query one character at a time with `fold_for_search` so the snippet (the match with 40 characters on each side) maps back to the original text; it pages by row value `(thread_id, seq, id) > cursor`, reads 500 rows at a time and stops reading after limit + 1 hits. Pure helpers in src/lib/assistant/transcript.ts; tests in voice_transcript_search_tests.rs and check-assistant-ui.ts.
- [x] T040 [US2] Implement `assistant::conversation` (state machine, timers, `AssistantState` of data-model.md) in src-tauri/src/assistant/conversation.rs and src-tauri/src/assistant/mod.rs
  - 2026-11-02 (PR C part 3): pure logic in src-tauri/src/assistant/conversation.rs: `AssistantState::handle(Event, now) -> Vec<Effect>`; the caller classifies utterances (`phrases`) and carries out the effects (T041, T044); `next_deadline()` tells it when to send `Tick`. `Working` after 4 s without words (FR-023, `WORKING_AFTER`). An open question listens without wake word for one follow-up window only; after that it waits for the next wake word, which asks it again (FR-040). A stop word also aborts a round that waits for an answer, and its questions go with it. `spoken_secret` (PR F) and `echo_mode` (PR G) are not in the state yet.
- [x] T041 [US2] Implement `assistant::turn` in src-tauri/src/assistant/turn.rs: start turns through `start_turn` with `origin = 'voice'` and an `emit` closure that forwards every event to the window and feeds `chat-token` deltas through `sentences` into the voice; the model as for a typed turn (operator 2026-11-02: a new session takes the last selected model; no `assistant.modelId`); delegate notice once per conversation (spec Edge Cases)
  - 2026-11-02 (PR C part 4): `assistant::turn::Bridge` (pure, tests in turn_tests.rs) turns the chat events of one round into `Output::Say` sentences (through `speakable::Speech`) and `conversation` events (`FirstWords` before every batch, `Question` for `tool-permission-request` and `chat-choice-request`, `TurnEnded` on `chat-turn-complete`); a `chat-message-complete` of the tool loop ends the sentence in progress. `turn::start` waits up to 10 s for a busy chat (`ChatState::operation_busy`, saying `say.busy`/`say.busyGaveUp`, research R12), uses the loaded model like a typed turn and loads the last selected one (`resolve_default_model`, last active first) only when none is loaded (operator 2026-11-02: no own model for conversations, `assistant.modelId` dropped from spec, data-model and settings), and calls `start_turn` with `origin = Voice` and an `emit` that forwards every event to the window. The delegate notice is the new word list row `say.delegate`, said once per conversation (`Output::SpokeDelegateNotice`). Approval questions carry the tool name until PR E words them. Nothing calls `turn::start` yet; the driver with the speaker follows with T044.
  - 2026-11-02 (after chat PR #752): `turn::start` now goes through `chat::default_model::use_session_model(thread_id)`, the same rule as the chat: a continued conversation answers with the model its session used last, a new one with the model picked last in the model menu; a different loaded model is swapped. The "use whatever is loaded" shortcut is gone.
- [x] T154 [US2] Create src-tauri/src/assistant/lexicon.json (rows of research R22 for all six languages) and src-tauri/src/assistant/lexicon.rs (load once via `include_str!`, `hears(id, lang, text)`, `say(id, lang, args)`, `languages()`); switch `stt::interrupt::match_interrupt` in src-tauri/src/stt/interrupt.rs to the row `hear.stop` (wire values of `InterruptCommand` unchanged, all 008 interrupt tests stay green) and `SUPPORTED_LANGUAGES` in src-tauri/src/stt/local.rs to `lexicon::languages()`
  - Done 2026-11-01 (PR C part 1): rows `hear.stop`, `hear.end`, `hear.endExtra`, `hear.filler`, `hear.yes`, `hear.no`, `detect`, the `say.*` sentences PR C needs and `prompt.voice`; `spell.*` and `ordinal.*` come with PR F and PR E. Builds without `voice` compile the list through `assistant_stub.rs`. Stop words now match in all six languages, ignoring punctuation ("Stopp!").
- [x] T042 [US2] Implement `assistant::phrases` (end phrases, request to repeat, busy, goodbye; all text from the word list, T154) in src-tauri/src/assistant/phrases.rs and stop words via `stt::interrupt` (finishes spec 008 US2 for the assistant; FR-017)
  - Done 2026-11-01 (PR C part 1): `phrases::{is_end, is_empty, stop, repeat, busy, busy_gave_up, goodbye}`. An end phrase ends only with nothing around it but the small additions of `hear.endExtra` ("danke", "erstmal"); "danke" alone does not end.
- [x] T043 [US2] Use the shared warm Parakeet adapter of `VoiceState` (T134) for utterances in src-tauri/src/assistant/listen.rs (no second model in memory)
  - 2026-11-02 (PR C part 5): `listen::transcribe(app, utterance)` goes through `VoiceState::stt_adapter`, the warm adapter push-to-talk uses.
- [x] T044 [US2] Commands `assistant_get_state`, `assistant_talk`, `assistant_end_conversation`, `assistant_set_muted` and events `assistant-state`, `assistant-transcript` in src-tauri/src/assistant/commands.rs per contracts/tauri-commands.md
  - 2026-11-02 (PR C part 5):
    - Pure core `assistant::driver::Core` behind a `Ports` trait, tests in driver_tests.rs:
      - classifies utterances (`phrases`) and speaks the answer only while the conversation speaks it;
      - listens only once haex is quiet (its tones and sentences are over), since without echo suppression (PR G) the microphone would hear haex;
      - drops outputs of aborted rounds and utterances of stopped listeners.
    - The runtime in commands.rs is one task per process with one inbox:
      - `assistant_talk {language, utcOffsetMinutes}` loads the models of the conversation without downloading and opens the stream;
      - until PR D the stream lives for one conversation and closes once haex is quiet; the mode tones stay silent.
    - `tts::local::Speaker::start_reporting` reports every sentence once it has sounded or was stopped, in ticket order (two tests in local_tests.rs).
    - `turn::Bridge` reports open questions as resolved once the turn goes on (`chat-tool-call`, `chat-tool-result`, `chat-token`) or ends, so a question answered in the window no longer leaves the conversation waiting.
    - Errors go out as `assistant-error`, tones as `assistant-tone`.
    - `assistant-transcript` follows with T125, since its entries carry the stored `seq`.
    - Not yet: spoken answers to questions and the progress line (PR E), and repeating the `working` tone.
- [x] T045 [US2] Device setting `assistant.preview` (default off) in KI → Sprache that enables talk shortcut and indicator until PR D, in src/components/settings/assistant/AssistantSetting.vue
  - 2026-11-02 (PR C part 7): page "Sprachassistent" (`ai.speech.assistant`, src/components/settings/assistant/AssistantSetting.vue) with the preview switch and what is still missing (`assistant_readiness`, the wake word left out). Each missing model links to the page that downloads it with its question (043 FR-028) instead of downloading here. `assistant.preview` is in the data-model table.
- [x] T046 [P] [US2] Store src/stores/assistant.ts: state from `assistant-state`, transcript from `assistant-transcript`, commands
  - 2026-11-02 (PR C part 7): wire types and pure helpers in src/lib/assistant/wire.ts. The store loads the stored transcript of the current thread (`assistant_transcript_list`) and adds live entries; it keeps the raw last error for `useErrorString`.
- [x] T047 [US2] Floating indicator src/components/assistant/AssistantBubble.vue mounted in src/components/wm/Desktop.vue above windows and dock, below dialogs; draggable, position saved as `assistant.bubble.position` ("JSON {x, y} (Anteil 0–1 der Fläche)"); one look per phase (Lucide icons until sprites, T118) plus text for screen readers and tooltip (FR-041)
  - 2026-11-02 (PR C part 7): src/components/assistant/AssistantBubble.vue in an area above windows and dock (z-30, dialogs stay above). It is shown with the preview on or while a conversation runs. Dragging saves the position on release; a click or Enter/Space opens the compact transcript. Pure helpers in src/lib/assistant/bubble.ts.
- [x] T048 [US2] Compact transcript src/components/assistant/CompactTranscript.vue: last turns live, open question highlighted, buttons "Sprechen", Stumm, Ein/Aus (hidden until PR D), "Gespräch zeigen", "Gespräch beenden"; Escape or click collapses (FR-042)
  - 2026-11-02 (PR C part 7): the last six entries; the unspoken rest of an interrupted answer is greyed. Every button runs its action. Closes on Escape or a click outside.
- [x] T049 [US2] Actions in src/lib/actions/assistantActions.ts: `assistant.talk` (`defaultKeys` Ctrl+Shift+Space, macOS Cmd+Shift+Space), `assistant.mute.set`, `assistant.conversation.end`, `assistant.conversation.show` — all `guardrails`; handlers in a new src/stores/assistantActionHandlers.ts registered in src/plugins/actions.client.ts
  - 2026-11-02 (PR C part 7): mac chord `Shift+Meta+Space` (fixed modifier order). `assistant.talk` does nothing without the preview setting. `assistant.conversation.show` runs `chat.conversation.open` with the running or last thread.
- [x] T050 [P] [US2] Rules in scripts/check-assistant-actions.ts: every `assistant.*` action has scope `guardrails` and `agentCallable: false`; `assistant.talk` has default keys and no clash with existing `defaultKeys`
  - 2026-11-02 (PR C part 7): scripts/check-assistant-actions.ts; the UI helpers are tested in check-assistant-ui.ts.
- [x] T161 [US2] Operator decision 2026-11-02 after the first live test (spec Clarifications, FR-001, FR-003, FR-011, FR-013, FR-014, FR-041, FR-042): one on/off toggle `assistant.toggle` in the dock (control `assistant`, never stored in the synced `dock.items`, shown while this device's preview is on or the assistant runs, so it can always be switched off) and in the launcher. No floating indicator, no compact transcript, no buttons, no shortcut; a spoken conversation is a normal chat session and the chat does not open by itself.
  - Backend: `assistant_set_enabled {enabled, language, utcOffsetMinutes}` replaces `assistant_talk`. `conversation` has no follow-up window any more: haex listens until "Pause" (new words in `hear.end`, all six languages) or the toggle goes off. `Timing` keeps only the continuation window; `assistant.followUpSecs` and `assistant.bubble.position` are gone from data-model and settings.
  - Until the wake word (PR D): switching on starts a conversation at once (its listening tone stands for the mode tone); after a pause the assistant switches itself off once quiet, without a second tone. Switching off plays "Modus aus".
  - Frontend: AssistantBubble.vue, CompactTranscript.vue and the four actions of T049 are removed. src/lib/assistant/indicator.ts holds the icon per phase for dock and launcher. src/plugins/assistant.client.ts shows assistant errors as toasts.
  - Superseded by T161: T047 and T048 entirely, T049 except its handler file, the transcript part of T046.
- [x] T164 [US2] Live test 2026-11-03: a typed message sent while a spoken round was just ending failed with "Ungültige Eingabe.: a chat or vault operation is still in progress". `send_message` now waits up to 10 s (`SEND_WAIT_WHEN_BUSY`, `ChatState::wait_until_free`), as `turn::start` does the other way round (R12); if the chat stays busy, the window says "haex ist noch mit einer anderen Anfrage beschäftigt" (`errors.operationBusy`). Tests: commands_tests.rs, check-chat-tool-text.ts.
- [x] T162 [US2] Operator decision 2026-11-03 (spec Clarifications, FR-002a): the agent answers in the interface language; spoken conversations take `assistant.language` from the assistant page (device, `"ui"` default, de/en/fr/es/it/pt), which also picks recognition, word list and voice
  - 2026-11-03: `chat::reply_language` resolves the language (`read_typed`: vault `general.language`; `read_spoken`: device `assistant.language`, else the interface). `prompt::system_prompt` takes it: the tool instructions say "Reply in <language>" instead of "the user's language", and every turn gets "Always reply in <language>, even when the user writes in another language." `send.rs` reads `read_typed` for a typed turn, a spoken turn brings the language of its conversation (`StartTurn.reply_language`, so replies, voice and word list cannot drift apart when the setting changes while the mode is on); `assistant_set_enabled` loads the models of the spoken language; Parakeet detects the utterance language. AssistantSetting.vue has the select ("Wie die Oberfläche (Deutsch)" plus the six languages); the list of what is missing is gone, a voice that is not here shows its own download (T163). Tests: reply_language_tests.rs, prompt_tests.rs, check-assistant-ui.ts.
- [x] T163 [US8] Operator decision 2026-11-03: "Stimme" and "Sprachassistent" are one settings page. `ai.speech.voice` is gone (registry, appRoutes, locale titles; no redirect); AssistantSetting.vue shows VoiceSetting.vue below its settings: the voices of the conversation language ("Stimmen · <Sprache>", the `language` prop follows a new choice), each its own row with its own download, then the speaking rate; no language tabs, no speech model row. The page is found by the labels of voice and speaking rate too.
- [x] T051 [US2] Mark voice conversations in the chat history list (icon "Sprachgespräch") from `origin` in the chat sidebar components under src/components/chat/
  - 2026-11-03 (PR C part 9): `list_threads` (`ThreadPayload`) carries `origin`; "Verlauf" (AgentSessionsPopover.vue) and the session header (AgentHeader.vue) show `VOICE_THREAD_ICON` (src/lib/chat/threadOrigin.ts, the listening icon) with the label "Sprachgespräch". Tests: thread_commands_tests.rs, check-assistant-ui.ts.
- [x] T052 [P] [US2] Tone and indicator for every turn step (FR-018): `listening`, `understood`, `notUnderstood`, `aborted`, `conversationEnd` from src-tauri/src/assistant/conversation.rs — reconciled 2026-11-08: emitted in conversation.rs, shown by src/lib/assistant/indicator.ts via `assistant-state`

**Checkpoint**: Conversations by shortcut work end to end behind the preview setting.

---

## Phase 5: User Story 5 - Antworten, die man hören kann (Priority: P2) (PR C)

**Goal**: Spoken answers are short and free of formatting; tables, links and code become a short
pointer; numbers, dates and times sound natural.

**Independent Test**: Scripted model answers with a list, a table, a link and a code block; the
spoken text has no markup and points to the transcript.

- [x] T053 [P] [US5] Tests in src-tauri/src/assistant/speakable_tests.rs: headings, emphasis, list markers, inline code removed; tables, fenced code and bare URLs replaced by the pointer phrase; ISO date/time ("2026-10-29T10:00") spoken as "morgen um zehn Uhr" relative to an injected "now" (de) and "tomorrow at ten" (en); long answers (> 4 sentences) cut to a summary plus "Soll ich mehr vorlesen?"
  - Done 2026-11-01 (PR C part 1): also the other four languages, clock times in running text, offsets (`Z`, `+02:00`) in local time, and the cut after three of more than four sentences while streaming (`speakable::Speech`). "Now" comes in as `speakable::Clock`; there is no date crate.
- [x] T054 [US5] Implement `assistant::speakable` in src-tauri/src/assistant/speakable.rs and apply it in `assistant::turn` before synthesis; the transcript keeps the full text
  - 2026-11-01 (PR C part 1): `assistant::speakable` is built and tested; applying it in `assistant::turn` follows with T041.
  - 2026-11-02 (PR C part 4): applied in `assistant::turn::Bridge`; `Bridge::answer` keeps the full text for the transcript, `Bridge::rest` what was cut.
- [x] T055 [US5] Voice instruction (FR-021): `assistant::turn::voice_instruction` passes the word list row `prompt.voice` in the conversation language as the given system prompt of every spoken round (src-tauri/src/assistant/turn.rs; `chat::tools::prompt::system_prompt` puts it first, ahead of the tool instruction and the reply language), so delegates get it too. Operator 2026-11-07: the row also says the answer is read aloud (never "I cannot read aloud"), that the user's words are a transcript, and that a requested long text is read to its end without asking whether to go on. Measuring the text with the 032 tool eval stays open (T055a). Test in turn_tests.rs
- [ ] T055a [US5] Measure the voice instruction with the 032 tool eval (release build): tool calls with and without `prompt.voice` must not get worse
- [ ] T056 [US5] Measure the instruction with the tool eval of spec 032 (release build, Qwen3-4B) and a set of 10 spoken-style requests added to the eval fixtures; record the score in research.md R10
- [x] T057 [US5] ~~"Mehr vorlesen"~~ Dropped (operator 2026-11-07): answers are no longer cut after three sentences; `assistant::speakable::Speech` speaks every sentence, the word list row `say.askMore` is gone, tests in speakable_tests.rs and turn_tests.rs

**Checkpoint (PR C done)**: US2 and US5 work behind the preview setting.

---

## Phase 6: User Story 1 - Modus einschalten und per Wake Word fragen (Priority: P1) 🎯 MVP (PR D)

**Goal**: The launcher switch turns the mode on; "Haex" / "Hey Haex" starts a conversation while
haex runs (desktop also minimized); the tray shows the state.

**Independent Test**: Switch on in the launcher, minimize, say "Hey Haex, …"; tone within 0.3 s,
spoken answer, back to "bereit"; lock the vault stops everything; after unlock the mode is on again.

### Outside this repo (operator, before PR D)

- [x] T058 [US1] Check the licence of openWakeWord's shared melspectrogram and embedding models; record the result in research.md R4 (if not free, the training retrains them) — done 2026-11-08: openWakeWord's feature models have no confirmed free licence; Google `speech_embedding/1` via the Apache-2.0 ONNX conversion `androiddrew/google-speech-embedding-onnx` (pinned) replaces both, see R4
- [x] T059 [US1] Create the repo `haex-space/haex-wakeword` with the training pipeline (openWakeWord architecture, Python 3.12): positives "Haex" and "Hey Haex" synthesized with Pocket TTS and CC0/CC-BY Piper voices in German and English, the built-in reference voices of all six languages (T138) plus volunteer recordings; negatives from Common Voice (CC0) and own noise; data licences listed in its README — done 2026-11-09: https://github.com/haex-space/haex-wakeword; word said "Hex" [hɛks]; MLS/FLEURS instead of Common Voice (no account), see R4
- [x] T060 [US1] Train, evaluate against SC-003 (≥ 9 of 10 at ≤ 3 m) and SC-004 (≤ 1 false accept in 8 h at medium sensitivity) on a held-out set, and export ONNX — done 2026-11-09 for SC-004 (medium 0.05 false accepts/h on 37 h held-out speech); SC-003 with real voices is measured in the app (T065), see R4
- [x] T061 [US1] Publish the model on Hugging Face under the `haex-space` account with a pinned revision and licence; put repo, revision, size and licence into src-tauri/src/assistant/assets.json — done 2026-11-09: haex-space/haex-wakeword@d0d2dc62; files, sizes and hashes in src-tauri/src/assistant/wakeword/mod.rs instead of assets.json

### Tests first

- [x] T062 [P] [US1] Wake word tests in src-tauri/src/assistant/wakeword/model_tests.rs with WAV fixtures in src-tauri/src/assistant/fixtures/ (self-made, CC0): "Haex" and "Hey Haex" trigger, unrelated speech does not; sensitivity `"low"|"medium"|"high"` maps to thresholds monotonically; detection runs only while VAD reports speech — done 2026-11-09 (PR D1): detector tests with a fake scorer in wakeword/detector_tests.rs (rule, refractory, VAD gating, backfill, block sizes, sensitivities); the real models against pipeline values in wakeword/model_tests.rs (`--ignored`, needs `HOLZI_WAKEWORD_DIR`); no WAV fixtures
- [ ] T063 [P] [US1] Integration test in src-tauri/tests/assistant_conversation.rs: mode on → wake word + question in one breath → the question includes the pre-roll audio after the keyword; own playback through the recording sink fed back as source does not trigger (FR-010)
- [ ] T064 [P] [US1] Integration test: vault close during speech stops output, listener and conversation (FR-004); readiness with a missing voice refuses to enable and names it (FR-005)
  - Status 2026-11-08: vault close tested (driver_tests.rs, conversation_tests.rs); readiness no longer refuses (T162/T163); integration test missing
- [ ] T065 [P] [US1] Measurement test src-tauri/tests/assistant_wakeword_eval.rs (`#[ignore]`, release): recall on the positive test set and false accepts per hour on the negative set; CPU share in "bereit" (SC-010)

### Implementation

- [x] T066 [US1] Implement `assistant::wakeword::model` in src-tauri/src/assistant/wakeword/{mod.rs,model.rs}: run the default model (via `ort` or sherpa per T007) on VAD-gated windows; thresholds from `assistant.sensitivity` — done 2026-11-09 (PR D1): wakeword/{mod.rs,detector.rs,model.rs} with tract; `listen::watch_wake_word`; the driver waits "bereit" with the model installed and strips the wake word from the question (row `hear.wake`)
- [ ] T067 [US1] Self-trigger guard in src-tauri/src/assistant/wakeword/mod.rs: while haex plays audio and echo cancellation is not active (PR G), raise the threshold and ignore detections whose window overlaps own output by more than half
- [x] T068 [US1] Commands `assistant_set_enabled` and `assistant_readiness` in src-tauri/src/assistant/commands.rs; device preferences `assistant.enabled` ("bool", default `false`) and `assistant.muted`; restore after unlock in the vault-open path next to `privacy::screen_capture::apply_for_vault` in src-tauri/src/instances/open.rs
  - Status 2026-11-08: `assistant_set_enabled` and `assistant_readiness` exist; `assistant.enabled`/`assistant.muted` are not written and the mode is not restored after unlock
  - Done 2026-11-10 (PR D2a): both commands store `assistant.enabled` / `assistant.muted` (`commands::write_switch`, test in commands_tests.rs); the window restores the mode after unlock in `stores/assistant.ts` (`restoreAsync`, quiet when something is missing) instead of `instances/open.rs`, because a conversation needs the window's language and clock like the toggle; `assistant_readiness` takes the conversation language like `assistant_set_enabled`
- [x] T069 [US1] Launcher tile "Sprachassistent" next to the lock tile in src/components/wm/Launcher.vue (state on/off, runs `assistant.mode.set`); readiness dialog with download offer when something is missing
  - Status 2026-11-08: launcher tile exists while preview is on or the assistant runs; the readiness dialog is gone (T162/T163)
  - Done 2026-11-10 (PR D2a, spec Clarifications 2026-11-10): the tile is always there (hidden in builds without `voice`); the dock control `assistant` is a stored default entry the user can remove and add back (src/lib/wm/dock.ts `withAssistant`: shown while on even when removed); with something missing the toggle names it in a toast and opens the settings page that sets it up (`setupPath` in src/lib/assistant/indicator.ts) instead of a dialog
- [x] T070 [P] [US1] Dock control: add `"assistant"` to `DockControlId` in src/lib/wm/dock.ts and render it in src/components/wm/DockItem.vue; pinnable like the other controls — reconciled 2026-11-08: rendered by DockItem.vue; never stored in synced `dock.items` (T161), so not pinnable
- [x] T071 [US1] Action `assistant.mode.set` (`guardrails`) in src/lib/actions/assistantActions.ts and its handler — reconciled 2026-11-08: built as `assistant.toggle` (T161 replaced `assistant.mode.set`), handler in src/stores/assistantActionHandlers.ts
- [ ] T072 [US1] Desktop tray in src-tauri/src/assistant/tray.rs: enable Tauri feature `tray-icon` in src-tauri/Cargo.toml, add `libayatana-appindicator` to flake.nix and the Linux bundle; icon per phase, menu "Ein/Aus", "Stumm", "Gespräch zeigen"; shown only while the mode is on; removed on close
- [x] T073 [US1] First-enable notice (FR-037): one-time dialog that anyone saying the wake word can talk and confirm, also secrets; `assistant.noticeShown` — done 2026-11-10 (PR D2a): src/components/assistant/NoticeDialog.vue on the desktop, asked by `confirmNoticeAsync` in the toggle's handler; cancelling leaves the assistant off
- [ ] T074 [US1] Settings page src/components/settings/assistant/AssistantSetting.vue: mode, mute, sensitivity, pause length, follow-up window, continuation time, model for conversations; remove the preview setting of T045
  - Status 2026-11-08: page has preview, language, pause, voices, speakers, tones; mode, mute, sensitivity and continuation time missing; follow-up window obsolete (Clarifications 2026-11-02)
  - Status 2026-11-09: the wake word group (download, sensitivity) is on the page (`WakeWordSetting.vue`, PR D1)
  - Status 2026-11-10: the preview setting is gone (PR D2a; `assistant.preview` out of data-model.md, settings.ts, locales and the registry); continuation time still missing; mode and mute stay off the page (FR-001: the toggle in dock and launcher, mute by voice)
- [ ] T075 [P] [US1] e2e scenario scripts/e2e/scenarios/voice-assistant.test.ts with feature `e2e-audio`: enable in launcher, WAV script "Hey Haex, …", indicator phases in order, answer recorded by the sink, transcript in chat history, lock stops

**Checkpoint (PR D done)**: The MVP: a hands-free spoken conversation on the desktop.

---

## Phase 7: User Story 3 - Aufgaben per Sprache erledigen (Priority: P1) (PR E)

**Goal**: Tool use with spoken approvals, progress notices, background tasks that report back,
open questions, and cancellation by active end.

**Independent Test**: Approval mode "Manuell", an instruction with two changing actions: each is
announced and runs after "Ja", is skipped after "Nein"; silence leaves the question open and the
next wake word asks it again; "erledige das und gib mir Bescheid" reports by itself; active end
cancels.

### Tests first

- [ ] T076 [P] [US3] Answer classification tests in src-tauri/src/assistant/approval_tests.rs: rows `hear.yes`/`hear.no` of the word list in all six languages incl. "ja, mach", "okay", "nein, lass", "nicht"; anything else is unclear; unclear twice → declined; silence → stays open; the model is never asked
- [ ] T077 [P] [US3] Announcement tests in src-tauri/src/assistant/approval_tests.rs: effect and target from action description, `riskClass` and redacted inputs (`mcp_server::redact`); destructive adds the "cannot be undone" phrase (FR-024)
- [ ] T078 [P] [US3] Integration tests in src-tauri/tests/assistant_conversation.rs: two approvals yes/no; silence → open question, next trigger asks it first (FR-040); background completion plays `attention` and speaks, then follow-up window; active end cancels the turn and reports progress (FR-039); muted → completion only visible; a guardrail request is refused with the spoken hint (FR-028)
- [ ] T079 [P] [US3] Integration test: an approval answered in an open chat window closes the spoken question, and vice versa

### Implementation

- [ ] T080 [US3] Extract the body of `respond_tool_permission` into `chat::approvals::resolve(request_id, decision)` in src-tauri/src/chat/approvals.rs; the command calls it; existing tests stay green
  - Status 2026-11-08: private `resolve_tool_permission` in chat/commands.rs; no public `chat::approvals::resolve`
- [ ] T081 [US3] Implement `assistant::approval` in src-tauri/src/assistant/approval.rs: react to `tool-permission-request` in the emit bridge, play `question`, announce, classify, resolve through `chat::approvals::resolve`
- [x] T082 [US3] Open questions in src-tauri/src/assistant/conversation.rs: track request ids of this conversation, keep the follow-up timer off while open, ask the oldest first on the next trigger, phase `awaitingAnswer` — reconciled 2026-11-08: `open_questions`, phase `AwaitingAnswer`, tests in conversation_tests.rs
- [ ] T083 [US3] Progress notice and `working` tone after 4 s without spoken output (FR-023) in src-tauri/src/assistant/turn.rs, naming the running tool from `chat-tool-call` in plain words
  - Status 2026-11-08: `working` tone after 4 s built; the progress line is never spoken (driver maps `Line::Progress` to nothing)
- [ ] T084 [US3] Background reporting (FR-040) in src-tauri/src/assistant/conversation.rs: turn completion or a new question while no turn is being spoken → `attention` tone and speech (only visible when muted); summary of done/not done (US3 scenario 8)
  - Status 2026-11-08: `attention` tone on an answer while waiting; completion/new question and summary not spoken
- [ ] T085 [US3] Active end (FR-039): `assistant_end_conversation`, end phrase, mode off → `abort_turn` and a short spoken status in src-tauri/src/assistant/conversation.rs
  - Status 2026-11-08: abort paths built; `Line::DoneSoFar` is emitted but not spoken
- [ ] T086 [US3] Screen-needed steps (spec Edge Cases): when a tool needs the window, raise it (`raiseWindowAsync` pattern), say so and keep the step as open question kind `"screen"`
- [ ] T128 [US3] Transcript entries for approvals and questions (FR-047): `approvalQuestion`/`approvalAnswer` with `outcome` updated from `open` when answered, `choiceQuestion`/`choiceAnswer`, `progress`, `report`, in src-tauri/src/assistant/approval.rs, choice.rs and conversation.rs; cover in T078
  - Status 2026-11-08: question and answer entries written; `set_outcome`, `progress` and `report` entries missing
- [x] T087 ~~[P] [US3] Show the open question highlighted in src/components/assistant/CompactTranscript.vue and the phase `awaitingAnswer` in the indicator and tray~~ Superseded by T161 (spec Clarifications 2026-11-02): CompactTranscript.vue is gone; `awaitingAnswer` is an indicator phase of the dock toggle

**Checkpoint**: Spoken workflows with approvals work.

---

## Phase 8: User Story 4 - Rückfragen und Vorschläge (Priority: P2) (PR E)

**Goal**: The agent asks targeted questions with at most three options; answers by ordinal, name
or attribute are mapped locally; suggestions do nothing without consent.

**Independent Test**: Ambiguous request → spoken question with ≤ 3 options; "die zweite" picks the
second; a declined suggestion causes no action.

- [ ] T088 [P] [US4] Choice mapping tests in src-tauri/src/assistant/choice_tests.rs: ordinals from the `ordinal.*` rows in all six languages ("die zweite", "the second one", "letzte"), option names, attribute match ("die von Montag" when an option contains "Montag"); no match → free text to the agent
- [ ] T089 [US4] Implement `assistant::choice` in src-tauri/src/assistant/choice.rs: react to `chat-choice-request`, read ≤ 3 options (more → say the count and ask for a narrowing attribute), answer through the existing choice path
- [ ] T090 [US4] Integration test in src-tauri/tests/assistant_conversation.rs: scripted model calls `ask_user` with two options, answer "die zweite" selects option 2; a scripted suggestion answered "nein, danke" leads to no tool call
- [ ] T091 [US4] Extend the tool-eval fixtures with three ambiguous spoken requests and check that the model asks instead of guessing; record the score in research.md R10

**Checkpoint (PR E done)**: US3 and US4 work.

---

## Phase 9: User Story 6 - Geheimnisse vorlesen lassen (Priority: P2) (PR F)

**Goal**: A secret is read aloud after a spoken yes per retrieval, never reaching the model,
transcript, log or an external service (ADR-0020).

**Independent Test**: Ask for the guest Wi-Fi password, confirm, hear it spelled; "nochmal
langsamer" repeats; no outward path carries it.

- [ ] T092 [P] [US6] Spelling tests in src-tauri/src/assistant/spell_tests.rs: upper/lower case, digits, every printable ASCII symbol and umlauts have a `spell.*` row in the word list; groups of 4 with pauses; slow mode
- [ ] T093 [P] [US6] Contract test src-tauri/tests/assistant_secret_contract.rs: with a known secret, capture every emitted event, every log line (test logger), the model request, stored `chat_messages` and requests of an external voice mock; none contains the secret; the model result is exactly `{"spoken": true}`; reject consent reuse across items, fields, conversations and after the follow-up window, while allowing the same secret within that window (contracts/agent-and-actions.md)
- [ ] T094 [P] [US6] Access tests in src-tauri/src/passwords/service/speech_tests.rs: `reveal_for_speech` refuses without `SpokenConsent`; `SpokenConsent` has no public constructor outside `assistant::approval` (compile-fail doc test); `Caller::BuiltinAgent` still cannot reach any secret
- [ ] T095 [US6] Implement `reveal_for_speech(item, field, consent)` in src-tauri/src/passwords/service/speech.rs (caller user, same checks as `passwords/reveal.rs`), and `SpokenConsent` in src-tauri/src/assistant/approval.rs
- [ ] T096 [US6] Implement the native tool `assistant.secret.speak` in src-tauri/src/assistant/secret.rs per contracts/agent-and-actions.md: offered only in voice turns, always asks (any approval level), plays `secret` before speaking, local voice only, `noLocalVoice` → masked field with "Anzeigen" in the compact transcript, transcript note "Passwort von ‚<Titel>‘ vorgelesen"
- [ ] T129 [US6] Write the read-aloud note as transcript entry `kind = 'secretNote'` (never the secret) in src-tauri/src/assistant/secret.rs; T093 also scans `voice_transcript_entries`
  - Status 2026-11-08: only `EntryKind::SecretNote` exists; no writer
- [ ] T097 [US6] Repeat within the same follow-up window from a `Zeroizing<String>` that is overwritten when the window ends (FR-026) in src-tauri/src/assistant/secret.rs
- [ ] T098 [US6] Set ADR-0020 to "accepted" in docs/adr/0020-spoken-secrets-bypass-the-model.md

**Checkpoint (PR F done)**: US6 works.

---

## Phase 10: User Story 2 - Dazwischenreden mit Echo-Unterdrückung (Priority: P1) (PR G)

**Goal**: Free barge-in over speakers where AEC passes SC-005; elsewhere only wake word and stop
words; any speech with headphones.

**Independent Test**: Long answer over speakers, interrupt with free speech → stops within 0.5 s;
50 answers without self-trigger; platforms that fail fall back and are documented.

- [x] T099 [P] [US2] Echo tests in src-tauri/src/audio/echo_tests.rs (a speech-like signal through a room of three reflections: AEC3 takes ≥ 20 dB of haex speaking alone out, the user alone passes, the user over haex survives; the stage joins uneven reference and microphone blocks into 10 ms frames) and the self-filter in driver_tests.rs (`echoes`: an utterance of haex's own words after a barge-in is dropped, haex listens on)
- [x] T100 [US2] `sonora` =0.2.0 from crates.io (0.3.0 was days old) behind the trait `EchoCanceller` in src-tauri/src/audio/echo.rs (`Aec3`, 16 kHz mono, no noise suppression or gain control); `EchoStage` in the stream worker joins the mixer reference (src-tauri/src/audio/mixer.rs `reference()`, resampled to 16 kHz) and the microphone in 10 ms frames, so every subscriber and the ring buffer hear the cancelled signal; only device streams (`AudioStream::open_devices`, `cancels_echo()`)
- [x] T101 [US2] Barge-in: the driver (src-tauri/src/assistant/driver.rs) runs a watcher while the phase is `Speaking` on a stream that cancels echo (`Core::set_barge_in`, `Ports::watch_barge_in`); `assistant::listen::watch_barge_in` reports once the pause detection hears speech (≥ 250 ms, Silero) whose last 250 ms reach `BARGE_IN_MIN_RMS`; `Input::BargeIn` → `Event::BargeIn` (stop output, listening tone) and the next listener starts with the preroll (`listen_after_barge_in`, `Start::OnTrigger`); `echoMode` is `full` there, `keywordOnly` otherwise (FR-016). Tests in driver_tests.rs
- [ ] T102 [US2] Headphone detection: Android route from the plugin (`audioRoute`), desktop from the device name, overridable by `assistant.headphones` ("\"auto\" | \"yes\" | \"no\"", default `"auto"`); headphones → full barge-in
- [ ] T103 [US2] Measurement test src-tauri/tests/assistant_echo_eval.rs (`#[ignore]`, release): 50 answers through speakers with free barge-in on; count self-triggers and stop latency; run on Linux, macOS, Windows and Android
- [ ] T104 [US2] Write docs/voice-assistant-platforms.md (echo mode per platform from T103, origin of the tones per FR-046) and set the per-platform default `echoMode`; show the active mode in AssistantSetting.vue

- [ ] T171 [US2] Operator live check of barge-in on the desktop over speakers (Pocket and Qwen): interrupt a long reading with free speech and with "Stopp"; note self-triggers; tune `BARGE_IN_MIN_RMS` and the AEC delay hint if needed

**Checkpoint (PR G done)**: Barge-in as clarified.

---

## Phase 10b: Only enrolled voices (FR-016a, operator 2026-11-07) (PR L)

**Goal**: haex follows only enrolled people; anyone else's speech changes nothing.

**Independent Test**: enrol one person; another person's request, stop word and barge-in change nothing; the enrolled person's do.

- [x] T172 [US2] Research (R24: CAM++ VoxCeleb, Apache-2.0, 29.6 MB, pinned revision and SHA-256): speaker embedding model for sherpa-onnx 2.0.1 (`SpeakerEmbeddingExtractor`), candidates 3D-Speaker CAM++/ERes2Net (Apache-2.0) and WeSpeaker ResNet; size, licence, speed on the reference phone, threshold for SC-018; record in research.md (R24)
- [x] T173 [US2] Model download (own pinned download with progress as `model-download-progress` id `speaker-campplus`, not in the speech catalog) and `assistant::speakers` (embedding of an utterance, cosine match against the enrolled prints, threshold) with tests on fixed embeddings
- [x] T174 [US2] Synced table `haex_assistant_speakers` (id, name, embedding, model id, created) in a migration, CRDT like `haex_tts_voices`; storage module with tests
- [x] T175 [US2] Enrolment: `assistant_speaker_add_sentence` takes a recording of the own-voice sessions as one sentence (three passages from `enrolmentSentences` in src/lib/assistant/readAloud.ts), `assistant_speaker_save` checks they agree and stores the mean print; list, rename, delete, model status and download; SpeakersSetting.vue + SpeakerEnrolDialog.vue on the Sprachassistent page, setting `assistant.enrolledOnly` (device, default on); the commands are user-only (scripts/check-assistant-actions.ts)
- [x] T176 [US2] Gate in the runtime (`speakers::Gate`, set up per conversation in `open_audio`): `RealPorts::transcribe` drops an utterance of nobody enrolled before transcription (the listener goes on, nothing is noted); `listen::watch_barge_in` checks the first second of speech and ignores a stranger's; tests of matching, agreement and models in speakers_tests.rs, storage and migration tests
- [ ] T177 [US2] Measurement for SC-018 (`#[ignore]`, release) and the operator's live check with two people

**Checkpoint (PR L done)**: Only enrolled voices control haex.

---

## Phase 11: User Story 9 - Im Hintergrund und auf dem Telefon (Priority: P2) (PR H)

**Goal**: Android listens in the background through a foreground service with notification; iOS
only in the foreground; phone calls pause listening.

**Independent Test**: Android: mode on, leave the app, lock the screen, wake word works; the
notification shows the state and turns it off; a call pauses.

- [ ] T105 [US9] Kotlin service src-tauri/plugins/holzi-android/android/src/main/java/space/haex/holzi/android/AssistantService.kt: `foregroundServiceType="microphone"`, persistent notification "haex hört zu" with "Ausschalten" and "Stumm", updated by `updateAssistantNotification`; declare `<service>`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MICROPHONE`, `POST_NOTIFICATIONS` in the plugin AndroidManifest.xml
- [ ] T106 [US9] Plugin methods `startAssistantService`, `stopAssistantService`, `updateAssistantNotification`, `audioRoute` and events `assistant-notification-action`, `audio-focus` in HolziAndroidPlugin.kt, src-tauri/plugins/holzi-android/src/{android.rs,other.rs,lib.rs}
- [ ] T107 [US9] Start the service only from the visible app: on enable and on every resume while the mode is on (Android 14+ rule, research R15), in src-tauri/src/assistant/mod.rs
- [ ] T108 [US9] Pause on audio focus loss or capture errors (calls, exclusive use) with phase `paused`, resume automatically (FR-036)
  - Status 2026-11-08: `Event::Pause` and the `paused` wire field exist; nothing raises it from audio focus or capture errors
- [ ] T109 [US9] iOS: stop listening on `RunEvent` background, resume in the foreground, explain once (FR-035) in src-tauri/src/assistant/mod.rs and AssistantSetting.vue
- [ ] T110 [P] [US9] Android e2e scenario scripts/e2e/scenarios/android-voice-assistant.test.ts: enable, background the app, the notification exists with both actions, "Ausschalten" turns the mode off

**Checkpoint (PR H done)**: US9 on all platforms.

---

## Phase 12: User Story 7 - Eigenes Wake Word einlernen (Priority: P3) (PR I)

**Goal**: Users record their own wake word four times, haex checks and learns it locally, a probe
phase confirms, raw recordings are gone.

**Independent Test**: Enroll "Hallo Eichhörnchen"; probe shows detections; afterwards only it
triggers; no audio file in the data directory.

- [ ] T111 [US7] Spike (research R5): compare embedding templates against sherpa open-vocabulary KWS (keyword text from Parakeet transcripts, threshold from the samples) on 10 German and 10 English words with 5 speakers; record recall and false accepts in research.md R5 and keep the better method as `method` in `CustomWakeWord`
- [ ] T112 [P] [US7] Template tests in src-tauri/src/assistant/wakeword/templates_tests.rs: `templates` "genau 4", each "höchstens 2 s Einbettung", `threshold` "in (0, 1)"; invalid or missing entry falls back to `"default"`
- [ ] T113 [P] [US7] Enrollment tests in src-tauri/src/assistant/wakeword/enroll_tests.rs: too short (< 2 syllables or < 0.4 s) → `tooShort`; dissimilar samples → `inconsistent`; close to a word of the shipped common-word list → `confusable` with `similar`; cancel drops everything; raw buffers are zeroed after embedding
- [ ] T114 [US7] Implement `assistant::wakeword::{templates,enroll}` and commands `assistant_enroll_start/record/finish/cancel` and `assistant_wakeword_probe` with event `assistant-wakeword-probe`; store `assistant.wakeWord.custom` per data-model.md
- [ ] T115 [US7] Settings page src/components/settings/assistant/WakeWordSetting.vue: default or own word, guided four recordings with progress, problems in plain words, probe phase, "Zurück zum Standardwort"

**Checkpoint (PR I done)**: US7 works.

---

## Phase 13: User Story 8 - Externer Sprachdienst (Priority: P3) (PR J)

**Goal**: An external speech service can be set up; the indicator marks it permanently; failures
fall back to the local voice; secrets never use it.

**Independent Test**: Set up an OpenAI-compatible service, the indicator marks it; cut the network,
the local voice speaks and says so once.

- [ ] T116 [P] [US8] Tests in src-tauri/src/tts/external_tests.rs against a local mock server: request shape of `/audio/speech`, credentials from the provider store, timeout → fallback signal; secrets are refused by type (`SpeakableText` without secret variant)
- [ ] T117 [US8] Implement src-tauri/src/tts/external.rs and the provider type "Sprachausgabe" in the provider settings; `assistant.ttsProvider`; `externalTts` in `AssistantStateWire`; indicator and tray mark it (FR-031); STT external marking `externalStt` (FR-032) shown the same way

**Checkpoint (PR J done)**: All stories done.

---

## Phase 13b: User Story 8 - Eigene Stimmen (Priority: P3) (PR K)

**Goal**: Users clone a voice from a recording or file with confirmed consent, synced through the
vault, and import model files that stay on the device.

**Independent Test**: Clone a voice from `de_sample.wav`, hear the sample, save; it appears on a
second device of the same vault and speaks the next answer there. Import a model folder and pick
it. A folder with a `.so` file is refused.

- [ ] T137 (deferred with FR-051, operator 2026-11-03) Spike (research R21): load one model of each sherpa 2.x TTS engine (Pocket community model, Kokoro, Supertonic, Matcha, Kitten, VITS) without espeak-ng; record which speak correctly and their required files in research.md R21; the import accepts only those

### Tests first

- [x] T141 [P] [US8] Migration tests in src-tauri/src/identity/migrations_tts_voices_tests.rs: tables exactly as data-model.md (`language` "'de'|'en'|'fr'|'es'|'it'|'pt'", `reference_hash` → `haex_tts_voices_binaries.hash`, `consent_confirmed_at INTEGER NOT NULL`); `haex_tts_voices` and the binaries table are CRDT-tracked, `haex_tts_models_no_sync` is not
  - 2026-11-03 (PR K part 1): migration `0052_tts_voices` (trigger version 33 → 34), tests in src-tauri/src/identity/migrations_tts_voices_tests.rs. `haex_tts_models_no_sync` belongs to the model import and comes with PR K part 2.
- [x] T142 [P] [US8] Clone tests in src-tauri/src/tts/clone_tests.rs with WAV fixtures: < 10 s speech → `tooShort`; a recording beyond 30 s is cut at its last pause before 30 s (T166); low level → `tooQuiet`; samples near full scale → `clipped`; noisy pauses → `noisy`; save without `consent: true` is refused; save stores the binary by SHA-256 and `consent_confirmed_at`; delete removes the row, orphans the binary and resolves refs to the language default
  - 2026-11-03 (PR K part 1): synthetic fixtures (tone for speech, noise for pauses) instead of WAV files in src-tauri/src/tts/clone_tests.rs; storage in storage/tts_voices_tests.rs; consent, name and language in tts/clone_commands_tests.rs. Speech beyond 30 s is cut (research R20), so `tooLong` stays in the contract but is never reported; spec US8 scenario 7 lists "mehr als 30 Sekunden" as a problem, R20 and this task cut instead.
- [ ] T143 (deferred with FR-051) [P] [US8] Import tests in src-tauri/src/tts/import_tests.rs with small fixture folders: each engine accepted by T137 is detected with languages and speakers; `.so`, `.dll`, `.dylib`, scripts → `forbiddenFile`; an ONNX file with a custom operator domain → `forbiddenFile`; > 2 GB → `tooLarge`; anything else → `unknownLayout`; import without `rightsConfirmed: true` is refused
- [x] T144 [P] [US8] Rule in scripts/check-assistant-actions.ts: no action, agent tool or extension API calls `tts_clone_*` or `tts_model_import` (FR-050, FR-051)
  - 2026-11-03 (PR K part 1): the rule reads src/lib/actions, src/stores, src-tauri/src/{chat,mcp_server,extensions,agent_browser} and allows the commands in the window only in src/composables/useTtsVoices.ts, and its wrappers (`clone*Async`, `voiceDeleteAsync`) only in VoiceSetting.vue and VoiceCloneDialog.vue.

### Implementation

- [x] T145 [US8] Migration with the next free number in a new src-tauri/src/identity/migrations_tts_voices.rs (tables of data-model.md), registered in migrations.rs; `HOLZI_TRIGGER_VERSION` +1; storage in a new src-tauri/src/storage/tts_voices.rs
  - 2026-11-03 (PR K part 1): src-tauri/src/identity/migrations_tts_voices.rs (`0052_tts_voices`), storage in src-tauri/src/storage/tts_voices.rs (insert by SHA-256, recording by id, delete marks an unused recording, prune after seven days on vault open).
- [x] T146 [US8] Implement src-tauri/src/tts/clone.rs and the commands `tts_clone_record_start`, `tts_clone_record_stop`, `tts_clone_from_file`, `tts_clone_preview`, `tts_clone_save`, `tts_voice_delete` and the event `tts-clone-level` per contracts/tauri-commands.md; recording through the duplex stream only while the dialog is open
  - 2026-11-03 (PR K part 1): src-tauri/src/tts/clone.rs (check in 20-ms frames per US8 scenario 7, silence cut at both ends) and src-tauri/src/tts/clone_commands.rs. One session at a time (a new recording or file ends the last one and releases its microphone; an older file check that finishes later does not take the slot); recording via `Capture` (the assistant's stream when it runs), level from the new `Capture::tail`. `tts_clone_from_file` takes the picked file (`{file: PickedFile}`) like the tone import instead of `{path}`, and `tts_clone_preview` takes an optional `rate`. File recordings are kept at most at 24 kHz, microphone recordings at 16 kHz. `VoiceRef` `clone:<id>` now resolves to the own recording (`voices::choose`, `preview_choice`) for the assistant and samples; `tts_voices_list` lists own voices (kind `clone`).
- [ ] T147 (deferred with FR-051) [US8] Implement src-tauri/src/tts/import.rs and `tts_model_inspect`, `tts_model_import`: folder, `.tar.bz2` archive or Hugging Face repo (revision pinned at import, download through `models::download`); copy to `<AppLocalData>/models/user-tts-<id>/`, register in `device_downloaded_models_no_sync`, metadata in `haex_tts_models_no_sync`
- [x] T148 [US8] Sync: emit `tts-voices-changed` when `haex_tts_voices` changes (vault-data-changed pattern); a selected but deleted voice falls back to the language default; choosing a synced voice whose language model is missing downloads it first
  - 2026-11-03 (PR K part 1): vault_events emits `tts-voices-changed` when `haex_tts_voices` changes; a deleted or foreign-language own voice resolves to the language default; choosing a voice whose speech model is missing asks to download it (VoiceSetting.vue, PR #786).
- [x] T149 [US8] Dialogs src/components/settings/assistant/VoiceCloneDialog.vue (record with level meter or choose file, problems in plain words, sample, name, language, consent checkbox "Das ist meine Stimme oder die Person hat eingewilligt", save) and VoiceImportDialog.vue (source, inspection result or reason, name, languages, licence, speaker, rights checkbox); delete with confirmation in VoiceSetting.vue; texts in de.json and en.json — reconciled 2026-11-08: VoiceCloneDialog.vue and own-voice rows in VoiceSetting.vue; VoiceImportDialog.vue falls under the FR-051 deferral
  - 2026-11-03 (PR K part 1): VoiceCloneDialog.vue done (record with level meter or audio file, problems in words, sample, name, language preset to the conversation language, consent checkbox, save), reached from "Eigene Stimme anlegen" in the voices group of VoiceSetting.vue; own voices are rows there with a delete that asks first. VoiceImportDialog.vue follows with PR K part 2.
- [ ] T150 [P] [US8] e2e scenario scripts/e2e/scenarios/voice-clone-sync.test.ts on the multi-device rig (033): clone from a WAV fixture on device A, the voice appears on device B; delete on B, A falls back to the default

- [x] T165 [US8] Operator 2026-11-03: the clone dialog asks for the language first and offers a light text to read aloud in it (src/lib/assistant/readAloud.ts, five per language, 55–85 words ≈ 20–30 s), with "Anderer Text"; an audio file stays the other way. Model import (FR-051: T137, T143, T147, VoiceImportDialog) is deferred: a 10–30 s recording is enough for an own voice. Checks in check-assistant-ui.ts.
- [x] T166 [US8] Operator 2026-11-03 (own voices not convincing): record the clone from the device at its own rate (`Capture::stop_native`, fallback to the 16 kHz stream), store at 24 kHz through `resample_filtered` (low-pass) in src-tauri/src/tts/clone_commands.rs, keep at most 30 s ending on a pause (src-tauri/src/tts/clone.rs), and let sherpa read the whole reference (`max_reference_audio_len` 30 in src-tauri/src/tts/local.rs); tests in clone_tests.rs, clone_commands_tests.rs, resample_tests.rs, local_tests.rs
- [x] T167 [US2] Operator 2026-11-04 (times, numbers and names spoken badly): FR-022b in src-tauri/src/tts/spoken.rs (dates and times in spoken/dates.rs, units in spoken/units.rs, digit groups in spoken/digits.rs, words and abbreviations in spoken/words.rs), applied by `PocketModel::generate` (which now knows its language) to every sentence: clock times, German and ISO dates, amounts with cents, percentages, units, ranges, ordinals, negative numbers, years, digit-by-digit telephone numbers (also in dashed groups, never as a range), abbreviations; a German ordinal only where the period does not end the sentence, an English year only where a word announces it; "2:1" or "11/04/2026" stay as written rather than half read. The number words moved from src-tauri/src/assistant/speakable.rs to src-tauri/src/assistant/number_words.rs (`cardinal` up to the billions, `cardinal_large` up to 999 billion), the day and time words to src-tauri/src/assistant/calendar_words.rs. Tests in spoken_tests.rs and spoken_edge_tests.rs (odd input in every language, long digit runs, long texts). Names stay open (pronunciation list, operator to send examples).
- [x] T168 [US2] Operator 2026-11-06 (listen while the agent works): in src-tauri/src/assistant/conversation.rs a request in `Thinking`/`Working` no longer stops listening; utterances there are taken: a request is noted in `Conversation::queued` (tone "verstanden", listening goes on), a stop word aborts the round and the noted requests, an end phrase ends the conversation, an empty utterance is ignored without "Wie bitte?"; `after_round` starts the noted requests as the next round in one message. Listening stops when the answer starts speaking (no echo cancellation yet, PR G) and pauses around the progress line; unmuting during a round listens again. Tests in conversation_tests.rs and driver_tests.rs.
- [x] T169 [US5] Operator 2026-11-07 (FR-022c): speaker and copy buttons under every finished answer of the agent (src/components/chat/MessageActions.vue in MessageList.vue, src/composables/useMessageSpeech.ts, one reading at a time); `chat_message_speak`/`chat_message_speak_stop` in src-tauri/src/tts/commands.rs run src-tauri/src/tts/read_aloud.rs (language by the word list's `detect`, `speakable::speak_all`, the conversation's `Speaker` with its hold rule, the sample slot of `VoiceState`); the speech model loading shared with the assistant (`tts::commands::load_installed`, `voice_settings`). Tests in read_aloud_tests.rs
- [x] T170 [US5] Operator 2026-11-07 (FR-022d): `assistant::passage::PassageLanguage` decides the language sentence by sentence (`Lexicon::detect_hits`, `detect` rows extended to ~60 common words per language; switch only to a sole leader with more clues than the current language and at least two, or the current none; reset per answer); `tts::local::Passage` + `LanguageVoices` let the speaker speak a sentence with another language's model, voice and number words; `tts::languages::AppVoices` loads them from the device settings on first need (never downloads). Used by the conversation (`RealPorts::say_with`, answers only) and by reading aloud. Tests in passage_tests.rs and local_tests.rs

**Checkpoint (PR K done)**: Own voices and imported models work.

---

## Phase 14: Polish & Cross-Cutting Concerns

- [x] T118 ~~[P] Squirrel sprites per phase (ready, listening, thinking, working, speaking, awaiting answer, muted, paused) via the image-model workflow (one row of 4 frames, magenta key, aligned on the tail) in src/assets/assistant/; flip-book like src/components/files/Loader.vue in AssistantBubble.vue~~ Superseded by T161: AssistantBubble.vue is gone; the dock toggle uses Lucide icons per phase. Sprites for the dock toggle would need a new decision
- [ ] T119 [P] Accessibility: every phase and tone has a visible and screen-reader equivalent (FR-044); check with scripts/check-assistant-ui.ts
  - Status 2026-11-08: tone names and phase labels checked by check-assistant-ui.ts; `muted`/`paused` have no phase, `assistant-tone` events have no visible equivalent
- [ ] T120 [P] Update plans/README.md row 066 after each merged PR
- [ ] T121 [P] Listening test with 5 people for SC-014 (core tones); adjust tones.rs if fewer than 4 of 5 are recognized
- [ ] T122 Battery measurement on the reference phone (1 h "bereit", SC-010) and CPU on the reference desktop; record in research.md
- [ ] T151 [P] Native speakers review the columns fr, es, it and pt of src-tauri/src/assistant/lexicon.json and the date and number rules in speakable.rs, calendar_words.rs, number_words.rs and tts/spoken/ and judge the default voices (SC-017); record results in research.md R22
- [ ] T152 [P] Clone test with 5 people for SC-016 (time to a saved voice, recognized in the sample)
- [x] T161 Pause before answering (operator 2026-11-02: a thinking pause of 0.8 s ended what was said): `assistant.pauseMs` default 1500, range 500–3000 (data-model.md, src-tauri/src/assistant/commands.rs, vad.rs), slider in src/components/settings/assistant/AssistantSetting.vue, `parsePause` in src/lib/assistant/settings.ts, check in scripts/check-assistant-ui.ts
- [ ] T123 Manual quickstart M1–M16 (specs/066-voice-assistant/quickstart.md); record results in the last PR description

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** → **Foundational (Phase 2)** → US8 local (Phase 3): PR B. A failed spike
  changes the plan before Phase 2.
- **US2 + US5 (Phases 4–5, PR C)** depend on PR B.
- **US1 (Phase 6, PR D)** depends on PR C and on the published model (T058–T061).
- **US3 + US4 (Phases 7–8, PR E)**, **US2 barge-in (Phase 10, PR G)**, **US9 (Phase 11, PR H)**,
  **US7 (Phase 12, PR I)** and **US8 external (Phase 13, PR J)** depend on PR D and not on each
  other.
- **US8 own voices (Phase 13b, PR K)** depends only on PR B and can run alongside PR C.
- **US6 (Phase 9, PR F)** depends on PR E (spoken approval and `SpokenConsent` home).
- **Polish (Phase 14)** runs alongside; T123 last.

### Within Each Story

- Tests first and failing, then implementation.
- Rust types before `pnpm generate:ts-types`, then frontend.
- `chat::send::start_turn` (T038) before `assistant::turn` (T041); `chat::approvals::resolve`
  (T080) before `assistant::approval` (T081).

### Parallel Opportunities

- Phase 1: T003 and T004 parallel to T001–T002; spikes T006–T009 and T130 in parallel after T005.
- Phase 2: all tests T010–T017 in parallel; T024 parallel to T021–T023.
- US2: T033–T037 in parallel; T046 and T050 parallel to the Rust work.
- US1: T062–T065 in parallel; T070 parallel to T069.
- PR E, G, H, I, J can be built in parallel worktrees after PR D; PR K after PR B.

## Parallel Example: User Story 2

```text
Task: "T033 State machine tests in src-tauri/src/assistant/conversation_tests.rs"
Task: "T034 Phrase tests in src-tauri/src/assistant/phrases_tests.rs"
Task: "T035 Sentence splitting tests in src-tauri/src/assistant/sentences_tests.rs"
Task: "T046 Store src/stores/assistant.ts"
Task: "T050 Rules in scripts/check-assistant-actions.ts"
```

## Implementation Strategy

### MVP First (PR B–D)

1. PR B: spike, audio, voice, tones (hidden).
2. PR C: conversation by shortcut, spoken answers (preview setting).
3. Train and publish the default wake word (outside the repo).
4. PR D: wake word, launcher switch, tray. **Stop and validate**: M1, M2, M11.

### Incremental Delivery

1. PR E → tasks, approvals, questions, background reporting (M3, M4).
2. PR F → secrets (M5).
3. PR G → free barge-in (M6).
4. PR H → Android and iOS (M9, M10).
5. PR I → own wake word (M7).
6. PR J → external voice (M8).

Each PR keeps the mode off by default and adds value without breaking the earlier ones.
