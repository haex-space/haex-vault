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
- [ ] T003 [P] Create empty modules `src-tauri/src/assistant/mod.rs` and `src-tauri/src/tts/mod.rs` behind `#[cfg(feature = "voice")]` with stub commands for `--no-default-features` (pattern of src-tauri/src/voice.rs), and register them in src-tauri/src/lib.rs
- [ ] T004 [P] Create `scripts/check-assistant-actions.ts` and `scripts/check-assistant-ui.ts` (empty rule sets) and the script `check:assistant` in package.json; wire it into the CI job of `check:agent-actions` in .github/workflows/
- [ ] T005 Spike build: build the app with sherpa on Linux, macOS, Windows, Android (arm64) and iOS (simulator) in CI; record build time and added binary size per target in research.md R3
- [ ] T006 Spike VAD: run Silero VAD through sherpa on `src-tauri/src/stt/fixtures/de_sample.wav` in a throwaway test; record CPU per second of audio on the reference desktop and the reference phone in research.md R6
- [ ] T007 Spike ONNX sharing: load a small ONNX model through `ort` with `load-dynamic` pointed at the shipped `libonnxruntime` while sherpa runs in the same process (no duplicate symbols, same version); decide whether `ort` is needed for R4 and record it in research.md R3/R4
- [ ] T008 Spike voice: synthesize five sentences per language (de, en, fr, es, it, pt; standard and `24l` variant) with Kyutai Pocket TTS through sherpa, each with a native-speaker reference recording, on the reference desktop and phone; record time to first audio, real-time factor, model size, a listening verdict incl. accent, and which languages k2-fsa ships as sherpa exports, in research.md R8; if German fails, evaluate the Piper `de_DE-thorsten` fallback with a free G2P and record the licence check
- [ ] T130 Spike STT (research R7): run the existing candle Whisper (tiny, base, small) and Parakeet TDT 0.6B v3 (sherpa `OfflineRecognizer`, `nemo_transducer`, int8) on `src-tauri/src/stt/fixtures/de_sample.wav`, `jfk.wav` and 20 own utterances of 2–15 s in each of de, en, fr, es, it, pt; record word error rate, real-time factor, time from end of speech to text and memory on the reference desktop and phone in research.md R7; also record whether Parakeet through sherpa reports the spoken language and how often Whisper picks the right one of the six candidates on short utterances (research R22); propose the default for new installs (Whisper stays selectable)
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
- [x] T131 [P] STT catalog tests in src-tauri/src/stt/catalog_tests.rs: every entry has `engine` in `"candle-whisper"|"sherpa-parakeet"`; the existing Whisper entries are unchanged; the Parakeet entry has a pinned 40-hex `hf_revision`, all its files with sizes, `languages` including `de` and `en`, licence `CC-BY-4.0` and attribution
- [x] T132 [P] Parakeet adapter tests in src-tauri/src/stt/sherpa_tests.rs: `de_sample.wav` and `jfk.wav` give the expected words (tolerant comparison like local_tests.rs); silence gives an empty text; a missing model file gives `SttError` without panic
- [x] T016 [P] Assets tests in src-tauri/src/assistant/assets_tests.rs: ~~`assets.json` lists the VAD and wake word models with pinned revisions~~ the shipped VAD model is the documented file and is written/repaired on disk (research R6; the wake word entry in `assets.json` comes with T061); `assistant_readiness` reports exactly the missing items of `Missing` in contracts/tauri-commands.md
- [x] T017 [P] Tone tests in src-tauri/src/assistant/tones_tests.rs: every `ToneId` of contracts/tauri-commands.md renders, all except `working` are shorter than 400 ms, all share one base pitch family, peak level below clipping; `working` repeats every 2 s

### Implementation

- [x] T018 Implement `audio::ring`, `audio::resample` and `audio::source` (traits `Source`, `Sink`; WAV source and recording sink only with feature `e2e-audio` or in tests) in src-tauri/src/audio/{ring,resample,source}.rs
- [x] T019 Implement the duplex stream in src-tauri/src/audio/stream.rs: one cpal input stream feeding the ring buffer through the stateful resampler and one cpal output stream driven by src-tauri/src/audio/mixer.rs; start and stop through `VaultGate`; lock-free hand-over between callback and worker (e.g. an SPSC queue)
- [x] T020 Let the push-to-talk recording of spec 008 read from the stream when it runs, and keep the current one-shot capture when it does not, in src-tauri/src/audio/mod.rs and src-tauri/src/voice.rs; existing voice tests stay green
- [x] T021 Implement `assistant::listen` in src-tauri/src/assistant/listen.rs: Silero VAD through sherpa on the ring buffer, utterances with pre-roll, pause length from the device setting, maximum 60 s
- [x] T022 Implement the voice catalog and download in src-tauri/src/tts/{catalog.rs,tts_catalog.json,commands.rs} reusing `models::download` and `models::paths::slug_dir`; commands `list_tts_catalog`, `list_installed_tts_voices`, `download_tts_voice` with `assistant-download-progress`
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

- [ ] T027 [P] [US8] Check rules in scripts/check-assistant-ui.ts: every setting key of data-model.md has a de and en label; the voice and tone pages exist under the route `ai/speech`
- [ ] T028 [US8] Command `assistant_tts_preview {voiceId, rate}` in src-tauri/src/tts/commands.rs; downloads a missing voice first
- [ ] T138 [US8] Built-in voices (research R8): pick 2–3 native-speaker reference recordings per language (de, en, fr, es, it, pt) from CC0 (Common Voice, Kyutai `voice-donations`) or CC-BY sources (VCTK, CML-TTS), or record volunteers with written consent as CC0; never Expresso, EARS or other NC sources; trim to 5–20 s; store in src-tauri/src/tts/voices/ with `voices.json` (`id`, `name`, `language`, `default`, source, licence, attribution) and provenance in voices/README.md
- [x] T139 [P] [US8] Voice list tests in src-tauri/src/tts/voices_tests.rs: every language of de, en, fr, es, it, pt has 2–3 voices and exactly one default; licence in `CC0-1.0`|`CC-BY-4.0`; every file decodes, is mono and 5–20 s long; `VoiceRef` parsing of `builtin:`, `clone:`, `model:<id>#<speaker>`; an unknown or deleted ref resolves to the default of its language
- [x] T140 [US8] Implement src-tauri/src/tts/voices.rs (`include_bytes!` of the references, `VoiceRef` resolution with fallback) and `tts_voices_list`; `tts::local` passes the reference as `reference_audio` and sets `voice_embedding_cache_capacity`; `assistant_tts_preview` takes a `VoiceRef`
  - Done 2026-11-01: `voices::{builtin, resolve, decode_wav, reference}`, `VoiceRef` and `tts_voices_list`. `VoiceOption` also carries `modelId`, the catalog entry to download (contract updated). Own voices and imported models resolve to the language default until PR K. `assistant_tts_preview` with a `VoiceRef` follows with T028.
  - T138 is still open: the recordings in `tts/voices/` are a provisional pick (two per language), the operator chooses the final ones by listening.
- [ ] T029 [US8] Settings page src/components/settings/assistant/VoiceSetting.vue under KI → Sprache: voices per language for all six languages from `tts_voices_list` (built-in now, own voices and imported models from PR K) with size and fit (OptionRow, pattern of SttModelSetting.vue), "Probe", rate slider; saved on selection as device preferences `assistant.voice.<sprache>` (a `VoiceRef`) and `assistant.speechRate`
- [x] T136 [US8] STT model list in src/components/settings/SttModelSetting.vue shows the engine and, for Parakeet, "25 europäische Sprachen"; texts in de.json and en.json; selection saved as today
- [ ] T030 [US8] Settings page src/components/settings/assistant/TonesSetting.vue: list of tones with "Anhören", on/off `assistant.tones.enabled`, volume `assistant.tones.volume`; note that "Hört zu" and "Geheimnis" always play (FR-045)
- [ ] T031 [P] [US8] Register both pages in src/lib/settings/registry.ts and src/components/wm/appRoutes.ts below `ai.speech`; texts in src/i18n/locales/de.json and en.json under `assistant.*`
- [ ] T032 [P] [US8] Credits for CC-BY voice weights in the app's licence view (where third-party licences are listed) from the `attribution` fields of `tts_catalog.json` and `tts/voices/voices.json`

**Checkpoint (PR B done)**: Voice and tone settings work; the assistant itself is still hidden.

---

## Phase 4: User Story 2 - Ganz normal weitersprechen (Priority: P1) (PR C)

**Goal**: A conversation started by shortcut or indicator runs over several turns with follow-up
window, stop words, end phrases and continuation; answers are spoken.

**Independent Test**: With the preview setting on, start a conversation by shortcut, ask, follow up
without trigger, say "Stopp" while it speaks, end with "Danke, das war's"; the transcript is a chat
conversation marked as voice.

### Tests first

- [ ] T033 [P] [US2] State machine tests in src-tauri/src/assistant/conversation_tests.rs with an injected clock: every transition of the diagram in data-model.md; follow-up window (`assistant.followUpSecs` "int 3–30", default 8) runs only when no turn runs and no question is open; continuation (`assistant.continueMins` "int 0–60", default 5) reuses the thread
- [ ] T153 [P] [US2] Word list tests in src-tauri/src/assistant/lexicon_tests.rs: every entry has a non-empty cell for every language in `languages`; `hear.*` cells are lists and have `match` in `"whole"|"start"|"contains"`; `say.*` cells use the same `{placeholders}` in every column; `hear.yes` and `hear.no` do not share a word within a language; ids are unique
- [ ] T034 [P] [US2] Phrase tests in src-tauri/src/assistant/phrases_tests.rs: end phrases (rows `hear.end` of the word list, all six languages) are recognized only as whole utterances or with small additions ("danke, das war's erstmal"), never inside a longer request; stop words delegate to `stt::interrupt::match_interrupt`
- [ ] T035 [P] [US2] Sentence splitting tests in src-tauri/src/assistant/sentences_tests.rs: token deltas are cut at sentence ends (., !, ?, :, line break) but not inside numbers ("3.5"), abbreviations ("z. B.", "e.g.") or URLs
- [ ] T036 [P] [US2] Integration test in src-tauri/tests/assistant_conversation.rs: WAV source + fake STT + scripted model + recording sink; two turns with context; "Stopp" during speech aborts the turn within the budget of SC-006 and keeps the conversation open; an empty utterance gives a request to repeat and no model call (FR-019); a conversation without understandable utterance leaves no thread
- [ ] T037 [P] [US2] Integration test in src-tauri/tests/assistant_conversation.rs: a chat turn already running in a window → the assistant says it is busy, waits up to 10 s, then gives up (research R12)

### Implementation

- [ ] T038 [US2] Extract the body of `send_message` into `chat::send::start_turn(ctx, StartTurn { thread_id, content, origin, emit })` in src-tauri/src/chat/send.rs; `send_message` in src-tauri/src/chat/commands.rs calls it; all chat turn tests stay green
- [ ] T039 [US2] Migration with the next free number: `ALTER TABLE chat_threads ADD COLUMN origin TEXT` ("NULL = getippt, 'voice' = Sprachgespräch") in a new src-tauri/src/identity/migrations_chat_origin.rs, registered in src-tauri/src/identity/migrations.rs; `HOLZI_TRIGGER_VERSION` 29 → 30; read/write `origin` in src-tauri/src/storage/chat_threads.rs; migration test in migrations_chat_origin_tests.rs
- [ ] T124 [US2] Same migration as T039: table `voice_transcript_entries` exactly as in data-model.md (device-local contiguous `seq`, ties allowed across devices, `speaker` "'user' | 'haex'", `kind` list, `outcome` "'approved'|'declined'|'open'|'cancelled'", no cascading foreign key), CRDT-tracked; explicitly delete transcript rows before a thread in the same CRDT write; storage functions `append`, `set_outcome`, `list_for_thread` in a new src-tauri/src/storage/voice_transcript.rs with tests in voice_transcript_tests.rs
- [ ] T125 [US2] Write every utterance and every output to the transcript (FR-047) from src-tauri/src/assistant/conversation.rs and turn.rs: `utterance`, `answer` (full text, `spoken_until` on barge-in or stop), `announcement`, `stop`, `closing`, `notUnderstood`; never pre-wake-word audio
- [ ] T126 [P] [US2] Integration test in src-tauri/tests/assistant_conversation.rs for SC-015: a scripted conversation with stop and closing yields transcript entries in order with timestamps and kinds; deleting the thread deletes them
- [ ] T127 [US2] Transcript view in the chat window for voice conversations (toggle "Mitschrift", time stamps, speaker, kind as small label, interrupted answers greyed after `spoken_until`) in a new src/components/chat/VoiceTranscript.vue; search over transcripts with `assistant_transcript_search {query, cursor?, limit?}` in a new src-tauri/src/storage/voice_transcript_search.rs using `passwords::ids::fold_for_search` (no FTS exists; 006 excludes it), enforcing the page limit and stable cursor ordering from contracts/tauri-commands.md, with tests in voice_transcript_search_tests.rs
- [ ] T040 [US2] Implement `assistant::conversation` (state machine, timers, `AssistantState` of data-model.md) in src-tauri/src/assistant/conversation.rs and src-tauri/src/assistant/mod.rs
- [ ] T041 [US2] Implement `assistant::turn` in src-tauri/src/assistant/turn.rs: start turns through `start_turn` with `origin = 'voice'` and an `emit` closure that forwards every event to the window and feeds `chat-token` deltas through `sentences` into the voice; model from `assistant.modelId` (null = default chat model); delegate notice once per conversation (spec Edge Cases)
- [ ] T154 [US2] Create src-tauri/src/assistant/lexicon.json (rows of research R22 for all six languages) and src-tauri/src/assistant/lexicon.rs (load once via `include_str!`, `hears(id, lang, text)`, `say(id, lang, args)`, `languages()`); switch `stt::interrupt::match_interrupt` in src-tauri/src/stt/interrupt.rs to the row `hear.stop` (wire values of `InterruptCommand` unchanged, all 008 interrupt tests stay green) and `SUPPORTED_LANGUAGES` in src-tauri/src/stt/local.rs to `lexicon::languages()`
- [ ] T042 [US2] Implement `assistant::phrases` (end phrases, request to repeat, busy, goodbye; all text from the word list, T154) in src-tauri/src/assistant/phrases.rs and stop words via `stt::interrupt` (finishes spec 008 US2 for the assistant; FR-017)
- [ ] T043 [US2] Use the shared warm STT adapter of `VoiceState` (Whisper or Parakeet, whichever is selected, T134) for utterances in src-tauri/src/assistant/listen.rs (no second model in memory)
- [ ] T044 [US2] Commands `assistant_get_state`, `assistant_talk`, `assistant_end_conversation`, `assistant_set_muted` and events `assistant-state`, `assistant-transcript` in src-tauri/src/assistant/commands.rs per contracts/tauri-commands.md
- [ ] T045 [US2] Device setting `assistant.preview` (default off) in KI → Sprache that enables talk shortcut and indicator until PR D, in src/components/settings/assistant/AssistantSetting.vue
- [ ] T046 [P] [US2] Store src/stores/assistant.ts: state from `assistant-state`, transcript from `assistant-transcript`, commands
- [ ] T047 [US2] Floating indicator src/components/assistant/AssistantBubble.vue mounted in src/components/wm/Desktop.vue above windows and dock, below dialogs; draggable, position saved as `assistant.bubble.position` ("JSON {x, y} (Anteil 0–1 der Fläche)"); one look per phase (Lucide icons until sprites, T118) plus text for screen readers and tooltip (FR-041)
- [ ] T048 [US2] Compact transcript src/components/assistant/CompactTranscript.vue: last turns live, open question highlighted, buttons "Sprechen", Stumm, Ein/Aus (hidden until PR D), "Gespräch zeigen", "Gespräch beenden"; Escape or click collapses (FR-042)
- [ ] T049 [US2] Actions in src/lib/actions/assistantActions.ts: `assistant.talk` (`defaultKeys` Ctrl+Shift+Space, macOS Cmd+Shift+Space), `assistant.mute.set`, `assistant.conversation.end`, `assistant.conversation.show` — all `guardrails`; handlers in a new src/stores/assistantActionHandlers.ts registered in src/plugins/actions.client.ts
- [ ] T050 [P] [US2] Rules in scripts/check-assistant-actions.ts: every `assistant.*` action has scope `guardrails` and `agentCallable: false`; `assistant.talk` has default keys and no clash with existing `defaultKeys`
- [ ] T051 [US2] Mark voice conversations in the chat history list (icon "Sprachgespräch") from `origin` in the chat sidebar components under src/components/chat/
- [ ] T052 [P] [US2] Tone and indicator for every turn step (FR-018): `listening`, `understood`, `notUnderstood`, `aborted`, `conversationEnd` from src-tauri/src/assistant/conversation.rs

**Checkpoint**: Conversations by shortcut work end to end behind the preview setting.

---

## Phase 5: User Story 5 - Antworten, die man hören kann (Priority: P2) (PR C)

**Goal**: Spoken answers are short and free of formatting; tables, links and code become a short
pointer; numbers, dates and times sound natural.

**Independent Test**: Scripted model answers with a list, a table, a link and a code block; the
spoken text has no markup and points to the transcript.

- [ ] T053 [P] [US5] Tests in src-tauri/src/assistant/speakable_tests.rs: headings, emphasis, list markers, inline code removed; tables, fenced code and bare URLs replaced by the pointer phrase; ISO date/time ("2026-10-29T10:00") spoken as "morgen um zehn Uhr" relative to an injected "now" (de) and "tomorrow at ten" (en); long answers (> 4 sentences) cut to a summary plus "Soll ich mehr vorlesen?"
- [ ] T054 [US5] Implement `assistant::speakable` in src-tauri/src/assistant/speakable.rs and apply it in `assistant::turn` before synthesis; the transcript keeps the full text
- [ ] T055 [US5] Voice instruction (FR-021) as a section appended in src-tauri/src/chat/tools/prompt.rs only when `origin = 'voice'` (in the conversation language), text from the word list row `prompt.voice` per contracts/agent-and-actions.md
- [ ] T056 [US5] Measure the instruction with the tool eval of spec 032 (release build, Qwen3-4B) and a set of 10 spoken-style requests added to the eval fixtures; record the score in research.md R10
- [ ] T057 [US5] "Mehr vorlesen": an affirmative follow-up after the summary reads the rest of the stored answer in src-tauri/src/assistant/turn.rs

**Checkpoint (PR C done)**: US2 and US5 work behind the preview setting.

---

## Phase 6: User Story 1 - Modus einschalten und per Wake Word fragen (Priority: P1) 🎯 MVP (PR D)

**Goal**: The launcher switch turns the mode on; "Haex" / "Hey Haex" starts a conversation while
haex runs (desktop also minimized); the tray shows the state.

**Independent Test**: Switch on in the launcher, minimize, say "Hey Haex, …"; tone within 0.3 s,
spoken answer, back to "bereit"; lock the vault stops everything; after unlock the mode is on again.

### Outside this repo (operator, before PR D)

- [ ] T058 [US1] Check the licence of openWakeWord's shared melspectrogram and embedding models; record the result in research.md R4 (if not free, the training retrains them)
- [ ] T059 [US1] Create the repo `haex-space/haex-wakeword` with the training pipeline (openWakeWord architecture, Python 3.12): positives "Haex" and "Hey Haex" synthesized with Pocket TTS and CC0/CC-BY Piper voices in German and English, the built-in reference voices of all six languages (T138) plus volunteer recordings; negatives from Common Voice (CC0) and own noise; data licences listed in its README
- [ ] T060 [US1] Train, evaluate against SC-003 (≥ 9 of 10 at ≤ 3 m) and SC-004 (≤ 1 false accept in 8 h at medium sensitivity) on a held-out set, and export ONNX
- [ ] T061 [US1] Publish the model on Hugging Face under the `haex-space` account with a pinned revision and licence; put repo, revision, size and licence into src-tauri/src/assistant/assets.json

### Tests first

- [ ] T062 [P] [US1] Wake word tests in src-tauri/src/assistant/wakeword/model_tests.rs with WAV fixtures in src-tauri/src/assistant/fixtures/ (self-made, CC0): "Haex" and "Hey Haex" trigger, unrelated speech does not; sensitivity `"low"|"medium"|"high"` maps to thresholds monotonically; detection runs only while VAD reports speech
- [ ] T063 [P] [US1] Integration test in src-tauri/tests/assistant_conversation.rs: mode on → wake word + question in one breath → the question includes the pre-roll audio after the keyword; own playback through the recording sink fed back as source does not trigger (FR-010)
- [ ] T064 [P] [US1] Integration test: vault close during speech stops output, listener and conversation (FR-004); readiness with a missing voice refuses to enable and names it (FR-005)
- [ ] T065 [P] [US1] Measurement test src-tauri/tests/assistant_wakeword_eval.rs (`#[ignore]`, release): recall on the positive test set and false accepts per hour on the negative set; CPU share in "bereit" (SC-010)

### Implementation

- [ ] T066 [US1] Implement `assistant::wakeword::model` in src-tauri/src/assistant/wakeword/{mod.rs,model.rs}: run the default model (via `ort` or sherpa per T007) on VAD-gated windows; thresholds from `assistant.sensitivity`
- [ ] T067 [US1] Self-trigger guard in src-tauri/src/assistant/wakeword/mod.rs: while haex plays audio and echo cancellation is not active (PR G), raise the threshold and ignore detections whose window overlaps own output by more than half
- [ ] T068 [US1] Commands `assistant_set_enabled` and `assistant_readiness` in src-tauri/src/assistant/commands.rs; device preferences `assistant.enabled` ("bool", default `false`) and `assistant.muted`; restore after unlock in the vault-open path next to `privacy::screen_capture::apply_for_vault` in src-tauri/src/instances/open.rs
- [ ] T069 [US1] Launcher tile "Sprachassistent" next to the lock tile in src/components/wm/Launcher.vue (state on/off, runs `assistant.mode.set`); readiness dialog with download offer when something is missing
- [ ] T070 [P] [US1] Dock control: add `"assistant"` to `DockControlId` in src/lib/wm/dock.ts and render it in src/components/wm/DockItem.vue; pinnable like the other controls
- [ ] T071 [US1] Action `assistant.mode.set` (`guardrails`) in src/lib/actions/assistantActions.ts and its handler
- [ ] T072 [US1] Desktop tray in src-tauri/src/assistant/tray.rs: enable Tauri feature `tray-icon` in src-tauri/Cargo.toml, add `libayatana-appindicator` to flake.nix and the Linux bundle; icon per phase, menu "Ein/Aus", "Stumm", "Gespräch zeigen"; shown only while the mode is on; removed on close
- [ ] T073 [US1] First-enable notice (FR-037): one-time dialog that anyone saying the wake word can talk and confirm, also secrets; `assistant.noticeShown`
- [ ] T074 [US1] Settings page src/components/settings/assistant/AssistantSetting.vue: mode, mute, sensitivity, pause length, follow-up window, continuation time, model for conversations; remove the preview setting of T045
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
- [ ] T081 [US3] Implement `assistant::approval` in src-tauri/src/assistant/approval.rs: react to `tool-permission-request` in the emit bridge, play `question`, announce, classify, resolve through `chat::approvals::resolve`
- [ ] T082 [US3] Open questions in src-tauri/src/assistant/conversation.rs: track request ids of this conversation, keep the follow-up timer off while open, ask the oldest first on the next trigger, phase `awaitingAnswer`
- [ ] T083 [US3] Progress notice and `working` tone after 4 s without spoken output (FR-023) in src-tauri/src/assistant/turn.rs, naming the running tool from `chat-tool-call` in plain words
- [ ] T084 [US3] Background reporting (FR-040) in src-tauri/src/assistant/conversation.rs: turn completion or a new question while no turn is being spoken → `attention` tone and speech (only visible when muted); summary of done/not done (US3 scenario 8)
- [ ] T085 [US3] Active end (FR-039): `assistant_end_conversation`, end phrase, mode off → `abort_turn` and a short spoken status in src-tauri/src/assistant/conversation.rs
- [ ] T086 [US3] Screen-needed steps (spec Edge Cases): when a tool needs the window, raise it (`raiseWindowAsync` pattern), say so and keep the step as open question kind `"screen"`
- [ ] T128 [US3] Transcript entries for approvals and questions (FR-047): `approvalQuestion`/`approvalAnswer` with `outcome` updated from `open` when answered, `choiceQuestion`/`choiceAnswer`, `progress`, `report`, in src-tauri/src/assistant/approval.rs, choice.rs and conversation.rs; cover in T078
- [ ] T087 [P] [US3] Show the open question highlighted in src/components/assistant/CompactTranscript.vue and the phase `awaitingAnswer` in the indicator and tray

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
- [ ] T097 [US6] Repeat within the same follow-up window from a `Zeroizing<String>` that is overwritten when the window ends (FR-026) in src-tauri/src/assistant/secret.rs
- [ ] T098 [US6] Set ADR-0020 to "accepted" in docs/adr/0020-spoken-secrets-bypass-the-model.md

**Checkpoint (PR F done)**: US6 works.

---

## Phase 10: User Story 2 - Dazwischenreden mit Echo-Unterdrückung (Priority: P1) (PR G)

**Goal**: Free barge-in over speakers where AEC passes SC-005; elsewhere only wake word and stop
words; any speech with headphones.

**Independent Test**: Long answer over speakers, interrupt with free speech → stops within 0.5 s;
50 answers without self-trigger; platforms that fail fall back and are documented.

- [ ] T099 [P] [US2] Echo tests in src-tauri/src/assistant/echo_tests.rs: with a recorded room impulse response applied to the reference, the cancelled signal stays below the barge-in threshold; the transcript self-filter drops text equal to what was just spoken
- [ ] T100 [US2] Add `sonora` pinned to a git SHA (fallback `webrtc-audio-processing` 2.x) behind the trait `EchoCanceller` in src-tauri/src/assistant/echo.rs, fed by the mixer reference (src-tauri/src/audio/mixer.rs)
- [ ] T101 [US2] Barge-in in src-tauri/src/assistant/conversation.rs: in `Speaking`, VAD speech ≥ 250 ms above the raised threshold stops output (`stop_speech`) and starts listening; `echoMode = "keywordOnly"` accepts only wake word or stop words (FR-016)
- [ ] T102 [US2] Headphone detection: Android route from the plugin (`audioRoute`), desktop from the device name, overridable by `assistant.headphones` ("\"auto\" | \"yes\" | \"no\"", default `"auto"`); headphones → full barge-in
- [ ] T103 [US2] Measurement test src-tauri/tests/assistant_echo_eval.rs (`#[ignore]`, release): 50 answers through speakers with free barge-in on; count self-triggers and stop latency; run on Linux, macOS, Windows and Android
- [ ] T104 [US2] Write docs/voice-assistant-platforms.md (echo mode per platform from T103, origin of the tones per FR-046) and set the per-platform default `echoMode`; show the active mode in AssistantSetting.vue

**Checkpoint (PR G done)**: Barge-in as clarified.

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
- [ ] T109 [US9] iOS: stop listening on `RunEvent` background, resume in the foreground, explain once (FR-035) in src-tauri/src/assistant/mod.rs and AssistantSetting.vue
- [ ] T110 [P] [US9] Android e2e scenario scripts/e2e/scenarios/android-voice-assistant.test.ts: enable, background the app, the notification exists with both actions, "Ausschalten" turns the mode off

**Checkpoint (PR H done)**: US9 on all platforms.

---

## Phase 12: User Story 7 - Eigenes Wake Word einlernen (Priority: P3) (PR I)

**Goal**: Users record their own wake word four times, haex checks and learns it locally, a probe
phase confirms, raw recordings are gone.

**Independent Test**: Enroll "Hallo Eichhörnchen"; probe shows detections; afterwards only it
triggers; no audio file in the data directory.

- [ ] T111 [US7] Spike (research R5): compare embedding templates against sherpa open-vocabulary KWS (keyword text from Whisper, threshold from the samples) on 10 German and 10 English words with 5 speakers; record recall and false accepts in research.md R5 and keep the better method as `method` in `CustomWakeWord`
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

- [ ] T137 Spike (research R21): load one model of each sherpa 2.x TTS engine (Pocket community model, Kokoro, Supertonic, Matcha, Kitten, VITS) without espeak-ng; record which speak correctly and their required files in research.md R21; the import accepts only those

### Tests first

- [ ] T141 [P] [US8] Migration tests in src-tauri/src/identity/migrations_tts_voices_tests.rs: tables exactly as data-model.md (`language` "'de'|'en'|'fr'|'es'|'it'|'pt'", `reference_hash` → `haex_tts_voices_binaries.hash`, `consent_confirmed_at INTEGER NOT NULL`); `haex_tts_voices` and the binaries table are CRDT-tracked, `haex_tts_models_no_sync` is not
- [ ] T142 [P] [US8] Clone tests in src-tauri/src/tts/clone_tests.rs with WAV fixtures: < 10 s speech → `tooShort`; speech beyond 30 s is cut to 30 s; low level → `tooQuiet`; samples near full scale → `clipped`; noisy pauses → `noisy`; save without `consent: true` is refused; save stores the binary by SHA-256 and `consent_confirmed_at`; delete removes the row, orphans the binary and resolves refs to the language default
- [ ] T143 [P] [US8] Import tests in src-tauri/src/tts/import_tests.rs with small fixture folders: each engine accepted by T137 is detected with languages and speakers; `.so`, `.dll`, `.dylib`, scripts → `forbiddenFile`; an ONNX file with a custom operator domain → `forbiddenFile`; > 2 GB → `tooLarge`; anything else → `unknownLayout`; import without `rightsConfirmed: true` is refused
- [ ] T144 [P] [US8] Rule in scripts/check-assistant-actions.ts: no action, agent tool or extension API calls `tts_clone_*` or `tts_model_import` (FR-050, FR-051)

### Implementation

- [ ] T145 [US8] Migration with the next free number in a new src-tauri/src/identity/migrations_tts_voices.rs (tables of data-model.md), registered in migrations.rs; `HOLZI_TRIGGER_VERSION` +1; storage in a new src-tauri/src/storage/tts_voices.rs
- [ ] T146 [US8] Implement src-tauri/src/tts/clone.rs and the commands `tts_clone_record_start`, `tts_clone_record_stop`, `tts_clone_from_file`, `tts_clone_preview`, `tts_clone_save`, `tts_voice_delete` and the event `tts-clone-level` per contracts/tauri-commands.md; recording through the duplex stream only while the dialog is open
- [ ] T147 [US8] Implement src-tauri/src/tts/import.rs and `tts_model_inspect`, `tts_model_import`: folder, `.tar.bz2` archive or Hugging Face repo (revision pinned at import, download through `models::download`); copy to `<AppLocalData>/models/user-tts-<id>/`, register in `device_downloaded_models_no_sync`, metadata in `haex_tts_models_no_sync`
- [ ] T148 [US8] Sync: emit `tts-voices-changed` when `haex_tts_voices` changes (vault-data-changed pattern); a selected but deleted voice falls back to the language default; choosing a synced voice whose language model is missing downloads it first
- [ ] T149 [US8] Dialogs src/components/settings/assistant/VoiceCloneDialog.vue (record with level meter or choose file, problems in plain words, sample, name, language, consent checkbox "Das ist meine Stimme oder die Person hat eingewilligt", save) and VoiceImportDialog.vue (source, inspection result or reason, name, languages, licence, speaker, rights checkbox); delete with confirmation in VoiceSetting.vue; texts in de.json and en.json
- [ ] T150 [P] [US8] e2e scenario scripts/e2e/scenarios/voice-clone-sync.test.ts on the multi-device rig (033): clone from a WAV fixture on device A, the voice appears on device B; delete on B, A falls back to the default

**Checkpoint (PR K done)**: Own voices and imported models work.

---

## Phase 14: Polish & Cross-Cutting Concerns

- [ ] T118 [P] Squirrel sprites per phase (ready, listening, thinking, working, speaking, awaiting answer, muted, paused) via the image-model workflow (one row of 4 frames, magenta key, aligned on the tail) in src/assets/assistant/; flip-book like src/components/files/Loader.vue in AssistantBubble.vue
- [ ] T119 [P] Accessibility: every phase and tone has a visible and screen-reader equivalent (FR-044); check with scripts/check-assistant-ui.ts
- [ ] T120 [P] Update plans/README.md row 066 after each merged PR
- [ ] T121 [P] Listening test with 5 people for SC-014 (core tones); adjust tones.rs if fewer than 4 of 5 are recognized
- [ ] T122 Battery measurement on the reference phone (1 h "bereit", SC-010) and CPU on the reference desktop; record in research.md
- [ ] T151 [P] Native speakers review the columns fr, es, it and pt of src-tauri/src/assistant/lexicon.json and the date and number rules in speakable.rs and judge the default voices (SC-017); record results in research.md R22
- [ ] T152 [P] Clone test with 5 people for SC-016 (time to a saved voice, recognized in the sample)
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
