---
description: 'Task list for Composer Toolbar Parity'
---

# Tasks: Composer Toolbar Parity (Real Effort, Sub-Agent Activity, Attachments)

**Input**: Design documents from `specs/011-composer-toolbar-parity/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/tauri-commands.md

**Tests**: Included as mandatory, not optional — the spaex constitution's
"Test code must be maintained in files separate from production code" MUST
clause applies to every new module here, and this repo's own convention
(confirmed repo-wide: no inline `#[cfg(test)] mod tests`) is a sibling
`*_tests.rs` file per module.

**Organization**: Grouped by user story (US1 = effort, US2 = sub-agent
activity, US3 = attachments), matching spec.md's priorities. No Setup or
Foundational phase: plan.md's Constitution Check found no new dependency or
schema work, and the three stories touch largely disjoint new files (only
`commands.rs`, `claude.rs`, and `[instance].vue` are shared, in different
regions each) — each story is independently implementable straight from
plan.md/data-model.md.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on other
  unchecked tasks)
- **[Story]**: US1 / US2 / US3

---

## Phase 1: User Story 1 - The effort control does what it says (P1) 🎯 MVP

**Goal**: Changing the effort level in the composer actually changes how
much a directly-connected model or the Claude Code delegate reasons; models/
backends with no such control don't show one.

**Independent Test**: quickstart.md §1.

### Tests for User Story 1

- [x] T001 [P] [US1] Write failing tests for `EffortLevel::{as_str,parse}`,
      `anthropic_supported_levels`, `claude_delegate_levels`, and `clamp` in
      new `src-tauri/src/adapters/effort_tests.rs` — cover: every level
      round-trips through `as_str`/`parse`; `claude-sonnet-5` includes
      `xhigh`; `claude-opus-4-6` includes `max` but not `xhigh`; an
      unrecognized model id returns `&[]`; `clamp(XHigh, &[Low,Medium,High])`
      returns `Some(High)`; `clamp(_, &[])` returns `None`.
- [x] T002 [P] [US1] Write failing tests in `src-tauri/src/adapters/request_tests.rs`
      asserting `build_messages_body` includes `output_config: {"effort": "xhigh"}`
      when `req.effort_level = Some(XHigh)` and the model supports it, omits
      `output_config` entirely when `effort_level` is `None`, and clamps
      down (never sends an unsupported value) when the model doesn't
      support the requested level.
- [x] T003 [P] [US1] Write failing tests in `src-tauri/src/adapters/cli_delegate/claude_tests.rs`
      asserting `build_command` appends `--effort <level>` when a level is
      passed and omits the flag entirely otherwise.
- [x] T004 [P] [US1] Write failing tests in `src-tauri/src/chat/commands_tests.rs`
      for the new effort-levels resolution: a local composite id returns
      `[]`; an `api_key`/`anthropic` provider id returns that model's table
      entry; a `cli_delegate`/`claude` provider id always returns the full
      five-value set; a `cli_delegate`/`codex` provider id returns `[]`.

### Implementation for User Story 1

- [x] T005 [US1] Implement `EffortLevel`, `anthropic_supported_levels`,
      `claude_delegate_levels`, and `clamp` in new
      `src-tauri/src/adapters/effort.rs` (data-model.md); register
      `pub mod effort;`/`mod effort_tests;` in `src-tauri/src/adapters/mod.rs`.
      Makes T001 pass.
- [x] T006 [US1] Add `pub effort_level: Option<EffortLevel>` to `ChatRequest`
      in `src-tauri/src/adapters/types.rs` (next to `reasoning_requested`).
- [x] T007 [US1] Implement the `output_config.effort` branch in
      `build_messages_body` (`src-tauri/src/adapters/request.rs`), using
      `effort::anthropic_supported_levels` + `clamp`. Makes T002 pass.
- [x] T008 [US1] Add an `effort_level: Option<EffortLevel>` parameter to
      `build_command` and its caller `spawn_claude_invocation`
      (`src-tauri/src/adapters/cli_delegate/claude.rs`); append `--effort
<level>` when set. Makes T003 pass.
- [x] T009 [US1] Add `effort_level: Option<EffortLevel>` to `SendMessageArgs`
      (`src-tauri/src/chat/commands.rs`) and thread it into the `ChatRequest`
      built in `send_message`.
- [x] T010 [US1] Implement the `get_effort_levels(model_id: String) ->
Result<Vec<String>>` Tauri command (`src-tauri/src/chat/commands.rs`,
      alongside `send_message`), resolving the provider row via
      `storage::providers::get_provider` for a composite id and returning
      `[]` for a bare (local) id — contracts/tauri-commands.md. Makes T004
      pass. Register the command in `src-tauri/src/lib.rs`'s
      `generate_handler!`.
- [x] T011 [US1] Frontend: add `getEffortLevelsAsync(modelId): Promise<string[]>`
      and the `effortLevel`/`attachments` fields' effort half to
      `SendMessageArgs` in `src/composables/useChat.ts`.
- [x] T012 [US1] Frontend: in `src/components/chat/ComposerSettingsPopover.vue`,
      replace the fixed `ShadcnSlider` effort section with a `ShadcnSelect`
      bound to a new `effortLevels: string[]` prop (frontend prepends
      `"auto"`), `v-if="effortLevels.length"` around the whole effort
      block, remove the now-unused `effortLevels` local constant/slider
      markup and `effortIndex`/`updateEffortFromSlider`.
- [x] T013 [US1] Frontend: in `src/pages/chat/[instance].vue`, change
      `effortLevel` to `ref<EffortLevel | null>(null)`, remove
      `effortTokens`/the `maxNewTokens: effortTokens[effortLevel.value]`
      mapping in `send()` (send `maxNewTokens: undefined` — FR-006, this
      feature does not add a dedicated response-length control), add an
      `effortLevels` ref refreshed via `getEffortLevelsAsync` on every
      `activeModel`/`isDelegateModel` change, reset `effortLevel` to `null`
      when the new list doesn't contain the current value, and pass
      `effortLevel`/`effortLevels` to `ChatComposerSettingsPopover`.
- [x] T014 [US1] i18n: update `chat.effort.*` in `src/i18n/locales/en.json`
      and `de.json` — add `auto`, `xhigh`, `max`; keep `low`/`medium`/`high`.
- [x] T015 [US1] Run `cargo test --lib` and `pnpm typecheck`; fix fallout.

**Checkpoint**: User Story 1 fully functional and independently testable
(quickstart.md §1) without US2/US3.

---

## Phase 2: User Story 2 - See sub-agent activity while a delegate is working (P2)

**Goal**: A live "N agents" indicator appears in the toolbar while the
Claude Code delegate has sub-agents running, grouped by dispatch batch, and
disappears once they finish or the response is stopped.

**Independent Test**: quickstart.md §2.

### Tests for User Story 2

- [x] T016 [P] [US2] Write failing tests for the `Tracker` state machine in
      new `src-tauri/src/adapters/cli_delegate/subagents_tests.rs`
      (data-model.md): a single top-level dispatch promoted by a matching
      parent reference reports `active_count: 1, batch_size: Some(1)`; two
      ids from the same `observe_top_level_tool_use` call both promoted
      report `batch_size: Some(2)` on first promotion and no `batch_size`
      on the second; `observe_tool_result` for an active id decrements
      `active_count`; an unrelated `parent_tool_use_id`/`tool_use_id` is a
      no-op.
- [x] T017 [P] [US2] Write failing tests in
      `src-tauri/src/adapters/cli_delegate/claude_tests.rs` for `parse_line`
      given synthetic `assistant`/`user` stream-json lines carrying
      `parent_tool_use_id` and `tool_use`/`tool_result` blocks (research.md
      §2 shapes), asserting the right `LineOutcome::Chunk(StreamChunk::AgentActivity{..})`
      sequence, and that ordinary `stream_event`/`result` line handling is
      unchanged.

### Implementation for User Story 2

- [x] T018 [US2] Implement `subagents::Tracker` per data-model.md in new
      `src-tauri/src/adapters/cli_delegate/subagents.rs`; register
      `mod subagents;`/`mod subagents_tests;` in
      `src-tauri/src/adapters/cli_delegate/mod.rs`. Makes T016 pass.
- [x] T019 [US2] Add `StreamChunk::AgentActivity { active_count: usize,
batch_size: Option<usize> }` to `src-tauri/src/adapters/types.rs`.
- [x] T020 [US2] Extend `parse_line`'s signature with a `&mut
subagents::Tracker` parameter and add the `assistant`/`user` line
      branches (`src-tauri/src/adapters/cli_delegate/claude.rs`); update its
      one call site in `spawn_claude_invocation` to own a `Tracker` across
      the read loop. Makes T017 pass.
- [x] T021 [US2] Add `EVENT_CHAT_AGENT_ACTIVITY` and `AgentActivityEvent
{ message_id, active_count, batch_size }` to
      `src-tauri/src/chat/events.rs` (contracts/tauri-commands.md).
- [x] T022 [US2] Add the `StreamChunk::AgentActivity` match arm to
      `consume_stream` in `src-tauri/src/chat/turn/step.rs`, emitting
      `EVENT_CHAT_AGENT_ACTIVITY`.
- [x] T023 [P] [US2] Frontend: add `AgentActivityEvent` and `onAgentActivity`
      to `src/composables/useChat.ts` (mirrors `onToken`'s shape).
- [x] T024 [US2] Frontend: new `src/components/chat/AgentActivityIndicator.vue`
      (`props: { count: number }`, `v-if="count > 0"`, green-dot + "N
      agent(s)" pill per the reference screenshot).
- [x] T025 [US2] Frontend: in `src/pages/chat/[instance].vue`, subscribe to
      `onAgentActivity`, add `activeAgentCount`/`lastAgentBatchSize` refs,
      reset them wherever `streamingMessageId`/`busy` are already reset
      (turn-complete, message-error, abort), render
      `AgentActivityIndicator` in the toolbar row before
      `ChatComposerSettingsPopover`.
- [x] T026 [US2] i18n: add `chat.agentActivity.count` (with singular/plural
      handling) to `en.json`/`de.json`.
- [x] T027 [US2] Run `cargo test --lib` and `pnpm typecheck`; fix fallout.

**Checkpoint**: User Stories 1 and 2 both independently functional
(quickstart.md §1–§2).

---

## Phase 3: User Story 3 - Attach documents to a message (P3)

**Goal**: A "+" control lets the user attach files to a message; usable
attachments reach the model, unusable ones are flagged before send.

**Independent Test**: quickstart.md §3.

### Tests for User Story 3

- [x] T028 [P] [US3] Write failing tests in new
      `src-tauri/src/chat/attachments_tests.rs` for `classify_attachment`
      (kind detection by extension, size-cap rejection per kind per
      research.md §4), `usability_for` (the four-backend matrix from
      data-model.md), and `read_attachment_content` (missing-file error).

### Implementation for User Story 3

- [x] T029 [US3] Implement `AttachmentKind`, `AttachmentInfo`, `Attachment`,
      `classify_attachment`, `usability_for`, `read_attachment_content` in
      new `src-tauri/src/chat/attachments.rs` per data-model.md; register
      `mod attachments;`/`mod attachments_tests;` in
      `src-tauri/src/chat/mod.rs`. Makes T028 pass.
- [x] T030 [US3] Add `pub attachments: Vec<Attachment>` (default empty) to
      `ChatMessage` in `src-tauri/src/adapters/types.rs`; confirm
      `history_to_messages` (`commands.rs`) compiles unchanged (default
      empty for every historical row).
- [x] T031 [US3] Extend `build_messages` in
      `src-tauri/src/adapters/request.rs` to fold the current user
      message's `attachments` into Anthropic `image`/`document` content
      blocks (research.md §3) alongside its existing text.
- [x] T032 [US3] Extend `spawn_claude_invocation`/`build_transcript_prompt`
      in `src-tauri/src/adapters/cli_delegate/claude.rs` to write each
      attachment's bytes into the invocation's existing `TempDir` and
      mention its in-sandbox path in the transcript prompt (research.md
      §3).
- [x] T033 [US3] Add `attachments: Vec<AttachmentInput { path: String }>` to
      `SendMessageArgs` (`src-tauri/src/chat/commands.rs`); in
      `send_message`, build the current turn's `ChatMessage.attachments` via
      `read_attachment_content`, dropping and reporting (not failing the
      whole send on) any that fail re-validation (FR-018).
- [x] T034 [US3] Implement the `inspect_attachment(path: String) ->
Result<AttachmentInfo>` Tauri command
      (`src-tauri/src/chat/commands.rs`), using `classify_attachment` +
      `usability_for` against the currently active model/provider (same
      resolution as T010's `get_effort_levels`); register in
      `src-tauri/src/lib.rs`.
- [x] T035 [P] [US3] Frontend: add `AttachmentInfo`/`AttachmentInput` types,
      `inspectAttachmentAsync`, and the `attachments` field on
      `SendMessageArgs` to `src/composables/useChat.ts`.
- [x] T036 [US3] Frontend: new `src/components/chat/ComposerAttachments.vue`
      — a "+" trigger using `@tauri-apps/plugin-dialog`'s `open()` (multi-
      select file picker) and `@tauri-apps/plugin-fs` as needed, a chip per
      staged attachment (name, remove control, usability/error state from
      `inspectAttachmentAsync`).
- [x] T037 [US3] Frontend: in `src/pages/chat/[instance].vue`, add an
      `attachments` ref, render `ComposerAttachments` in the toolbar (before
      `ChatComposerSettingsPopover`, matching the reference's leading "+"),
      include the staged paths in `sendMessageAsync`'s call, clear
      `attachments` alongside `input.value = ''` on send, and re-run
      `inspectAttachmentAsync` for every staged attachment whenever
      `activeModel` changes.
- [x] T038 [US3] i18n: add `chat.composer.attachments.*` (attach button
      label/aria, remove, and the oversize/unsupported-type/unusable-for-
      model/missing-file reason strings) to `en.json`/`de.json`.
- [x] T039 [US3] Run `cargo test --lib`, `pnpm typecheck`; fix fallout.

**Checkpoint**: All three user stories independently functional.

### Paste from the clipboard (US3 scenario 6, FR-019)

- [x] T043 [US3] Backend: `src-tauri/src/chat/pasted_attachments.rs` (tests in
      `pasted_attachments_tests.rs`) and the command `stage_pasted_attachment` (raw bytes + file name
      header): writes the bytes to a fresh folder under the app cache folder, refuses an empty or
      too large paste, removes staged folders older than a day, returns the path.
- [x] T044 [US3] Frontend: `ComposerShell.vue` emits `pasteFiles` for a paste that carries files
      (and a `file:` list), `useComposerAttachments` stages them and adds them like picked files;
      `Composer.vue` and `ChatApp.vue` pass it on.

---

## Phase 4: Polish

- [x] T040 [P] Diff `en.json`/`de.json` key trees to confirm they stayed in
      sync across T014/T026/T038.
- [x] T041 Run the full quickstart.md walkthrough (§1–§4) end-to-end.
- [x] T042 Final full-suite pass: `cargo test --lib`, `cargo test --test
cli_delegate_claude`, `pnpm typecheck`.

---

## Dependencies & Execution Order

- **US1, US2, US3** are mutually independent at the phase level — each can
  be implemented and validated on its own per plan.md's "largely disjoint
  new files" structure decision. Priority order (P1 → P2 → P3) is
  recommended, matching spec.md, but a contributor could pick up US2 or US3
  first without blocking on US1.
- Within each story: tests (marked [P] where they touch different files)
  before the implementation task(s) that make them pass; a pure-logic task
  (e.g. T005, T018, T029) before the tasks that wire it into an adapter
  (T007/T008, T020, T031/T032); backend wiring before the frontend tasks
  that call the new command/event; i18n and the final test-run task last in
  each story.
- Phase 4 depends on all three stories being complete.

## Parallel Example: User Story 1

```bash
# Tests, all different files:
Task: "T001 effort_tests.rs"
Task: "T002 request_tests.rs"
Task: "T003 claude_tests.rs"
Task: "T004 commands_tests.rs"
```
