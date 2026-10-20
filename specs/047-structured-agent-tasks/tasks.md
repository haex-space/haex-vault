# Tasks: Structured Agent Tasks for Extensions

> **Abgelöst:** Diese Aufgabenliste ist ein historischer Entwurf aus Spec 047.
> Sie ist nicht ausführbar; neue Aufgaben gehören in
> [Spec 055](../055-extension-model-access/tasks.md).

## Phase 1: Setup

- [ ] T001 [P] Create the `agent_tasks` module skeleton and register it from `src-tauri/src/lib.rs`, keeping task lifecycle, bridge handlers, validation, and provider execution in separate files below the 500-LoC project limit.
- [ ] T002 [P] Add deterministic fixture types and test helpers for `paper-note-recognition` in `src-tauri/src/agent_tasks/fixture.rs` and `src-tauri/src/agent_tasks/fixture_tests.rs`, with no network or real model dependency.
- [x] T003 [P] Add the public task contract examples and status/error vocabulary to `specs/047-structured-agent-tasks/contracts/task.md`, including the asynchronous start, completion, and cancellation messages consumed by extensions.

## Phase 2: Foundational host infrastructure

- [ ] T004 Define versioned task input, processing provenance, lifecycle status, structured error, and task-result types in `src-tauri/src/agent_tasks/model.rs`, with serde round-trip tests in `src-tauri/src/agent_tasks/model_tests.rs`.
- [ ] T005 Define the in-memory task registry, ownership key, cancellation token, timeout metadata, and bounded-per-extension concurrency policy in `src-tauri/src/agent_tasks/state.rs`, with transition tests in `src-tauri/src/agent_tasks/state_tests.rs`.
- [ ] T006 Add the task-definition registry for `paper-note-recognition` in `src-tauri/src/agent_tasks/definitions.rs`, including schema version, required capabilities, allowed input fields, and the absence of general tools or arbitrary file access.
- [ ] T007 Extend the extension-host state and frame lifecycle so an extension bridge handler can access the task manager without receiving provider credentials or bypassing frame-session ownership checks in `src-tauri/src/extensions/host.rs`, `src-tauri/src/extensions/bridge/dispatch.rs`, and `src-tauri/src/extensions/commands/frames.rs`.
- [ ] T008 Add a host-side result validator in `src-tauri/src/agent_tasks/validation.rs` for the paper-note region schema, rejecting malformed JSON, missing required fields, invalid confidence/range values, unsupported region kinds, and schema-version mismatches; cover rejection cases in `src-tauri/src/agent_tasks/validation_tests.rs`.
- [ ] T009 Add structured-task error mapping in `src-tauri/src/agent_tasks/errors.rs` so profile, capability, timeout, cancellation, consent, provider, and invalid-result failures never escape as raw provider text or unstructured bridge failures.

## Phase 3: User Story 1 — Extensions start and receive structured tasks

### Tests first

- [ ] T010 [US1] Add a fixture-runner test in `src-tauri/src/agent_tasks/runner_tests.rs` proving that a valid image request produces a schema-valid result and that free-form model text cannot be emitted as a successful task result.
- [ ] T011 [US1] Add bridge allowlist and frame-ownership tests in `src-tauri/src/extensions/bridge/dispatch_tests.rs` proving that only an active originating frame can start or cancel its task and that another extension receives no completion event.
- [ ] T012 [US1] Add an async lifecycle integration test in `src-tauri/tests/extension_agent_tasks.rs` covering start response, completion event, failure event, cancellation, and idempotent cancellation using the deterministic fixture runner.

### Implementation

- [ ] T013 [US1] Implement request decoding and definition lookup in `src-tauri/src/agent_tasks/bridge.rs`, accepting only `paper-note-recognition`, schema version 1, declared image metadata, locale, and supported options.
- [ ] T014 [US1] Implement `extension_ai_task_start` as an asynchronous bridge entry point in `src-tauri/src/agent_tasks/bridge.rs` and `src-tauri/src/extensions/bridge/dispatch.rs`, returning a task ID immediately while scheduling work outside the blocking bridge call.
- [ ] T015 [US1] Implement the fixture-backed task runner in `src-tauri/src/agent_tasks/bridge.rs` with start, success, structured failure, and cancellation transitions, never mutating the submitted image bytes.
- [ ] T016 [US1] Implement completion-event emission through `src-tauri/src/extensions/bridge/events.rs`, routing `haextension:ai-task:completed` only to frames of the originating extension and preserving the task ID and processing provenance.
- [ ] T017 [US1] Implement `extension_ai_task_cancel` and cleanup hooks for originating-frame close, extension disable/remove, and vault shutdown in `src-tauri/src/agent_tasks/bridge.rs`, `src-tauri/src/extensions/commands/frames.rs`, and the relevant extension-host lifecycle module.
- [ ] T018 [US1] Register the two bridge methods and their request validation in `src-tauri/src/extensions/bridge/dispatch.rs` and wire the task registry into the extension host state.

## Phase 4: User Story 2 — Profile, harness, model, and capability selection

### Tests first

- [ ] T019 [US2] Add capability tests in `src-tauri/src/model_capabilities_tests.rs` proving that a text-only model is rejected for an image/OCR task before provider execution and that structured-output support is required.
- [ ] T020 [US2] Add profile-resolution tests in `src-tauri/src/agent_tasks/profile_tests.rs` proving default-profile selection, explicit profile selection, unavailable-profile errors, and provenance generation for local and remote profiles.
- [ ] T021 [US2] Add provider-boundary tests in `src-tauri/src/agent_tasks/provider_tests.rs` proving that image bytes, task instructions, and schema constraints reach the selected adapter while general chat tools and credentials do not.

### Implementation

- [ ] T022 [US2] Extend `ModelCapabilities` in `src-tauri/src/model_capabilities.rs` with OCR, structured-output, and layout-region capability fields that remain backward-compatible with persisted capability JSON; image input reuses the existing `accepted_attachment_kinds` (`AttachmentKind::Image`) instead of a second field.
- [ ] T023 [US2] Implement task profile resolution in `src-tauri/src/agent_tasks/profile.rs` using the existing provider/model/harness preferences and model catalog, returning a non-secret profile snapshot plus a clear unavailable-profile error.
- [ ] T024 [US2] Implement preflight capability checks in `src-tauri/src/agent_tasks/runner.rs` and `src-tauri/src/agent_tasks/profile.rs`, rejecting unsupported image/OCR/structured-output combinations before constructing a provider request.
- [ ] T025 [US2] Add the structured provider request path in `src-tauri/src/agent_tasks/provider.rs`, reusing `ProviderAdapter` and the existing session/model selection while adding image attachments, schema-constrained instructions, task timeout, and no general tool registry.
- [ ] T026 [US2] Map adapter output into the versioned paper-note result and populate actual model, harness, profile, mode, task version, and optional destination provenance in `src-tauri/src/agent_tasks/runner.rs`.
- [ ] T027 [US2] Expose task-profile selection through the existing Holzi model/harness preferences so changing the configured profile takes effect for haex-notes without rebuilding the extension or migrating its canvas data.
- [ ] T028 [US2] Add a local Vision/OCR-capable adapter capability fixture and document that the existing text-only local Qwen profile is rejected until an attachment-capable local backend is configured in `specs/047-structured-agent-tasks/research.md` and `specs/047-structured-agent-tasks/quickstart.md`.

## Phase 5: User Story 3 — Privacy, consent, cancellation, and failure safety

### Tests first

- [ ] T029 [US3] Add consent-gate tests in `src-tauri/src/agent_tasks/privacy_tests.rs` proving that remote profiles return `consent_required` with an opaque consent request ID, without sending image bytes, and that only the host-owned consent command can approve or deny it.
- [ ] T030 [US3] Add locality tests in `src-tauri/src/agent_tasks/privacy_tests.rs` proving that a local profile cannot fall back to a remote adapter when the local profile is unavailable or fails.
- [ ] T031 [US3] Add timeout, cancellation, frame-close, and vault-close tests in `src-tauri/src/agent_tasks/lifecycle_tests.rs` proving that the original image remains available to the extension after every failure path.
- [ ] T032 [US3] Add payload-sanitization tests in `src-tauri/src/agent_tasks/privacy_tests.rs` proving that credentials, full local paths, and raw provider responses are absent from requests, events, and structured errors.

### Implementation

- [ ] T033 [US3] Implement remote consent gating and destination reporting in `src-tauri/src/agent_tasks/profile.rs` and `src-tauri/src/agent_tasks/bridge.rs`; return an opaque consent request ID, expose `agent_task_consent_resolve` only to Holzi's own window, and require explicit approval before any remote adapter receives image bytes.
- [ ] T034 [US3] Enforce local-only execution for local profiles and remove any implicit fallback path in `src-tauri/src/agent_tasks/runner.rs` and the provider selection layer.
- [ ] T035 [US3] Apply the bounded task timeout and cooperative cancellation token to provider execution, and translate all cancellation/timeout outcomes into the contract’s structured completion errors.
- [ ] T036 [US3] Add lifecycle hooks that cancel tasks when their originating frame, extension, or vault is closed, without deleting or rewriting extension-owned input data.
- [ ] T037 [US3] Centralize provenance/error sanitization in `src-tauri/src/agent_tasks/errors.rs` and `src-tauri/src/agent_tasks/runner.rs`, ensuring secrets, raw model responses, and machine-local absolute paths cannot cross the bridge.

## Phase 6: Polish, verification, and downstream handoff

- [ ] T038 Update `specs/047-structured-agent-tasks/quickstart.md` with the final bridge call sequence, host-only consent behavior, fixture setup, local Vision/OCR profile prerequisites, and the exact task name, schema version, result region shape, provenance fields, and cancellation behavior required by haex-notes.
- [ ] T039 Run `cargo fmt --check`, targeted `cargo test` for `agent_tasks` and extension bridge tests, the integration test suite, and `cargo check`; record any platform-specific limitation in `specs/047-structured-agent-tasks/quickstart.md`.
- [ ] T040 Review the final diff for the 500-LoC-per-file limit, absence of secrets and machine-local paths, local-no-fallback behavior, and cross-repository contract references before opening the implementation PR.

## Dependencies and execution order

1. Phase 1 establishes the module boundary and the versioned contract.
2. Phase 2 is the shared foundation and must complete before any user-story implementation.
3. Phase 3 delivers a deterministic end-to-end fixture path and is independently testable without a model.
4. Phase 4 replaces the fixture execution path with the selected Holzi provider/model/harness and adds explicit capability checks.
5. Phase 5 hardens the path against data exfiltration, implicit fallback, cancellation, and lifecycle changes.
6. Phase 6 verifies the integrated contract and prepares the downstream haex-notes adapter.

The critical path is `T004 → T005/T006/T007 → T013–T018 → T023–T026 → T033–T037 → T039–T040`.
Tests marked `[P]` may run in parallel with unrelated setup work. Story tests should be written before their corresponding implementation tasks; the fixture path makes this possible without network access.

## Parallel examples

### After Phase 2

```text
Track A: T010–T012, then T013–T018 (fixture lifecycle and bridge)
Track B: T019–T021, then T022–T028 (capabilities and provider boundary)
Track C: T029–T032, then T033–T037 (privacy and lifecycle hardening)
```

Tracks B and C depend on the task model/state and bridge context from Phase 2, but can proceed independently once those interfaces are stable. Merge conflicts should be resolved in the bridge registration files and runner boundary rather than duplicating handlers.

## Implementation strategy

1. Ship the fixture-backed asynchronous contract first so haex-notes can integrate against a stable event shape.
2. Add profile and capability resolution before enabling real provider execution; a text-only local Qwen model must fail clearly.
3. Add the real image-capable provider path behind the same runner and schema validator.
4. Finish consent, locality, cancellation, and sanitization checks before enabling remote profiles.
5. Only then wire the haex-notes extension adapter and its canvas mapping in the separate haextension worktree.
