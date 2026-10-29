---
description: 'Task list for spec 067-meeting-transcription'
---

# Tasks: Meeting-Mitschrift mit Sprecherzuordnung

**Input**: Design documents from `/specs/067-meeting-transcription/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/commands-and-actions.md](./contracts/commands-and-actions.md),
[quickstart.md](./quickstart.md); spec 066 PR B–D merged.

**Tests**: Included (constitution: one executable check per non-trivial logic). Unit tests in sibling
`*_tests.rs`; integration tests in `src-tauri/tests/`; measurements `#[ignore]` in release builds;
rules in `scripts/check-meetings-*.ts`; e2e with the `e2e-audio` feature of 066. Tests first.

**Organization**:

| Delivery | Phases     | Stories                                |
| -------- | ---------- | -------------------------------------- |
| PR A     | –          | documents                              |
| PR B     | Phases 1–3 | Setup + spike, Foundational, US1 (mic) |
| PR C     | Phases 4–5 | US2, US3                               |
| PR D     | Phase 6    | US4                                    |
| PR E     | Phase 7    | US1 (device audio)                     |
| PR F     | Phases 8–9 | US5, US6                               |
| PR G     | Phase 10   | US7                                    |
| –        | Phase 11   | Polish                                 |

**Conventions** (as in 066 tasks.md): files ≤ 500 lines; test code separate; no transcript, audio,
embedding or secret in logs, errors or `Debug`; Rust only via `nix develop --command
scripts/with-nix-host-bridge.sh cargo … -j 4`, one test target at a time; `pnpm generate:ts-types`
after Rust type changes; prettier on touched Markdown/JSON; `graphify query` before new named
artifacts; Conventional Commits without agent attribution.

---

## Phase 1: Setup and spike (PR B)

- [ ] T001 Create module `src-tauri/src/meetings/mod.rs` behind the `voice` feature with stub commands for `--no-default-features`, registered in src-tauri/src/lib.rs
- [ ] T002 [P] Create `scripts/check-meetings-actions.ts`, `scripts/check-meetings-ui.ts` and the script `check:meetings` in package.json; wire into the CI job of `check:agent-actions`
- [ ] T003 [P] Build test meetings in src-tauri/src/meetings/fixtures/: four speakers (German and English) composed from distinct local voices of 066 and volunteer recordings (CC0), with a JSON truth file (speaker per time span, reference text); document provenance in fixtures/README.md
- [ ] T004 Add the speaker embedding model (WeSpeaker ResNet34-LM, pinned by file name and SHA-256 digest) and pyannote segmentation-3.0 (ungated MIT mirror) to src-tauri/src/assistant/assets.json; licence and attribution fields filled
- [ ] T005 Spike STT: run `src-tauri/tests/stt_compare_eval.rs` from 066 (Whisper and Parakeet TDT 0.6B v3) on the meeting fixtures and a 4 h concatenation on the reference desktop and phone; record word error rate, real-time factor and backlog in research.md R2 and decide the meeting STT (Whisper stays selectable, 066 R7)
- [ ] T006 Spike diarization: live incremental clustering and end re-clustering on the fixtures; record the share of correctly attributed speech time (live and final) in research.md R3; if final < 85 %, record the gap and options for the operator before PR C
- [ ] T007 [P] Spike device audio: confirm cpal 0.18.2 loopback on Windows (output device as input), macOS 14.6 (process tap, permission denied → zeros) and Linux (PipeWire/Pulse monitor); record findings in research.md R5
- [ ] T008 [P] Spike sync: write 30 MB of 1-MiB BLOB chunks in one vault and sync to a second test vault through the existing sync test harness; record time and memory in research.md R7
- [ ] T009 [P] Spike search: fold+contains over 100 h of synthetic transcript rows; record time in research.md R8

**Checkpoint**: decisions R2, R3, R5, R7, R8 confirmed or changed in research.md.

---

## Phase 2: Foundational (PR B, blocking)

### Tests first

- [ ] T010 [P] Migration test src-tauri/src/identity/migrations_meetings_tests.rs: new columns on `voice_transcript_entries` (`channel` "'mic' | 'device'", `assignment` "'live' | 'final' | 'user'"), the five tables exactly as data-model.md, all CRDT-tracked, cascades from `chat_threads` reach every meeting row
- [ ] T011 [P] Participant/entry storage tests in src-tauri/src/meetings/{participants,entries}_tests.rs: create "Sprecher N" with increasing `number`, rename, link `contact_id` (missing contact reads as unlinked), `merged_into` resolution, split at char offset keeps time order

### Implementation

- [ ] T012 Migration with the next free number in src-tauri/src/identity/migrations_meetings.rs (columns + tables of data-model.md), registered in src-tauri/src/identity/migrations.rs, `HOLZI_TRIGGER_VERSION` +1
- [ ] T013 Storage in src-tauri/src/meetings/{participants.rs,entries.rs} and meeting rows in src-tauri/src/meetings/mod.rs; ids per the derived-id pattern of src-tauri/src/contacts/ids.rs
- [ ] T014 Speaker embedding wrapper over sherpa in src-tauri/src/meetings/cluster.rs (`embed(&CanonicalPcm) -> Embedding`), on a dedicated thread

---

## Phase 3: User Story 1 - Meeting aufzeichnen und live mitlesen (Priority: P1) 🎯 MVP (PR B)

**Goal**: Start with consent, live transcript from the microphone with "Sprecher N", pause, end,
meetings list.

**Independent Test**: Play a fixture into the `e2e-audio` source, record it as meeting; entries
appear live with separated speakers; end → listed with title, date, duration, participants.

- [ ] T015 [P] [US1] Clustering tests in src-tauri/src/meetings/cluster_tests.rs: utterances of the same fixture speaker join one cluster, a new voice opens "Sprecher N+1", more than 10 → "Unbekannt"
- [ ] T016 [P] [US1] Integration test src-tauri/tests/meetings_session.rs: start without `consent` is refused (FR-002); start → entries with time and speaker; pause → no entries, `pause` entry; vault close ends the meeting and keeps entries (FR-007); a second start while one runs is refused (FR-008)
- [ ] T017 [P] [US1] Rules in scripts/check-meetings-actions.ts: `meetings.record.start`/`.stop` in `guardrails`, not agent-callable
- [ ] T018 [US1] Implement `meetings::session` in src-tauri/src/meetings/session.rs: reads the 066 audio stream (mic channel), utterances via 066 `assistant::listen`, STT per R2 in a queue that never drops, `cluster` per utterance, entries `kind = 'speech'`, `assignment = 'live'`; backlog > 30 s emits `meeting-warning {kind:"backlog"}`
- [ ] T019 [US1] Commands `meeting_start`, `meeting_pause`, `meeting_resume`, `meeting_end`, `meeting_get`, `meeting_list`, `meeting_transcript`, `meeting_rename` and events `meeting-state`, `meeting-entry` in src-tauri/src/meetings/commands.rs per the contract
- [ ] T020 [US1] Optional start announcement (FR-003) through the 066 local voice in session.rs
- [ ] T021 [US1] Wire shutdown in src-tauri/src/instances/close.rs next to the assistant (FR-007)
- [ ] T022 [US1] App `system.meetings`: entry in src/lib/wm/apps.ts, icon in public/app-icons/, routes registry src/lib/meetings/registry.ts, routes in src/components/wm/appRoutes.ts, frame src/components/apps/MeetingsApp.vue, store src/stores/meetings.ts
- [ ] T023 [US1] Views src/components/meetings/{List.vue,Detail.vue,Transcript.vue,Toolbar.vue}: list with title, date, duration, participants, audio mark (FR-022); live transcript following new entries
- [ ] T024 [US1] Start dialog src/components/meetings/StartDialog.vue: consent checkbox (required), announcement, keep audio (with storage warning, FR-016), answers speak/show, source (desktop only, from PR E); legal hint text (spec Assumptions)
- [ ] T025 [US1] Actions `meetings.record.start`/`.stop` (`guardrails`) and `meetings.list` in src/lib/actions/meetingsActions.ts with handlers in src/stores/meetingsActionHandlers.ts; launcher tile "Meeting aufzeichnen" in src/components/wm/Launcher.vue and dock control `"meeting"` in src/lib/wm/dock.ts
- [ ] T026 [US1] Indicator and tray state "Meeting" with duration, pause and end in the 066 components src/components/assistant/AssistantBubble.vue and src-tauri/src/assistant/tray.rs (FR-005); Android notification text via the 066 plugin method
- [ ] T027 [P] [US1] iOS: hint at start that recording stops in the background (FR-009)
- [ ] T028 [P] [US1] Texts de/en under `meetings.*` in src/i18n/locales/{de,en}.json; rules in scripts/check-meetings-ui.ts
- [ ] T029 [US1] e2e scenario scripts/e2e/scenarios/meeting-record.test.ts: launcher → start with consent → WAV script → entries appear → end → list
- [ ] T030 [US1] Measurement src-tauri/tests/meetings_stt_eval.rs (`#[ignore]`, release): latency (SC-001), 4 h without backlog (SC-002), word error rate (SC-005)

**Checkpoint (PR B)**: usable mic-only meeting transcription.

---

## Phase 4: User Story 2 - Sprecher benennen und korrigieren (Priority: P1) (PR C)

**Goal**: Rename, link contacts, reassign, split, merge, edit; final re-assignment respects user
choices.

**Independent Test**: In a fixture meeting rename and link a speaker, reassign one entry, merge two,
split one, edit text; end; user choices survive the final pass; data persists after restart.

- [ ] T031 [P] [US2] Re-assignment tests in src-tauri/src/meetings/reassign_tests.rs: final clusters map to live labels by majority overlap; entries with `assignment = 'user'` never change; changed entries are reported; `embedding` columns are cleared afterwards
- [ ] T032 [P] [US2] Edit tests in src-tauri/src/meetings/entries_tests.rs: split at offset, merge participants (`merged_into`), edit keeps `original_text`, reassign sets `assignment = 'user'`
- [ ] T033 [US2] Implement `meetings::reassign` in src-tauri/src/meetings/reassign.rs (state `assigning` → `ended`, event `meeting-assignment-done`); on vault reopen offer "Zuordnung verbessern" for meetings ended without the pass
- [ ] T034 [US2] Commands `meeting_participant_update`, `meeting_participants_merge`, `meeting_entry_assign`, `meeting_entry_split`, `meeting_entry_edit` in src-tauri/src/meetings/commands.rs
- [ ] T035 [P] [US2] Reusable src/components/contacts/ContactPicker.vue (pattern of src/components/passwords/ReferencePicker.vue, using `useContactsStore` and `filterContacts`)
- [ ] T036 [US2] Participants panel src/components/meetings/Participants.vue and entry actions in Transcript.vue: rename/link/mark as "Ich" in at most two steps (SC-006), reassign in at most two steps, split, merge, edit with "bearbeitet" and original on demand
- [ ] T037 [US2] One-time hint in Detail.vue when `meeting-assignment-done` reports changes (US1 scenario 9)

---

## Phase 5: User Story 3 - Eigene Stimme einrichten (Priority: P2) (PR C)

**Goal**: The user's own voice profile; "Ich" recognized in meetings.

**Independent Test**: Enroll, record a fixture with the user's voice, entries under "Ich"; delete the
profile, next meeting shows "Sprecher N".

- [ ] T038 [P] [US3] Profile tests in src-tauri/src/meetings/profile_tests.rs: six sentences → one normalized embedding; raw buffers zeroed; threshold calibrated; at most one row (derived id); delete removes it
- [ ] T039 [US3] Implement `meetings::profile` and commands `voice_profile_enroll_start/record/finish/cancel`, `voice_profile_delete` in src-tauri/src/meetings/profile.rs; cluster.rs marks matching utterances as the `is_me` participant
- [ ] T040 [US3] Settings page src/components/settings/meetings/VoiceProfileSetting.vue under KI → Sprache (guided six sentences, progress, delete); hint at meeting start when no profile exists (US3 scenario 4)
- [ ] T041 [US3] Measurement src-tauri/tests/meetings_diarization_eval.rs (`#[ignore]`, release): correct speech time live/final (SC-003), "Ich" precision/recall (SC-004)

**Checkpoint (PR C)**: speakers named and corrected; "Ich" recognized.

---

## Phase 6: User Story 4 - Audio behalten und nachhören (Priority: P2) (PR D)

**Goal**: Optional encrypted audio per meeting, playback per entry, delete, recompute.

**Independent Test**: Meeting with "Audio behalten"; an entry plays its span; size ≤ 30 MB/h;
"Audio löschen" keeps the transcript; no audio without the option.

- [ ] T042 [P] [US4] Encoder tests in src-tauri/src/meetings/audio/encode_tests.rs: Ogg Opus mono 16 kbit/s VOIP, 20 ms frames, one stream per channel; 1 h of fixture ≤ 7.5 MB per channel
- [ ] T043 [P] [US4] Chunk tests in src-tauri/src/meetings/audio/chunks_tests.rs: chunks ≤ 1 MiB with `start_ms`; seeking by time finds the right chunk; delete removes only this meeting's chunks
- [ ] T044 [P] [US4] Source tests in src-tauri/src/meetings/audio/source_tests.rs: `ChunkedAudioSource` serves byte ranges across chunk borders (pattern of src-tauri/src/files/streaming.rs)
- [ ] T045 [US4] Add `opus` and `ogg` to src-tauri/Cargo.toml behind `voice`; libopus in flake.nix and the Android build
- [ ] T046 [US4] Implement src-tauri/src/meetings/audio/{encode.rs,chunks.rs,source.rs}; write chunks from session.rs only when `keep_audio`; storage check before start and stop on full disk (FR-016)
- [ ] T047 [US4] Commands `meeting_audio_open` (register with src-tauri/src/files/media/server.rs, release with the tab) and `meeting_audio_delete`
- [ ] T048 [US4] Player src/components/meetings/Player.vue (based on src/components/files/MediaViewer.vue): play from an entry, seek, current entry highlighted
- [ ] T049 [US4] `meeting_reassign` with audio: final pass reruns with overlap detection (pyannote segmentation) and keeps user choices (US4 scenario 6)
- [ ] T050 [US4] Integration test src-tauri/tests/meetings_sync.rs: a meeting with 30 MB audio syncs to a second test vault; deleting audio on one removes it on the other

---

## Phase 7: User Story 1 - Online-Meetings mit Geräteton (Priority: P1) (PR E)

**Goal**: On desktop, record device audio next to the mic; no duplicates over speakers; mic speech of
a headphone user proposed as "Ich".

**Independent Test**: WAV played as device audio plus mic fixture; remote speakers in the transcript
with headphones; over speakers no sentence twice (SC-010).

- [ ] T051 [P] [US1] Dedup tests in src-tauri/src/meetings/session_tests.rs: device-audio reference feeds the 066 `EchoCanceller`; zero-filled gaps; late reference frames dropped; text+time safety net drops a mic utterance matching a device utterance
- [ ] T052 [US1] Implement src-tauri/src/meetings/device_audio.rs: cpal loopback per OS (Windows output-device input with silence fill and reopen on device change; macOS ≥ 14.6 process tap with zero-detection → `meeting-warning {kind:"deviceAudioDenied"}`; Linux monitor or `pipewire` fallback per T007); `NSAudioCaptureUsageDescription` in src-tauri/Info.plist
- [ ] T053 [US1] Two channels in session.rs: separate clustering per channel, participants with `channel`, mic speech proposed as "Ich" without profile when headphones are in use (FR-029)
- [ ] T054 [US1] Source choice in StartDialog.vue (desktop only; default "Mikrofon und Geräteton" when another program plays audio, FR-028); Android/iOS show only "Mikrofon" with hint
- [ ] T055 [US1] e2e scenario scripts/e2e/scenarios/meeting-online.test.ts on Linux CI with a PipeWire null sink playing the WAV

---

## Phase 8: User Story 5 - Den Assistenten während des Meetings fragen (Priority: P2) (PR F)

**Goal**: Wake word or "Fragen" during a meeting starts a 066 conversation with the transcript as
context; recording continues.

- [ ] T056 [P] [US5] Tool tests in src-tauri/src/meetings/transcript_tool_tests.rs: `meetings.transcript.read` filters by time, speaker and query, `limit` "≤ 200", offered only in meeting contexts (FR-021), read-only
- [ ] T057 [US5] Implement the native tool in src-tauri/src/meetings/transcript_tool.rs and register it per ADR-0011
- [ ] T058 [US5] In src-tauri/src/assistant/turn.rs (066): when a meeting runs, add the last 10 minutes of the transcript to the system text, offer the tool, and write question and answer as `haex`/user entries of the meeting; answers spoken or shown per `answers` (FR-018)
- [ ] T059 [US5] "Fragen" button in the indicator during a meeting (works without assistant mode, US5 scenario 5)
- [ ] T060 [US5] Integration test in src-tauri/tests/meetings_session.rs: a question during recording gets an answer from the transcript and recording has no gap

---

## Phase 9: User Story 6 - Auswertung nach dem Meeting (Priority: P2) (PR F)

**Goal**: Summary, decisions, tasks with references; meeting chat; hand tasks to the agent.

- [ ] T061 [P] [US6] Evaluation tests in src-tauri/src/meetings/evaluation_tests.rs: schema `Evaluation` of data-model.md validated; unknown `refs` dropped; new version keeps the old; long transcripts split into sections and combined
- [ ] T062 [US6] Implement `meetings::evaluation` and `meeting_evaluate` in src-tauri/src/meetings/evaluation.rs: as a 068 task when available, otherwise a built-in agent turn with the default model
- [ ] T063 [US6] Evaluation view src/components/meetings/Evaluation.vue: summary, decisions, tasks with owner and due date, jump to referenced entries, earlier versions, "Übernehmen" sends the task as an instruction to the chat of the meeting (FR-020)
- [ ] T064 [US6] Meeting chat as a tab in Detail.vue using the chat of `thread_id` with `meetings.transcript.read` offered
- [ ] T065 [US6] Measure evaluation on a fixture transcript with known decisions and tasks (SC-008) and record in research.md R9

---

## Phase 10: User Story 7 - Meetings wiederfinden und exportieren (Priority: P3) (PR G)

- [ ] T066 [P] [US7] Search tests in src-tauri/src/meetings/search_tests.rs with the shared fold: hits in title, participants, transcript, evaluation with snippet and entry id; paging
- [ ] T067 [P] [US7] Export tests in src-tauri/src/meetings/export_tests.rs: Markdown with title, date, participants, transcript (time, speaker, text), evaluation
- [ ] T068 [US7] Implement `meeting_search`, `meeting_export` (via `files::picked::PickedFile`), `meeting_delete`, `meetings_of_contact`; actions `meetings.search`, `meetings.ofContact`
- [ ] T069 [US7] Search in src/components/meetings/Toolbar.vue (`?q=`) with jump to entry; export via `usePickedFile.pickSaveTargetAsync`; delete with confirmation
- [ ] T070 [US7] Contact section src/components/contacts/MeetingsSection.vue in src/components/contacts/EntryPage.vue (pattern of FileSharesSection.vue) (FR-026)

---

## Phase 11: Polish

- [ ] T071 [P] Hint every 30 minutes after 4 h of recording (spec Edge Cases)
- [ ] T072 [P] Update plans/README.md with a row for 067 after each merged PR
- [ ] T073 Battery measurement on the reference phone (SC-007) and record in research.md
- [ ] T074 Manual quickstart M1–M11; record results in the last PR description

---

## Dependencies & Execution Order

- 066 PR B–D before PR B; 066 PR G before PR E; 066 PR H for Android in the background.
- Phase 1 → Phase 2 → US1 (PR B). PR C, D, E, F, G depend only on PR B and can run in parallel
  worktrees; T049 (reassign with audio) needs PR C.
- Within a story: tests first, Rust before `pnpm generate:ts-types`, then frontend.

### Parallel Opportunities

- Spikes T005–T009 in parallel after T004.
- PR C–G in parallel after PR B.

## Implementation Strategy

1. PR B: mic-only transcription with live speakers (MVP), after the spikes confirm STT and
   diarization quality.
2. PR C: names and corrections make it usable for real meetings.
3. PR E: online meetings; PR D: audio; PR F: assistant and evaluation; PR G: search and export.
