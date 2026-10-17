# Tasks: Erweiterungen nutzen holzis Modelle

**Input**: Design documents from `/specs/055-extension-model-access/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/bridge.md, contracts/job-format.md,
quickstart.md

**Tests**: Test-first per task pair (write the failing check, then implement). Rust unit tests in sibling
`*_tests.rs` files (never inline `mod tests { … }`), integration tests in `src-tauri/tests/`. Pure frontend
logic in `scripts/check-*.ts` (`node --test`, relative `.ts` imports, no `~/` alias). vault-sdk tests in
vitest. No new runtime dependency is planned (plan, Technical Context).

**Conventions**: Pure modules under `src/lib/` import only relative `.ts` paths. i18n keys go into both
`src/i18n/locales/de.json` and `en.json`. Every changed file stays under 500 lines. New `#[ts(export)]` types
carry a domain prefix (`ModelJob…`, `ExtensionModel…`) and their bindings are regenerated with only the new
files committed (strip trailing whitespace; revert unrelated drift in `src/types/bindings`). A column or
table added to a synced table bumps `HOLZI_TRIGGER_VERSION`. Cross-repo references pin full commit SHAs
(constitution IV). Commit per task group with Conventional Commits. vault-sdk work happens in its own
worktree on a topic branch from `origin/main`; its PR follows the operator's account rules.

**Deliveries** (plan): A = Phase 7 (US5, provider); B + D = Phases 1–6 (job core, permissions, dialogs);
C = Phase 8 (US6, local images, starts with the spike T057); E = Phase 9 (US7, test extension, SDK).
Each delivery is its own PR. Order: A, B + D, C, E. A and B + D do not depend on each other except for the
remote-model tests in B, which use a fake adapter.

## Phase 1: Setup

- [ ] T001 Rebase `055-extension-model-access` work on `origin/main`; confirm `src-tauri/src/agent_tasks/`, `extensions/bridge/agent_tasks.rs` and `extensions/commands/frames.rs` (cancel hooks) are unchanged since the plan, and that no caller of `extension_ai_task_*` exists in `src/`, `docs/` or vault-sdk
- [ ] T002 Rename module `src-tauri/src/agent_tasks/` to `src-tauri/src/model_jobs/` (git mv, behavior unchanged): update `lib.rs`, `extensions/host.rs` (`agent_tasks` field and accessor), `extensions/bridge/agent_tasks.rs` to `model_jobs.rs`, `extensions/bridge/mod.rs`, `extensions/commands/frames.rs`; all existing tests stay green
- [ ] T003 [P] Lift the schema subset validator from `src-tauri/src/extensions/agent_tools/defs.rs` into `src-tauri/src/schema_subset.rs` with the keywords and limits of `contracts/job-format.md` §Schema-Teilmenge; move its tests to `src-tauri/src/schema_subset_tests.rs`; `agent_tools` uses the shared module and its tests stay green

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: The types, permission kinds, limits and the guard every story needs.

- [ ] T004 Tests first for the new permission kinds in `src-tauri/src/extensions/permissions/model_tests.rs` and `target_tests.rs`: `Model` and `ModelDestination` parse and print, `ALL` has 13 entries, effort order `none` < `low` < `medium` < `high` with `Action::covers`, `send` for destinations, `Target::parse` for a model id or `*` and for a provider id
- [ ] T005 Add `PermissionKind::Model` (`model`) and `PermissionKind::ModelDestination` (`modelDestination`) in `src-tauri/src/extensions/permissions/model.rs` (`ALL: [_; 13]`, `as_str`, scope vault-wide), `Action` and `Target` variants and parsers in `target.rs`; `manifest_map.rs` gets "no manifest category" for both like `AgentTool`; bindings regenerated
- [ ] T006 [P] Tests first in `src-tauri/src/model_jobs/place_tests.rs`: `ProviderKind::Local` and loopback addresses are `this_device`; RFC 1918, link-local and `.local` hosts are `network`; everything else and every `CliDelegate` provider is `cloud`
- [ ] T007 [P] Implement `Place` and its classification from a provider in `src-tauri/src/model_jobs/place.rs`
- [ ] T008 [P] Tests first in `src-tauri/src/model_jobs/guard_tests.rs`: one case each for empty, a loop (distinct-word ratio below 0.35 at 12+ words, a line three times in a row), a script switch (CJK, Cyrillic, Arabic, Devanagari absent from instruction and input), invalid JSON (a code fence is stripped first) and a schema violation; invented but well-formed English text stays unflagged on purpose
- [ ] T009 Implement the guard in `src-tauri/src/model_jobs/guard.rs` (`check(output, instruction, input_text, schema) -> Result<Value, Unusable>` with a reason) using `schema_subset` from T003
- [ ] T010 [P] Tests first in `src-tauri/src/model_jobs/effort_tests.rs`: capping to the granted ceiling, mapping to `Presets` with two, three and five options, `ModelManaged`, `Unavailable`, missing wish means `none` (`contracts/job-format.md` §Aufwand)
- [ ] T011 [P] Implement `src-tauri/src/model_jobs/effort.rs` (`effective(wish, ceiling, &ModelCapabilities) -> Effort` and the provider-native option id)
- [ ] T012 [P] Tests first in `src-tauri/src/model_jobs/job_tests.rs`: valid job; too many steps (more than 4); duplicate or malformed step ids; `from` pointing at a later or unknown step; a pointer that cannot occur in the earlier step's schema; a schema with `pattern` or `$ref`; instruction over 16 KiB; more than 8 images; an image over 25 MiB; unsupported mime type; `images` in a step whose `needs` lack `image`
- [ ] T013 Implement `Job`, `Step`, `Input`, `Format` with parsing and preflight in `src-tauri/src/model_jobs/job.rs` and the constants in `src-tauri/src/model_jobs/limits.rs` (research R9)
- [ ] T014 [P] Tests first in `src-tauri/src/model_jobs/errors_tests.rs`: `JobErrorCode` (`InvalidInput`, `LimitExceeded` with the limit name, `NoCapableModel`, `ConsentDenied`, `Unusable`, `Unavailable`, `Timeout`, `Cancelled`) maps to `BridgeError` and to the event error; a message never contains an address, key or provider response body (seeded values)
- [ ] T015 Replace `src-tauri/src/model_jobs/errors.rs` (from `TaskErrorCode`) with `JobErrorCode`, the mapping, and `sanitize(message)`; delete the paper-note specific code in `model.rs`, `definitions.rs`, `validation.rs`, `fixture.rs` and their `*_tests.rs` (`PaperNote*`, `run_fixture`) once nothing uses them
- [ ] T016 Tests first in `src-tauri/src/model_jobs/state_tests.rs`: `JobRecord` holds only id, owner, status, cancel token and deadline; a finished record is removed after the completion event; `cancel_owned_by` aborts all jobs of a frame; at most 2 running jobs per frame; the deadline aborts
- [ ] T017 Rework `src-tauri/src/model_jobs/state.rs` (from `TaskRegistry`): `JobRecord` without provenance, removal after completion, deadline from the extension's limit
- [ ] T018 [P] Add `model_job_timeout_ms` to `ExtensionLimits` in `src-tauri/src/extensions/registry/limits.rs` (default 300 000, `MIN` 10 000, `MAX` 1 800 000), tests in its `*_tests.rs`, regenerate `src/types/bindings/ExtensionLimits.ts`, show the field in `src/components/settings/extensions/ExtensionLimitsView.vue` with de/en labels
- [ ] T019 Migration `0036_extension_model_order` in `src-tauri/src/identity/migrations.rs` (table `extension_model_order(extension_id, model_id, position)`, primary key `(extension_id, model_id)`), CRDT tracking and a `HOLZI_TRIGGER_VERSION` bump with its doc line; store functions `append`, `set_order`, `ordered(extension_id)` in `src-tauri/src/extensions/permissions/order.rs`; tests in `order_tests.rs` and the migration test
- [ ] T020 Register `extension_model_list`, `extension_model_job_start` and `extension_model_job_cancel` in `src-tauri/src/extensions/bridge/allowlist.rs` (replacing `extension_ai_task_*`); handlers in `src-tauri/src/model_jobs/bridge.rs` parse the job and run the preflight but do not run a model yet; dispatch test in `src-tauri/src/extensions/bridge/dispatch_tests.rs`

**Checkpoint**: The new types and kinds exist, jobs are parsed and rejected or accepted, nothing runs a model.

---

## Phase 3: User Story 1 - An extension asks a model (Priority: P1) 🎯 MVP

**Goal**: A permitted extension sends a job and receives a checked result with its provenance; without a grant nothing is processed.

**Independent Test**: A test extension with a grant sends an image, an instruction and a format to a scripted fake adapter and receives a format-valid result; without a grant the adapter is never called.

- [ ] T021 [US1] Integration test `src-tauri/tests/extension_model_jobs.rs` with a scripted fake `ProviderAdapter`: (1) grant, then a result with provenance; (2) no grant, then a `PermissionPromptRequired` (1004) prompt and zero adapter calls; (3) no field of any answer or event holds a key or address (seeded provider data)
- [ ] T022 [P] [US1] Tests first in `src-tauri/src/model_jobs/model_source_tests.rs`: a remote model yields an adapter built from its `providers` and `models` rows; a missing or unreachable provider yields `Unavailable`; the local source is a trait with a fake for tests
- [ ] T023 [US1] Implement `src-tauri/src/model_jobs/model_source.rs` (`adapter_for(model_id)`: remote through `providers::build_adapter`, local through a trait implemented in Phase 8; until then local reports `Unavailable`)
- [ ] T024 [P] [US1] Tests first in `src-tauri/src/model_jobs/candidates_tests.rs`: candidates are the models the extension may use (grants, including `*`) that satisfy `needs` (image from `ModelCapabilities.accepted_attachment_kinds`), in the order of `extension_model_order`, then of grant time; do not filter by reachability before an attempt, so `Unavailable` remains visible in provenance and the fallback can run
- [ ] T025 [US1] Implement `src-tauri/src/model_jobs/candidates.rs` (`for_step(ctx, extension_id, step) -> Vec<Candidate>` with place and capped effort); check reachability only when attempting a candidate
- [ ] T026 [US1] Tests first in `src-tauri/src/model_jobs/runner_tests.rs` (single step): the request carries no tools, the instruction as system prompt, the images as attachments and the format's schema as a hint; the stream is collected to text; the deadline and the cancel token abort and drop the stream
- [ ] T027 [US1] Implement the single-step run in `src-tauri/src/model_jobs/runner.rs` (build `ChatRequest` with `tools: []`, `max_new_tokens` from `maxOutputTokens`, `reasoning_option` from `effort.rs`; collect `StreamChunk::Delta`; honour the cancel token and deadline)
- [ ] T028 [US1] Implement `start` and `cancel` in `src-tauri/src/model_jobs/bridge.rs`: preflight, spawn the run, emit `haextension:model-job:completed` with `provenance` through `extensions/bridge/events.rs`, remove the record afterwards; adapt the frame hooks in `src-tauri/src/extensions/commands/frames.rs` to `model_jobs`; tests in `bridge_tests.rs` (owner check, wrong owner `SecurityViolation`, unknown id `{cancelled: false}`)
- [ ] T029 [P] [US1] Implement `extension_model_list` in `src-tauri/src/model_jobs/bridge.rs`: only models the extension may use, with name, provider name, place, capabilities and `maxEffort`, no address or key (FR-030); empty list without grants; test in `bridge_tests.rs`

**Checkpoint**: US1 works with a fake adapter. Stop and run quickstart steps 2, 3 and 12.

---

## Phase 4: User Story 2 - The user decides models and effort per extension (Priority: P1)

**Goal**: The user grants models and an effort ceiling per extension; no grant means no access; a revocation applies on the next request.

**Independent Test**: With two models only one is granted; requests to it succeed, requests to the other are rejected; after the revocation the next request to the first fails too.

- [ ] T030 [US2] Tests first in `src-tauri/tests/extension_model_jobs.rs` (more cases): only the granted model is used; a revoked grant applies to the next request while a running call may finish; the effort wish above the ceiling is capped and the provenance names the capped value; ranking decides among several granted models
- [ ] T031 [US2] Tests first in `src-tauri/src/extensions/commands/model_grants_tests.rs`: list grants with ranking, set a grant (model and ceiling), revoke, reorder; each change takes effect on the next preflight
- [ ] T032 [US2] Implement `src-tauri/src/extensions/commands/model_grants.rs` (`extension_model_grants_list`, `_set`, `_revoke`, `_reorder`) and register them in `src-tauri/src/lib.rs`; types with the `ExtensionModel` prefix and regenerated bindings
- [ ] T033 [US2] Tests first in `src-tauri/src/extensions/commands/permissions_tests.rs`: a start without any grant returns `PermissionPromptRequired` (1004) with `{resourceType: "model", action, target}` for the default candidate; the resolve path stores the chosen model and ceiling with `remember` or holds the decision for the session
- [ ] T034 [US2] Implement the missing-grant prompt in the preflight (`src-tauri/src/model_jobs/bridge.rs`), the `Question` for the new kinds in `src-tauri/src/extensions/bridge/permissions.rs`, and the resolve handling in `src-tauri/src/extensions/commands/permissions.rs` (chosen model id and ceiling travel with the decision); append new grants to `extension_model_order`
- [ ] T035 [P] [US2] Pure logic in `src/lib/extensions/modelGrants.ts` (rank ordering, effort levels and their labels, which models fit `needs`) with `scripts/check-extensions-model-grants.ts` added to `check:extensions` in `package.json`
- [ ] T036 [US2] `src/components/extensions/PermissionRequestDialog.vue`: branch for `model` (models that fit the request with their place, an effort ceiling select, "for the session" or "permanently"); de/en keys under `extensions.permissionRequest.model`; update `scripts/check-extensions-queue.ts` fixtures if the event shape changed
- [ ] T037 [US2] `src/components/settings/extensions/ExtensionModelsView.vue`: the models granted to one extension with ceiling, drag or buttons for the ranking, revoke; mounted in the extension's settings next to the permissions; de/en keys under `settings.extensions.models`

**Checkpoint**: US2 works. Quickstart steps 2–4 and the ranking part of step 7.

---

## Phase 5: User Story 3 - Result in the agreed format, several steps, broken output dropped (Priority: P1)

**Goal**: Output that does not fit the format or is obviously broken is never delivered; jobs run in several steps; a failing model falls back to the next ranked one.

**Independent Test**: A scripted model returns valid, wrong-format, empty, looping and script-switching output in turn; only the valid one becomes a result, every other case gives a structured error.

- [ ] T038 [US3] Tests first in `src-tauri/src/model_jobs/runner_tests.rs` (guard and format): each broken output ends the attempt as `Unusable` with its reason; a valid output becomes the result; holzi never edits the model's text (the correction step's rule lives with the extension, FR-013)
- [ ] T039 [US3] Apply the guard and the schema per attempt in `src-tauri/src/model_jobs/runner.rs` using T009; the event error distinguishes `Unusable` from `Unavailable`, `Timeout`, `Cancelled`
- [ ] T040 [US3] Tests first in `src-tauri/src/model_jobs/runner_tests.rs` (steps): the result of step 1 reaches step 2 through `from` and its pointer; a pointer that does not resolve at run time ends the step with `Unusable` ("from fehlt") without fallback; the start fails before step 1 if any step has no candidate (FR-025)
- [ ] T041 [US3] Implement the multi-step run in `src-tauri/src/model_jobs/runner.rs` (inject the selected part as text, pass results forward, collect every step's result) and the all-steps preflight in `src-tauri/src/model_jobs/bridge.rs`
- [ ] T042 [US3] Tests first in `src-tauri/src/model_jobs/runner_tests.rs` (fallback): model 1 unusable, model 2 used; each model at most once; all models exhausted gives `Unusable` or the last reason; the provenance lists every attempt with outcome and reason (FR-023, FR-027)
- [ ] T043 [US3] Implement the fallback chain in `src-tauri/src/model_jobs/runner.rs` over `candidates::for_step`, recording `attempts[]` in the completion event
- [ ] T044 [US3] Test and enforce no tools (FR-026) in `src-tauri/src/model_jobs/runner_tests.rs`: the `ChatRequest` has an empty tool list for every step; an instruction or image text that asks for a command or file access cannot cause any action (the fake adapter returns a tool-call chunk, which the runner ignores and reports as `Unusable`)

**Checkpoint**: US3 works. Quickstart steps 7–9.

---

## Phase 6: User Story 4 - Images leave the device only with consent (Priority: P1)

**Goal**: Local first; a destination outside the device needs the user's consent before the first send; a fallback never goes to a destination without consent.

**Independent Test**: A request to a cloud model without consent sends nothing; a request to a local model creates no network request.

- [ ] T045 [US4] Tests first in `src-tauri/tests/extension_model_jobs.rs`: (1) a cloud model without consent gives the prompt and zero adapter calls; (2) after "deny" nothing is sent and the job ends with `ConsentDenied`; (3) a local model never builds a network client (SC-003); (4) a fallback candidate without consent is skipped and never tried
- [ ] T046 [US4] Tests first in `src-tauri/src/extensions/commands/permissions_tests.rs`: `modelDestination` allowed "for the session" holds in memory and asks again after a new process; allowed permanently writes a row and is revocable; a denial is remembered only when asked to
- [ ] T047 [US4] Implement the destination check in the preflight (`src-tauri/src/model_jobs/bridge.rs`): for the first candidate of each step with a place other than `this_device`, evaluate the consent and return `PermissionPromptRequired` with `{resourceType: "modelDestination", action: "send", target: <provider id>}`; drop fallback candidates without a consent (T025 filter)
- [ ] T048 [US4] `src/components/extensions/PermissionRequestDialog.vue`: branch for `modelDestination` showing provider name, host and place ("another device", "server", "cloud"), that images and texts will be sent, and "for the session" or "permanently"; de/en keys under `extensions.permissionRequest.modelDestination`
- [ ] T049 [US4] `src/components/settings/extensions/ExtensionPermissionsView.vue`: list and revoke `modelDestination` consents and held session decisions with the provider's name; de/en keys; extend `scripts/check-extensions-agent-tools.ts` or add `check-extensions-model-consent.ts` for the pure part

**Checkpoint**: US4 works. Quickstart steps 5 and 6.

---

## Phase 7: User Story 5 - A model on another device, a server or in the cloud (Priority: P2)

**Goal**: The user adds a generic OpenAI-compatible provider with an address and a key; its models appear like any other; the phone only takes the photo.

**Independent Test**: A test server with this interface is entered as a provider; its models show in the grant list and a granted extension receives a result through it.

- [x] T050 [US5] Tests first in `src-tauri/src/adapters/openai_compatible_tests.rs` against a local mock server: model list; streaming with `delta.content`, `delta.reasoning_content` and `[DONE]`; fragments of `tool_calls`; an image as an `image_url` data URI; `reasoning_effort`; 401 and 403 give `InvalidCredentials`, 429 and 5xx are transient; a TLS error is reported through `transport_error`
- [x] T051 [US5] Implement `src-tauri/src/adapters/openai_compatible.rs` (`ProviderAdapter`: `list_models`, `stream_chat`, `Authorization: Bearer`) and export it in `src-tauri/src/adapters/mod.rs`
- [ ] T052 [US5] Tests first in `src-tauri/src/providers/openai_compatible_tests.rs`: adding a provider with the adapter `openai_compatible` stores base URL and encrypted key, refreshes the models; a wrong key fails with a message that holds no key; `build_adapter` returns the adapter
- [x] T053 [US5] Register `openai_compatible` in `validate_adapter`, `build_adapter` and `add_provider` in `src-tauri/src/providers/mod.rs`
- [ ] T054 [US5] Tests first for the user's model capabilities in `src-tauri/src/providers/capabilities_override_tests.rs`: the user sets "takes images" and the effort levels for a model; the setting wins over the provider's answer in `models.capabilities_json`; candidates and `extension_model_list` read it (FR-019)
- [x] T055 [US5] Implement the capability override command in `src-tauri/src/providers/` and its use in `model_capabilities.rs`; regenerate bindings
- [ ] T056 [US5] Frontend: the provider dialog gets the vendor "OpenAI-kompatibel" (address, key), the model list a "takes images" switch and the effort levels in `src/components/settings/providers/…`; de/en keys; a pure check for the address validation in `scripts/check-provider-address.ts`

**Checkpoint**: US5 works (delivery A, can land first). Quickstart step 1.

---

## Phase 8: User Story 6 - Local models understand images (Priority: P2)

**Goal**: A local vision model reads an image without network access; a text-only model refuses an image job before anything is sent.

**Independent Test**: With a local vision test model an image job runs without network access; with a text-only model the same job is rejected before any processing.

- [ ] T057 [US6] **Spike** (research R8): an ignored test `src-tauri/src/llm/local/multimodal_spike_tests.rs` loads Qwen3-VL-4B through `MultimodalModelBuilder` with 4-bit in-situ quantisation on the development card and reads a test page; record peak VRAM and time per page in `research.md` R8 and decide: go (T058–T062) or fallback (T063). Needs a full CUDA build and a model download: ask first and run memory-limited through the devShell (`systemd-run --user --scope -p MemoryMax=10G nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 2 …`), with the holzi dev app closed
- [ ] T058 [US6] Tests first in `src-tauri/src/llm/local/stream_tests.rs`: `build_request` decodes `Attachment` bytes to `DynamicImage` and uses `add_image_message`; a corrupt image gives an error, not a silent drop
- [ ] T059 [US6] Add `LocalModel::load_multimodal(dir, isq)` in `src-tauri/src/llm/local/loader.rs` and the image path in `src-tauri/src/llm/local/stream.rs`
- [ ] T060 [US6] Catalog fields `format`, `vision`, `isq` in `src-tauri/src/catalog/model_catalog.json` and its Rust type, a vision entry (Qwen3-VL-4B) and the download of a directory model (progress, `relative_path` of the directory, summed `size_bytes` in `device_downloaded_models_no_sync`); tests next to the catalog code
- [ ] T061 [US6] Tests first in `src-tauri/src/model_capabilities_tests.rs`: a local model's capabilities come from the catalog (`vision` gives `Image`, text models stay `Some(vec![])`); then implement in `src-tauri/src/model_capabilities.rs` and `usability_for` in `src-tauri/src/chat/attachments.rs`
- [ ] T062 [US6] Tests first in `src-tauri/src/model_jobs/model_source_tests.rs` (local slot): a job with another local model replaces the chat model in the single slot, waits behind a running chat turn, and the chat reloads on its next message; then implement the local source in `src-tauri/src/model_jobs/model_source.rs` over `chat/model_loading.rs`
- [ ] T063 [US6] Only if the spike fails: document local vision through a localhost OpenAI-compatible provider (Ollama or a llama.cpp server) in `research.md` R8 and `quickstart.md`, and mark T058–T062 as not needed; no code
- [ ] T064 [US6] Text-only model rejects an image job in the preflight (`src-tauri/src/model_jobs/bridge.rs`, via `candidates`), tested in `src-tauri/tests/extension_model_jobs.rs` (SC-006)

**Checkpoint**: US6 works (delivery C). The "only with delivery C" step of the quickstart.

---

## Phase 9: User Story 7 - Page recognition as the first example (Priority: P2)

**Goal**: haex-notes describes a page recognition job with its own instruction and format; holzi checks it like any other format.

**Independent Test**: A test extension describes this format; a scripted model returns a page with a text, an image and a table region; holzi validates it, and a table with a missing cell is dropped.

- [ ] T065 [US7] Signed test extension `src-tauri/tests/fixtures/extension_e2e/model-jobs/` (with `.xt`, built like `agent-tools.xt`, the released vault-sdk bundled) that sends the two steps of `contracts/job-format.md` §Beispiel and has test modes for a loop, empty output and CJK output; add its build to the fixture script
- [ ] T066 [US7] Integration test in `src-tauri/tests/extension_model_jobs.rs`: the page format with a table passes; a table row with a missing cell is `Unusable`; the sideways-photo case returns the model's own hint without a holzi-side rotation
- [ ] T067 [US7] E2E scenario `scripts/e2e/scenarios/extension-model-jobs.test.ts`: grant, consent for a session, a job with the test extension, revocation (quickstart steps 2–6)
- [ ] T068 [US7] vault-sdk (own repo, own PR): `sdk.models.list()`, `sdk.models.run(job)`, `sdk.models.cancel(id)` and the completion event in TypeScript with vitest tests against the contract; holzi pins the released version in the fixture build
- [ ] T069 [US7] Update `specs/047-structured-agent-tasks/spec.md` status and `specs/017-extension-host/contracts/bridge.md` (the three new methods, the removed `extension_ai_task_*`); note in `specs/046-paper-notes-canvas/plan.md` that recognition goes through spec 055

---

## Phase 10: Polish & Cross-Cutting Concerns

- [ ] T070 [P] Storage test `src-tauri/tests/extension_model_jobs_storage.rs`: after completed, cancelled and failed jobs the content of every table equals the content before, except permissions and consents (SC-011); and no `tracing` field logs an image, instruction or answer (grep test over `model_jobs/`)
- [ ] T071 [P] Check every new or changed file stays under 500 lines (`wc -l` over the plan's structure tree)
- [ ] T072 Remove leftovers of `agent_tasks` and `extension_ai_task_*` in code, comments and docs (`rg "agent_tasks|extension_ai_task|ai-task"`)
- [ ] T073 Run the checks through the devShell: `nix develop --command scripts/with-nix-host-bridge.sh cargo fmt --check`, `nix develop --command scripts/with-nix-host-bridge.sh cargo check --manifest-path src-tauri/Cargo.toml` and the corresponding `cargo test --manifest-path src-tauri/Cargo.toml` commands for both default and `--no-default-features`, `nix develop --command pnpm lint:rust` (both feature sets), and `nix develop --command pnpm <script>` for every `check:*`, `typecheck`, `typecheck:scripts`, `lint`, `format:check` and the E2E scenario; run clippy for the Android target as well and keep Rust builds memory-limited
- [ ] T074 Walk through `specs/055-extension-model-access/quickstart.md` steps 1–12 (and the local-image step with delivery C); record skipped steps in the PR description
- [ ] T075 Add **Modellauftrag**, **Modellfreigabe** and **Ziel (Ort)** to `CONTEXT.md` (extensions section)

---

## Dependencies & Execution Order

- Phase 1 (T001–T003) before Phase 2. Phase 2 blocks every story; inside it T004–T005 (permission kinds), T006–T007, T008–T011, T012–T015 and T016–T017 are separate groups (parallel where marked), T019 and T020 last.
- US1 (Phase 3) needs Phase 2. US2 needs US1 (a grant is checked in the same preflight). US3 needs US1. US4 needs US1 and US2 (it adds to the prompt and the candidate filter). US5 is independent and can land first. US6 needs US1 (a source for local models) and ideally US5 for the fallback. US7 needs US1–US4; T068 (SDK) can run in parallel once `contracts/bridge.md` is stable.
- Delivery B + D = Phases 1–6; A = Phase 7; C = Phase 8; E = Phase 9; Phase 10 closes each delivery's PR for its own part.

### Within Each Story

Test before implementation; Rust types before commands; commands before frontend; frontend checks after.

### Parallel Opportunities

- T003 ∥ T006 ∥ T008 ∥ T010 ∥ T012 ∥ T014 ∥ T018 (different files, after T002).
- T022 ∥ T024; T029 ∥ T027.
- T035 ∥ T031 (pure TypeScript against the shared shape of the grant).
- US5 (Phase 7) in parallel to Phases 3–6 if two people work.

## Parallel Example: Foundational

```text
T006/T007 place.rs   |  T008/T009 guard.rs   |  T010/T011 effort.rs   |  T012/T013 job.rs
```

## Implementation Strategy

### MVP First

Phases 1–3: a permitted extension sends a job to a remote model through a scripted adapter and gets a checked
result. Stop and run quickstart steps 2, 3 and 12.

### Incremental Delivery

1. Delivery B (+ D for the dialogs): Phases 1–6, tested with the fake adapter. 2. Delivery A: Phase 7 (can land first
   or in parallel). 3. Delivery C: spike T057, then T058–T062 or the fallback T063. 4. Delivery E: Phase 9, vault-sdk
   PR and the test extension. The page recognition in haex-notes (spec 046) follows in `haextension` after E.
