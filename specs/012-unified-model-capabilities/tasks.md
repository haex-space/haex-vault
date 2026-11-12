---
description: 'Task list for Unified, Cached Model Capabilities'
---

# Tasks: Unified, Cached Model Capabilities

**Input**: Design documents from `specs/012-unified-model-capabilities/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/tauri-commands.md,
contracts/capabilities-json.md, quickstart.md

**Tests**: Included as mandatory, not optional — the spaex constitution requires test code in files
separate from production code and one runnable check per piece of non-trivial logic. Rust tests live
in sibling `*_tests.rs` files declared with the repository's existing
`#[cfg(test)] #[path = "x_tests.rs"] mod x_tests;` idiom (see `src-tauri/src/voice.rs`) or under
`src-tauri/tests/`. Frontend tests are cases in the established replay harness
`scripts/check-chat-state.ts` (`pnpm check:chat-state`, a CI gate) — no new test runner.

**Organization**: Grouped by user story (US1 = composer matches the model, US2 = per-model effort
memory, US3 = honest about the unknown), preceded by a Foundational phase holding the shared record,
storage, local-model population and the behavior-preserving store split that every story needs.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on another unchecked task)
- **[Story]**: US1 / US2 / US3 (only in story phases)
- All paths are repository-relative. Rust paths are under `src-tauri/`.
- Commits follow Conventional Commits, one per checkpoint, **without** any agent/model trailer
  (constitution MUST NOT). Work lands on `main` through a PR from this branch; no squash-merge.

---

## Phase 1: Setup

- [x] T001 Confirm the work happens in `.worktrees/012-unified-model-capabilities` on branch
      `012-unified-model-capabilities` (`git worktree list`, `git branch --show-current`); run a real
      `pnpm install` there — never symlink `node_modules` from another checkout. Run all dev/build
      commands inside the Nix dev shell (`IN_NIX_SHELL` set).
- [x] T002 Record the baseline before any edit: `cargo test --manifest-path src-tauri/Cargo.toml`
      pass count, `pnpm check:chat-state` test count, `pnpm typecheck`, `pnpm lint`,
      `pnpm format:check` all green. Keep the numbers for T017 (the store split must not change the
      harness count).

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: the capability record, its storage, local-model population, and the frontend store
split. No user-visible behavior changes yet.

### Backend: record and storage

- [x] T003 [P] In `src-tauri/src/adapters/types.rs` extend the existing `AttachmentKind` (variants
      `Image | Document | Text`) with `Eq, Serialize, Deserialize` and
      `#[serde(rename_all = "snake_case")]` — do not add a second enum (graphify-first: extend the
      existing candidate).
- [x] T004 Create `src-tauri/src/model_capabilities.rs` (top-level leaf module) and register
      `pub mod model_capabilities;` in `src-tauri/src/lib.rs`. Implement per data-model.md:
      `ModelCapabilities { reasoning: Option<ReasoningControl>, accepted_attachment_kinds: Option<Vec<AttachmentKind>>, thinking_style: Option<ThinkingStyle> }` deriving
      `Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize` (`ProviderModel` already derives
      `PartialEq, Eq`); `ReasoningControl = Unavailable | ModelManaged | Presets { options: Vec<ReasoningOption> }`; `ReasoningOption { id: String, label: String }`;
      `ThinkingStyle = Adaptive | Manual`. Serde: structs `rename_all = "camelCase"`, enums an
      internal `kind` tag with `snake_case` variants, **"every field is `#[serde(default)]`; unknown
      JSON fields are ignored"**. **"`Presets.options` is non-empty by construction (an empty list
      maps to `Unavailable`/`ModelManaged` at the adapter boundary)"** — enforce with a
      `ReasoningControl::presets(options)` constructor that returns `Unavailable` for an empty list. Because serde bypasses that constructor, also add `ModelCapabilities::normalized(self) -> Self` mapping a `Presets` with empty `options` to `Unavailable` (everything else unchanged); every reader of stored or received JSON calls it (T008).
      Add `ModelCapabilities::local(model_id)` (the record for a built-in model, shared by registration and the backfill), `is_undetermined()` (true when all three fields are `None`) and
      `ReasoningControl::offers(&self, id: &str) -> bool`. Add
      `local_model_reasoning(model_id: &str) -> ReasoningControl` — the body of
      `chat::commands::model_supports_reasoning` moved without its Anthropic-specific branches (`claude-*` ids get their record from the provider's live answer, FR-019), returning `ModelManaged` when it was `true` and `Unavailable` otherwise, with a `ponytail:` comment (naive id heuristic; upgrade
      path: read the GGUF chat template). Declare the test module via
      `#[cfg(test)] #[path = "model_capabilities_tests.rs"] mod model_capabilities_tests;`.
      The original `model_supports_reasoning` stays until T027 deletes it.
- [x] T005 [P] Create `src-tauri/src/model_capabilities_tests.rs`: `None` (not determined) stays
      distinct from `Some(Unavailable)` after a JSON round-trip; serialized shape equals the examples
      in contracts/capabilities-json.md; unknown extra JSON fields are ignored; missing fields
      default to `None`; `ReasoningControl::presets(vec![])` yields `Unavailable`; `normalized` maps an empty `Presets` to `Unavailable` and leaves all other values unchanged; `offers` is true only for ids in `Presets`; `is_undetermined` semantics; `local_model_reasoning` cases **moved**
      from `src-tauri/src/chat/commands_tests.rs::reasoning_capability_is_derived_conservatively_from_the_model_id`
      (delete the original there in T022 so nothing is duplicated).
- [x] T006 In `src-tauri/src/identity/migrations.rs` add migration
      `0018_models_add_capabilities` = `ALTER TABLE models ADD COLUMN capabilities_json TEXT;` (nullable,
      no default, with a comment stating `NULL` means not determined), bump `HOLZI_TRIGGER_VERSION`
      from 9 to 10, and add the `- 10:` line to the version list in the constant's doc comment
      (mandatory because `models` is CRDT-tracked, see the file's own rule).
- [x] T007 [P] Extend `src-tauri/tests/vault_upgrade.rs` following its existing pattern: a vault
      provisioned at trigger version 9 reopens at 10 and a `capabilities_json` write on `models`
      produces a sync payload row (proves the trigger regeneration).
- [x] T008 In `src-tauri/src/storage/models.rs`: add `capabilities: Option<ModelCapabilities>` to
      `ModelRow`; append `capabilities_json` to `SELECT_COLUMNS` and to `upsert_model` (new `?14`
      parameter; in `ON CONFLICT` overwrite **unconditionally** like `context_window` — **not**
      `COALESCE`); in `row_to_model` parse leniently: **"Unparseable JSON → `None` + `log::warn!`
      with model id"** (include the serde error; never propagate, never panic — FR-021), then pass the parsed record through `ModelCapabilities::normalized`. Add
      `pub fn backfill_local_capabilities(conn, local_provider_id: Uuid) -> Result<usize>` as a
      sibling of `backfill_source_kind`: for each row of the local provider whose `capabilities_json IS NULL`, write `ModelCapabilities { reasoning: Some(local_model_reasoning(&id)), accepted_attachment_kinds: Some(vec![]), thinking_style: None }` with
      `{HLC_TIMESTAMP_COLUMN} = current_hlc()`; idempotent; never touches provider-kind rows or rows
      already determined. Declare `#[cfg(test)] #[path = "models_tests.rs"] mod models_tests;`.
- [x] T009 [P] Cover the storage layer. Unit tests in `src-tauri/src/storage/models_tests.rs` for the pure column codec (`capabilities_from_column`/`capabilities_to_column`: round trip, `NULL`, malformed JSON → `None`, stored empty `presets` → `Unavailable`). Because `upsert_model` and the backfill need `current_hlc()` from an open vault, the database-backed cases live in the new integration test `src-tauri/tests/model_capabilities_storage.rs`: a populated record round-trips through `get_model`/`list_models_by_provider`/`list_all_models`; an undetermined record is stored as `NULL`; a second upsert **replaces** the first entirely (including with `None`); a corrupt stored value reads `None` while every other column is intact and the row stays listed (FR-021); `backfill_local_capabilities` fills only `NULL` local rows, is idempotent, and leaves `api_key`/`cli_delegate` rows and already-determined rows untouched.
- [x] T010 In `src-tauri/src/adapters/mod.rs` add `pub capabilities: ModelCapabilities` to
      `ProviderModel` (doc: `ModelCapabilities::default()` means "everything not determined"); set
      `ModelCapabilities::default()` in the two existing literals —
      `src-tauri/src/adapters/anthropic.rs` (replaced in T023) and the Codex arm of
      `src-tauri/src/adapters/cli_delegate/mod.rs::list_models`.
- [x] T011 In `src-tauri/src/providers/mod.rs::compose_model_row` set
      `capabilities: (!m.capabilities.is_undetermined()).then_some(m.capabilities)` (an all-`None`
      record persists as SQL `NULL`, see contracts/capabilities-json.md). Then fix every remaining
      `ModelRow` literal mechanically with `capabilities: None`, driven by `cargo build --tests`:
      `src-tauri/src/providers/mod.rs` (2nd literal), `src-tauri/src/models/commands.rs` (`register_downloaded`, temporarily `capabilities: None` until T012), `src-tauri/src/storage/models.rs`,
      `src-tauri/tests/huggingface_models.rs` (9), `src-tauri/tests/provider_models.rs` (2).
- [x] T012 In `src-tauri/src/models/commands.rs`: `register_downloaded` builds its `ModelRow` with
      the local record from T008 (`local_model_reasoning(&id)`, `Some(vec![])`, `None`) — this is the
      single creation site shared by every download/import path; in `list_installed_models` call
      `models_store::backfill_local_capabilities(conn, local_provider_id)` right after
      `backfill_source_kind` (same error mapping). Add a `ponytail:` comment on the lazy backfill
      (runs on every listing; ceiling: one indexed `IS NULL` scan; upgrade path: one-time migration hook).
- [x] T013 [P] Extend `src-tauri/src/models/commands_tests.rs`: registering a local model records `ModelCapabilities::local(id)` (reasoning family → `ModelManaged`, other → `Unavailable`, attachments `Some([])`). The lazy backfill's behavior is covered by T009's integration test; the one-line call from `list_installed_models` needs a Tauri `AppHandle` and has no unit seam.
- [x] T014 Backend foundation checkpoint: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`,
      `cargo build --manifest-path src-tauri/Cargo.toml --tests`, `cargo test --manifest-path src-tauri/Cargo.toml` all green. Commit: `feat(models): persist a per-model capability record`.

### Frontend: behavior-preserving store split (parallel with the backend group)

- [x] T015 [P] Create `src/composables/useModelInventory.ts` and move out of `src/stores/models.ts`:
      `installedModels`, `catalogEntries`, `providerList`, `providerModels`,
      `refreshInstalledAndCatalog`, `refreshProviders`, `modelGroups`, `noModelsInstalled`,
      `findModelName` and the `DELEGATE_VENDORS`/`ModelGroup` definitions. It takes its
      dependencies as parameters (`models`, `catalog`, `providers` composable instances, `t`,
      `errString`, a `setError(msg)` callback) and uses **no Nuxt auto-imports** internally, so the
      harness's generic `~/composables/*` branch loads it unchanged. The store imports it explicitly
      (`import { useModelInventory } from '~/composables/useModelInventory'`) and re-exports the
      identical public names.
- [x] T016 Create `src/composables/useModelIntegrity.ts` and move `integrityDialog`,
      `integrityBusy`, `integrityActionError` and the five `onIntegrity*` handlers out of
      `src/stores/models.ts` (parameters: `chat`, `models`, `t`, `errString`, `installedModels`,
      `refreshInstalledAndCatalog`, `setError`, and the load-state hooks `beginLoadingModel`/
      `clearLoadingModel` it uses today). Same conventions as T015; explicit import in the store;
      identical public API.
- [x] T017 Verify the split is behavior-preserving: `pnpm check:chat-state` reports the **same test
      count as T002** and all pass, `pnpm typecheck`, `pnpm check:templates`, `pnpm lint`,
      `pnpm format:check`; `wc -l src/stores/models.ts src/composables/useModelInventory.ts src/composables/useModelIntegrity.ts` — each under 500. Commit:
      `refactor(models): split the models store into inventory and integrity composables`.

**Checkpoint**: shared record, storage, local population and a slimmed store exist. Nothing the user
sees has changed yet.

---

## Phase 3: User Story 1 - The composer offers exactly what the selected model supports (P1) 🎯 MVP

**Goal**: Effort options, reasoning output and attachment acceptance come from the selected model's
cached record, populated live from Anthropic (API key and Claude Code delegate share one code
path), refreshable from Settings; the five legacy mechanisms are deleted.

**Independent Test**: quickstart.md §2 steps 2–7 — refresh a Claude provider, switch between models
with different supported levels and attachment support, see the composer follow each model;
`cargo test` proves the wire mapping and consumers; `pnpm check:chat-state` proves the composer
states.

### Tests for User Story 1 (write first; expected to fail until the implementation tasks land)

- [x] T018 [P] [US1] Create `src-tauri/src/adapters/anthropic_capabilities_tests.rs` (declared from `anthropic_capabilities.rs` with the repository's `#[cfg(test)] #[path = ...]` idiom) and test `map_capabilities` directly with the seven cases listed in contracts/capabilities-json.md (full adaptive tree with all provider-reported options; provider-native subset + manual thinking; thinking without effort → `ModelManaged`; neither → `Unavailable`; capabilities object absent → all `None`; `pdf_input` absent → attachments `None`; unknown top-level leaves ignored while provider-native effort keys and their response order are preserved). In `src-tauri/src/adapters/anthropic_tests.rs` add one wiremock end-to-end case proving `fetch_models` returns the mapped capabilities for a full-tree listing and still succeeds for a listing whose capabilities object is absent.
- [x] T019 [P] [US1] In `src-tauri/src/adapters/request_tests.rs` update fixtures to the new
      `ChatRequest` fields and prove: `Presets` + valid `reasoning_option` → `output_config.effort` equals the option id; absent or not-offered option →
      no `output_config`; `thinking_style: Adaptive` → `{"type":"adaptive"}`; `Manual` → the existing
      budget form with its `max_tokens`/budget clamps; reasoning `None`/`Unavailable` or
      `thinking_style: None` → no `thinking` field. Remove the `supports_adaptive_thinking` tests.
- [x] T020 [P] [US1] In `src-tauri/src/adapters/cli_delegate/claude_tests.rs` update the three
      `EffortLevel` uses: `--effort` receives the validated option id string unchanged; no
      `--effort` when `None`.
- [x] T021 [P] [US1] In `src-tauri/src/adapters/cli_delegate/mod_tests.rs` extend the existing
      Claude-delegate model-list test through the shared `fetch_models` path so the returned
      `ProviderModel`s carry the mapped capabilities (no change needed in `cli_delegate` production
      code), and fix the `ChatRequest` literal.
- [x] T022 [P] [US1] In `src-tauri/src/chat/commands_tests.rs` delete the tests of `effort_levels_for` and the moved
      reasoning-derivation test (now in T005); add tests for the new pure helpers: `reasoning_requested_for` is true for `Presets`/`ModelManaged`, false
      for `Unavailable`/`None`, and false for a qwen3 request with tools; option validation keeps an
      id only when it is in `Presets.options`. In `src-tauri/src/chat/attachments_tests.rs` replace the four `usability_for` cases with:
      kind in `Some(list)` → usable; kind not in `Some(list)` → not accepted; `None` → undetermined.

### Implementation: backend consumers

- [x] T023 [US1] Create `src-tauri/src/adapters/anthropic_capabilities.rs` (keeps `anthropic.rs`, 448 lines today, under the 500-LoC boundary) holding the `pub(super)` wire structs (`image_input`, `pdf_input`, `thinking{supported, types{enabled, adaptive}}`, and provider-native `effort{supported, provider-native option entries}`; **every nested field `#[serde(default)]`) and `pub(super) fn map_capabilities(&WireCapabilities) -> ModelCapabilities` implementing research.md R3 verbatim (`Presets` in provider response order with ids equal to the wire keys, built through `ReasoningControl::presets`; else `ModelManaged`; else `Unavailable`; missing subtree → `None`; `thinking_style` Adaptive over Manual; attachments `Some([Text] + Image + Document)` only when **both** `image_input` and `pdf_input` are present, else `None`). Register `mod anthropic_capabilities;` in `src-tauri/src/adapters/mod.rs`; in `anthropic.rs` add `#[serde(default)] capabilities: Option<WireCapabilities>` to `ModelInfo` and call `map_capabilities` once in `fetch_models` (replacing the T010 placeholder). Add a `ponytail:` comment on the whole-list attachment granularity (ceiling: one missing key undetermines all kinds; upgrade path: per-kind fallback, research R3).
- [x] T024 [US1] In `src-tauri/src/adapters/types.rs` replace `ChatRequest.effort_level: Option<EffortLevel>` with `reasoning_option: Option<String>` and add `capabilities: Option<ModelCapabilities>`; drop the `EffortLevel` import. In
      `src-tauri/src/adapters/cli_delegate/claude.rs` change `spawn_claude_invocation`'s
      `effort_level` parameter to `Option<&str>` and pass `req.reasoning_option.as_deref()`.
- [x] T025 [US1] In `src-tauri/src/adapters/request.rs` make the serializer read `req.capabilities`
      and `req.reasoning_option`: send `thinking` only when reasoning is `Presets | ModelManaged`
      **and** `thinking_style` is `Some` (`Adaptive` → `{"type":"adaptive"}`, `Manual` → existing
      budget form); send `output_config.effort` only when `reasoning_option` is one of the model's
      `Presets` ids; delete `supports_adaptive_thinking` and the `effort` import.
- [x] T026 [US1] In `src-tauri/src/chat/attachments.rs` change `usability_for` (keep the name —
      extend the existing candidate) to `usability_for(kind: &AttachmentKind, capabilities: Option<&ModelCapabilities>) -> AttachmentUsability` with a three-variant enum
      (`Usable`, `NotAccepted`, `Undetermined`); update its doc comment.
- [x] T027 [US1] In `src-tauri/src/chat/commands.rs`: rename `SendMessageArgs.effort_level` to
      `reasoning_option: Option<String>`; in `send_message`'s existing blocking lookup also read
      `models_store::get_model(conn, &session.model_id)` (local and composite ids both resolve) and
      derive from that **one** snapshot `reasoning_requested` (`Presets | ModelManaged`, keeping the
      qwen3-with-tools exception in `reasoning_requested_for`), the validated `reasoning_option`
      (dropped silently when not offered, per contracts/tauri-commands.md) and `capabilities`; in
      `inspect_attachment` remove the `Provider` lookup and `(ProviderKind, adapter)` match, read
      the row once, and return `usable: false` with reason
      `"not usable by the selected model"` (not accepted) or
      `"attachment support for this model is not yet known"` (undetermined). Delete
      `get_effort_levels`, `effort_levels_for`, `model_supports_reasoning` and the
      `EffortLevel` import.
- [x] T028 [US1] Remove the `get_effort_levels` import and registration in `src-tauri/src/lib.rs`;
      delete `src-tauri/src/adapters/effort.rs` and `src-tauri/src/adapters/effort_tests.rs` and their
      `pub mod effort;` / `mod effort_tests;` lines in `src-tauri/src/adapters/mod.rs`.
- [x] T029 [US1] Mechanical (no behavior change beyond the T024 field swap; call it out as mechanical in the PR description): replace `effort_level: None` with `reasoning_option: None, capabilities: None` in every remaining `ChatRequest` literal, iterating `cargo build --manifest-path src-tauri/Cargo.toml --tests` until clean. Measured sites (17 files):
      `src-tauri/src/adapters/anthropic_stream_tests.rs`, `src-tauri/src/chat/turn/step_tests.rs`,
      `src-tauri/tests/common/tool_loop_fixture.rs`, `src-tauri/tests/chat_tool_loop_permissions.rs`,
      `src-tauri/tests/cli_delegate_{approval,autonomy_audit_trail,autonomy_deny_rules, autonomy_gated_permissive,autonomy_unavailable,autonomy_ungated,claude,codex_live, disconnect_in_flight,isolation_live}.rs`, `src-tauri/tests/local_inference.rs`.
- [x] T030 [US1] Expose capabilities to the frontend: add `capabilities: Option<ModelCapabilities>`
      to `InstalledModelPayload` (`src-tauri/src/models/commands.rs`, filled from the row) and to
      `ProviderModelPayload` (`src-tauri/src/providers/mod.rs`, filled in `list_provider_models`);
      add a maintainability-exception header to `src-tauri/src/providers/mod.rs` (562 lines): reason
      it stays together and a concrete split plan (extract `do_refresh` + payload structs), matching
      the header style in `src-tauri/src/chat/commands.rs`.
- [x] T031 [US1] Backend checkpoint: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`,
      `cargo test --manifest-path src-tauri/Cargo.toml`, `pnpm lint:rust`. Commit:
      `feat(chat): drive effort, reasoning and attachments from cached model capabilities`.

### Implementation: frontend

- [x] T032 [P] [US1] In `src/composables/useModels.ts` add the TS mirrors `ModelCapabilities`,
      `ReasoningControl` (discriminated on `kind`: `unavailable | model_managed | presets`) and
      `ReasoningOption`; add `capabilities: ModelCapabilities | null` to `InstalledModel` there and
      to `ProviderModel` in `src/composables/useProviders.ts` (type-only import).
- [x] T033 [US1] Add the US1 harness cases to `scripts/check-chat-state.ts` (write before T034–T036;
      they fail until then): options offered change with the displayed model (`Presets` A vs B);
      `Unavailable` → `effortState` `hidden`; `ModelManaged` → `managed`; a row whose `capabilities` is `null` → `unknown`; no resolved model row (lists still loading, nothing selected, the `delegate-*:not-connected` placeholder) → `hidden`, never `unknown`; a
      selection made from `Presets` is sent as `reasoningOption` by `send()`; switching models issues
      no additional capability-related `invoke` (SC-005). Extend the fixture lists
      (`list_installed_models`, `list_providers`, `list_provider_models`) with capability payloads.
- [x] T034 [US1] In `src/composables/useModelInventory.ts` add `capabilitiesFor(modelId)` (lookup across `installedModels` and `providerModels`, no IPC; `null` when no row matches). Create `src/composables/useReasoningPreference.ts` with the **in-memory** part: `effortLevel: Ref<string | null>`, `effortState` and `updateEffortLevel(id | null)` that accepts only `null` or an id offered by the current `Presets`, and resets to `null` when a capability change removes the selected option (persistence arrives in US2). `effortState` is `'hidden'` when no model row is resolved or when reasoning is `Unavailable` (or a `Presets` with no options), `'managed'` for `ModelManaged`, `'unknown'` **only** when a row exists and its `capabilities` (or its `reasoning`) is `null`, otherwise `'selectable'`. Wire both into `src/stores/models.ts`, which exports `displayModelCapabilities` (`computed` over `displayModelId` and `capabilitiesFor`), `effortState`, `effortLevel`, `updateEffortLevel`. Keep `models.ts` under 500 lines.
- [x] T035 [US1] In `src/composables/useChat.ts` delete `getEffortLevelsAsync` and the `EffortLevel` type; rename the `sendMessageAsync` argument `effortLevel` → `reasoningOption: string | null`. The file is 575 lines with no exception on record: add a maintainability-exception header with a concrete split plan (move the wire/event interface declarations — `Thread` through `ModelLoadErrorEvent`, roughly lines 4–255 — into a type-only `src/composables/useChatTypes.ts` re-exported from `useChat.ts`, leaving the ~320-line command wrappers), in the style of the header in `src-tauri/src/chat/commands.rs`.
- [x] T036 [US1] In `src/pages/chat/[instance].vue` remove the page-local `effortLevel`,
      `effortLevels`, `updateEffortLevel`, `refreshEffortLevels`, the `EffortLevel` import and the
      `watch(displayModelId, refreshEffortLevels, ...)` line; read `effortLevel`, `effortState`,
      `updateEffortLevel` and the options from `modelStore`; the effort label uses
      `chat.effort.<id ?? 'auto'>` and falls back to the option's own `label` for an id without an
      i18n key; `send()` sends `reasoningOption: modelStore.effortLevel`. Leave
      `refreshAttachmentUsability`/`inspectAttachmentAsync` untouched. Keep the page's maintainability
      header accurate.
- [x] T037 [P] [US1] In `src/components/chat/ComposerSettingsPopover.vue` replace the fixed
      `effortLevels`/`effortIndex` model with the offered options (`{ id, label }[]`) plus an
      `effortState` prop: `hidden` renders nothing, `managed`/`unknown` render a disabled control with
      the state label (the `unknown` label points to the provider refresh in Settings), `selectable`
      renders the options plus Auto. Props only — no store access inside the component.
- [x] T038 [P] [US1] Add i18n keys to both `src/i18n/locales/en.json` and `de.json`:
      `chat.effort.managed`, `chat.effort.unknown` (with the "refresh models in Settings" hint) and
      `settings.cliDelegate.refreshModels`, `.refreshing`, `.refreshed`, `.refreshFailed`.
- [x] T039 [US1] In `src/components/settings/ConnectDelegateProvider.vue` add a "Refresh models"
      button next to the existing reconnect action for each connected provider, calling
      `refreshModelsAsync(provider.id)` from `useProviders()` (its first caller). States: idle,
      in-flight (button disabled, progress shown), success (brief confirmation), failure (message via
      `errString`; previous capabilities untouched). No store coupling — the chat page's `initialize()` re-reads the lists on mount. The repository has no component test runner, so these states are verified manually (T041, T055); the only logic is one call to an existing command.
- [x] T040 [US1] Frontend checkpoint: `pnpm check:chat-state`, `pnpm typecheck`,
      `pnpm typecheck:scripts`, `pnpm check:templates`, `pnpm lint`, `pnpm format:check`. Commit:
      `feat(chat): show model-specific effort states and add a provider refresh action`.
- [ ] T041 [US1] Manual validation in the real app (quickstart §2 steps 2–7) and the one-off record-shape checks (quickstart §3): capture a live `GET /v1/models` response with an API key **and** one with the OAuth bearer the Claude Code delegate uses, and confirm both contain `pdf_input` and the `thinking`/`effort` subtrees. If `pdf_input` is absent, apply the per-kind fallback from research R3. If the OAuth listing lacks `capabilities`, try `GET /v1/models/{id}` with the same bearer; if neither returns them, stop and amend the spec (FR-003) rather than reintroducing a static table (FR-019). If no credentials or dev shell are available, state exactly which steps were not run.

**Checkpoint**: US1 is fully functional and testable on its own — MVP.

---

## Phase 4: User Story 2 - Each model remembers its own effort choice (P2)

**Goal**: The selected option is remembered per model (per provider connection) on this device,
restored on switch and restart, and falls back to Auto when no longer offered.

**Independent Test**: quickstart.md §2 steps 8–11 and the US2 harness cases.

### Tests for User Story 2

- [x] T042 [US2] Add the US2 harness cases to `scripts/check-chat-state.ts` (and the one-line
      `useDevice` auto-import for the store in `createChatState`, plus stateful `set_pref` /
      `clear_pref` / `get_pref` invoke handlers): a saved valid option is restored when the same
      model is selected again and after a fresh store (restart); each of two models restores its own
      option; selecting Auto calls `clear_pref` and nothing is stored; a saved option no longer in
      `Presets` yields Auto and `clear_pref`; a capability refresh that removes the selected option
      yields Auto and `clear_pref`; a slow preference read for the previous model cannot overwrite the
      newly selected model's value (deferred promise resolved after the switch); a rejected
      `set_pref` rolls back to the previous value and sets `lastError`; the same remote model id under
      two provider ids uses two different keys; no `get_pref`/`set_pref` runs before the device UUID is known, and the saved option for the first displayed model is still loaded once the UUID arrives (the active model is set during `initialize()` before the lists load). Update the file header's case count.

### Implementation for User Story 2

- [x] T043 [US2] Extend `src/composables/useReasoningPreference.ts` with persistence. Key:
      **`chat.reasoning_option.<model-id>`** (composite `{provider_id}:{remote_id}` for provider
      models, plain id for local), scope `{ kind: 'device', uuid: vaultDeviceUuid }`, value = the
      option id, **"absent means Auto"**. `loadEffortPreference(modelId)`: monotonically increasing
      token; after the awaited `getPrefAsync` confirm both the token and `displayModelId === modelId` before applying; apply the saved id only if `Presets` offers it, otherwise set `null`
      and `clearPrefAsync` the stale key. `updateEffortLevel`: optimistic set, then `setPrefAsync`
      (or `clearPrefAsync` for `null`); on failure roll back to the previous value and report through
      the store's `lastError`. A watcher on the display model's capabilities revalidates the active
      model's effective and stored option (FR-017). Take `usePreferences` functions, the device UUID
      ref, `displayModelId` and `setError` as parameters (no auto-imports).
- [x] T044 [US2] In `src/stores/models.ts` resolve `useDevice().currentDeviceInfoAsync()` **as the first step of `initialize()`** (before `refreshActiveModel()`, which sets the first display model) and keep `vaultDeviceUuid`; run no preference read or write until it is set; trigger `loadEffortPreference` on `displayModelId` changes (including the initial value) **and once more when the UUID becomes available** for an already-displayed model. A failed device lookup surfaces through `lastError` and leaves the in-memory behavior working.
- [x] T045 [US2] Story checkpoint: `pnpm check:chat-state`, `pnpm typecheck`, `pnpm check:templates`,
      `pnpm lint`, `pnpm format:check`, then manual validation quickstart §2 steps 8–11. Commit:
      `feat(chat): remember the reasoning option per model and device`.

**Checkpoint**: US1 and US2 both work independently.

---

## Phase 5: User Story 3 - The composer is honest about what is not yet known (P3)

**Goal**: A model whose capabilities are not determined (Codex, a provider connected before this
build and not yet refreshed, incomplete provider data) is never presented as unsupported, and last
learned capabilities survive failures.

**Independent Test**: quickstart.md §2 steps 1, 3, 5–7 and the US3 tests below.

### Tests for User Story 3

- [x] T046 [P] [US3] Cover refreshing against a real vault and a Wiremock provider in `src-tauri/src/providers/providers_tests.rs` (beside `do_refresh`, which is private): a successful refresh of a legacy `NULL` provider row makes it determined; a refresh that returns absent capability data **replaces** the stored record with the new (undetermined) answer, no stale merging; a **failed** refresh leaves the stored capabilities intact (FR-010).
- [x] T047 [P] [US3] In `src-tauri/src/adapters/cli_delegate/mod_tests.rs` assert the Codex
      delegate's synthetic model has `ModelCapabilities::default()` (`is_undetermined()`), is
      **not** `Some(Unavailable)`, and persists as SQL `NULL` via `compose_model_row`; add a doc
      comment on the Codex arm of `src-tauri/src/adapters/cli_delegate/mod.rs::list_models` stating
      "not determined until Codex exposes capability metadata".
- [x] T048 [P] [US3] In `scripts/check-chat-state.ts` add cases: an undetermined model (`None` and
      an Anthropic-partial record) yields `effortState: 'unknown'` and never reports `unavailable`
      anywhere; an attachment inspected against an undetermined model surfaces the "not yet known"
      reason rather than the "not usable" one; a local `ModelManaged` row yields `managed`, a local
      `Unavailable` row yields `hidden`. Update the header's case count.

### Implementation for User Story 3

- [x] T049 [US3] Verify in `src/pages/chat/[instance].vue` that `refreshAttachmentUsability` shows the backend `reason` unchanged (no client-side remapping to a generic text), so the undetermined case reads "not yet known" exactly as T027 returns it; if it remaps, pass the backend reason through.
- [x] T050 [US3] Story checkpoint: `cargo test --manifest-path src-tauri/Cargo.toml`,
      `pnpm check:chat-state`, `pnpm typecheck`, `pnpm lint`, then manual validation quickstart §2
      steps 1, 3, 5–7. Commit: `test(chat): cover undetermined capability states`.

**Checkpoint**: all three stories independently functional.

---

## Phase 6: Polish and cross-cutting

- [x] T051 [P] Legacy-removal check (FR-019): `rg -n "get_effort_levels|effort_levels_for|model_supports_reasoning|supports_adaptive_thinking|anthropic_supported_levels|claude_delegate_levels|getEffortLevelsAsync|EffortLevel" src src-tauri/src src-tauri/tests scripts`
      returns nothing outside historical `specs/` and `docs/`, and `rg -n "ProviderKind|adapter ===|adapter ==" src-tauri/src/chat/commands.rs src/pages/chat src/components/chat src/composables/useReasoningPreference.ts src/composables/useModelInventory.ts` shows no capability decision keyed on provider kind (FR-020, SC-007; only the unrelated autonomy-mode delegate checks may remain); fix stale doc comments that still
      mention them (e.g. in `src-tauri/src/chat/attachments.rs`, `src-tauri/src/adapters/types.rs`).
- [x] T052 [P] Diff the `en.json` and `de.json` key trees to confirm the new keys (T038) exist in
      both.
- [x] T053 [P] Size audit: `wc -l` on every file touched; new files, `src/stores/models.ts` and `src-tauri/src/adapters/anthropic.rs` stay under 500 lines; `scripts/check-chat-state.ts` header states its new size and its cap — **no further cases are added to this file after this feature; the next change must first extract `createChatState` into `scripts/lib/chat-state-harness.ts` per the header's split plan** (plan.md Complexity Tracking).
- [x] T054 Run the complete CI-parity set from quickstart §1 once more end to end:
      `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo test --manifest-path src-tauri/Cargo.toml`, `pnpm lint:rust`, `pnpm check:chat-state`, `pnpm check:templates`,
      `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check` — and report exactly
      what ran and anything that could not run.
- [ ] T055 Run the full quickstart §2 manual walkthrough on one build and record results in the PR
      description. Open the PR(s) to `main` as laid out in Implementation Strategy (topic branch `012-unified-model-capabilities`); list the
      disclosed user-visible changes from spec.md Assumptions; merge with rebase-merge or a
      merge-commit carrying a Conventional Commits subject (never squash).

---

## Phase 7: Detection for OpenAI-compatible servers (FR-024)

- [x] T056 `model_capabilities.rs`: `kind` on the record (`ModelKind`: text, image, audio, other) and, in `UserCapabilities`, the provider's answer a statement replaced, so `without_user` restores it; tests in `model_capabilities_tests.rs` (withdrawal restores, a refresh keeps the statement over the new answer, an older record withdraws to "not determined").
- [x] T057 `adapters/openai_compatible_capabilities.rs`: read modalities (flat and OpenRouter's `architecture`), `model_type`, `tools`, `thinking` and `supported_parameters` leniently into the record; `openai_compatible.rs` flattens it into the model entry; tests in `openai_compatible_capabilities_tests.rs`.
- [x] T058 `useModelInventory.ts`: the chat's model list offers `text` models only (an undescribed model counts as text).
- [x] T059 `ModelServerView.vue` with `lib/models/servers.ts` helpers: models by kind; a reported image answer is shown, the choice only where the server is silent; the effort field only for models that reason or servers that describe nothing, with the one-click suggestion low, medium, high; checks in `scripts/check-provider-address.ts`; de/en texts.
- [ ] T060 Manual: add Abacus (`https://routellm.abacus.ai/v1`) and check that image-capable models read "Takes images (reported by the server)", image models sit under "Image models" and are absent from the chat's list.

## Dependencies & Execution Order

- **Phase 1 → Phase 2 → stories → Polish.** Phase 2 blocks all stories.
- **Phase 2 has two independent groups**: the backend group (T003–T014) and the frontend split
  (T015–T017) touch disjoint files and can proceed in parallel.
- **US1 (P1)** needs Phase 2 complete. Its backend tests (T018–T022) are written first; consumer
  changes T023–T030 land together because they share the `ChatRequest` type change (T024) and only
  compile as a set — T029 is the compiler-driven sweep. Frontend T032 → T033 (tests) → T034–T039.
- **US1 backend and frontend ship together**: T028 removes `get_effort_levels`, which the current frontend still calls, so the backend checkpoint (T031) is not releasable on its own.
- **US2 (P2)** needs US1's `useReasoningPreference` (T034) and the popover/page wiring; it extends
  rather than replaces them.
- **US3 (P3)** needs US1's undetermined branches; its tests can be written in parallel once US1's
  backend and store exist.
- Within a story: tests → backend → frontend → checkpoint. Any task touching
  `src/stores/models.ts` (T015, T016, T034, T044) or `scripts/check-chat-state.ts`
  (T033, T042, T048) is sequential with the others touching the same file.
- **Removing legacy code (T027/T028) is safe only because Phase 2 already populates local models**
  (T012); do not reorder those.

## Parallel Example

```bash
# Phase 2 — the two groups run side by side:
Task: "T003–T014 backend record, storage, local population"
Task: "T015 useModelInventory (then T016, T017) frontend split"

# User Story 1 — tests, all different files:
Task: "T018 anthropic_tests.rs"
Task: "T019 request_tests.rs"
Task: "T020 claude_tests.rs"
Task: "T021 cli_delegate/mod_tests.rs"
Task: "T022 chat/commands_tests.rs + attachments_tests.rs"

# User Story 1 — independent frontend files after T034:
Task: "T037 ComposerSettingsPopover.vue"
Task: "T038 en.json + de.json"
```

## Implementation Strategy

1. **MVP = Phase 1 + Phase 2 + US1.** After T041 the composer is accurate for Claude (API key and
   delegate), local models keep their reasoning, legacy mechanisms are gone, and Settings has the
   refresh action. Validate and demo here.
2. **Increment 2 = US2** — persistence of the choice; a pure frontend/store addition.
3. **Increment 3 = US3** — verification and hardening of the undetermined paths; mostly tests.
4. **PR structure** (constitution review-size guidance: about 1000 net lines is a prompt to split), stacked and merged in order with rebase-merge or a merge-commit, never squash:
   - PR A — this spec, plan and tasks (docs only).
   - PR B — Phase 2, T003–T017 (behavior-preserving foundation).
   - PR C — US1, T018–T041 (one PR because T028 removes a command the current frontend calls; T029 is a mechanical sweep and is labelled as such in the description).
   - PR D — US2, T042–T045.
   - PR E — US3 and Polish, T046–T055.
5. One commit per checkpoint (T014, T017, T031, T040, T045, T050), Conventional Commits, no agent trailers. Each checkpoint compiles and passes its own commands, so every PR can be reviewed stage by stage.
